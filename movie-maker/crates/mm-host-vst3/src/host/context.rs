//! ホストがプラグインに渡す COM の部品（`IHostApplication`、`IComponentHandler`、`IParameterChanges`、
//! `IEventList`）と、VST3 の文字の読み書き。
//!
//! 編集の画面を開かないので、プラグインからの編集の知らせ（`performEdit` など）は受けて捨てる。
//! パラメータの値はアプリが毎回イベントで渡す。

use std::cell::{Cell, RefCell};
use std::ffi::{c_char, c_void};

use vst3::Steinberg::Vst::*;
use vst3::Steinberg::*;
use vst3::{Class, ComWrapper};

use mm_host_vst3::shm::ParamEvent;

/// このホストのプラグインへの名乗り。
const HOST_NAME: &str = "movie-maker";

/// NUL で終わる 8 ビットの文字（UTF-8）を読む。
pub fn read_c(chars: &[c_char]) -> String {
    let bytes: Vec<u8> = chars
        .iter()
        .take_while(|c| **c != 0)
        .map(|c| *c as u8)
        .collect();
    String::from_utf8_lossy(&bytes).into_owned()
}

/// NUL で終わる UTF-16 の文字を読む。
pub fn read_w(chars: &[TChar]) -> String {
    let units: Vec<u16> = chars.iter().take_while(|c| **c != 0).copied().collect();
    String::from_utf16_lossy(&units)
}

/// `src` を UTF-16 で `dst` に書き、NUL で終える。入りきらなければ切る。
fn write_w(src: &str, dst: &mut [TChar]) {
    let Some(room) = dst.len().checked_sub(1) else {
        return;
    };
    let units: Vec<u16> = src.encode_utf16().take(room).chain([0]).collect();
    dst[..units.len()].copy_from_slice(&units);
}

/// `IHostApplication`。インスタンスを作る頼み（メッセージなど）は拒否する。
pub struct HostApp;

impl Class for HostApp {
    type Interfaces = (IHostApplication,);
}

impl IHostApplicationTrait for HostApp {
    unsafe fn getName(&self, name: *mut String128) -> tresult {
        write_w(HOST_NAME, &mut *name);
        kResultOk
    }

    unsafe fn createInstance(
        &self,
        _cid: *mut TUID,
        _iid: *mut TUID,
        obj: *mut *mut c_void,
    ) -> tresult {
        *obj = std::ptr::null_mut();
        kNotImplemented
    }
}

/// `IComponentHandler`。編集の知らせと作り直しの頼みを受けて捨てる。
pub struct Handler;

impl Class for Handler {
    type Interfaces = (IComponentHandler,);
}

impl IComponentHandlerTrait for Handler {
    unsafe fn beginEdit(&self, _id: ParamID) -> tresult {
        kResultOk
    }

    unsafe fn performEdit(&self, _id: ParamID, _value: ParamValue) -> tresult {
        kResultOk
    }

    unsafe fn endEdit(&self, _id: ParamID) -> tresult {
        kResultOk
    }

    unsafe fn restartComponent(&self, _flags: int32) -> tresult {
        kResultOk
    }
}

/// 1 つのパラメータの値の変化の並び（位置の昇順）。
pub struct ParamQueue {
    id: Cell<ParamID>,
    points: RefCell<Vec<(int32, ParamValue)>>,
}

impl Class for ParamQueue {
    type Interfaces = (IParamValueQueue,);
}

impl IParamValueQueueTrait for ParamQueue {
    unsafe fn getParameterId(&self) -> ParamID {
        self.id.get()
    }

    unsafe fn getPointCount(&self) -> int32 {
        self.points.borrow().len() as int32
    }

    unsafe fn getPoint(&self, index: int32, offset: *mut int32, value: *mut ParamValue) -> tresult {
        match self.points.borrow().get(index as usize) {
            Some(&(o, v)) => {
                *offset = o;
                *value = v;
                kResultOk
            }
            None => kInvalidArgument,
        }
    }

    unsafe fn addPoint(&self, offset: int32, value: ParamValue, index: *mut int32) -> tresult {
        let mut points = self.points.borrow_mut();
        if !index.is_null() {
            *index = points.len() as int32;
        }
        points.push((offset, value));
        kResultOk
    }
}

