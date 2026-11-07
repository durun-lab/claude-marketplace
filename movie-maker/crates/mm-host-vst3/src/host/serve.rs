//! `mm-host-vst3 serve`: 標準入力の `HostRequest` に 1 行ずつ `HostResponse` で答える。
//!
//! `Prepare` で音のスレッドを 1 つ起こし、そのスレッドがモジュールの読み込み、初期化、活性化、処理、状態の消去を
//! 全部行う。VST3 の部品は呼ぶスレッドを決めている（初期化は主のスレッド、処理は音のスレッド）が、編集の画面を
//! 開かないこのホストでは 1 つのスレッドに寄せ、プラグインから見える呼び出しの順を 1 本にする。

use std::fs::File;
use std::sync::mpsc;

use mm_host_pipe::audio::serve::{serve_blocks, serve_requests, Link};
use mm_host_vst3::shm::CHANNELS;
use mm_host_vst3::{class_id_hex, parse_class_id, PrepareParams};

use super::module::{self, ClassInfo, Module, AUDIO_MODULE_CLASS};
use super::plugin::Instance;
use super::process::Processing;

/// 標準入力が閉じるか `Shutdown` が来るまで答え続ける。応答は `out` に書く。
pub fn run(out: &mut File) -> Result<(), String> {
    serve_requests("mm-host-vst3", out, start)
}

/// 音のスレッドを起こし、プラグインを読み込んで処理を始められたかを待って返す。
fn start(params: PrepareParams) -> Result<(), String> {
    let (tx, rx) = mpsc::channel();
    std::thread::Builder::new()
        .name("mm-host-vst3-audio".into())
        .spawn(move || match prepare(&params) {
            Ok((mut processing, mut link)) => {
                // 受け取る側は Prepare の中で待っているので、送れないのは主スレッドが終わったときだけ。
                let _ = tx.send(Ok(()));
                serve_blocks(&mut processing, &mut link);
            }
            Err(e) => {
                let _ = tx.send(Err(e));
            }
        })
        .map_err(|e| format!("音のスレッドを起こせない: {e}"))?;
    rx.recv()
        .map_err(|_| "音のスレッドが返事をしない".to_string())?
}

/// プラグインを読み込み、ステレオの効果として活性にして、記述を共有メモリに書く。
fn prepare(params: &PrepareParams) -> Result<(Processing, Link), String> {
    let mut link = Link::open(&params.link)?;
    let module = module::load(&params.path)?;
    let class = pick(&module, params.plugin.as_deref())?;
    let instance = Instance::create(&module, &class.cid)?;
    let desc = instance.desc(&class);
    std::mem::forget(module);
    if desc.input_channels as usize != CHANNELS || desc.output_channels as usize != CHANNELS {
        return Err(format!(
            "{}（{}）はステレオの効果でない（入力 {} ch、出力 {} ch）",
            desc.name, desc.id, desc.input_channels, desc.output_channels
        ));
    }
    instance.activate(params.sample_rate)?;
    let info = serde_json::to_vec(&desc).map_err(|e| e.to_string())?;
    link.mapping
        .block()
        .set_info(&info)
        .map_err(|e| e.to_string())?;
    Ok((Processing::new(instance, params.sample_rate), link))
}

/// 使う Audio Module Class。指定（クラスの ID の 16 進）が無ければ最初のもの。
fn pick(module: &Module, wanted: Option<&str>) -> Result<ClassInfo, String> {
    let wanted = match wanted {
        None => None,
        Some(w) => Some(
            parse_class_id(w).ok_or_else(|| format!("クラスの ID {w} は 32 桁の 16 進でない"))?,
        ),
    };
    module
        .classes()
        .into_iter()
        .filter(|c| c.category == AUDIO_MODULE_CLASS)
        .find(|c| wanted.is_none_or(|w| w == c.cid))
        .ok_or_else(|| match wanted {
            None => "Audio Module Class が 1 つも無い".to_string(),
            Some(w) => format!("クラス {} が無い", class_id_hex(&w)),
        })
}
