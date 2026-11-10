//! GPU のデバイスを同時に作るプロセスの数を、PC 全体で絞る。
//!
//! 多くのプロセス（並べて回すテストと、別の worktree のテスト）が同時に D3D12 のデバイスを作ると、VRAM に余裕があっても
//! デバイスの作成が `Not enough memory left`（E_OUTOFMEMORY）や `D3D12CreateDevice` の失敗で落ちる。作り終えたデバイスが
//! いくつあっても落ちないので、作る間だけ枠を 1 つ持つ。枠は名前付きのミューテックスで、持ち主のプロセスが落ちても OS が放す。

/// 同時にデバイスを作れるプロセスの数。4 なら mm-ui-wgpu の結合テストを 4 本並べて（各 48 スレッド）回しても落ちず、
/// 1 本で回す時間も変わらない。
#[cfg(windows)]
const SLOTS: usize = 4;

/// デバイスを作る枠。作る間だけ持ち、落とすと放す（中身は落とすときだけ使う）。
pub(crate) struct DeviceSlot {
    #[cfg(windows)]
    _held: Option<win::Held>,
}

/// 枠が空くまで待って 1 つ取る。ミューテックスを作れなければ、絞らずに作らせる（枠を持たない）。
pub(crate) fn acquire() -> DeviceSlot {
    DeviceSlot {
        #[cfg(windows)]
        _held: win::acquire(SLOTS),
    }
}

#[cfg(windows)]
mod win {
    use windows::core::HSTRING;
    use windows::Win32::Foundation::{CloseHandle, HANDLE, WAIT_ABANDONED_0};
    use windows::Win32::System::Threading::{
        CreateMutexW, ReleaseMutex, WaitForMultipleObjects, INFINITE,
    };

    /// 取った枠。`index` 番のミューテックスを持っている。
    pub(super) struct Held {
        handles: Vec<HANDLE>,
        index: usize,
    }

    /// `count` 個のミューテックスのどれかが空くまで待って持つ。前の持ち主が落ちて放されたもの（abandoned）も持てる。
    pub(super) fn acquire(count: usize) -> Option<Held> {
        let mut handles = Vec::with_capacity(count);
        for i in 0..count {
            let name = HSTRING::from(format!("Local\\mm-gpu-device-slot-{i}"));
            // SAFETY: 名前は呼ぶ間だけ生きていればよく、返ったハンドルは `Held` か下の失敗の道で閉じる。
            match unsafe { CreateMutexW(None, false, &name) } {
                Ok(h) => handles.push(h),
                Err(e) => {
                    tracing::warn!("GPU のデバイスを作る枠のミューテックスを作れない: {e}");
                    close(&handles);
                    return None;
                }
            }
        }
        // SAFETY: ハンドルは上で作った生きているミューテックスだけ。
        let result = unsafe { WaitForMultipleObjects(&handles, false, INFINITE) }.0 as usize;
        let abandoned = WAIT_ABANDONED_0.0 as usize;
        let index = if result < count {
            result
        } else if (abandoned..abandoned + count).contains(&result) {
            result - abandoned
        } else {
            close(&handles);
            return None;
        };
        Some(Held { handles, index })
    }

    fn close(handles: &[HANDLE]) {
        for h in handles {
            // SAFETY: 自分で作ったハンドルを 1 回だけ閉じる。
            let _ = unsafe { CloseHandle(*h) };
        }
    }

    impl Drop for Held {
        fn drop(&mut self) {
            // SAFETY: `index` 番は待ちで持ったミューテックスで、ここで 1 回だけ放してから全部を閉じる。
            let _ = unsafe { ReleaseMutex(self.handles[self.index]) };
            close(&self.handles);
        }
    }
}
