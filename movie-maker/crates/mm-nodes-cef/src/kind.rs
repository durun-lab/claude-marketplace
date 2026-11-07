//! `web.cef` の種類。

use std::path::PathBuf;
use std::sync::atomic::{AtomicU64, Ordering};
use std::sync::{Arc, Mutex, MutexGuard, PoisonError, Weak};

use mm_gpu::{GpuContext, WgpuGpu};
use mm_host_api::{HostLauncher, SharedFrame};
use mm_node_api::{Cost, FrameCx, FrameRef, GpuTexture, Isolation, NodeKind, Traits};
use mm_types::{MmError, ParamSchema, ParamSpec, ParamType, ParamValue, Params, Result, RgbaImage};

use crate::cache::{Cached, FrameCache, FrameKey};
use crate::gpu::{copy_into, Converter};
use crate::hosts::{FrameTransport, HostPages};
use crate::page::{FrameRequest, Page, DATA_PARAM, HTML_PARAM, STATIC_PARAM};
use crate::transfer::rec1886_from_srgb_linear;
use crate::{mock, KIND_ID, KIND_VERSION};

/// 移行できる最も古いバージョン。パラメータの形は 1 から変わっていない（上げたのは画素が変わったため）。
const OLDEST_VERSION: u32 = 1;

/// 描いた数と、キャッシュから返した数。
#[derive(Debug, Clone, Copy, PartialEq, Eq, Default)]
pub struct Stats {
    pub drawn: u64,
    pub hits: u64,
}

/// HTML と CSS のページをレイヤーとして描く種類。ページは `window.mmSetTime(t, data)` を定義し、
/// レイヤーの始まりからの秒 `t` と `data` の関数としてだけ画面を作る（同じ入力なら同じ画素）。
///
/// レンダラーが `WgpuGpu`（D3D12）なら共有テクスチャで受けて GPU のまま変換し、それ以外は CPU の画像で受ける。
/// CEF が無い環境ではモック（[`WebCefKind::mock`]）が矩形を描く。
pub struct WebCefKind {
    /// ホストの組。`None` ならモック。作るときに決まるので、ホストの有無はロックを取らずに分かる。
    hosts: Option<Mutex<HostPages>>,
    cache: Mutex<FrameCache>,
    converter: Mutex<Option<Converter>>,
    device: Mutex<DeviceGeneration>,
    drawn: AtomicU64,
    hits: AtomicU64,
}

/// 最後に共有テクスチャで描いたデバイスと、その世代。デバイスを作り直すと世代が進み、ホストを起こし直す。
#[derive(Default)]
struct DeviceGeneration {
    ctx: Weak<GpuContext>,
    generation: u64,
}

impl DeviceGeneration {
    /// `ctx` の世代。前と違うデバイスなら世代を進める。
    fn of(&mut self, ctx: &Arc<GpuContext>) -> u64 {
        if !self.ctx.upgrade().is_some_and(|c| Arc::ptr_eq(&c, ctx)) {
            self.ctx = Arc::downgrade(ctx);
            self.generation += 1;
        }
        self.generation
    }
}

/// ロックを取る。キャッシュ、変換、世代は 1 回の代入で書き換えるだけなので、パニックの後も中身はそのまま使える。
fn relock<T>(m: &Mutex<T>) -> MutexGuard<'_, T> {
    m.lock().unwrap_or_else(PoisonError::into_inner)
}

impl WebCefKind {
    fn with_hosts(hosts: Option<HostPages>) -> Self {
        Self {
            hosts: hosts.map(Mutex::new),
            cache: Mutex::new(FrameCache::default()),
            converter: Mutex::new(None),
            device: Mutex::new(DeviceGeneration::default()),
            drawn: AtomicU64::new(0),
            hits: AtomicU64::new(0),
        }
    }

    /// `executable`（`mm-host-cef`）を `launcher` で起こして描く。
    pub fn with_host(launcher: Arc<dyn HostLauncher>, executable: PathBuf) -> Self {
        Self::with_hosts(Some(HostPages::new(launcher, executable)))
    }

    /// CEF の代わりに矩形を描く。
    pub fn mock() -> Self {
        Self::with_hosts(None)
    }

    /// ホストで描くか（偽ならモック）。
    pub fn uses_host(&self) -> bool {
        self.hosts.is_some()
    }

    /// 描いた数とキャッシュから返した数。
    pub fn stats(&self) -> Stats {
        Stats {
            drawn: self.drawn.load(Ordering::Relaxed),
            hits: self.hits.load(Ordering::Relaxed),
        }
    }

