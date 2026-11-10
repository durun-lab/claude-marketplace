//! レイヤーの時間と有効の操作 `shiftLayer` と `setLayerEnabled`。

use crate::common;

use common::{canon, project, secs};
use mm_doc::{
    CaptionPlacement, Crop, CutReframe, Effect, Framing, Interpolation, Keyframe, Layer,
    LayerAudio, Mask, MaskRect, Node, Param, Project, TimeKeyframe, TimeRemap, Transform,
};
use mm_ops::{
    Edge, LockLayer, MoveKeyframe, Op, OpContext, OpError, ParamRef, SetLayerEnabled, ShiftLayer,
    TrimLayer,
};
use mm_types::{Id, ParamValue, RationalTime, Rgba};
use serde_json::{json, Value};

fn shift(layer: &str, by: i64) -> Op {
    ShiftLayer {
        layer: Id::new(layer),
        by: secs(by),
    }
    .into()
}

fn enable(layer: &str, enabled: bool) -> Op {
    SetLayerEnabled {
        layer: Id::new(layer),
        enabled,
    }
    .into()
}

fn times(p: &Project, layer: &str) -> Vec<i64> {
    let (_, l) = p.layer(&Id::new(layer)).unwrap();
    l.opacity
        .keyframes
        .iter()
        .map(|k| k.time.to_f64() as i64)
        .collect()
}

fn number(v: f64) -> Param {
    Param::constant(ParamValue::Number(v))
}

/// 省略できるパラメータを全部持たせる（再フレーミングとタイムリマップも）。
fn fill_optional(l: &mut Layer) {
    l.transform.anchor = Some(Param::constant(ParamValue::Vec2([0.0, 0.0])));
    l.crop = Crop {
        left: Some(number(1.0)),
        top: Some(number(1.0)),
        right: Some(number(1.0)),
        bottom: Some(number(1.0)),
    };
    l.masks = vec![Mask::rect("mk", MaskRect::new([0.0, 0.0], [8.0, 8.0], 1.0))];
    l.effects = vec![Effect {
        id: Id::new("fx"),
        node: Node::new("blur", 1).with_param("radius", ParamValue::Number(2.0)),
        enabled: true,
    }];
    l.source.inputs =
        vec![Node::new("solid", 1).with_param("color", ParamValue::Color(Rgba::WHITE))];
    l.audio = Some(LayerAudio::new("voice"));
    l.framing = Framing {
        cuts: vec![CutReframe {
            cut: Id::new("k1"),
            transform: Transform::default(),
        }],
        caption: Some(CaptionPlacement {
            position: Param::constant(ParamValue::Vec2([160.0, 170.0])),
            width: number(200.0),
        }),
    };
    let remap = |t| TimeKeyframe {
        time: secs(t),
        source: secs(t),
        interpolation: Interpolation::Linear,
    };
    l.time_remap = Some(TimeRemap {
        keyframes: vec![remap(0), remap(10)],
    });
}

/// パラメータ（`value` と `keyframes` を持つ JSON）を全部たどり、時刻 1 秒のキーフレームを 1 つずつ置く。
/// JSON でたどるので `Layer` に足したフィールドのパラメータにも入り、`shiftLayer` の動かし忘れが試験に出る。
fn key_all(v: &mut Value) {
    match v {
        Value::Object(o) => {
            if let (Some(value), Some(Value::Array(_))) =
                (o.get("value").cloned(), o.get("keyframes"))
            {
                let key = json!({"time": secs(1), "value": value, "interpolation": "hold"});
                o.insert("keyframes".into(), json!([key]));
            }
            o.values_mut().for_each(key_all);
        }
        Value::Array(a) => a.iter_mut().for_each(key_all),
        _ => {}
    }
}

/// 再フレーミングを除いた、パラメータのキーフレームの時刻の並び。
fn comp_key_times(layer: &Value) -> Vec<Value> {
    fn walk(v: &Value, out: &mut Vec<Value>) {
        match v {
            Value::Object(o) => {
                if let (Some(_), Some(Value::Array(keys))) = (o.get("value"), o.get("keyframes")) {
                    out.extend(keys.iter().map(|k| k["time"].clone()));
                }
                o.values().for_each(|c| walk(c, out));
            }
            Value::Array(a) => a.iter().for_each(|c| walk(c, out)),
            _ => {}
        }
    }
    let mut layer = layer.clone();
    layer["framing"]["cuts"] = json!([]);
    let mut out = Vec::new();
    walk(&layer, &mut out);
    out
}

