//! サーフェスのテクスチャの取得の結果を、描くテクスチャか立て直し方に分ける。

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

#[cfg(test)]
mod tests {
    use super::*;
    use wgpu::CurrentSurfaceTexture::{Lost, Occluded, Outdated, Timeout, Validation};

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
}
