//! インスタンスと 1 フレームの描画（OpenFX の「The Plug-in Life Cycle」の後半）。
//! 1 回の `RenderFrame` を、BeginSequenceRender、Render、EndSequenceRender の 1 フレームの区間として呼ぶ。
//! クリップの深さはプラグインが対応するもののうち精度の高いものに全部そろえ、GetClipPreferences は呼ばない。

use std::sync::Arc;

use mm_types::{Params, RationalTime, Size};

use super::effect::{Clip, Effect};
use super::ffi::STAT_OK;
use super::names::*;
use super::pixels::{Depth, Pixels};
use super::plugin::Plugin;
use super::props::PropSet;

/// インスタンスを作るときの仮のプロジェクトの大きさ。最初の `RenderFrame` で頼まれた大きさに変える。
const INITIAL_SIZE: Size = Size {
    width: 1920,
    height: 1080,
};

/// 作ったインスタンス。捨てるときに `DestroyInstance` を呼ぶ。
pub struct Instance {
    plugin: Plugin,
    effect: Box<Effect>,
    depth: Depth,
    frame_rate: f64,
}

impl Instance {
    /// 文脈の記述子からインスタンスを作り、`CreateInstance` を呼ぶ。
    pub fn create(
        plugin: &Plugin,
        in_context: &Effect,
        context: &str,
        depth: Depth,
        frame_rate: f64,
    ) -> Result<Self, String> {
        let effect = Box::new(in_context.instantiate());
        let p = &effect.props;
        p.set_str(FX_CONTEXT, &[context]);
        p.set_int(PROP_IS_INTERACTIVE, &[0]);
        p.set_ptr(PROP_INSTANCE_DATA, std::ptr::null_mut());
        p.set_int(FX_SEQUENTIAL_RENDER, &[0]);
        p.set_double(FX_FRAME_RATE, &[frame_rate]);
        p.set_double(FX_EFFECT_DURATION, &[1.0]);
        let instance = Self {
            plugin: plugin.clone(),
            effect,
            depth,
            frame_rate,
        };
        instance.set_project(INITIAL_SIZE);
        instance
            .plugin
            .expect(ACTION_CREATE_INSTANCE, instance.effect.handle(), None)?;
        Ok(instance)
    }

    fn set_project(&self, size: Size) {
        let (w, h) = (f64::from(size.width), f64::from(size.height));
        let p = &self.effect.props;
        p.set_double(FX_PROJECT_SIZE, &[w, h]);
        p.set_double(FX_PROJECT_EXTENT, &[w, h]);
        p.set_double(FX_PROJECT_OFFSET, &[0.0, 0.0]);
        p.set_double(FX_PROJECT_PIXEL_ASPECT, &[1.0]);
    }

    fn configure_clip(&self, clip: &Clip, size: Size, frame: f64, connected: bool) {
        let p = &clip.props;
        for key in [FX_PIXEL_DEPTH, CLIP_UNMAPPED_DEPTH] {
            p.set_str(key, &[self.depth.ofx_name()]);
        }
        for key in [FX_COMPONENTS, CLIP_UNMAPPED_COMPONENTS] {
            p.set_str(key, &[COMPONENT_RGBA]);
        }
        for key in [FX_FRAME_RATE, FX_UNMAPPED_FRAME_RATE] {
            p.set_double(key, &[self.frame_rate]);
        }
        for key in [FX_FRAME_RANGE, FX_UNMAPPED_FRAME_RANGE] {
            p.set_double(key, &[frame, frame]);
        }
        p.set_str(FX_PREMULTIPLICATION, &[PREMULTIPLIED]);
        p.set_double(IMAGE_PIXEL_ASPECT, &[1.0]);
        p.set_double(
            FX_PROJECT_SIZE,
            &[f64::from(size.width), f64::from(size.height)],
        );
        p.set_int(CLIP_CONNECTED, &[i32::from(connected)]);
        p.set_str(CLIP_FIELD_ORDER, &[FIELD_NONE]);
        p.set_int(CLIP_CONTINUOUS_SAMPLES, &[0]);
    }

