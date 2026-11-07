//! テストに使う VST3 のプラグイン。`vst3` クレート（生成済みのバインディング）で書き、SDK を使わない。
//!
//! - `MM Test Gain`: 1 つの部品が処理と編集（`IEditController`）を兼ねる形。パラメータは gain（正規化した値の 4 倍）、
//!   crash（0.5 を超えると処理の中で止まる）、sleep（正規化した値 × 20 秒だけ処理の中で眠る）。
//! - `MM Test Delay`: 処理と編集を別のクラスに分ける形。1 サンプルの遅延で、活性を入れ直すと中身を消す。

use std::cell::Cell;
use std::ffi::{c_char, c_void};
use std::time::Duration;

use vst3::Steinberg::Vst::*;
use vst3::Steinberg::*;
use vst3::{uid, Class, ComRef, ComWrapper};

const GAIN_CID: TUID = uid(0x4D4D_5445, 0x5354_4741, 0x494E_0000, 0x0000_0001);
const DELAY_CID: TUID = uid(0x4D4D_5445, 0x5354_4445, 0x4C41_5900, 0x0000_0002);
const DELAY_CONTROLLER_CID: TUID = uid(0x4D4D_5445, 0x5354_4445, 0x4C41_5900, 0x0000_0003);
const P_GAIN: u32 = 1;
const P_CRASH: u32 = 2;
const P_SLEEP: u32 = 3;
/// gain の正規化した値 1 が表す倍率。
const GAIN_MAX: f64 = 4.0;
/// crash がこの値を超えると止まる（段数 1 のパラメータの真ん中）。
const CRASH_AT: f64 = 0.5;
/// sleep の正規化した値 1 が表すミリ秒。
const SLEEP_MAX_MS: f64 = 20_000.0;
const CHANNELS: usize = 2;
const AUDIO_MODULE: &str = "Audio Module Class";

fn copy_c(src: &str, dst: &mut [c_char]) {
    let n = src.len().min(dst.len() - 1);
    for (d, s) in dst.iter_mut().zip(src.as_bytes()[..n].iter().chain([&0])) {
        *d = *s as c_char;
    }
}

fn copy_w(src: &str, dst: &mut [TChar]) {
    let units: Vec<u16> = src.encode_utf16().chain([0]).collect();
    for (d, s) in dst.iter_mut().zip(&units) {
        *d = *s;
    }
}

/// 処理の部品。`DELAY` が偽なら gain（編集を兼ねる）、真なら遅延（編集は別のクラス）。
struct Effect<const DELAY: bool> {
    gain: Cell<f64>,
    crash: Cell<f64>,
    sleep: Cell<f64>,
    last: Cell<[f32; CHANNELS]>,
}

impl<const DELAY: bool> Effect<DELAY> {
    fn new() -> Self {
        Self {
            gain: Cell::new(1.0 / GAIN_MAX),
            crash: Cell::new(0.0),
            sleep: Cell::new(0.0),
            last: Cell::new([0.0; CHANNELS]),
        }
    }

    fn set(&self, id: u32, value: f64) {
        match id {
            P_GAIN => self.gain.set(value),
            P_CRASH => self.crash.set(value),
            P_SLEEP => self.sleep.set(value),
            _ => {}
        }
    }

    /// 1 フレームを処理する。直前の値で決まる振る舞い（crash、sleep）もここで起こす。
    fn frame(&self, input: [f32; CHANNELS]) -> [f32; CHANNELS] {
        if self.crash.get() > CRASH_AT {
            std::process::abort();
        }
        let sleep = self.sleep.replace(0.0);
        if sleep > 0.0 {
            std::thread::sleep(Duration::from_millis((sleep * SLEEP_MAX_MS) as u64));
        }
        if DELAY {
            return self.last.replace(input);
        }
        let g = (self.gain.get() * GAIN_MAX) as f32;
        input.map(|s| s * g)
    }
}

impl Class for Effect<false> {
    type Interfaces = (IComponent, IAudioProcessor, IEditController);
}

impl Class for Effect<true> {
    type Interfaces = (IComponent, IAudioProcessor);
}

