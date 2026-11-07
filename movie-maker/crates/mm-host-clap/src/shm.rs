//! アプリとホストの間で音のブロックを渡す共有メモリと、2 つのイベント（要求、完了）。
//!
//! アプリが名前つきで作り、ホストが名前で開く。1 回の往復は、アプリが入力とパラメータのイベントを書いて
//! 要求のイベントを立て、ホストが処理して出力を書き完了のイベントを立てる。番を持たない側は触らない。
//! イベントの待ちと立てるのが両方のプロセスのメモリの順序をそろえる。
//! 名前つきの区画とイベントは共通の実装（`mm_host_pipe::shm`）で、ここは区画の中の並び（[`Block`]）を持つ。

use mm_types::{MmError, Result};

/// 1 回に渡せるフレーム数。長いブロックはアプリが分けて渡す。
pub const MAX_FRAMES: usize = 4096;
/// チャンネル数（ステレオ）。
pub const CHANNELS: usize = 2;
/// 1 回に渡せるパラメータのイベントの数。
pub const MAX_EVENTS: usize = 4096;
/// ホストが返すエラーの文の上限（バイト）。
const ERROR_BYTES: usize = 512;
/// ホストが `Prepare` の後に書くプラグインの記述（JSON）の上限（バイト）。
const INFO_BYTES: usize = 256 * 1024;
/// 共有メモリの先頭の印。
const MAGIC: u32 = u32::from_le_bytes(*b"MMCL");
/// 共有メモリの並びの版。並びを変えたら上げる。
const LAYOUT_VERSION: u32 = 1;

/// 要求の種類。
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
#[repr(u32)]
pub enum Command {
    /// `frames` フレームを処理する。
    Process = 0,
    /// プラグインの内部の状態（遅延の中身など）を消す（CLAP の `reset`）。
    Reset = 1,
}

impl Command {
    /// 共有メモリの値から。
    pub fn from_raw(raw: u32) -> Option<Self> {
        match raw {
            0 => Some(Self::Process),
            1 => Some(Self::Reset),
            _ => None,
        }
    }
}

/// パラメータの値の変化 1 つ。`offset` はブロックの先頭からのフレームの位置。
#[derive(Debug, Clone, Copy, PartialEq, Default)]
#[repr(C)]
pub struct ParamEvent {
    pub offset: u32,
    pub param_id: u32,
    pub value: f64,
}

/// 共有メモリの中身。音は CLAP と同じくチャンネルごとに分けて（planar）持つ。
#[repr(C)]
pub struct Block {
    magic: u32,
    version: u32,
    command: u32,
    /// 今回のフレーム数。
    pub frames: u32,
    /// `events` の有効な数。
    pub event_count: u32,
    /// 0 なら成功。
    status: u32,
    /// 要求の番号。ホストは処理し終えた番号を `done_seq` に書く。
    pub seq: u64,
    pub done_seq: u64,
    error_len: u32,
    info_len: u32,
    error: [u8; ERROR_BYTES],
    pub events: [ParamEvent; MAX_EVENTS],
    pub input: [[f32; MAX_FRAMES]; CHANNELS],
    pub output: [[f32; MAX_FRAMES]; CHANNELS],
    info: [u8; INFO_BYTES],
}

impl Block {
    /// 作った側が先頭の印と版を書く。
    fn init(&mut self) {
        self.magic = MAGIC;
        self.version = LAYOUT_VERSION;
    }

    /// 開いた側が並びの版を確かめる。
    fn check(&self) -> Result<()> {
        if self.magic != MAGIC || self.version != LAYOUT_VERSION {
            return Err(MmError::Internal(format!(
                "共有メモリの並びが違う（版 {}、このホストは {LAYOUT_VERSION}）",
                self.version
            )));
        }
        Ok(())
    }

    /// 要求の種類。
    pub fn command(&self) -> Option<Command> {
        Command::from_raw(self.command)
    }