    /// クリップに画素を置く。出力は透明で用意し、入力は `source`、ほかの入力（マスクなど）は未接続の透明。
    fn attach_clips(&self, size: Size, frame: f64, source: Pixels) -> Result<Arc<Pixels>, String> {
        let source = Arc::new(source);
        let mut output = None;
        for name in self.effect.clip_names() {
            let clip = self.effect.clip(&name).ok_or("クリップが消えた")?;
            let pixels = match name.as_str() {
                CLIP_OUTPUT => output
                    .insert(Arc::new(Pixels::new(size, self.depth)?))
                    .clone(),
                CLIP_SOURCE => source.clone(),
                _ => Arc::new(Pixels::new(size, self.depth)?),
            };
            self.configure_clip(
                &clip,
                size,
                frame,
                matches!(name.as_str(), CLIP_OUTPUT | CLIP_SOURCE),
            );
            clip.attach(pixels);
        }
        output.ok_or_else(|| format!("{} に Output のクリップが無い", self.plugin.id))
    }

    fn apply_values(&self, values: &Params) -> Result<(), String> {
        for (name, value) in &values.0 {
            let param = self
                .effect
                .params
                .find(name)
                .ok_or_else(|| format!("{} に {name} のパラメータが無い", self.plugin.id))?;
            param.apply(value)?;
        }
        Ok(())
    }

    fn sequence_args(frame: f64) -> PropSet {
        let args = PropSet::default();
        args.set_double(FX_FRAME_RANGE, &[frame, frame]);
        args.set_double(FX_FRAME_STEP, &[1.0]);
        args.set_int(PROP_IS_INTERACTIVE, &[0]);
        args.set_double(FX_RENDER_SCALE, &[1.0, 1.0]);
        args.set_int(FX_SEQUENTIAL_RENDER_STATUS, &[0]);
        args.set_int(FX_INTERACTIVE_RENDER_STATUS, &[0]);
        args
    }

    fn render_args(frame: f64, size: Size) -> PropSet {
        let args = PropSet::default();
        args.set_double(PROP_TIME, &[frame]);
        args.set_str(FX_FIELD_TO_RENDER, &[FIELD_NONE]);
        let (w, h) = (
            i32::try_from(size.width).unwrap_or(i32::MAX),
            i32::try_from(size.height).unwrap_or(i32::MAX),
        );
        args.set_int(FX_RENDER_WINDOW, &[0, 0, w, h]);
        args.set_double(FX_RENDER_SCALE, &[1.0, 1.0]);
        args.set_int(FX_SEQUENTIAL_RENDER_STATUS, &[0]);
        args.set_int(FX_INTERACTIVE_RENDER_STATUS, &[0]);
        args.set_int(FX_RENDER_QUALITY_DRAFT, &[0]);
        args
    }

    /// クリップの画素の深さ。入力の画素はこの深さで作る。
    pub fn depth(&self) -> Depth {
        self.depth
    }

    /// 時刻 `time` の 1 フレームを `size` で描き、出力のクリップの画素を返す。
    /// `source` は入力のクリップの画素（大きさは `size`、深さは [`Instance::depth`]）。
    pub fn render(
        &self,
        time: RationalTime,
        size: Size,
        source: Pixels,
        values: &Params,
    ) -> Result<Arc<Pixels>, String> {
        let frame = time.to_f64() * self.frame_rate;
        self.set_project(size);
        let output = self.attach_clips(size, frame, source)?;
        self.apply_values(values)?;
        let handle = self.effect.handle();
        let seq = Self::sequence_args(frame);
        self.plugin
            .expect(ACTION_BEGIN_SEQUENCE_RENDER, handle, Some(&seq))?;
        let rendered = match self.plugin.call(
            ACTION_RENDER,
            handle,
            Some(&Self::render_args(frame, size)),
            None,
        ) {
            STAT_OK => Ok(()),
            st => Err(self.plugin.failure(ACTION_RENDER, st)),
        };
        self.plugin
            .expect(ACTION_END_SEQUENCE_RENDER, handle, Some(&seq))?;
        rendered?;
        Ok(output)
    }
}

impl Drop for Instance {
    fn drop(&mut self) {
        let _ = self
            .plugin
            .call(ACTION_DESTROY_INSTANCE, self.effect.handle(), None, None);
    }
}