impl<const DELAY: bool> IPluginBaseTrait for Effect<DELAY> {
    unsafe fn initialize(&self, _context: *mut FUnknown) -> tresult {
        // 標準出力に書いても、ホストの応答に混ざらないことを確かめる。
        println!("mm_vst3_test: initialize");
        kResultOk
    }

    unsafe fn terminate(&self) -> tresult {
        kResultOk
    }
}

impl<const DELAY: bool> IComponentTrait for Effect<DELAY> {
    unsafe fn getControllerClassId(&self, class_id: *mut TUID) -> tresult {
        if !DELAY {
            return kNotImplemented;
        }
        *class_id = DELAY_CONTROLLER_CID;
        kResultOk
    }

    unsafe fn setIoMode(&self, _mode: IoMode) -> tresult {
        kResultOk
    }

    unsafe fn getBusCount(&self, media: MediaType, _dir: BusDirection) -> int32 {
        (media == MediaTypes_::kAudio as MediaType) as int32
    }

    unsafe fn getBusInfo(
        &self,
        media: MediaType,
        dir: BusDirection,
        index: int32,
        bus: *mut BusInfo,
    ) -> tresult {
        if media != MediaTypes_::kAudio as MediaType || index != 0 {
            return kInvalidArgument;
        }
        let bus = &mut *bus;
        bus.mediaType = media;
        bus.direction = dir;
        bus.channelCount = CHANNELS as int32;
        copy_w("Main", &mut bus.name);
        bus.busType = BusTypes_::kMain as BusType;
        bus.flags = BusInfo_::BusFlags_::kDefaultActive as uint32;
        kResultOk
    }

    unsafe fn getRoutingInfo(&self, _in: *mut RoutingInfo, _out: *mut RoutingInfo) -> tresult {
        kNotImplemented
    }

    unsafe fn activateBus(&self, _: MediaType, _: BusDirection, _: int32, _: TBool) -> tresult {
        kResultOk
    }

    unsafe fn setActive(&self, state: TBool) -> tresult {
        if state != 0 {
            self.last.set([0.0; CHANNELS]);
        }
        kResultOk
    }

    unsafe fn setState(&self, _state: *mut IBStream) -> tresult {
        kResultOk
    }

    unsafe fn getState(&self, _state: *mut IBStream) -> tresult {
        kResultOk
    }
}

impl<const DELAY: bool> IAudioProcessorTrait for Effect<DELAY> {
    unsafe fn setBusArrangements(
        &self,
        inputs: *mut SpeakerArrangement,
        num_ins: int32,
        outputs: *mut SpeakerArrangement,
        num_outs: int32,
    ) -> tresult {
        let stereo = num_ins == 1
            && num_outs == 1
            && *inputs == SpeakerArr::kStereo
            && *outputs == SpeakerArr::kStereo;
        if stereo {
            kResultTrue
        } else {
            kResultFalse
        }
    }

    unsafe fn getBusArrangement(
        &self,
        _dir: BusDirection,
        index: int32,
        arr: *mut SpeakerArrangement,
    ) -> tresult {
        if index != 0 {
            return kInvalidArgument;
        }
        *arr = SpeakerArr::kStereo;
        kResultOk
    }

    unsafe fn canProcessSampleSize(&self, size: int32) -> tresult {
        if size == SymbolicSampleSizes_::kSample32 as int32 {
            kResultTrue
        } else {
            kResultFalse
        }
    }

    unsafe fn getLatencySamples(&self) -> uint32 {
        DELAY as uint32
    }

    unsafe fn setupProcessing(&self, _setup: *mut ProcessSetup) -> tresult {
        kResultOk
    }

    unsafe fn setProcessing(&self, _state: TBool) -> tresult {
        kResultOk
    }

