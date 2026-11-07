//! 1 つのクラスのインスタンス（処理の部品 `IComponent` と `IAudioProcessor`、編集の部品 `IEditController`）の
//! 作成、記述（バスとパラメータ）、活性化、状態の消去と、バンドルの列挙。
//!
//! 編集の部品は、処理の部品が兼ねていればそれを使い、別のクラスならファクトリーで作って `IConnectionPoint` で
//! 互いにつなぐ（SDK の参考のホストと同じ手順）。呼び出しは全部 1 つのスレッドから行う。

use std::path::Path;

use vst3::Steinberg::Vst::*;
use vst3::Steinberg::*;
use vst3::{ComPtr, ComWrapper};

use mm_host_vst3::shm::MAX_FRAMES;
use mm_host_vst3::{class_id_hex, BundleDesc, ParamDesc, PluginDesc, Skipped};

use super::context::{read_w, Handler, HostApp};
use super::module::{self, ClassInfo, Module, AUDIO_MODULE_CLASS};

/// 1 つの向き（入力か出力）の音のバス。
#[derive(Debug, Clone, Default)]
pub struct Buses {
    /// バスごとのチャンネル数。
    pub channels: Vec<i32>,
    /// 主のバス（`kMain`）の番号。無ければ最初のバス。
    pub main: Option<usize>,
}

impl Buses {
    /// 主のバスのチャンネル数。バスが無ければ 0。
    pub fn main_channels(&self) -> u32 {
        self.main
            .and_then(|i| self.channels.get(i))
            .map_or(0, |c| (*c).max(0) as u32)
    }
}

/// 1 つのクラスのインスタンス。
pub struct Instance {
    pub component: ComPtr<IComponent>,
    pub processor: ComPtr<IAudioProcessor>,
    controller: Option<ComPtr<IEditController>>,
    pub inputs: Buses,
    pub outputs: Buses,
    _host: ComWrapper<HostApp>,
    _handler: ComWrapper<Handler>,
}

/// 結果の値が成功でなければ、何をしていたかをつけて失敗にする。
pub fn check(result: tresult, what: &str) -> Result<(), String> {
    if result == kResultOk {
        Ok(())
    } else {
        Err(format!("{what} が失敗した（{result}）"))
    }
}

impl Instance {
    /// `cid` のクラスを作り、初期化して編集の部品とつなぐ。
    pub fn create(module: &Module, cid: &TUID) -> Result<Self, String> {
        let host = ComWrapper::new(HostApp);
        let handler = ComWrapper::new(Handler);
        let context = host
            .to_com_ptr::<FUnknown>()
            .ok_or("ホストの部品を作れない")?;
        let component: ComPtr<IComponent> = module.create(cid)?;
        // SAFETY: 作ったばかりの部品を、SDK の決まった順で初期化する。
        unsafe {
            check(component.initialize(context.as_ptr()), "initialize")?;
            let processor = component
                .cast::<IAudioProcessor>()
                .ok_or("IAudioProcessor が無い")?;
            let controller = match component.cast::<IEditController>() {
                Some(c) => Some(c),
                None => separate_controller(module, &component, &context),
            };
            if let (Some(c), Some(h)) = (&controller, handler.to_com_ptr::<IComponentHandler>()) {
                c.setComponentHandler(h.as_ptr());
            }
            Ok(Self {
                inputs: buses(&component, BusDirections_::kInput),
                outputs: buses(&component, BusDirections_::kOutput),
                component,
                processor,
                controller,
                _host: host,
                _handler: handler,
            })
        }
    }

    /// クラスの記述とインスタンスから読んだバスとパラメータを合わせる。
    pub fn desc(&self, class: &ClassInfo) -> PluginDesc {
        PluginDesc {
            id: class_id_hex(&class.cid),
            name: class.name.clone(),
            vendor: class.vendor.clone(),
            version: class.version.clone(),
            description: String::new(),
            features: class
                .sub_categories
                .split('|')
                .filter(|s| !s.is_empty())
                .map(str::to_owned)
                .collect(),
            params: self.params(),
            input_channels: self.inputs.main_channels(),
            output_channels: self.outputs.main_channels(),
        }
    }