/// `IParameterChanges`。並びは使い回し、`used` から先は空きとして持つ。
pub struct ParamChanges {
    queues: RefCell<Vec<ComWrapper<ParamQueue>>>,
    used: Cell<usize>,
}

impl Class for ParamChanges {
    type Interfaces = (IParameterChanges,);
}

impl ParamChanges {
    pub fn new() -> Self {
        Self {
            queues: RefCell::new(Vec::new()),
            used: Cell::new(0),
        }
    }

    /// 全部の並びを空にする。
    pub fn clear(&self) {
        self.used.set(0);
    }

    /// `id` の並びの番号。無ければ空きの並びを使うか足す。
    fn queue_for(&self, id: ParamID) -> usize {
        let mut queues = self.queues.borrow_mut();
        let used = self.used.get();
        if let Some(i) = queues[..used].iter().position(|q| q.id.get() == id) {
            return i;
        }
        if used == queues.len() {
            queues.push(ComWrapper::new(ParamQueue {
                id: Cell::new(id),
                points: RefCell::new(Vec::new()),
            }));
        }
        queues[used].id.set(id);
        queues[used].points.borrow_mut().clear();
        self.used.set(used + 1);
        used
    }

    /// 共有メモリのイベント（位置の昇順）を、パラメータごとの並びに入れ直す。
    pub fn fill(&self, events: &[ParamEvent]) {
        self.clear();
        for e in events {
            let i = self.queue_for(e.param_id);
            let queues = self.queues.borrow();
            queues[i]
                .points
                .borrow_mut()
                .push((e.offset as int32, e.value));
        }
    }

    fn queue_ptr(&self, index: usize) -> *mut IParamValueQueue {
        self.queues
            .borrow()
            .get(index)
            .filter(|_| index < self.used.get())
            .and_then(|q| q.as_com_ref::<IParamValueQueue>())
            .map_or(std::ptr::null_mut(), |r| r.as_ptr())
    }
}

impl IParameterChangesTrait for ParamChanges {
    unsafe fn getParameterCount(&self) -> int32 {
        self.used.get() as int32
    }

    unsafe fn getParameterData(&self, index: int32) -> *mut IParamValueQueue {
        self.queue_ptr(index as usize)
    }

    unsafe fn addParameterData(
        &self,
        id: *const ParamID,
        index: *mut int32,
    ) -> *mut IParamValueQueue {
        let i = self.queue_for(*id);
        if !index.is_null() {
            *index = i as int32;
        }
        self.queue_ptr(i)
    }
}

/// `IEventList`。ノートなどのイベントは渡さず、プラグインが出すものは捨てる。
pub struct NoEvents;

impl Class for NoEvents {
    type Interfaces = (IEventList,);
}

impl IEventListTrait for NoEvents {
    unsafe fn getEventCount(&self) -> int32 {
        0
    }

    unsafe fn getEvent(&self, _index: int32, _e: *mut Event) -> tresult {
        kInvalidArgument
    }

    unsafe fn addEvent(&self, _e: *mut Event) -> tresult {
        kResultOk
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn イベントはパラメータごとの並びに位置の順で入る() {
        let changes = ParamChanges::new();
        let e = |offset, param_id, value| ParamEvent {
            offset,
            param_id,
            value,
        };
        changes.fill(&[e(0, 1, 0.5), e(0, 2, 1.0), e(64, 1, 0.25)]);
        // SAFETY: 自分で作った並びを読むだけ。
        unsafe {
            assert_eq!(changes.getParameterCount(), 2);
            let q = &changes.queues.borrow()[0];
            assert_eq!(q.getParameterId(), 1);
            assert_eq!(*q.points.borrow(), [(0, 0.5), (64, 0.25)]);
        }
        changes.fill(&[e(3, 2, 0.0)]);
        unsafe { assert_eq!(changes.getParameterCount(), 1) };
    }

    #[test]
    fn 長い名前は切って_nul_で終える() {
        let mut dst = [1u16; 4];
        write_w("movie", &mut dst);
        assert_eq!(read_w(&dst), "mov");
        assert_eq!(dst[3], 0);
    }
}
