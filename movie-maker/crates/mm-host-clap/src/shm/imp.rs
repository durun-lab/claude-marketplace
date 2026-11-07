//! Windows の名前つきの共有メモリ（ページファイルを裏に持つファイルマッピング）とイベント。

use std::ptr::NonNull;
use std::time::Duration;

use windows::core::HSTRING;
use windows::Win32::Foundation::{CloseHandle, HANDLE, INVALID_HANDLE_VALUE, WAIT_OBJECT_0};
use windows::Win32::System::Memory::{
    CreateFileMappingW, MapViewOfFile, OpenFileMappingW, UnmapViewOfFile, FILE_MAP_ALL_ACCESS,
    MEMORY_MAPPED_VIEW_ADDRESS, PAGE_READWRITE,
};
use windows::Win32::System::Threading::{
    CreateEventW, OpenEventW, OpenProcess, SetEvent, WaitForMultipleObjects, EVENT_MODIFY_STATE,
    PROCESS_SYNCHRONIZE, SYNCHRONIZATION_SYNCHRONIZE,
};

use mm_types::{MmError, Result};

use super::{Block, Wake};

fn os_error(what: &str, e: windows::core::Error) -> MmError {
    MmError::Internal(format!("{what}: {e}"))
}

/// 閉じ忘れないハンドル。
struct Owned(HANDLE);

// SAFETY: カーネルのオブジェクトのハンドルはプロセスの中のどのスレッドからでも使える。
unsafe impl Send for Owned {}
// SAFETY: 同上。ハンドルを書き換える操作は無い。
unsafe impl Sync for Owned {}

impl Drop for Owned {
    fn drop(&mut self) {
        // SAFETY: このクレートが作るか開いたハンドルで、ほかに閉じる所は無い。
        let _ = unsafe { CloseHandle(self.0) };
    }
}

/// 共有メモリ 1 つ。
pub struct Mapping {
    view: NonNull<Block>,
    _handle: Owned,
}

// SAFETY: 中身へは番を持つ側だけが `&mut` で触る（モジュールの冒頭の約束）。ポインターはどのスレッドでも有効。
unsafe impl Send for Mapping {}

impl Mapping {
    /// 名前つきで作る（アプリ）。中身は 0 で埋まっている。
    pub fn create(name: &str) -> Result<Self> {
        let size = std::mem::size_of::<Block>();
        // SAFETY: ページファイルを裏に持つマッピングを名前つきで作る。失敗は Err で返る。
        let handle = unsafe {
            CreateFileMappingW(
                INVALID_HANDLE_VALUE,
                None,
                PAGE_READWRITE,
                (size as u64 >> 32) as u32,
                size as u32,
                &HSTRING::from(name),
            )
        }
        .map_err(|e| os_error("共有メモリを作れない", e))?;
        let mut mapping = Self::map(Owned(handle))?;
        mapping.block().init();
        Ok(mapping)
    }

    /// 名前で開く（ホスト）。
    pub fn open(name: &str) -> Result<Self> {
        // SAFETY: 名前で開くだけ。失敗は Err で返る。
        let handle =
            unsafe { OpenFileMappingW(FILE_MAP_ALL_ACCESS.0, false, &HSTRING::from(name)) }
                .map_err(|e| os_error("共有メモリを開けない", e))?;
        let mut mapping = Self::map(Owned(handle))?;
        mapping.block().check()?;
        Ok(mapping)
    }

    fn map(handle: Owned) -> Result<Self> {
        let size = std::mem::size_of::<Block>();
        // SAFETY: 生きているマッピングのハンドルを、`Block` の大きさだけ読み書きで写す。
        let view = unsafe { MapViewOfFile(handle.0, FILE_MAP_ALL_ACCESS, 0, 0, size) };
        let view = NonNull::new(view.Value.cast::<Block>())
            .ok_or_else(|| MmError::Internal("共有メモリを写せない".into()))?;
        Ok(Self {
            view,
            _handle: handle,
        })
    }

    /// 中身。番を持っている間だけ使う（相手は自分の番の間しか書かない）。
    pub fn block(&mut self) -> &mut Block {
        // SAFETY: 写したページはこの値が生きている間有効で、`Block` の大きさと並び（4 KiB 境界）を満たす。
        // 相手のプロセスはこちらがイベントを立ててから相手がイベントを立てるまでの間しか触らない。
        unsafe { self.view.as_mut() }
    }
}

impl Drop for Mapping {
    fn drop(&mut self) {
        let addr = MEMORY_MAPPED_VIEW_ADDRESS {
            Value: self.view.as_ptr().cast(),
        };
        // SAFETY: `map` で写したアドレス。ほかに外す所は無い。
        let _ = unsafe { UnmapViewOfFile(addr) };
    }
}

/// 自動で戻るイベント 1 つ。
pub struct Event(Owned);

impl Event {
    /// 名前つきで作る（アプリ）。
    pub fn create(name: &str) -> Result<Self> {
        // SAFETY: 自動で戻る、立っていないイベントを名前つきで作る。
        let h = unsafe { CreateEventW(None, false, false, &HSTRING::from(name)) }
            .map_err(|e| os_error("イベントを作れない", e))?;
        Ok(Self(Owned(h)))
    }

    /// 名前で開く（ホスト）。
    pub fn open(name: &str) -> Result<Self> {
        let access = EVENT_MODIFY_STATE | SYNCHRONIZATION_SYNCHRONIZE;
        // SAFETY: 名前で開くだけ。
        let h = unsafe { OpenEventW(access, false, &HSTRING::from(name)) }
            .map_err(|e| os_error("イベントを開けない", e))?;
        Ok(Self(Owned(h)))
    }

    /// 立てる。
    pub fn set(&self) -> Result<()> {
        // SAFETY: 生きているイベントのハンドル。
        unsafe { SetEvent(self.0 .0) }.map_err(|e| os_error("イベントを立てられない", e))
    }

    /// 立つまで待つ。`peer` があれば、そのプロセスが終わったときも戻る。`None` の期限は無期限。
    pub fn wait(&self, peer: Option<&Process>, timeout: Option<Duration>) -> Wake {
        let ms = timeout.map_or(u32::MAX, |t| {
            t.as_millis().min(u128::from(u32::MAX - 1)) as u32
        });
        let mut handles = vec![self.0 .0];
        handles.extend(peer.map(|p| p.0 .0));
        // SAFETY: 生きているハンドルの列。どれか 1 つで戻る。
        let r = unsafe { WaitForMultipleObjects(&handles, false, ms) };
        match r.0.wrapping_sub(WAIT_OBJECT_0.0) {
            0 => Wake::Signaled,
            1 => Wake::Exited,
            _ => Wake::TimedOut,
        }
    }
}

/// 終わるのを待てる相手のプロセス。
pub struct Process(Owned);

impl Process {
    /// プロセス ID から開く。
    pub fn open(pid: u32) -> Result<Self> {
        // SAFETY: 待つための権限だけで開く。
        let h = unsafe { OpenProcess(PROCESS_SYNCHRONIZE, false, pid) }
            .map_err(|e| os_error("ホストのプロセスを開けない", e))?;
        Ok(Self(Owned(h)))
    }
}
