//! 音のスレッド。要求のイベントを待ち、共有メモリのブロックをプラグインで処理して完了のイベントを立てる。
//!
//! 時刻（`steady_time`）とトランスポートは渡さない。再生と書き出しで始める位置が違っても、同じ入力と
//! 同じパラメータなら同じサンプルになるよう、プラグインに位置を知らせない。

use std::sync::mpsc;

use clack_host::events::event_types::ParamValueEvent;
use clack_host::prelude::*;

use mm_host_clap::shm::{Block, CHANNELS, MAX_EVENTS, MAX_FRAMES};
use mm_host_pipe::audio::serve::{serve_blocks, Link, Plugin};

use super::plugin::MmHost;

/// 処理中のプラグインと、1 回の処理に使い回す CLAP の入れ物。
struct Clap {
    processor: StartedPluginAudioProcessor<MmHost>,
    ports_in: AudioPorts,
    ports_out: AudioPorts,
    events: EventBuffer,
}

/// 記述を共有メモリに書き、音のスレッドを起こして処理を始める。始められたかを待って返す。
pub fn start(
    processor: StoppedPluginAudioProcessor<MmHost>,
    mut link: Link,
    info: &[u8],
) -> Result<(), String> {
    link.mapping
        .block()
        .set_info(info)
        .map_err(|e| e.to_string())?;
    let (tx, rx) = mpsc::channel();
    std::thread::Builder::new()
        .name("mm-host-clap-audio".into())
        .spawn(move || match processor.start_processing() {
            Ok(started) => {
                // 受け取る側は Prepare の中で待っているので、送れないのは主スレッドが終わったときだけ。
                let _ = tx.send(Ok(()));
                let mut clap = Clap {
                    processor: started,
                    ports_in: AudioPorts::with_capacity(CHANNELS, 1),
                    ports_out: AudioPorts::with_capacity(CHANNELS, 1),
                    events: EventBuffer::with_capacity(MAX_EVENTS),
                };
                serve_blocks(&mut clap, &mut link);
            }
            Err(e) => {
                let _ = tx.send(Err(format!("処理を始められない: {e}")));
            }
        })
        .map_err(|e| format!("音のスレッドを起こせない: {e}"))?;
    rx.recv()
        .map_err(|_| "音のスレッドが返事をしない".to_string())?
}

impl Plugin for Clap {
    fn reset(&mut self) -> Result<(), String> {
        self.processor.reset();
        Ok(())
    }

    /// 共有メモリの `frames` フレームとパラメータのイベントをプラグインで処理し、出力を書く。
    fn process(&mut self, block: &mut Block) -> Result<(), String> {
        let n = block.frames as usize;
        let count = block.event_count as usize;
        if n == 0 || n > MAX_FRAMES || count > MAX_EVENTS {
            return Err(format!("フレーム数 {n} かイベントの数 {count} が範囲の外"));
        }
        self.events.clear();
        for e in &block.events[..count] {
            let id = ClapId::from_raw(e.param_id).ok_or("パラメータの id が不正")?;
            self.events.push(&ParamValueEvent::new(
                e.offset,
                id,
                Pckn::match_all(),
                e.value,
            ));
        }
        let Block { input, output, .. } = block;
        let [in_l, in_r] = input;
        let [out_l, out_r] = output;
        let inputs = self.ports_in.with_input_buffers([AudioPortBuffer {
            latency: 0,
            channels: AudioPortBufferType::f32_input_only(
                [&mut in_l[..n], &mut in_r[..n]]
                    .into_iter()
                    .map(InputChannel::variable),
            ),
        }]);
        let mut outputs = self.ports_out.with_output_buffers([AudioPortBuffer {
            latency: 0,
            channels: AudioPortBufferType::f32_output_only(
                [&mut out_l[..n], &mut out_r[..n]].into_iter(),
            ),
        }]);
        self.processor
            .process(
                &inputs,
                &mut outputs,
                &InputEvents::from_buffer(&self.events),
                &mut OutputEvents::void(),
                None,
                None,
            )
            .map(|_| ())
            .map_err(|e| format!("プラグインの処理が失敗した: {e}"))
    }
}
