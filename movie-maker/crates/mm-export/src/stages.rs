//! 1 つのレンダラーの書き出しの流れ。描画の段（`Engine` を持つ）と符号化の段（`Engine` を持たない別のスレッド）に
//! 分け、上限のある受け渡しのキューでつなぐ。FFmpeg のフィルターグラフとエンコーダーのスレッド、OBS の描画スレッドと
//! 符号化スレッドの間のキュー、GStreamer の `queue` 要素と同じ形。
//!
//! - 描画の段はリードバックを `READBACK_AHEAD` 枚先行させ、フレームを 1 つ渡すごとに `between` に `Engine` を貸す。
//!   配信の描画のスレッドは、そこで `GET /frame` と静止画のフレームを挟む。
//! - 受け渡しのキューが満ちたら描画の段が待つ（背圧）。溜まるのはキューの `QUEUE_FRAMES` 枚と、符号化中の 1 枚まで。
//! - 符号化の失敗は送信の失敗で描画の段に、描画の失敗と取り消しは終わりの印なしに閉じた受け渡しのキューで符号化の段に
//!   伝わり、どちらの段も待ち続けない。

use std::collections::VecDeque;
use std::sync::mpsc::{sync_channel, Receiver, SyncSender};
use std::time::Instant;

use mm_decode_api::{EncodeSettings, Encoder};
use mm_doc::Project;
use mm_node_api::Quality;
use mm_render_api::{ExportFrame, PendingFrame, RenderRequest};
use mm_runtime::Engine;
use mm_types::{MmError, RationalTime, Result};

use crate::passes::{Plan, Progress};
use crate::CANCELLED_MARK;

/// 描画の段で先行させるリードバックの数。描画から符号化までにこの分の遅れが入る。
const READBACK_AHEAD: usize = 3;
/// 受け渡しのキューの長さ（フレーム）。1080p で NV12 は 1 枚 3 MB、RGBA の f32 は 33 MB。
const QUEUE_FRAMES: usize = 4;
/// 符号化の段が先に止まったときの、描画の段の送信のエラー。符号化の段のエラーを返すので外へは出ない。
const ENCODER_STOPPED: &str = "符号化の段が止まった";
/// 描画の段が終わりの印を送らずに止まったときの、符号化の段のエラー。描画の段のエラーを返すので外へは出ない。
const DRAW_STOPPED: &str = "描画の段が止まった";

/// フレームの間に `Engine` を貸す先。
pub(crate) type Between<'a> = &'a mut dyn FnMut(&mut Engine);

/// 描画の段から符号化の段への受け渡し。
enum Handoff {
    Frame(ExportFrame),
    /// 区間の全部を渡し終えた。これが来ずに受け渡しのキューが閉じたら、描画の段が止まった。
    End,
}

/// 1 つのレンダラーで区間を描き、別のスレッドの符号化の段でエンコーダーに押す。押したフレームの数を返す。
pub(crate) fn export_staged(
    engine: &mut Engine,
    draw: &Draw<'_, '_>,
    encoder: &mut dyn Encoder,
    progress: &mut Progress<'_>,
    between: Between<'_>,
) -> Result<usize> {
    let nv12 = accepts_nv12(encoder)?;
    let (queue, received) = sync_channel(QUEUE_FRAMES);
    // 符号化の段のスパン（`export.encode`）を呼んだ側の購読者と親のスパンの下に集める。
    let dispatch = tracing::dispatcher::get_default(Clone::clone);
    let parent = tracing::Span::current();
    std::thread::scope(|scope| {
        let encoding = std::thread::Builder::new()
            .name("export-encode".into())
            .spawn_scoped(scope, move || {
                tracing::dispatcher::with_default(&dispatch, || {
                    parent.in_scope(|| encode_stage(encoder, &received))
                })
            })
            .map_err(MmError::Io)?;
        let drawn = draw.run(engine, nv12, &queue, progress, between);
        drop(queue);
        let encoded = encoding.join().unwrap_or_else(|_| {
            Err(MmError::Internal(
                "符号化の段のスレッドがパニックした".into(),
            ))
        });
        merge(drawn, encoded)
    })
}

/// エンコーダーが NV12 を受けるか。空のスライスで問い合わせる（`Encoder::push_nv12_frames`）。
fn accepts_nv12(encoder: &mut dyn Encoder) -> Result<bool> {
    match encoder.push_nv12_frames(&[]) {
        Ok(()) => Ok(true),
        Err(MmError::Unsupported(_)) => Ok(false),
        Err(e) => Err(e),
    }
}

/// 両方の段の結果をまとめる。先に止まった段のエラーを返し、もう一方の「相手が止まった」は捨てる。
fn merge(drawn: Result<usize>, encoded: Result<usize>) -> Result<usize> {
    let stopped = |e: &MmError| matches!(e, MmError::Internal(m) if m == ENCODER_STOPPED);
    match (drawn, encoded) {
        (Err(d), _) if !stopped(&d) => Err(d),
        (_, Err(e)) => Err(e),
        (Err(d), Ok(_)) => Err(d),
        (Ok(d), Ok(e)) if d == e => Ok(d),
        (Ok(d), Ok(e)) => Err(MmError::Internal(format!(
            "描画の段が渡した {d} フレームと、符号化の段が押した {e} フレームが違う"
        ))),
    }
}

