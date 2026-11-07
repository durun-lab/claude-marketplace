//! VST3 のモジュールの読み込みと、ファクトリーのクラスの一覧、インスタンスの作成。
//!
//! 共有ライブラリは解放しない（`ExitDll` も呼ばない）。プラグインが残したスレッドや終了時の処理が、
//! 解放後にクラッシュすることがあるため。プロセスごと捨てる（`mm-host-clap` と同じ）。

use std::ffi::c_void;
use std::path::Path;

use vst3::Steinberg::*;
use vst3::{ComPtr, Interface};

use super::context::read_c;

/// 音の効果を含むクラスのカテゴリー。
pub const AUDIO_MODULE_CLASS: &str = "Audio Module Class";

/// 1 つのクラスの記述（`PClassInfo2`、無ければ `PClassInfo`）。
pub struct ClassInfo {
    pub cid: TUID,
    pub category: String,
    pub name: String,
    pub sub_categories: String,
    pub vendor: String,
    pub version: String,
}

/// 読み込んだモジュールのファクトリー。
pub struct Module {
    factory: ComPtr<IPluginFactory>,
    vendor: String,
}

type InitDll = unsafe extern "system" fn() -> bool;
type GetPluginFactory = unsafe extern "system" fn() -> *mut IPluginFactory;

/// バンドルのモジュールを読み込み、ファクトリーを受け取る。
pub fn load(path: &Path) -> Result<Module, String> {
    let file = mm_host_vst3::module_file(path);
    // SAFETY: 共有ライブラリを読み込み、決まった名前の関数を呼ぶ。プラグインのコードはこのプロセスで動き、
    // クラッシュはこのプロセスだけを止める（モジュールの冒頭）。
    unsafe {
        let lib = libloading::Library::new(&file)
            .map_err(|e| format!("{} を読み込めない: {e}", file.display()))?;
        if let Ok(init) = lib.get::<InitDll>(b"InitDll\0") {
            if !init() {
                return Err(format!("{} の InitDll が失敗した", file.display()));
            }
        }
        let get = lib
            .get::<GetPluginFactory>(b"GetPluginFactory\0")
            .map_err(|e| format!("{} に GetPluginFactory が無い: {e}", file.display()))?;
        let factory = ComPtr::from_raw(get()).ok_or("GetPluginFactory が空を返した")?;
        std::mem::forget(lib);
        let mut info: PFactoryInfo = std::mem::zeroed();
        let vendor = match factory.getFactoryInfo(&mut info) {
            r if r == kResultOk => read_c(&info.vendor),
            _ => String::new(),
        };
        Ok(Module { factory, vendor })
    }
}

impl Module {
    /// ファクトリーの全部のクラス。
    pub fn classes(&self) -> Vec<ClassInfo> {
        let f2 = self.factory.cast::<IPluginFactory2>();
        // SAFETY: ファクトリーの関数に、ゼロで埋めた構造体を渡して書かせる。
        let count = unsafe { self.factory.countClasses() };
        (0..count)
            .filter_map(|i| unsafe {
                match &f2 {
                    Some(f2) => self.class_info2(f2, i),
                    None => self.class_info(i),
                }
            })
            .collect()
    }

    unsafe fn class_info2(&self, f2: &ComPtr<IPluginFactory2>, i: int32) -> Option<ClassInfo> {
        let mut c: PClassInfo2 = std::mem::zeroed();
        if f2.getClassInfo2(i, &mut c) != kResultOk {
            return self.class_info(i);
        }
        let vendor = Some(read_c(&c.vendor)).filter(|v| !v.is_empty());
        Some(ClassInfo {
            cid: c.cid,
            category: read_c(&c.category),
            name: read_c(&c.name),
            sub_categories: read_c(&c.subCategories),
            vendor: vendor.unwrap_or_else(|| self.vendor.clone()),
            version: read_c(&c.version),
        })
    }

    unsafe fn class_info(&self, i: int32) -> Option<ClassInfo> {
        let mut c: PClassInfo = std::mem::zeroed();
        (self.factory.getClassInfo(i, &mut c) == kResultOk).then(|| ClassInfo {
            cid: c.cid,
            category: read_c(&c.category),
            name: read_c(&c.name),
            sub_categories: String::new(),
            vendor: self.vendor.clone(),
            version: String::new(),
        })
    }

    /// `cid` のクラスのインスタンスを `I` の口で作る。
    pub fn create<I: Interface>(&self, cid: &TUID) -> Result<ComPtr<I>, String> {
        let mut obj: *mut c_void = std::ptr::null_mut();
        // SAFETY: クラスの ID と口の ID は 16 バイトの列で、作られた口の参照を受け取る。
        unsafe {
            let result =
                self.factory
                    .createInstance(cid.as_ptr(), I::IID.as_ptr() as FIDString, &mut obj);
            if result != kResultOk {
                return Err(format!(
                    "クラス {} のインスタンスを作れない（{result}）",
                    mm_host_vst3::class_id_hex(cid)
                ));
            }
            ComPtr::from_raw(obj as *mut I).ok_or_else(|| "作ったインスタンスが空".to_string())
        }
    }
}