    unsafe fn process(&self, data: *mut ProcessData) -> tresult {
        let data = &*data;
        let points = param_points(data.inputParameterChanges);
        if data.numInputs < 1 || data.numOutputs < 1 {
            return kResultOk;
        }
        let n = data.numSamples as usize;
        let ins = std::slice::from_raw_parts((*data.inputs).__field0.channelBuffers32, CHANNELS);
        let outs = std::slice::from_raw_parts((*data.outputs).__field0.channelBuffers32, CHANNELS);
        let mut next = points.iter().peekable();
        for i in 0..n {
            while let Some((_, id, value)) = next.next_if(|p| p.0 <= i) {
                self.set(*id, *value);
            }
            let out = self.frame([*ins[0].add(i), *ins[1].add(i)]);
            for (c, s) in out.iter().enumerate() {
                *outs[c].add(i) = *s;
            }
        }
        kResultOk
    }

    unsafe fn getTailSamples(&self) -> uint32 {
        0
    }
}

/// 全部のパラメータの変化を（位置、id、値）の並びにし、位置の順に並べる。
unsafe fn param_points(changes: *mut IParameterChanges) -> Vec<(usize, u32, f64)> {
    let mut points = Vec::new();
    let Some(changes) = ComRef::from_raw(changes) else {
        return points;
    };
    for i in 0..changes.getParameterCount() {
        let Some(queue) = ComRef::from_raw(changes.getParameterData(i)) else {
            continue;
        };
        let id = queue.getParameterId();
        for p in 0..queue.getPointCount() {
            let (mut offset, mut value) = (0, 0.0);
            if queue.getPoint(p, &mut offset, &mut value) == kResultOk {
                points.push((offset.max(0) as usize, id, value));
            }
        }
    }
    points.sort_by_key(|p| p.0);
    points
}

/// パラメータの一覧（id、名前、段数、既定の正規化した値）。遅延のプラグインは持たない。
const PARAMS: [(u32, &str, int32, f64); 3] = [
    (P_GAIN, "gain", 0, 1.0 / GAIN_MAX),
    (P_CRASH, "crash", 1, 0.0),
    (P_SLEEP, "sleep", 0, 0.0),
];

/// 編集の部品の中身。gain は `PARAMS` を持ち、遅延の別のクラスは何も持たない。
macro_rules! edit_controller {
    ($ty:ty, $params:expr) => {
        impl IEditControllerTrait for $ty {
            unsafe fn setComponentState(&self, _state: *mut IBStream) -> tresult {
                kResultOk
            }
            unsafe fn setState(&self, _state: *mut IBStream) -> tresult {
                kResultOk
            }
            unsafe fn getState(&self, _state: *mut IBStream) -> tresult {
                kResultOk
            }
            unsafe fn getParameterCount(&self) -> int32 {
                $params.len() as int32
            }
            unsafe fn getParameterInfo(&self, index: int32, info: *mut ParameterInfo) -> tresult {
                let Some(&(id, name, steps, default)) = $params.get(index as usize) else {
                    return kInvalidArgument;
                };
                let info = &mut *info;
                info.id = id;
                copy_w(name, &mut info.title);
                copy_w(name, &mut info.shortTitle);
                copy_w("", &mut info.units);
                info.stepCount = steps;
                info.defaultNormalizedValue = default;
                info.unitId = 0;
                info.flags = ParameterInfo_::ParameterFlags_::kCanAutomate;
                kResultOk
            }
            unsafe fn getParamStringByValue(&self, _: u32, _: f64, _: *mut String128) -> tresult {
                kNotImplemented
            }
            unsafe fn getParamValueByString(&self, _: u32, _: *mut TChar, _: *mut f64) -> tresult {
                kNotImplemented
            }
            unsafe fn normalizedParamToPlain(&self, _id: u32, value: f64) -> f64 {
                value
            }
            unsafe fn plainParamToNormalized(&self, _id: u32, value: f64) -> f64 {
                value
            }
            unsafe fn getParamNormalized(&self, _id: u32) -> f64 {
                0.0
            }
            unsafe fn setParamNormalized(&self, _id: u32, _value: f64) -> tresult {
                kResultOk
            }
            unsafe fn setComponentHandler(&self, _handler: *mut IComponentHandler) -> tresult {
                kResultOk
            }
            unsafe fn createView(&self, _name: FIDString) -> *mut IPlugView {
                std::ptr::null_mut()
            }
        }
    };
}

edit_controller!(Effect<false>, PARAMS);
edit_controller!(DelayController, PARAMS[..0]);