    /// ホストの組のロック。モックなら `None`。描画の途中でパニックしたホストは応答の途中にいるかもしれないので、
    /// 毒の入ったロックは全部のホストを捨ててから使う。
    fn hosts(&self) -> Option<MutexGuard<'_, HostPages>> {
        let m = self.hosts.as_ref()?;
        Some(m.lock().unwrap_or_else(|poisoned| {
            let mut hosts = poisoned.into_inner();
            hosts.clear();
            m.clear_poison();
            hosts
        }))
    }

    fn cache(&self) -> MutexGuard<'_, FrameCache> {
        relock(&self.cache)
    }

    fn hit(&self) {
        self.hits.fetch_add(1, Ordering::Relaxed);
    }

    fn drew(&self) {
        self.drawn.fetch_add(1, Ordering::Relaxed);
    }

    /// 共有テクスチャで受けられるか。ホストが D3D12 のテクスチャを渡すので、D3D12 のデバイスが要る。
    fn can_share(&self, ctx: &GpuContext) -> bool {
        cfg!(windows) && ctx.backend() == wgpu::Backend::Dx12 && self.uses_host()
    }

    /// `ctx` の上で作ったキャッシュのテクスチャ。
    fn gpu_hit(&self, key: &FrameKey, ctx: &Arc<GpuContext>) -> Option<wgpu::Texture> {
        match self.cache().get(key)? {
            Cached::Gpu { texture, ctx: made }
                if made.upgrade().is_some_and(|c| Arc::ptr_eq(&c, ctx)) =>
            {
                self.hit();
                Some(texture)
            }
            _ => None,
        }
    }

    fn cpu_hit(&self, key: &FrameKey) -> Option<Arc<RgbaImage>> {
        match self.cache().get(key)? {
            Cached::Cpu(image) => {
                self.hit();
                Some(image)
            }
            Cached::Gpu { .. } => None,
        }
    }

    /// GPU の経路。共有テクスチャを使えなければ `None`。
    fn render_gpu(
        &self,
        gpu: &mut WgpuGpu,
        req: &FrameRequest<'_>,
        key: &FrameKey,
    ) -> Result<Option<FrameRef>> {
        let ctx = gpu.context().clone();
        if !self.can_share(&ctx) {
            return Ok(None);
        }
        self.cache().drop_other_devices(&ctx);
        let texture = match self.gpu_hit(key, &ctx) {
            Some(texture) => texture,
            None => self.draw_and_keep(&ctx, req, key)?,
        };
        let id = gpu.create(req.size)?;
        let dst = gpu
            .texture(id)
            .ok_or_else(|| MmError::Internal("確保したテクスチャが無い".into()))?;
        copy_into(&ctx, &texture, dst);
        Ok(Some(FrameRef::Gpu(GpuTexture { id, size: req.size })))
    }

    /// 共有テクスチャで描いてキャッシュする。ホストのロックを待つ間に別のスレッドが同じフレームを描いたかもしれないので、
    /// 取った後にもう一度引く。
    fn draw_and_keep(
        &self,
        ctx: &Arc<GpuContext>,
        req: &FrameRequest<'_>,
        key: &FrameKey,
    ) -> Result<wgpu::Texture> {
        let mut hosts = self
            .hosts()
            .ok_or_else(|| MmError::Internal("モックは共有テクスチャを出さない".into()))?;
        if let Some(texture) = self.gpu_hit(key, ctx) {
            return Ok(texture);
        }
        let texture = self.draw_shared(&mut hosts, ctx, req)?;
        let value = Cached::Gpu {
            texture: texture.clone(),
            ctx: Arc::downgrade(ctx),
        };
        self.cache().put(key.clone(), value);
        Ok(texture)
    }

    /// ホストに共有テクスチャで描かせ、中間の形式のテクスチャにする。
    fn draw_shared(
        &self,
        hosts: &mut HostPages,
        ctx: &Arc<GpuContext>,
        req: &FrameRequest<'_>,
    ) -> Result<wgpu::Texture> {
        let transport = FrameTransport::Shared {
            adapter: ctx.adapter.clone(),
            device: relock(&self.device).of(ctx),
        };
        let frame = hosts.render(req, &transport)?;
        self.drew();
        let shared = open_shared(ctx, frame, req)?;
        let mut converter = relock(&self.converter);
        let converter = match converter.take() {
            Some(c) if c.is_for(ctx) => converter.insert(c),
            _ => converter.insert(Converter::new(ctx)),
        };
        Ok(converter.convert(ctx, &shared))
    }

    /// CPU の経路。モックか、ホストの CPU の画像。
    fn render_cpu(&self, req: &FrameRequest<'_>, key: FrameKey) -> Result<FrameRef> {
        if let Some(image) = self.cpu_hit(&key) {
            return Ok(FrameRef::Cpu(image));
        }
        let Some(mut hosts) = self.hosts() else {
            return self.keep_cpu(req, key, mock::render(req));
        };
        // draw_and_keep と同じく、ロックを取った後にもう一度引く。
        if let Some(image) = self.cpu_hit(&key) {
            return Ok(FrameRef::Cpu(image));
        }
        let image = cpu_image(hosts.render(req, &FrameTransport::Cpu)?)?;
        self.keep_cpu(req, key, image)
    }

    /// 描いた CPU の画像を確かめてキャッシュする。
    fn keep_cpu(
        &self,
        req: &FrameRequest<'_>,
        key: FrameKey,
        image: RgbaImage,
    ) -> Result<FrameRef> {
        self.drew();
        if image.size() != req.size {
            return Err(MmError::Internal(format!(
                "web.cef のフレームの大きさ {:?} が頼んだ {:?} と違う",
                image.size(),
                req.size
            )));
        }
        let image = Arc::new(image);
        self.cache().put(key, Cached::Cpu(image.clone()));
        Ok(FrameRef::Cpu(image))
    }
}