    fn params(&self) -> Vec<ParamDesc> {
        let Some(c) = &self.controller else {
            return Vec::new();
        };
        // SAFETY: 編集の部品にゼロで埋めた構造体を渡して書かせる。
        unsafe {
            (0..c.getParameterCount())
                .filter_map(|i| {
                    let mut info: ParameterInfo = std::mem::zeroed();
                    (c.getParameterInfo(i, &mut info) == kResultOk).then(|| param_desc(&info))
                })
                .collect()
        }
    }

    /// 主のバスをステレオにして活性にし、処理を始める。ほかのバスは今の配置のまま止める。
    pub fn activate(&self, sample_rate: u32) -> Result<(), String> {
        // SAFETY: SDK の決まった順（配置、バスの活性、処理の設定、活性、処理の開始）で呼ぶ。
        unsafe {
            self.set_buses()?;
            if self
                .processor
                .canProcessSampleSize(SymbolicSampleSizes_::kSample32 as int32)
                != kResultTrue
            {
                return Err("32 ビットの浮動小数で処理できない".into());
            }
            let mut setup = ProcessSetup {
                processMode: ProcessModes_::kRealtime as int32,
                symbolicSampleSize: SymbolicSampleSizes_::kSample32 as int32,
                maxSamplesPerBlock: MAX_FRAMES as int32,
                sampleRate: f64::from(sample_rate),
            };
            check(
                self.processor.setupProcessing(&mut setup),
                "setupProcessing",
            )?;
            check(self.component.setActive(1), "setActive")?;
            // 処理の開始の知らせは実装しないプラグインがあるので、結果は見ない。
            self.processor.setProcessing(1);
        }
        Ok(())
    }

    /// 主のバスをステレオに配置して活性にし、ほかのバスは止める。主のバスがステレオにならなければ失敗する。
    unsafe fn set_buses(&self) -> Result<(), String> {
        let (Some(main_in), Some(main_out)) = (self.inputs.main, self.outputs.main) else {
            return Err("音の入力か出力のバスが無い".into());
        };
        let (input, output) = (BusDirections_::kInput, BusDirections_::kOutput);
        let mut ins = arrangements(&self.processor, input, &self.inputs, main_in);
        let mut outs = arrangements(&self.processor, output, &self.outputs, main_out);
        self.processor.setBusArrangements(
            ins.as_mut_ptr(),
            ins.len() as int32,
            outs.as_mut_ptr(),
            outs.len() as int32,
        );
        let kind = MediaTypes_::kAudio as MediaType;
        for (dir, main, n) in [(input, main_in, ins.len()), (output, main_out, outs.len())] {
            let mut arr: SpeakerArrangement = 0;
            self.processor
                .getBusArrangement(dir as BusDirection, main as int32, &mut arr);
            if arr != SpeakerArr::kStereo {
                return Err(format!("主のバスをステレオにできない（配置 {arr:#x}）"));
            }
            for i in 0..n {
                let on = (i == main) as TBool;
                self.component
                    .activateBus(kind, dir as BusDirection, i as int32, on);
            }
        }
        Ok(())
    }

    /// 内部の状態（遅延の中身など）を消す。VST3 に消す API は無いので、処理を止めて活性を切り、入れ直す。
    pub fn reset(&self) -> Result<(), String> {
        // SAFETY: 活性の間に呼ぶ。止めた順の逆で入れ直す。
        unsafe {
            self.processor.setProcessing(0);
            check(self.component.setActive(0), "setActive(false)")?;
            check(self.component.setActive(1), "setActive(true)")?;
            self.processor.setProcessing(1);
        }
        Ok(())
    }
}

