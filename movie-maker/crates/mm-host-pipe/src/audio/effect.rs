//! 1 つのプラグインのインスタンス（アプリ側）。ホストのプロセスを 1 つ持ち、音のブロックを共有メモリで往復させる。
//!
//! パラメータは、プラグインが今持っている値（`sent`）との差だけをイベントにして、その位置のサンプルから効かせる。
//! ホストを起こし直したら `sent` を空にし、次のブロックの先頭で全部のパラメータを送り直す。
//! ホストが落ちるか期限を過ぎたら、その呼び出しは失敗を返してホストを捨て、次の呼び出しで起こし直す。

use std::collections::BTreeMap;
use std::marker::PhantomData;
use std::path::PathBuf;
use std::time::Duration;

use mm_audio_api::AudioBlock;
use mm_host_api::{HostProcess, HostRequest, HostResponse, HostSpec};
use mm_types::{MmError, Result};

use super::block::{
    object_names, unique_link, Block, Command, Event, Mapping, ParamEvent, Process, Wake, CHANNELS,
    MAX_EVENTS, MAX_FRAMES,
};
use super::desc::{PluginDesc, PrepareParams};
use super::PluginStandard;
use crate::PipeHost;

/// 区間の中のある位置のパラメータの値。`offset` はブロックの先頭からのフレームの位置。
/// 載っていないパラメータはプラグインの既定値にする。
#[derive(Debug, Clone, PartialEq, Default)]
pub struct ParamPoint {
    pub offset: usize,
    /// パラメータの `id` と値（[`super::ParamDesc`] と同じ意味）。
    pub values: BTreeMap<u32, f64>,
}

/// 動いているホストとの接続。
struct Conn {
    host: PipeHost,
    mapping: Mapping,
    request: Event,
    done: Event,
    process: Process,
    seq: u64,
}

/// 1 つのプラグインのインスタンス。`S` は規格（CLAP、VST3）。
pub struct AudioEffect<S: PluginStandard> {
    spec: HostSpec,
    path: PathBuf,
    plugin: Option<String>,
    sample_rate: u32,
    conn: Option<Conn>,
    desc: PluginDesc,
    /// プラグインが今持っているパラメータの値。
    sent: BTreeMap<u32, f64>,
    standard: PhantomData<S>,
}

impl<S: PluginStandard> AudioEffect<S> {
    /// ホストを起こし、`path` の `plugin`（`None` なら最初のもの）を読み込んで処理を始める。
    pub fn start(
        spec: HostSpec,
        path: impl Into<PathBuf>,
        plugin: Option<String>,
        sample_rate: u32,
    ) -> Result<Self> {
        let mut effect = Self {
            spec,
            path: path.into(),
            plugin,
            sample_rate,
            conn: None,
            desc: PluginDesc::default(),
            sent: BTreeMap::new(),
            standard: PhantomData,
        };
        effect.connect()?;
        Ok(effect)
    }

    /// 読み込んだプラグインの記述（パラメータの一覧を含む）。
    pub fn desc(&self) -> &PluginDesc {
        &self.desc
    }

    /// サンプルレート。
    pub fn sample_rate(&self) -> u32 {
        self.sample_rate
    }

    /// ホストのプロセス ID。動いていなければ `None`。
    pub fn pid(&self) -> Option<u32> {
        self.conn.as_ref().and_then(|c| c.host.pid())
    }

    /// ホストが動いているか。
    pub fn is_alive(&self) -> bool {
        self.conn.as_ref().is_some_and(|c| c.host.is_alive())
    }

    /// ホストを止める（クラッシュの試験と、使わなくなったときの片付け）。次の処理で起こし直す。
    pub fn kill(&mut self) -> Result<()> {
        match self.conn.take() {
            Some(mut c) => c.host.kill(),
            None => Ok(()),
        }
    }

