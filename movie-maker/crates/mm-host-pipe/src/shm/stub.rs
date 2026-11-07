//! Windows のほかの環境の代わり。共有メモリの経路はまだ無いので、作ろうとすると `Unsupported`。

use std::time::Duration;

use mm_types::{MmError, Result};

use super::Wake;

fn unsupported<T>() -> Result<T> {
    Err(MmError::Unsupported(
        "ホストとの共有メモリは Windows だけ".into(),
    ))
}

/// 共有メモリ 1 つ（作れない）。
pub struct Mapping(Vec<u8>);

impl Mapping {
    pub fn create(_name: &str, _len: usize) -> Result<Self> {
        unsupported()
    }

    pub fn open(_name: &str, _len: usize) -> Result<Self> {
        unsupported()
    }

    pub fn len(&self) -> usize {
        self.0.len()
    }

    pub fn is_empty(&self) -> bool {
        self.0.is_empty()
    }

    pub fn bytes(&self) -> &[u8] {
        &self.0
    }

    pub fn bytes_mut(&mut self) -> &mut [u8] {
        &mut self.0
    }

    /// # Safety
    /// 作れないので呼ばれない。Windows の実装と同じ規約。
    pub unsafe fn get_mut<T>(&mut self) -> &mut T {
        unreachable!("共有メモリは作れない")
    }
}

/// イベント 1 つ（作れない）。
pub struct Event;

impl Event {
    pub fn create(_name: &str) -> Result<Self> {
        unsupported()
    }

    pub fn open(_name: &str) -> Result<Self> {
        unsupported()
    }

    pub fn set(&self) -> Result<()> {
        unsupported()
    }

    pub fn wait(&self, _peer: Option<&Process>, _timeout: Option<Duration>) -> Wake {
        Wake::TimedOut
    }
}

/// 相手のプロセス（開けない）。
pub struct Process;

impl Process {
    pub fn open(_pid: u32) -> Result<Self> {
        unsupported()
    }
}