/// パラメータの記述。値は正規化した値（0..=1）で、単位（`unitId`）はルート（0）でなければ置き場所に書く。
fn param_desc(info: &ParameterInfo) -> ParamDesc {
    let flag = |f: int32| info.flags & f != 0;
    ParamDesc {
        id: info.id,
        name: read_w(&info.title),
        module: if info.unitId != 0 {
            info.unitId.to_string()
        } else {
            String::new()
        },
        min: 0.0,
        max: 1.0,
        default: info.defaultNormalizedValue,
        stepped: info.stepCount > 0,
        hidden: flag(ParameterInfo_::ParameterFlags_::kIsHidden),
        read_only: flag(ParameterInfo_::ParameterFlags_::kIsReadOnly),
    }
}

/// 処理の部品が指す別の編集のクラスを作り、初期化して互いにつなぐ。作れなければ `None`（パラメータの一覧が空になる）。
unsafe fn separate_controller(
    module: &Module,
    component: &ComPtr<IComponent>,
    context: &ComPtr<FUnknown>,
) -> Option<ComPtr<IEditController>> {
    let mut cid: TUID = [0; 16];
    if component.getControllerClassId(&mut cid) != kResultOk || cid == [0; 16] {
        return None;
    }
    let controller = match module.create::<IEditController>(&cid) {
        Ok(c) => c,
        Err(e) => {
            eprintln!("mm-host-vst3: 編集のクラスを作れない: {e}");
            return None;
        }
    };
    if let Err(e) = check(
        controller.initialize(context.as_ptr()),
        "編集のクラスの initialize",
    ) {
        eprintln!("mm-host-vst3: {e}");
        return None;
    }
    let points = (
        component.cast::<IConnectionPoint>(),
        controller.cast::<IConnectionPoint>(),
    );
    if let (Some(a), Some(b)) = points {
        a.connect(b.as_ptr());
        b.connect(a.as_ptr());
    }
    Some(controller)
}

/// `dir` の向きの音のバスのチャンネル数と主のバス。
unsafe fn buses(component: &ComPtr<IComponent>, dir: BusDirections) -> Buses {
    let kind = MediaTypes_::kAudio as MediaType;
    let count = component.getBusCount(kind, dir as BusDirection).max(0);
    let mut out = Buses::default();
    for i in 0..count {
        let mut info: BusInfo = std::mem::zeroed();
        let ok = component.getBusInfo(kind, dir as BusDirection, i, &mut info) == kResultOk;
        out.channels.push(if ok { info.channelCount } else { 0 });
        if ok && info.busType == BusTypes_::kMain as BusType && out.main.is_none() {
            out.main = Some(i as usize);
        }
    }
    if out.main.is_none() && count > 0 {
        out.main = Some(0);
    }
    out
}

/// 主のバスをステレオに、ほかは今の配置にした並び。
unsafe fn arrangements(
    processor: &ComPtr<IAudioProcessor>,
    dir: BusDirections,
    buses: &Buses,
    main: usize,
) -> Vec<SpeakerArrangement> {
    (0..buses.channels.len())
        .map(|i| {
            if i == main {
                return SpeakerArr::kStereo;
            }
            let mut arr: SpeakerArrangement = 0;
            processor.getBusArrangement(dir as BusDirection, i as int32, &mut arr);
            arr
        })
        .collect()
}

/// バンドルの全部の Audio Module Class を、インスタンスを作って記述する。作れなかったものは理由つきで `skipped` に入る。
pub fn describe_bundle(path: &Path) -> Result<BundleDesc, String> {
    let module = module::load(path)?;
    let mut desc = BundleDesc {
        path: path.to_path_buf(),
        ..BundleDesc::default()
    };
    for class in module
        .classes()
        .iter()
        .filter(|c| c.category == AUDIO_MODULE_CLASS)
    {
        match Instance::create(&module, &class.cid) {
            Ok(instance) => {
                desc.plugins.push(instance.desc(class));
                // 片付けの呼び出しでクラッシュするプラグインがあるので、プロセスの終わりまで持つ（module.rs の冒頭）。
                std::mem::forget(instance);
            }
            Err(reason) => desc.skipped.push(Skipped {
                id: class_id_hex(&class.cid),
                reason: format!("{}: {reason}", class.name),
            }),
        }
    }
    std::mem::forget(module);
    Ok(desc)
}
