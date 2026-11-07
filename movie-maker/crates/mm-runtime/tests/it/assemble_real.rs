//! 実際の実装を選ぶ組み立ての分岐。GPU が無い環境ではスキップし、`MM_REQUIRE_GPU` があれば失敗にする。

use mm_node_api::NodeRegistry;
use mm_runtime::{assemble, Backend, RuntimeConfig};
use mm_types::MmError;

/// 描画を wgpu、文字を glyphon、音を dsp にし、デコードとエンコードはモックのままにした設定。
fn gpu_config() -> RuntimeConfig {
    RuntimeConfig {
        render: Backend::Wgpu,
        text: Backend::Glyphon,
        audio: Backend::Dsp,
        ..RuntimeConfig::sample()
    }
}

#[test]
fn wgpu_と_glyphon_と_dsp_を選べる() {
    let engine = match assemble(&gpu_config()) {
        Ok(e) => e,
        Err(MmError::Unsupported(e)) if std::env::var_os("MM_REQUIRE_GPU").is_none() => {
            eprintln!("GPU が無いのでスキップする: {e}");
            return;
        }
        Err(e) => panic!("組めない: {e}"),
    };
    let names: Vec<_> = engine.backend_names();
    assert!(names.contains(&("render", "wgpu")), "{names:?}");
    assert!(names.contains(&("audio", "dsp")), "{names:?}");
    // 種類の数は並行に足す実装のクレートで変わるので、名前で確かめる。
    for kind in mm_nodes_basic::all_kinds() {
        assert!(engine.registry().get(kind.id()).is_some(), "{}", kind.id());
    }
    assert!(engine.registry().get("lottie").is_some());
    assert!(engine.registry().get("web.cef").is_some());
    assert!(engine.registry().get("gen.ai").is_some());
    assert!(engine.registry().get("shader.hlsl").is_some());
    assert!(engine.registry().get(mm_host_clap::KIND_ID).is_some());
    assert!(engine.registry().get(mm_host_vst3::KIND_ID).is_some());
    // ofx は mm-host-ofx の実行ファイルがあるとき（ワークスペースを作った後）だけ登録する。
    let ofx = mm_host_ofx::find_host().is_some();
    assert_eq!(engine.registry().get("ofx").is_some(), ofx);
}

#[test]
fn 文字の_cpu_と色の_lut_はgpuなしで選べる() {
    let config = RuntimeConfig {
        text: Backend::Cpu,
        color: Backend::Lut,
        ..RuntimeConfig::sample()
    };
    assert!(!config.needs_gpu());
    let engine = assemble(&config).expect("組める");
    assert!(engine.backend_names().contains(&("color", "lut")));
}

#[cfg(not(feature = "ocio"))]
#[test]
fn ocio_は_feature_が無いと拒否する() {
    let config = RuntimeConfig {
        color: Backend::Ocio,
        ..RuntimeConfig::sample()
    };
    assert!(matches!(assemble(&config), Err(MmError::Unsupported(_))));
}

#[cfg(feature = "ocio")]
#[test]
fn ocio_を選べる() {
    let config = RuntimeConfig {
        color: Backend::Ocio,
        ..RuntimeConfig::sample()
    };
    let engine = assemble(&config).expect("組める");
    assert!(engine.backend_names().contains(&("color", "ocio")));
}

#[test]
fn インターフェースに合わない実装は拒否する() {
    let config = RuntimeConfig {
        audio: Backend::Wgpu,
        ..RuntimeConfig::sample()
    };
    assert!(matches!(assemble(&config), Err(MmError::Unsupported(_))));
}

#[cfg(not(feature = "ffmpeg"))]
#[test]
fn ffmpeg_は_feature_が切ってあれば拒否する() {
    let config = RuntimeConfig {
        decode: Backend::Ffmpeg,
        ..RuntimeConfig::sample()
    };
    assert!(matches!(assemble(&config), Err(MmError::Unsupported(_))));
}

#[cfg(feature = "ffmpeg")]
#[test]
fn ffmpeg_を選べる() {
    let config = RuntimeConfig {
        decode: Backend::Ffmpeg,
        encode: Backend::Ffmpeg,
        ..RuntimeConfig::sample()
    };
    let engine = assemble(&config).expect("組める");
    let names = engine.backend_names();
    assert!(names.contains(&("decode", "ffmpeg")), "{names:?}");
    assert!(names.contains(&("encode", "ffmpeg")), "{names:?}");
}