/// 遅延のプラグインの編集のクラス。パラメータを持たない。
struct DelayController;

impl Class for DelayController {
    type Interfaces = (IEditController,);
}

impl IPluginBaseTrait for DelayController {
    unsafe fn initialize(&self, _context: *mut FUnknown) -> tresult {
        kResultOk
    }

    unsafe fn terminate(&self) -> tresult {
        kResultOk
    }
}

/// クラスの一覧（ID、カテゴリー、名前、サブカテゴリー）。
const CLASSES: [(TUID, &str, &str, &str); 3] = [
    (GAIN_CID, AUDIO_MODULE, "MM Test Gain", "Fx"),
    (DELAY_CID, AUDIO_MODULE, "MM Test Delay", "Fx|Delay"),
    (
        DELAY_CONTROLLER_CID,
        "Component Controller Class",
        "MM Test Delay",
        "",
    ),
];

struct Factory;

impl Class for Factory {
    type Interfaces = (IPluginFactory2,);
}

impl IPluginFactoryTrait for Factory {
    unsafe fn getFactoryInfo(&self, info: *mut PFactoryInfo) -> tresult {
        let info = &mut *info;
        copy_c("durun-lab", &mut info.vendor);
        copy_c("https://github.com/durun-lab", &mut info.url);
        copy_c("", &mut info.email);
        info.flags = PFactoryInfo_::FactoryFlags_::kUnicode as int32;
        kResultOk
    }

    unsafe fn countClasses(&self) -> int32 {
        CLASSES.len() as int32
    }

    unsafe fn getClassInfo(&self, index: int32, info: *mut PClassInfo) -> tresult {
        let Some(&(cid, category, name, _)) = CLASSES.get(index as usize) else {
            return kInvalidArgument;
        };
        let info = &mut *info;
        info.cid = cid;
        info.cardinality = PClassInfo_::ClassCardinality_::kManyInstances as int32;
        copy_c(category, &mut info.category);
        copy_c(name, &mut info.name);
        kResultOk
    }

    unsafe fn createInstance(
        &self,
        cid: FIDString,
        iid: FIDString,
        obj: *mut *mut c_void,
    ) -> tresult {
        let instance = match *(cid as *const TUID) {
            GAIN_CID => ComWrapper::new(Effect::<false>::new()).to_com_ptr::<FUnknown>(),
            DELAY_CID => ComWrapper::new(Effect::<true>::new()).to_com_ptr::<FUnknown>(),
            DELAY_CONTROLLER_CID => ComWrapper::new(DelayController).to_com_ptr::<FUnknown>(),
            _ => None,
        };
        let Some(instance) = instance else {
            return kInvalidArgument;
        };
        let ptr = instance.as_ptr();
        ((*(*ptr).vtbl).queryInterface)(ptr, iid as *const TUID, obj)
    }
}

impl IPluginFactory2Trait for Factory {
    unsafe fn getClassInfo2(&self, index: int32, info: *mut PClassInfo2) -> tresult {
        let Some(&(cid, category, name, sub)) = CLASSES.get(index as usize) else {
            return kInvalidArgument;
        };
        let info = &mut *info;
        info.cid = cid;
        info.cardinality = PClassInfo_::ClassCardinality_::kManyInstances as int32;
        copy_c(category, &mut info.category);
        copy_c(name, &mut info.name);
        info.classFlags = 0;
        copy_c(sub, &mut info.subCategories);
        copy_c("durun-lab", &mut info.vendor);
        copy_c("1.0.0", &mut info.version);
        copy_c("VST 3.8.0", &mut info.sdkVersion);
        kResultOk
    }
}

#[cfg(windows)]
#[no_mangle]
extern "system" fn InitDll() -> bool {
    true
}

#[cfg(windows)]
#[no_mangle]
extern "system" fn ExitDll() -> bool {
    true
}

#[no_mangle]
extern "system" fn GetPluginFactory() -> *mut IPluginFactory {
    ComWrapper::new(Factory)
        .to_com_ptr::<IPluginFactory>()
        .map_or(std::ptr::null_mut(), |p| p.into_raw())
}