#[test]
fn 始まりとキーフレームを一緒に動かし_イン点とアウト点は変えない() {
    let mut p = project();
    let (_, before) = p.layer(&Id::new("l2")).unwrap();
    let (in_point, out_point) = (before.in_point, before.out_point);
    shift("l2", 3).apply(&mut p, &OpContext::human()).unwrap();
    let (_, l) = p.layer(&Id::new("l2")).unwrap();
    assert_eq!(l.start, secs(4));
    assert_eq!((l.in_point, l.out_point), (in_point, out_point));
    assert_eq!(times(&p, "l2"), vec![3, 5]);
}

#[test]
fn コンポジションの時刻のキーフレームを全部動かし_ほかの時刻は変えない() {
    let mut p = project();
    let layer = p.layer_mut(&Id::new("l1")).unwrap();
    fill_optional(layer);
    let mut keyed = serde_json::to_value(&*layer).unwrap();
    key_all(&mut keyed);
    *layer = serde_json::from_value(keyed).unwrap();
    let keyed = serde_json::to_value(&*layer).unwrap();
    shift("l1", 2).apply(&mut p, &OpContext::human()).unwrap();
    let after = serde_json::to_value(p.layer(&Id::new("l1")).unwrap().1).unwrap();
    for field in ["in", "out", "time_remap"] {
        assert_eq!(after[field], keyed[field], "{field}");
    }
    assert_eq!(after["framing"]["cuts"], keyed["framing"]["cuts"]);
    let (before, moved) = (comp_key_times(&keyed), comp_key_times(&after));
    assert!(before.len() >= 20, "keyed params: {}", before.len());
    assert_eq!(moved.len(), before.len());
    assert!(moved.iter().all(|t| *t == json!(secs(3))), "{moved:?}");
}

/// `Layer::keyframe_times`（Timeline の帯のマーカー）が、`shiftLayer` の動かすパラメータだけを全部読む。
/// 再フレーミングも含めて全部のパラメータに別々の時刻を置き、JSON でたどった時刻の並びと突き合わせる。
#[test]
fn キーフレームの時刻の一覧は_shift_layer_が動かすパラメータと同じ() {
    /// パラメータのキーフレームに 101 秒から別々の時刻を置く（タイムリマップの 0 秒と 10 秒に重ねない）。
    fn distinct(v: &mut Value, n: &mut i64) {
        match v {
            Value::Object(o) => {
                let param = o.contains_key("value");
                if let (true, Some(Value::Array(keys))) = (param, o.get_mut("keyframes")) {
                    for k in keys {
                        *n += 1;
                        k["time"] = json!(secs(100 + *n));
                    }
                }
                o.values_mut().for_each(|c| distinct(c, n));
            }
            Value::Array(a) => a.iter_mut().for_each(|c| distinct(c, n)),
            _ => {}
        }
    }
    let mut p = project();
    let layer = p.layer_mut(&Id::new("l1")).unwrap();
    fill_optional(layer);
    let mut keyed = serde_json::to_value(&*layer).unwrap();
    key_all(&mut keyed);
    distinct(&mut keyed, &mut 0);
    let mut expected: Vec<RationalTime> = comp_key_times(&keyed)
        .into_iter()
        .map(|t| serde_json::from_value(t).unwrap())
        .collect();
    expected.sort();
    let layer: Layer = serde_json::from_value(keyed).unwrap();
    assert!(expected.len() >= 20, "keyed params: {}", expected.len());
    assert_eq!(layer.keyframe_times(), expected);
}

