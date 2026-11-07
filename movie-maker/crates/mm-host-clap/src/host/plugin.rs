//! `.clap` のファイルの読み込み、descriptor の列挙、インスタンスから読むパラメータとポート。
//!
//! 共有ライブラリは解放しない（`PluginEntry` はプロセスの終わりまで持つ）。プラグインが残したスレッドや
//! 終了時の処理が、解放後にクラッシュすることがあるため。プロセスごと捨てる。

use std::ffi::{CStr, CString};
use std::path::Path;

use clack_extensions::audio_ports::{AudioPortFlags, AudioPortInfoBuffer, PluginAudioPorts};
use clack_extensions::params::{ParamInfoBuffer, ParamInfoFlags, PluginParams};
use clack_host::entry::LibraryEntry;
use clack_host::plugin::PluginDescriptor;
use clack_host::prelude::*;

use mm_host_clap::{BundleDesc, ParamDesc, PluginDesc, Skipped};

/// このホストのプラグインへの名乗り。
const HOST_NAME: &str = "movie-maker";
const HOST_VENDOR: &str = "durun-lab";
const HOST_URL: &str = "https://github.com/durun-lab";

/// このホストの共有の窓口。プラグインからの頼み（再起動、処理、主スレッドの呼び出し）は受けて捨てる。
/// 音は要求ごとに処理し、パラメータは毎回イベントで渡すので、頼みに応える必要が無い。
pub struct MmShared;

impl SharedHandler<'_> for MmShared {
    fn request_restart(&self) {}
    fn request_process(&self) {}
    fn request_callback(&self) {}
}

/// `clack-host` に渡すホストの型。
pub struct MmHost;

impl HostHandlers for MmHost {
    type Shared<'a> = MmShared;
    type MainThread<'a> = ();
    type AudioProcessor<'a> = ();
}

/// `.clap` のファイルを読み込む。
pub fn load(path: &Path) -> Result<PluginEntry, String> {
    // 依存の DLL の探し方を絞るため、clack-host の `PluginEntry::load` を使わずに自分で読み込んで渡す。
    // SAFETY: 共有ライブラリを読み込む。プラグインのコードはこのプロセスで動き、クラッシュはこのプロセスだけを止める。
    let library = unsafe { mm_host_pipe::load_library(path) }?;
    let bundle = CString::new(path.to_string_lossy().into_owned()).map_err(|e| e.to_string())?;
    // SAFETY: CLAP の規約の `clap_entry` を読み、その init を呼ぶ。クラッシュはこのプロセスだけを止める。
    let entry = unsafe { LibraryEntry::load_from_library(library) }
        .map_err(|e| format!("{} に clap_entry が無い: {e}", path.display()))?;
    // SAFETY: 同上。
    unsafe { PluginEntry::load_from(entry, &bundle) }
        .map_err(|e| format!("{} を初期化できない: {e}", path.display()))
}

/// インスタンスを作る。
pub fn instantiate(entry: &PluginEntry, id: &CStr) -> Result<PluginInstance<MmHost>, String> {
    let info = HostInfo::new(HOST_NAME, HOST_VENDOR, HOST_URL, env!("CARGO_PKG_VERSION"))
        .map_err(|e| e.to_string())?;
    PluginInstance::<MmHost>::new(|_| MmShared, |_| (), entry, id, &info)
        .map_err(|e| format!("{} のインスタンスを作れない: {e}", id.to_string_lossy()))
}

/// ファイルのプラグインの `id` の列。
pub fn plugin_ids(entry: &PluginEntry) -> Result<Vec<CString>, String> {
    let factory = entry
        .get_plugin_factory()
        .ok_or("プラグインのファクトリーが無い")?;
    Ok(factory
        .plugin_descriptors()
        .filter_map(|d| d.id().map(CStr::to_owned))
        .collect())
}

