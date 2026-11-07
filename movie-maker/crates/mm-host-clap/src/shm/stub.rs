//! Windows のほかの環境の代わり。共有メモリの経路はまだ無いので、作ろうとすると `Unsupported`。

use std::time::Duration;

use mm_types::{MmError, Result};

use super::{Block, Wake};

fn unsupported<T>() -> Result<T> {
    Err(MmError::Unsupported(
        "CLAP のホストの共有メモリは Windows だけ".into(),
    ))
}

/// 共有メモリ 1 つ（作れない）。
pub struct Mapping(Box<Block>);

impl Mapping {
    pub fn create(_name: &str) -> Result<Self> {
        unsupported()
    }

    pub fn open(_name: &str) -> Result<Self> {
        unsupported()
    }

    pub fn block(&mut self) -> &mut Block {
        &mut self.0
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
