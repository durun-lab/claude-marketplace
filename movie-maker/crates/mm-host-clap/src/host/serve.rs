//! `mm-host-clap serve`: 標準入力の `HostRequest` に 1 行ずつ `HostResponse` で答える。
//! 音のブロックはパイプを通らず、`Prepare` で始めた音のスレッドが共有メモリで処理する（`audio.rs`）。

use std::ffi::CString;
use std::io::{BufRead, Write};

use mm_host_api::{HostRequest, HostResponse, PROTOCOL_VERSION};
use mm_host_clap::shm::{self, Event, Mapping, CHANNELS, MAX_FRAMES};
use mm_host_clap::PrepareParams;

use clack_host::prelude::*;

use super::audio::{self, Link};
use super::plugin::{self, MmHost};

/// 読み込んだプラグイン。音のスレッドが動いている間、主スレッドのインスタンスを持ち続ける。
/// ライブラリとインスタンスはプロセスの終わりまで解放しない（`plugin.rs` の冒頭）。
struct Session {
    _instance: PluginInstance<MmHost>,
}

#[derive(Default)]
struct Server {
    session: Option<Session>,
}

/// 標準入力が閉じるか `Shutdown` が来るまで答え続ける。応答は `out` に書く。
pub fn run(out: &mut impl Write) -> Result<(), String> {
    let mut server = Server::default();
    for line in std::io::stdin().lock().lines() {
        let line = line.map_err(|e| format!("要求を読めない: {e}"))?;
        let (response, quit) = match serde_json::from_str::<HostRequest>(&line) {
            Ok(req) => server.handle(req),
            Err(e) => (error(format!("要求を読めない: {e}")), false),
        };
        let text = serde_json::to_string(&response).map_err(|e| e.to_string())?;
        writeln!(out, "{text}")
            .and_then(|()| out.flush())
            .map_err(|e| format!("応答を書けない: {e}"))?;
        if quit {
            break;
        }
    }
    // 音のスレッドはプラグインの中で止まっていることがあるので、待たずにプロセスごと終える。
    std::mem::forget(server);
    Ok(())
}

fn error(message: String) -> HostResponse {
    HostResponse::Error { message }
}

impl Server {
    fn handle(&mut self, req: HostRequest) -> (HostResponse, bool) {
        let pong = HostResponse::Pong {
            protocol_version: PROTOCOL_VERSION,
        };
        let result = match req {
            HostRequest::Ping => Ok(pong),
            HostRequest::Shutdown => return (pong, true),
            HostRequest::Prepare { params } => {
                self.prepare(params).map(|()| HostResponse::Prepared)
            }
            HostRequest::RenderFrame { .. } => {
                Err("CLAP のホストは画像を描かない。音は共有メモリで渡す".to_string())
            }
        };
        if let Err(e) = &result {
            eprintln!("mm-host-clap: {e}");
        }
        (result.unwrap_or_else(error), false)
    }

    /// プラグインを読み込み、共有メモリにつないで、音のスレッドで処理を始める。1 つのプロセスで 1 回だけ。
    fn prepare(&mut self, params: serde_json::Value) -> Result<(), String> {
        if self.session.is_some() {
            return Err(
                "Prepare は 1 つのホストで 1 回だけ。別のプラグインはホストを起こし直す".into(),
            );
        }
        let params: PrepareParams = serde_json::from_value(params)
            .map_err(|e| format!("Prepare の params が読めない: {e}"))?;
        if params.sample_rate == 0 {
            return Err("サンプルレートは 1 以上".into());
        }
        let link = open_link(&params.link)?;
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
        self.session = Some(Session {
            _instance: instance,
        });
        Ok(())
    }
}

fn open_link(link: &str) -> Result<Link, String> {
    let [mem, req, done] = shm::object_names(link);
    Ok(Link {
        mapping: Mapping::open(&mem).map_err(|e| e.to_string())?,
        request: Event::open(&req).map_err(|e| e.to_string())?,
        done: Event::open(&done).map_err(|e| e.to_string())?,
    })
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