/// `id` のプラグインの descriptor を記述にする。
pub fn base_desc(entry: &PluginEntry, id: &CStr) -> Result<PluginDesc, String> {
    let factory = entry
        .get_plugin_factory()
        .ok_or("プラグインのファクトリーが無い")?;
    let d = factory
        .plugin_descriptors()
        .find(|d| d.id() == Some(id))
        .ok_or_else(|| format!("プラグイン {} が無い", id.to_string_lossy()))?;
    Ok(descriptor_desc(d))
}

fn text(s: Option<&CStr>) -> String {
    s.map(|s| s.to_string_lossy().into_owned())
        .unwrap_or_default()
}

fn descriptor_desc(d: &PluginDescriptor) -> PluginDesc {
    PluginDesc {
        id: text(d.id()),
        name: text(d.name()),
        vendor: text(d.vendor()),
        version: text(d.version()),
        description: text(d.description()),
        features: d
            .features()
            .map(|f| f.to_string_lossy().into_owned())
            .collect(),
        ..PluginDesc::default()
    }
}

/// インスタンスからパラメータと主のポートのチャンネル数を読んで、記述に足す。
pub fn read_instance(instance: &mut PluginInstance<MmHost>, base: PluginDesc) -> PluginDesc {
    let handle = instance.plugin_handle();
    let params = handle
        .get_extension::<PluginParams>()
        .map(|ext| read_params(&ext, &handle))
        .unwrap_or_default();
    let ports = handle.get_extension::<PluginAudioPorts>();
    let channels = |is_input| {
        ports
            .as_ref()
            .map_or(0, |ext| main_channels(ext, &handle, is_input))
    };
    PluginDesc {
        params,
        input_channels: channels(true),
        output_channels: channels(false),
        ..base
    }
}

fn read_params(ext: &PluginParams, handle: &PluginMainThreadHandle) -> Vec<ParamDesc> {
    let mut buffer = ParamInfoBuffer::new();
    (0..ext.count(handle))
        .filter_map(|i| {
            let info = ext.get_info(handle, i, &mut buffer)?;
            Some(ParamDesc {
                id: info.id.get(),
                name: String::from_utf8_lossy(info.name).into_owned(),
                module: String::from_utf8_lossy(info.module).into_owned(),
                min: info.min_value,
                max: info.max_value,
                default: info.default_value,
                stepped: info.flags.contains(ParamInfoFlags::IS_STEPPED),
                hidden: info.flags.contains(ParamInfoFlags::IS_HIDDEN),
                read_only: info.flags.contains(ParamInfoFlags::IS_READONLY),
            })
        })
        .collect()
}

/// 主のポート（`IS_MAIN`、無ければ最初のポート）のチャンネル数。ポートが無ければ 0。
fn main_channels(ext: &PluginAudioPorts, handle: &PluginMainThreadHandle, is_input: bool) -> u32 {
    let mut buffer = AudioPortInfoBuffer::new();
    let mut first = None;
    for i in 0..ext.count(handle, is_input) {
        let Some(info) = ext.get(handle, i, is_input, &mut buffer) else {
            continue;
        };
        if info.flags.contains(AudioPortFlags::IS_MAIN) {
            return info.channel_count;
        }
        first.get_or_insert(info.channel_count);
    }
    first.unwrap_or(0)
}

/// ファイルの全部のプラグインを、インスタンスを作って記述する。作れなかったものは理由つきで `skipped` に入る。
pub fn describe_bundle(path: &Path) -> Result<BundleDesc, String> {
    let entry = load(path)?;
    let mut desc = BundleDesc {
        path: path.to_path_buf(),
        ..BundleDesc::default()
    };
    for id in plugin_ids(&entry)? {
        let described = base_desc(&entry, &id).and_then(|base| {
            let mut instance = instantiate(&entry, &id)?;
            Ok(read_instance(&mut instance, base))
        });
        match described {
            Ok(d) => desc.plugins.push(d),
            Err(reason) => desc.skipped.push(Skipped {
                id: id.to_string_lossy().into_owned(),
                reason,
            }),
        }
    }
    // 解放しない（モジュールの冒頭）。
    std::mem::forget(entry);
    Ok(desc)
}