    fn connect(&mut self) -> Result<()> {
        let link = unique_link(S::LINK_PREFIX);
        let [mem, req, done] = object_names(&link);
        let mut mapping = Mapping::create(&mem)?;
        let (request, done) = (Event::create(&req)?, Event::create(&done)?);
        let mut host = PipeHost::start(S::PIPE, self.spec.clone())?;
        let params = PrepareParams {
            path: self.path.clone(),
            plugin: self.plugin.clone(),
            sample_rate: self.sample_rate,
            link,
        };
        let req = HostRequest::Prepare {
            params: serde_json::to_value(params)?,
        };
        match host.request(&req, self.spec.default_timeout)? {
            HostResponse::Prepared => {}
            HostResponse::Error { message } => {
                return Err(MmError::Internal(format!(
                    "{} の Prepare: {message}",
                    S::NAME
                )))
            }
            other => {
                return Err(MmError::Internal(format!(
                    "{} の Prepare に答えが合わない: {other:?}",
                    S::NAME
                )))
            }
        }
        self.desc = serde_json::from_slice(mapping.block().info())?;
        let pid = host
            .pid()
            .ok_or_else(|| MmError::HostCrashed("ホストのプロセス ID が無い".into()))?;
        self.conn = Some(Conn {
            process: Process::open(pid)?,
            host,
            mapping,
            request,
            done,
            seq: 0,
        });
        self.sent.clear();
        Ok(())
    }

    /// 動いている接続。落ちていれば起こし直す。
    fn live(&mut self) -> Result<&mut Conn> {
        if !self.is_alive() {
            self.kill()?;
            self.connect()?;
        }
        self.conn
            .as_mut()
            .ok_or_else(|| MmError::HostCrashed("ホストが動いていない".into()))
    }

    /// プラグインの内部の状態（遅延の中身など）を消す。シークしたときに呼ぶ。パラメータの値は残る。
    pub fn reset(&mut self) -> Result<()> {
        let timeout = self.spec.default_timeout;
        let conn = self.live()?;
        conn.mapping.block().set_command(Command::Reset);
        let result = roundtrip::<S>(conn, timeout);
        self.drop_on_error(result)
    }

    /// `block`（ステレオ、インターリーブ）をその場で処理する。`points` は `offset` の昇順。
    /// 長いブロックは共有メモリに入る長さに分けて渡す。分け方に依らず同じサンプルになる。
    pub fn process(&mut self, block: &mut AudioBlock, points: &[ParamPoint]) -> Result<()> {
        if block.sample_rate != self.sample_rate || block.channels as usize != CHANNELS {
            return Err(MmError::InvalidArgument(format!(
                "{} の効果は {} Hz のステレオだけ（{} Hz、{} ch）",
                S::NAME,
                self.sample_rate,
                block.sample_rate,
                block.channels
            )));
        }
        let frames = block.frames();
        self.live()?;
        let events = self.param_events(points, frames);
        let result = self.process_chunks(block, &events);
        self.drop_on_error(result)
    }

    /// 失敗したらホストを捨てる。プラグインの状態とパラメータの値が分からなくなるため、次の呼び出しで起こし直す。
    fn drop_on_error(&mut self, result: Result<()>) -> Result<()> {
        if result.is_err() {
            // 返すのは元の失敗で、止められなかったことは PipeHost の Drop の後片付けに任せる。
            let _ = self.kill();
        }
        result
    }

    /// `points` の値を、プラグインが持つ値との差のイベント（ブロックの先頭からの位置）にする。
    fn param_events(&mut self, points: &[ParamPoint], frames: usize) -> Vec<(usize, ParamEvent)> {
        let mut out = Vec::new();
        for point in points.iter().filter(|p| p.offset < frames) {
            for p in self.desc.params.iter().filter(|p| !p.read_only) {
                let value = point
                    .values
                    .get(&p.id)
                    .copied()
                    .unwrap_or(p.default)
                    .clamp(p.min.min(p.max), p.max.max(p.min));
                if self.sent.get(&p.id) == Some(&value) {
                    continue;
                }
                self.sent.insert(p.id, value);
                let event = ParamEvent {
                    offset: 0,
                    param_id: p.id,
                    value,
                };
                out.push((point.offset, event));
            }
        }
        out
    }

    fn process_chunks(
        &mut self,
        block: &mut AudioBlock,
        events: &[(usize, ParamEvent)],
    ) -> Result<()> {
        let timeout = self.spec.default_timeout;
        let frames = block.frames();
        let conn = self.live()?;
        let mut start = 0;
        let mut next_event = 0;
        while start < frames {
            let end = chunk_end(start, frames, &events[next_event..])?;
            let taken = events[next_event..]
                .iter()
                .take_while(|(offset, _)| *offset < end)
                .count();
            let chunk = &events[next_event..next_event + taken];
            write_chunk(conn.mapping.block(), &block.samples, start, end, chunk);
            roundtrip::<S>(conn, timeout)?;
            read_chunk(conn.mapping.block(), &mut block.samples, start, end);
            next_event += taken;
            start = end;
        }
        Ok(())
    }
}