/// CPU の画像の応答を画像にする。共有テクスチャが来たらハンドルを閉じて誤りにする。
fn cpu_image(frame: SharedFrame) -> Result<RgbaImage> {
    match frame {
        SharedFrame::Cpu { size, mut data } => {
            rec1886_from_srgb_linear(&mut data);
            RgbaImage::from_raw(size, data)
        }
        SharedFrame::SharedTexture { handle, fence, .. } => {
            close_handles(handle, fence.map(|f| f.handle));
            Err(MmError::Internal(
                "CPU の画像を頼んだのに共有テクスチャが来た".into(),
            ))
        }
    }
}

#[cfg(windows)]
fn close_handles(handle: u64, fence: Option<u64>) {
    std::iter::once(handle)
        .chain(fence)
        .for_each(crate::shared::close);
}

#[cfg(not(windows))]
fn close_handles(_handle: u64, _fence: Option<u64>) {}

#[cfg(windows)]
fn open_shared(
    ctx: &GpuContext,
    frame: SharedFrame,
    req: &FrameRequest<'_>,
) -> Result<wgpu::Texture> {
    match frame {
        SharedFrame::SharedTexture {
            handle,
            size,
            fence: None,
        } if size == req.size => crate::shared::open(ctx, handle, size),
        SharedFrame::SharedTexture {
            handle,
            fence,
            size,
        } => {
            close_handles(handle, fence.map(|f| f.handle));
            Err(MmError::Unsupported(format!(
                "共有テクスチャの大きさ {size:?}（頼んだのは {:?}）かフェンスつきの受け渡しは受けられない",
                req.size
            )))
        }
        SharedFrame::Cpu { .. } => Err(MmError::Internal(
            "共有テクスチャを頼んだのに CPU の画像が来た".into(),
        )),
    }
}

#[cfg(not(windows))]
fn open_shared(_: &GpuContext, _: SharedFrame, _: &FrameRequest<'_>) -> Result<wgpu::Texture> {
    Err(MmError::Unsupported("共有テクスチャは Windows だけ".into()))
}

impl NodeKind for WebCefKind {
    fn id(&self) -> &'static str {
        KIND_ID
    }

    fn version(&self) -> u32 {
        KIND_VERSION
    }

    fn params(&self) -> ParamSchema {
        ParamSchema::new(vec![
            ParamSpec::new(HTML_PARAM, ParamType::Text, ParamValue::Text(String::new())),
            ParamSpec::new(
                DATA_PARAM,
                ParamType::Json,
                ParamValue::Json(serde_json::json!({})),
            ),
            ParamSpec::new(STATIC_PARAM, ParamType::Bool, ParamValue::Bool(false)),
        ])
    }

    /// モックは決定的と申告しない。申告するとレンダラーがモックの矩形をディスクに退避し、
    /// CEF のある次の起動で同じキーのページの代わりに引いてしまう。
    fn traits(&self) -> Traits {
        Traits {
            realtime: false,
            cost: Cost::Heavy,
            isolation: Isolation::OutOfProcess,
            deterministic: self.uses_host(),
            ..Traits::PIXEL
        }
    }

    fn migrate(&self, old: serde_json::Value, from: u32) -> Result<Params> {
        if !(OLDEST_VERSION..=KIND_VERSION).contains(&from) {
            return Err(MmError::Unsupported(format!(
                "{KIND_ID} のバージョン {from} からの移行が無い"
            )));
        }
        let params: Params = serde_json::from_value(old)?;
        self.params().validate(&params)?;
        Ok(params)
    }

    fn render(&self, cx: &mut FrameCx<'_>) -> Result<FrameRef> {
        let _span = tracing::info_span!("nodes_cef.render").entered();
        let page = Page::read(cx.params).filter(|_| cx.size.area() > 0);
        let Some(page) = page else {
            return Ok(FrameRef::Transparent { size: cx.size });
        };
        let req = FrameRequest {
            page: &page,
            time: page.time(cx.source_time),
            size: cx.size,
            scale: cx.scale,
        };
        let key = FrameKey::of(&req);
        if let Some(gpu) = cx.gpu.as_any().downcast_mut::<WgpuGpu>() {
            if let Some(frame) = self.render_gpu(gpu, &req, &key)? {
                return Ok(frame);
            }
        }
        self.render_cpu(&req, key)
    }
}

impl std::fmt::Debug for WebCefKind {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.debug_struct("WebCefKind")
            .field("host", &self.uses_host())
            .field("stats", &self.stats())
            .finish()
    }
}
