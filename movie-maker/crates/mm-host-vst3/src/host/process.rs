//! 音のブロックの処理。共有メモリのチャンネルごとの配列を、そのまま主のバスの入れ物として `IAudioProcessor::process` に渡す。
//!
//! 位置の情報は、テンポ 120、4/4 拍子、止まっている状態で、時刻 0 を毎回渡す。空の `processContext` で
//! 落ちるプラグインを避けつつ、始めた位置で結果が変わらないようにするため（CLAP の `steady_time` を渡さないのと同じ）。
//! 主でないバス（サイドチェインなど）には無音の入力と捨てる出力を渡す。

use vst3::ComWrapper;
use vst3::Steinberg::Vst::ProcessContext_::StatesAndFlags_;
use vst3::Steinberg::Vst::*;
use vst3::Steinberg::*;

use mm_host_pipe::audio::serve::Plugin;
use mm_host_vst3::shm::{Block, CHANNELS, MAX_EVENTS, MAX_FRAMES};

use super::context::{NoEvents, ParamChanges};
use super::plugin::{check, Buses, Instance};

/// 位置の情報のテンポ（拍毎分）。
const TEMPO: f64 = 120.0;
/// 位置の情報の拍子。
const TIME_SIG: (int32, int32) = (4, 4);

/// 1 つの向きのバスの入れ物。主でないバスのチャンネルは `scratch` を指す。
struct BusBuffers {
    buses: Buses,
    pointers: Vec<Vec<*mut Sample32>>,
    scratch: Vec<Vec<Sample32>>,
    buffers: Vec<AudioBusBuffers>,
}

impl BusBuffers {
    fn new(buses: &Buses) -> Self {
        let main = buses.main;
        let mut scratch = Vec::new();
        let pointers = buses
            .channels
            .iter()
            .enumerate()
            .map(|(i, n)| {
                let n = if Some(i) == main {
                    CHANNELS
                } else {
                    (*n).max(0) as usize
                };
                (0..n)
                    .map(|_| {
                        scratch.push(vec![0.0; MAX_FRAMES]);
                        scratch
                            .last_mut()
                            .map_or(std::ptr::null_mut(), |s| s.as_mut_ptr())
                    })
                    .collect()
            })
            .collect();
        Self {
            buses: buses.clone(),
            pointers,
            scratch,
            buffers: Vec::new(),
        }
    }

    /// 主のバスを `main` の配列に向け、入れ物の列を作り直す。`silent` なら主でないバスを無音にする。
    fn bind(&mut self, main: &mut [[f32; MAX_FRAMES]; CHANNELS], frames: usize, silent: bool) {
        if let Some(m) = self.buses.main {
            for (p, ch) in self.pointers[m].iter_mut().zip(main.iter_mut()) {
                *p = ch.as_mut_ptr();
            }
        }
        if silent {
            self.scratch.iter_mut().for_each(|s| s[..frames].fill(0.0));
        }
        self.buffers = self
            .pointers
            .iter_mut()
            .enumerate()
            .map(|(i, p)| AudioBusBuffers {
                numChannels: p.len() as int32,
                silenceFlags: if silent && Some(i) != self.buses.main {
                    u64::MAX
                } else {
                    0
                },
                __field0: AudioBusBuffers__type0 {
                    channelBuffers32: p.as_mut_ptr(),
                },
            })
            .collect();
    }
}

/// 活性にしたインスタンスと、処理に使い回す入れ物。
pub struct Processing {
    instance: Instance,
    inputs: BusBuffers,
    outputs: BusBuffers,
    in_changes: ComWrapper<ParamChanges>,
    out_changes: ComWrapper<ParamChanges>,
    events: ComWrapper<NoEvents>,
    context: ProcessContext,
}

impl Processing {
    /// 活性にしたインスタンスから作る。
    pub fn new(instance: Instance, sample_rate: u32) -> Self {
        // SAFETY: ProcessContext は整数と浮動小数だけを持ち、0 は正しい値。
        let mut context: ProcessContext = unsafe { std::mem::zeroed() };
        context.sampleRate = f64::from(sample_rate);
        context.tempo = TEMPO;
        (context.timeSigNumerator, context.timeSigDenominator) = TIME_SIG;
        context.state = (StatesAndFlags_::kTempoValid | StatesAndFlags_::kTimeSigValid) as uint32;
        Self {
            inputs: BusBuffers::new(&instance.inputs),
            outputs: BusBuffers::new(&instance.outputs),
            instance,
            in_changes: ComWrapper::new(ParamChanges::new()),
            out_changes: ComWrapper::new(ParamChanges::new()),
            events: ComWrapper::new(NoEvents),
            context,
        }
    }
}

impl Plugin for Processing {
    fn process(&mut self, block: &mut Block) -> Result<(), String> {
        let n = block.frames as usize;
        let count = block.event_count as usize;
        if n == 0 || n > MAX_FRAMES || count > MAX_EVENTS {
            return Err(format!("フレーム数 {n} かイベントの数 {count} が範囲の外"));
        }
        self.in_changes.fill(&block.events[..count]);
        self.out_changes.clear();
        self.inputs.bind(&mut block.input, n, true);
        self.outputs.bind(&mut block.output, n, false);
        let ptr = |w: &ComWrapper<ParamChanges>| {
            w.as_com_ref::<IParameterChanges>()
                .map_or(std::ptr::null_mut(), |r| r.as_ptr())
        };
        let (in_changes, out_changes) = (ptr(&self.in_changes), ptr(&self.out_changes));
        let events = self
            .events
            .as_com_ref::<IEventList>()
            .map_or(std::ptr::null_mut(), |r| r.as_ptr());
        let mut data = ProcessData {
            processMode: ProcessModes_::kRealtime as int32,
            symbolicSampleSize: SymbolicSampleSizes_::kSample32 as int32,
            numSamples: n as int32,
            numInputs: self.inputs.buffers.len() as int32,
            numOutputs: self.outputs.buffers.len() as int32,
            inputs: self.inputs.buffers.as_mut_ptr(),
            outputs: self.outputs.buffers.as_mut_ptr(),
            inputParameterChanges: in_changes,
            outputParameterChanges: out_changes,
            inputEvents: events,
            outputEvents: events,
            processContext: &mut self.context,
        };
        // SAFETY: 入れ物と COM の部品は全部 self が持ち、この呼び出しの間生きている。
        check(
            unsafe { self.instance.processor.process(&mut data) },
            "process",
        )
    }

    fn reset(&mut self) -> Result<(), String> {
        self.instance.reset()
    }
}