    /// 要求の種類を書く。
    pub fn set_command(&mut self, command: Command) {
        self.command = command as u32;
    }

    /// 成功を書く。
    pub fn set_ok(&mut self) {
        self.status = 0;
        self.error_len = 0;
    }

    /// 失敗と理由を書く。長い文は切る。
    pub fn set_error(&mut self, message: &str) {
        let bytes = truncate_utf8(message, ERROR_BYTES);
        self.error[..bytes.len()].copy_from_slice(bytes);
        self.error_len = bytes.len() as u32;
        self.status = 1;
    }

    /// 失敗なら理由。
    pub fn error(&self) -> Option<String> {
        (self.status != 0).then(|| {
            let n = (self.error_len as usize).min(ERROR_BYTES);
            String::from_utf8_lossy(&self.error[..n]).into_owned()
        })
    }

    /// プラグインの記述を書く。入りきらなければ `InvalidArgument`。
    pub fn set_info(&mut self, json: &[u8]) -> Result<()> {
        if json.len() > INFO_BYTES {
            return Err(MmError::InvalidArgument(format!(
                "プラグインの記述が {} バイトで、共有メモリの {INFO_BYTES} バイトに入らない",
                json.len()
            )));
        }
        self.info[..json.len()].copy_from_slice(json);
        self.info_len = json.len() as u32;
        Ok(())
    }

    /// ホストが書いたプラグインの記述。
    pub fn info(&self) -> &[u8] {
        &self.info[..(self.info_len as usize).min(INFO_BYTES)]
    }
}

/// UTF-8 の文字の途中で切らずに `limit` バイト以下にする。
fn truncate_utf8(s: &str, limit: usize) -> &[u8] {
    let mut end = s.len().min(limit);
    while !s.is_char_boundary(end) {
        end -= 1;
    }
    &s.as_bytes()[..end]
}

pub use mm_host_pipe::shm::{object_names, Event, Process, Wake};

/// このプロセスで重ならない元の名前（`Local\mm-clap-<pid>-<番号>`）。
pub fn unique_link() -> String {
    mm_host_pipe::shm::unique_link("mm-clap")
}

/// 音のブロックの共有メモリ。名前つきの区画（`mm_host_pipe::shm::Mapping`）を [`Block`] の大きさで写し、先頭を使う。
pub struct Mapping(mm_host_pipe::shm::Mapping);

impl Mapping {
    /// 名前つきで作る（アプリ）。中身は 0 で埋まっている。
    pub fn create(name: &str) -> Result<Self> {
        let mut mapping = Self(mm_host_pipe::shm::Mapping::create(
            name,
            std::mem::size_of::<Block>(),
        )?);
        mapping.block().init();
        Ok(mapping)
    }

    /// 名前で開く（ホスト）。
    pub fn open(name: &str) -> Result<Self> {
        let mut mapping = Self(mm_host_pipe::shm::Mapping::open(
            name,
            std::mem::size_of::<Block>(),
        )?);
        mapping.block().check()?;
        Ok(mapping)
    }

    /// 中身。番を持っている間だけ使う（相手は自分の番の間しか書かない）。
    pub fn block(&mut self) -> &mut Block {
        // SAFETY: Block は repr(C) で整数、浮動小数、その配列だけを持ち、どのビット列も正しい値。
        // 大きさは Block で写し、並び（8 バイト）は写しの先頭の 4 KiB 境界が満たす。
        unsafe { self.0.get_mut::<Block>() }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn エラーの文は文字の途中で切らない() {
        let s = "あ".repeat(ERROR_BYTES);
        let cut = truncate_utf8(&s, ERROR_BYTES);
        assert!(cut.len() <= ERROR_BYTES);
        assert!(std::str::from_utf8(cut).is_ok());
    }

    #[test]
    fn 元の名前は_mm_clap_で始まり重ならない() {
        assert!(unique_link().starts_with("Local\\mm-clap-"));
        assert_ne!(unique_link(), unique_link());
    }
}