/// 符号化の段。受け渡しのキューから受け取った順にエンコーダーに 1 枚ずつ押す。押したフレームの数を返す。
fn encode_stage(encoder: &mut dyn Encoder, received: &Receiver<Handoff>) -> Result<usize> {
    let mut pushed = 0;
    loop {
        match received.recv() {
            Ok(Handoff::Frame(ExportFrame::Nv12(frame))) => {
                encoder.push_nv12_frames(std::slice::from_ref(&frame))?;
            }
            Ok(Handoff::Frame(ExportFrame::Rgba(image))) => {
                encoder.push_frames(std::slice::from_ref(&image))?;
            }
            Ok(Handoff::End) => return Ok(pushed),
            Err(_) => return Err(MmError::Internal(DRAW_STOPPED.into())),
        }
        pushed += 1;
    }
}

/// 描画の段が描くもの。
pub(crate) struct Draw<'a, 'p> {
    pub project: &'a Project,
    pub plan: &'a Plan<'p>,
    pub settings: &'a EncodeSettings,
}

impl Draw<'_, '_> {
    /// 区間を描いて符号化の段へ渡し、終わりの印を送る。渡したフレームの数を返す。
    fn run(
        &self,
        engine: &mut Engine,
        nv12: bool,
        queue: &SyncSender<Handoff>,
        progress: &mut Progress<'_>,
        between: Between<'_>,
    ) -> Result<usize> {
        let registry = engine.registry().clone();
        let mut in_flight: VecDeque<Box<dyn PendingFrame>> =
            VecDeque::with_capacity(READBACK_AHEAD + 1);
        let mut sent = 0;
        for index in self.plan.range.frame_indices(self.settings.fps) {
            self.check_cancel()?;
            let started = Instant::now();
            let req = RenderRequest {
                project: self.project,
                composition: self.plan.composition,
                time: RationalTime::from_frames(index, self.settings.fps)?,
                size: self.settings.size,
                quality: Quality::Full,
                nodes: &registry,
            };
            in_flight.push_back(engine.renderer().export_frame(&req, nv12)?);
            if in_flight.len() > READBACK_AHEAD {
                hand_off(engine, &mut in_flight, queue, progress, started)?;
                sent += 1;
            }
            between(engine);
        }
        while !in_flight.is_empty() {
            self.check_cancel()?;
            hand_off(engine, &mut in_flight, queue, progress, Instant::now())?;
            sent += 1;
        }
        queue
            .send(Handoff::End)
            .map_err(|_| MmError::Internal(ENCODER_STOPPED.into()))?;
        Ok(sent)
    }

    fn check_cancel(&self) -> Result<()> {
        if self.plan.control.is_cancelled() {
            return Err(MmError::Internal(CANCELLED_MARK.into()));
        }
        Ok(())
    }
}

/// 一番古いフレームのリードバックを受け取り、符号化の段へ渡す。`started` から受け渡しのキューに入れる前までを
/// 1 フレームの描画の時間（`export.render`）にし、キューが満ちて待った時間は `export.queue_wait` に分ける。
fn hand_off(
    engine: &mut Engine,
    in_flight: &mut VecDeque<Box<dyn PendingFrame>>,
    queue: &SyncSender<Handoff>,
    progress: &mut Progress<'_>,
    started: Instant,
) -> Result<()> {
    let Some(pending) = in_flight.pop_front() else {
        return Ok(());
    };
    let frame = {
        let _span = tracing::info_span!("export.readback").entered();
        pending.finish(engine.renderer().gpu())?
    };
    progress.frame(started.elapsed());
    let _span = tracing::info_span!("export.queue_wait").entered();
    queue
        .send(Handoff::Frame(frame))
        .map_err(|_| MmError::Internal(ENCODER_STOPPED.into()))
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn 先に止まった段のエラーを返す() {
        let encoder_failed = merge(
            Err(MmError::Internal(ENCODER_STOPPED.into())),
            Err(MmError::Corrupt("push".into())),
        );
        assert!(matches!(encoder_failed, Err(MmError::Corrupt(m)) if m == "push"));
        let draw_failed = merge(
            Err(MmError::NotFound("素材".into())),
            Err(MmError::Internal(DRAW_STOPPED.into())),
        );
        assert!(matches!(draw_failed, Err(MmError::NotFound(m)) if m == "素材"));
        let last_frames = merge(Ok(10), Err(MmError::Corrupt("finish".into())));
        assert!(matches!(last_frames, Err(MmError::Corrupt(_))));
    }

    #[test]
    fn 渡した数と押した数が違えばエラー() {
        assert_eq!(merge(Ok(5), Ok(5)).unwrap(), 5);
        assert!(merge(Ok(5), Ok(4)).is_err());
    }
}