/// `start` から始める 1 回分の終わり。共有メモリのフレーム数とイベントの数に収める。
fn chunk_end(start: usize, frames: usize, events: &[(usize, ParamEvent)]) -> Result<usize> {
    let end = frames.min(start + MAX_FRAMES);
    match events.get(MAX_EVENTS) {
        Some((offset, _)) if *offset < end => {
            if *offset <= start {
                return Err(MmError::InvalidArgument(format!(
                    "1 つの位置のパラメータのイベントが {MAX_EVENTS} を超える"
                )));
            }
            Ok(*offset)
        }
        _ => Ok(end),
    }
}

/// `samples` の `[start, end)` のフレームをチャンネルごとに分けて書き、イベントの位置を 1 回分の先頭からにする。
fn write_chunk(
    block: &mut Block,
    samples: &[f32],
    start: usize,
    end: usize,
    events: &[(usize, ParamEvent)],
) {
    for (i, frame) in samples[start * CHANNELS..end * CHANNELS]
        .chunks_exact(CHANNELS)
        .enumerate()
    {
        for (c, s) in frame.iter().enumerate() {
            block.input[c][i] = *s;
        }
    }
    for (slot, (offset, e)) in block.events.iter_mut().zip(events) {
        *slot = ParamEvent {
            offset: (offset - start) as u32,
            ..*e
        };
    }
    block.event_count = events.len() as u32;
    block.frames = (end - start) as u32;
    block.set_command(Command::Process);
}

/// 出力を `samples` の `[start, end)` にインターリーブで書き戻す。
fn read_chunk(block: &mut Block, samples: &mut [f32], start: usize, end: usize) {
    for (i, frame) in samples[start * CHANNELS..end * CHANNELS]
        .chunks_exact_mut(CHANNELS)
        .enumerate()
    {
        for (c, s) in frame.iter_mut().enumerate() {
            *s = block.output[c][i];
        }
    }
}

/// 要求を送り、完了を待つ。ホストが終わったら `HostCrashed`、期限を過ぎたら `Timeout`。
fn roundtrip<S: PluginStandard>(conn: &mut Conn, timeout: Duration) -> Result<()> {
    conn.seq += 1;
    conn.mapping.block().seq = conn.seq;
    conn.request.set()?;
    match conn.done.wait(Some(&conn.process), Some(timeout)) {
        Wake::Signaled => {}
        Wake::Exited => {
            return Err(MmError::HostCrashed(format!(
                "{} のホストが処理の途中で終わった",
                S::NAME
            )))
        }
        Wake::TimedOut => {
            return Err(MmError::Timeout {
                what: format!("{}.process", S::PIPE.name),
                limit: timeout,
            })
        }
    }
    let block = conn.mapping.block();
    if block.done_seq != conn.seq {
        return Err(MmError::Internal(format!(
            "{} のホストの完了の番号が合わない（{} を待って {}）",
            S::NAME,
            conn.seq,
            block.done_seq
        )));
    }
    match block.error() {
        Some(e) => Err(MmError::Internal(format!("{} のプラグイン: {e}", S::NAME))),
        None => Ok(()),
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn event(offset: usize) -> (usize, ParamEvent) {
        (offset, ParamEvent::default())
    }

    #[test]
    fn 長いブロックは共有メモリのフレーム数で分ける() {
        assert_eq!(chunk_end(0, 10_000, &[]).unwrap(), MAX_FRAMES);
        assert_eq!(chunk_end(MAX_FRAMES * 2, 10_000, &[]).unwrap(), 10_000);
    }

    #[test]
    fn イベントが多ければその手前で分け_同じ位置に多すぎれば失敗する() {
        let spread: Vec<_> = (0..=MAX_EVENTS).map(event).collect();
        assert_eq!(chunk_end(0, 10_000, &spread).unwrap(), MAX_EVENTS);
        let same: Vec<_> = (0..=MAX_EVENTS).map(|_| event(0)).collect();
        assert!(chunk_end(0, 10_000, &same).is_err());
    }
}
