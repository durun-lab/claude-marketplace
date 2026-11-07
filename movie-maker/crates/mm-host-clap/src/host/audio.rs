//! 音のスレッド。要求のイベントを待ち、共有メモリのブロックをプラグインで処理して完了のイベントを立てる。
//!
//! 時刻（`steady_time`）とトランスポートは渡さない。再生と書き出しで始める位置が違っても、同じ入力と
//! 同じパラメータなら同じサンプルになるよう、プラグインに位置を知らせない。

use std::sync::mpsc;

use clack_host::events::event_types::ParamValueEvent;
use clack_host::prelude::*;

use mm_host_clap::shm::{Block, Command, Event, Mapping, Wake, CHANNELS, MAX_EVENTS, MAX_FRAMES};

use super::plugin::MmHost;

/// アプリとつなぐ共有メモリとイベント。
pub struct Link {
    pub mapping: Mapping,
    pub request: Event,
    pub done: Event,
}

/// 1 回の処理に使い回す CLAP の入れ物。
struct Buffers {
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
                serve(started, link);
            }
            Err(e) => {
                let _ = tx.send(Err(format!("処理を始められない: {e}")));
            }
        })
        .map_err(|e| format!("音のスレッドを起こせない: {e}"))?;
    rx.recv()
        .map_err(|_| "音のスレッドが返事をしない".to_string())?
}

/// 要求を待って処理し続ける。プロセスが終わるまで戻らない。
fn serve(mut processor: StartedPluginAudioProcessor<MmHost>, mut link: Link) {
    let mut buffers = Buffers {
        ports_in: AudioPorts::with_capacity(CHANNELS, 1),
        ports_out: AudioPorts::with_capacity(CHANNELS, 1),
        events: EventBuffer::with_capacity(MAX_EVENTS),
    };
    while link.request.wait(None, None) == Wake::Signaled {
        let block = link.mapping.block();
        let result = match block.command() {
            Some(Command::Process) => process(&mut processor, block, &mut buffers),
            Some(Command::Reset) => {
                processor.reset();
                Ok(())
            }
            None => Err("知らない要求".to_string()),
        };
        match result {
            Ok(()) => block.set_ok(),
            Err(e) => block.set_error(&e),
        }
        block.done_seq = block.seq;
        if link.done.set().is_err() {
            break;
        }
    }
}

/// 共有メモリの `frames` フレームとパラメータのイベントをプラグインで処理し、出力を書く。
fn process(
    processor: &mut StartedPluginAudioProcessor<MmHost>,
    block: &mut Block,
    buffers: &mut Buffers,
) -> Result<(), String> {
    let n = block.frames as usize;
    let count = block.event_count as usize;
    if n == 0 || n > MAX_FRAMES || count > MAX_EVENTS {
        return Err(format!("フレーム数 {n} かイベントの数 {count} が範囲の外"));
    }
    buffers.events.clear();
    for e in &block.events[..count] {
        let id = ClapId::from_raw(e.param_id).ok_or("パラメータの id が不正")?;
        buffers.events.push(&ParamValueEvent::new(
            e.offset,
            id,
            Pckn::match_all(),
            e.value,
        ));
    }
    let Block { input, output, .. } = block;
    let [in_l, in_r] = input;
    let [out_l, out_r] = output;
    let inputs = buffers.ports_in.with_input_buffers([AudioPortBuffer {
        latency: 0,
        channels: AudioPortBufferType::f32_input_only(
            [&mut in_l[..n], &mut in_r[..n]]
                .into_iter()
                .map(InputChannel::variable),
        ),
    }]);
    let mut outputs = buffers.ports_out.with_output_buffers([AudioPortBuffer {
        latency: 0,
        channels: AudioPortBufferType::f32_output_only(
            [&mut out_l[..n], &mut out_r[..n]].into_iter(),
        ),
    }]);
    processor
        .process(
            &inputs,
            &mut outputs,
            &InputEvents::from_buffer(&buffers.events),
            &mut OutputEvents::void(),
            None,
            None,
        )
        .map(|_| ())
        .map_err(|e| format!("プラグインの処理が失敗した: {e}"))
}
