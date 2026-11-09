//! レイヤーの時間の操作 `shiftLayer` と `setLayerEnabled`。

use crate::common;

use common::{canon, project, secs};
use mm_doc::{Interpolation, Keyframe, Param, Project};
use mm_ops::{LockLayer, Op, OpContext, OpError, SetLayerEnabled, ShiftLayer};
use mm_types::{Id, ParamValue};

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

#[test]
fn 開始とキーフレームを一緒に動かし_素材のイン点とアウト点は変えない() {
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
fn 変形とノードのパラメータのキーフレームも動かす() {
    let mut p = project();
    let key = |t| Keyframe::new(secs(t), ParamValue::Number(1.0), Interpolation::Linear);
    let l = p.layer_mut(&Id::new("l1")).unwrap();
    l.transform.rotation.keyframes = vec![key(1)];
    let mut size = Param::constant(ParamValue::Number(0.0));
    size.keyframes = vec![key(2)];
    l.source.params.insert("size".into(), size);
    shift("l1", 5).apply(&mut p, &OpContext::human()).unwrap();
    let (_, l) = p.layer(&Id::new("l1")).unwrap();
    assert_eq!(l.transform.rotation.keyframes[0].time, secs(6));
    assert_eq!(l.source.params["size"].keyframes[0].time, secs(7));
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
fn 開始が負になる動かし方は拒否し_何も変えない() {
    let mut p = project();
    let before = canon(&p);
    let err = shift("l2", -2)
        .apply(&mut p, &OpContext::human())
        .unwrap_err();
    assert!(err.to_string().contains("l2"), "{err}");
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