/// `Layer::comp_time_param_paths`（Timeline のキーフレームの行）のパスは、`ParamRef` で同じパラメータを指す。
/// パスで指せないのはノードの入力のノードのパラメータだけ。
#[test]
fn パラメータのパスは_param_ref_で同じパラメータを指す() {
    let mut p = project();
    let layer = p.layer_mut(&Id::new("l1")).unwrap();
    fill_optional(layer);
    let mut keyed = serde_json::to_value(&*layer).unwrap();
    key_all(&mut keyed);
    *layer = serde_json::from_value(keyed).unwrap();
    let paths: Vec<(String, Keyframe)> = layer
        .comp_time_param_paths()
        .into_iter()
        .map(|(path, param)| (path, param.keyframes[0].clone()))
        .collect();
    assert!(paths.len() >= 20, "keyed params: {}", paths.len());
    let inputs = layer.comp_time_params().len() - paths.len();
    assert_eq!(inputs, 1, "ノードの入力の color だけがパスの外");
    for (i, (path, key)) in (100..).zip(&paths) {
        let op: Op = MoveKeyframe {
            target: ParamRef::layer(&Id::new("l1"), path.clone()),
            from: key.time,
            keyframe: Keyframe {
                time: secs(i),
                ..key.clone()
            },
        }
        .into();
        let applied = op.apply(&mut p, &OpContext::human());
        assert!(applied.is_ok(), "{path}: {:?}", applied.err());
    }
    let (_, layer) = p.layer(&Id::new("l1")).unwrap();
    let moved: Vec<RationalTime> = layer
        .comp_time_param_paths()
        .into_iter()
        .map(|(_, param)| param.keyframes[0].time)
        .collect();
    let want: Vec<RationalTime> = (100..).take(paths.len()).map(secs).collect();
    assert_eq!(
        moved, want,
        "パスの順に動かした時刻が、同じ順のパラメータに入る"
    );
}

#[test]
fn 逆操作で元に戻り_触るのはレイヤーだけ() {
    let mut p = project();
    let before = canon(&p);
    let applied = shift("l2", 2).apply(&mut p, &OpContext::human()).unwrap();
    assert_eq!(applied.touches.to_vec(), vec!["l2".to_owned()]);
    applied.inverse.apply(&mut p, &OpContext::human()).unwrap();
    assert_eq!(canon(&p), before);
}

#[test]
fn 始まりが_0_より前になる動かし方も受け_逆操作で戻る() {
    let mut p = project();
    let before = canon(&p);
    let applied = shift("l2", -2).apply(&mut p, &OpContext::human()).unwrap();
    assert_eq!(p.layer(&Id::new("l2")).unwrap().1.start, secs(-1));
    assert_eq!(times(&p, "l2"), vec![-2, 0]);
    applied.inverse.apply(&mut p, &OpContext::human()).unwrap();
    assert_eq!(canon(&p), before);
}

#[test]
fn 始まりが_0_より前のレイヤーも動かせ_逆操作で戻る() {
    let mut p = project();
    let human = OpContext::human();
    let trim: Op = TrimLayer {
        layer: Id::new("l1"),
        edge: Edge::In,
        by: secs(-1),
    }
    .into();
    trim.apply(&mut p, &human).unwrap();
    let before = canon(&p);
    let applied = shift("l1", 2).apply(&mut p, &human).unwrap();
    assert_eq!(p.layer(&Id::new("l1")).unwrap().1.start, secs(1));
    applied.inverse.apply(&mut p, &human).unwrap();
    assert_eq!(canon(&p), before);
}

#[test]
fn 時刻の桁あふれは拒否し_何も変えない() {
    let mut p = project();
    let before = canon(&p);
    assert!(shift("l2", i64::MAX)
        .apply(&mut p, &OpContext::human())
        .is_err());
    assert_eq!(canon(&p), before);
}

#[test]
fn ロックしたレイヤーは_claude_から動かせない() {
    let mut p = project();
    let lock: Op = LockLayer {
        layer: Id::new("l2"),
        lock: true,
    }
    .into();
    lock.apply(&mut p, &OpContext::human()).unwrap();
    let claude = OpContext::claude();
    assert!(matches!(
        shift("l2", 1).apply(&mut p, &claude),
        Err(OpError::Locked(_))
    ));
    assert!(matches!(
        enable("l2", false).apply(&mut p, &claude),
        Err(OpError::Locked(_))
    ));
    shift("l2", 1).apply(&mut p, &OpContext::human()).unwrap();
}

#[test]
fn 有効を切り替え_逆操作が前の値を置く() {
    let mut p = project();
    let applied = enable("l1", false)
        .apply(&mut p, &OpContext::human())
        .unwrap();
    assert!(!p.layer(&Id::new("l1")).unwrap().1.enabled);
    assert_eq!(applied.touches.to_vec(), vec!["l1".to_owned()]);
    applied.inverse.apply(&mut p, &OpContext::human()).unwrap();
    assert!(p.layer(&Id::new("l1")).unwrap().1.enabled);
}

#[test]
fn 無いレイヤーは拒否する() {
    let mut p = project();
    assert!(shift("nope", 1).apply(&mut p, &OpContext::human()).is_err());
    assert!(enable("nope", false)
        .apply(&mut p, &OpContext::human())
        .is_err());
}
