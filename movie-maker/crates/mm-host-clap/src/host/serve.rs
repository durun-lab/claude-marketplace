//! `mm-host-clap serve`: 標準入力の `HostRequest` に 1 行ずつ `HostResponse` で答える。
//! 音のブロックはパイプを通らず、`Prepare` で始めた音のスレッドが共有メモリで処理する（`audio.rs`）。

use std::ffi::CString;
use std::fs::File;

use mm_host_clap::shm::{CHANNELS, MAX_FRAMES};
use mm_host_clap::PrepareParams;
use mm_host_pipe::audio::serve::{serve_requests, Link};

use clack_host::prelude::*;

use super::audio;
use super::plugin::{self, MmHost};

/// 標準入力が閉じるか `Shutdown` が来るまで答え続ける。応答は `out` に書く。
pub fn run(out: &mut File) -> Result<(), String> {
    // 音のスレッドが動いている間、主スレッドのインスタンスを持ち続ける。
    // ライブラリとインスタンスはプロセスの終わりまで解放しない（`plugin.rs` の冒頭）。
    let mut session: Option<PluginInstance<MmHost>> = None;
    serve_requests("mm-host-clap", out, |params| {
        session = Some(prepare(params)?);
        Ok(())
    })
}

/// プラグインを読み込み、共有メモリにつないで、音のスレッドで処理を始める。
fn prepare(params: PrepareParams) -> Result<PluginInstance<MmHost>, String> {
    let link = Link::open(&params.link)?;
    let entry = plugin::load(&params.path)?;
    let id = pick(&entry, params.plugin.as_deref())?;
    let mut instance = plugin::instantiate(&entry, &id)?;
    let desc = plugin::read_instance(&mut instance, plugin::base_desc(&entry, &id)?);
    std::mem::forget(entry);
    if desc.input_channels as usize != CHANNELS || desc.output_channels as usize != CHANNELS {
        return Err(format!(
            "{} はステレオの効果でない（入力 {} ch、出力 {} ch）",
            desc.id, desc.input_channels, desc.output_channels
        ));
    }
    let config = PluginAudioConfiguration {
        sample_rate: f64::from(params.sample_rate),
        min_frames_count: 1,
        max_frames_count: MAX_FRAMES as u32,
    };
    let processor = instance
        .activate(|_, _| (), config)
        .map_err(|e| format!("{} を活性にできない: {e}", desc.id))?;
    let info = serde_json::to_vec(&desc).map_err(|e| e.to_string())?;
    audio::start(processor, link, &info)?;
    Ok(instance)
}

/// 使うプラグインの `id`。指定が無ければファイルの最初のもの。
fn pick(entry: &PluginEntry, wanted: Option<&str>) -> Result<CString, String> {
    let ids = plugin::plugin_ids(entry)?;
    match wanted {
        None => ids
            .into_iter()
            .next()
            .ok_or_else(|| "プラグインが 1 つも無い".into()),
        Some(w) => ids
            .into_iter()
            .find(|id| id.to_bytes() == w.as_bytes())
            .ok_or_else(|| format!("プラグイン {w} が無い")),
    }
}
