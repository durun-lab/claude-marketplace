//! サーフェスのテクスチャの取得の結果を描くテクスチャか立て直し方に分け、取れなかったサーフェスを立て直す。

use super::{create_surface, Gpu};

/// サーフェスのテクスチャを取れなかったときの立て直し方。
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub(super) enum Recovery {
    /// サーフェスに手を入れず、待って次の周で取り直す。
    Wait,
    /// 今のサーフェスを張り直す。
    Reconfigure,
    /// サーフェスを作り直してから張り直す。
    Recreate,
}

/// 取得の結果を分ける。立て直し方は wgpu の `CurrentSurfaceTexture` の各バリアントの推奨に従う。
/// Timeout と Occluded は待てば取れ、張り直すと隠れている間ずっとスワップチェーンを作り直すので待つだけにする。
pub(super) fn classify(
    state: wgpu::CurrentSurfaceTexture,
) -> Result<wgpu::SurfaceTexture, Recovery> {
    use wgpu::CurrentSurfaceTexture::{
        Lost, Occluded, Outdated, Suboptimal, Success, Timeout, Validation,
    };
    let recovery = match state {
        Success(t) | Suboptimal(t) => return Ok(t),
        // 描画スレッドはエラーのスコープを積まず、検証のエラーは wgpu の既定で panic になるので、Validation は待つだけでよい。
        Timeout | Occluded | Validation => Recovery::Wait,
        Outdated => Recovery::Reconfigure,
        Lost => Recovery::Recreate,
    };
    tracing::debug!(?state, ?recovery, "サーフェスを取れなかった");
    Err(recovery)
}

impl Gpu {
    /// テクスチャを取れなかったサーフェスを `recovery` のとおりに立て直す。
    pub(super) fn recover(&mut self, hwnd: isize, recovery: Recovery) -> Result<(), String> {
        match recovery {
            Recovery::Wait => return Ok(()),
            Recovery::Reconfigure => {}
            Recovery::Recreate => self.surface = create_surface(&self.instance, hwnd)?,
        }
        self.surface.configure(&self.device, &self.config);
        Ok(())
    }
}

#[cfg(test)]
mod tests {
    use super::super::init_gpu;
    use super::*;
    use crate::win::state::Shared;
    use wgpu::CurrentSurfaceTexture::{Lost, Occluded, Outdated, Timeout, Validation};

    /// 試験の HWND とサーフェスの一辺（物理ピクセル）。
    const TEST_SIZE: u32 = 64;

    #[test]
    fn outdated_は張り直し_lost_は作り直す() {
        assert_eq!(classify(Outdated).err(), Some(Recovery::Reconfigure));
        assert_eq!(classify(Lost).err(), Some(Recovery::Recreate));
    }

    #[test]
    fn timeout_と_occluded_と検証のエラーはサーフェスに触れず待つ() {
        for state in [Timeout, Occluded, Validation] {
            assert_eq!(classify(state).err(), Some(Recovery::Wait));
        }
    }

    /// 試験に使う見えないトップレベルの HWND。落とすと破棄する。
    struct TestWindow(isize);

    impl TestWindow {
        fn new() -> Self {
            use windows::core::w;
            use windows::Win32::UI::WindowsAndMessaging::{
                CreateWindowExW, WINDOW_EX_STYLE, WS_OVERLAPPED,
            };
            let (title, size) = (w!("surface test"), TEST_SIZE as i32);
            let hwnd = unsafe {
                CreateWindowExW(
                    WINDOW_EX_STYLE(0),
                    w!("STATIC"),
                    title,
                    WS_OVERLAPPED,
                    0,
                    0,
                    size,
                    size,
                    None,
                    None,
                    None,
                    None,
                )
            }
            .expect("試験の HWND");
            Self(hwnd.0 as isize)
        }
    }

    impl Drop for TestWindow {
        fn drop(&mut self) {
            use windows::Win32::{Foundation::HWND, UI::WindowsAndMessaging::DestroyWindow};
            // 試験の後片付けなので、破棄に失敗しても続ける。
            let _ = unsafe { DestroyWindow(HWND(self.0 as *mut _)) };
        }
    }

    #[test]
    fn 張り直しと作り直しの後もテクスチャを取って_present_できる() {
        let window = TestWindow::new();
        let shared = Shared::new();
        let mut gpu = match init_gpu(&shared, window.0, None) {
            Ok(gpu) => gpu,
            Err(e) if std::env::var_os("MM_REQUIRE_GPU").is_some() => {
                panic!("MM_REQUIRE_GPU が有るのに GPU を作れない: {e}")
            }
            Err(e) => return eprintln!("GPU が無いのでスキップする: {e}"),
        };
        (gpu.config.width, gpu.config.height) = (TEST_SIZE, TEST_SIZE);
        for recovery in [
            Recovery::Reconfigure,
            Recovery::Recreate,
            Recovery::Recreate,
        ] {
            gpu.recover(window.0, recovery).expect("立て直せる");
            let texture = classify(gpu.surface.get_current_texture()).expect("取れる");
            gpu.queue.present(texture);
        }
    }
}
