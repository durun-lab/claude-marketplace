//! 見出しのボタンと見出しのドラッグが出す操作（`setLayerEnabled`、`lockLayer`、`setLayerAudio`、`moveLayer`）の試験。

use mm_doc::{Composition, Fade, Layer, Node, Param, Project};
use mm_ops::{apply_transaction, Entry, OpContext};
use mm_types::{Id, ParamValue, Rational, RationalTime};

use super::*;

/// 下から l0、l1、l2、l3 の 4 レイヤーを持つコンポジション `main`。Timeline の行は上から l3、l2、l1、l0。
fn stacked() -> Project {
    let secs = RationalTime::seconds;
    let layer =
        |id: &str, kind: &str| Layer::new(id, Node::new(kind, 1), secs(0), secs(0), secs(5));
    let mut p = Project::new();
    p.compositions.push(
        Composition::new(Id::new("main"), 64, 36, Rational::integer(30), secs(5)).with_layers(
            vec![
                layer("l0", "footage"),
                layer("l1", "solid"),
                layer("l2", "solid"),
                layer("l3", "solid"),
            ],
        ),
    );
    p
}

fn apply(p: &mut Project, op: Op) {
    apply_transaction(p, &[op], &OpContext::new(Entry::WebView)).expect("適用できる");
}

fn order(p: &Project) -> Vec<&str> {
    p.compositions[0]
        .layers
        .iter()
        .map(|l| l.id.as_str())
        .collect()
}

fn ids(rows: &[&str]) -> Vec<String> {
    rows.iter().map(|s| (*s).to_owned()).collect()
}

/// Timeline の行 `rows`（上から）で、行 `from` を境目 `slot` に動かして適用した後の、下からの並び。
fn moved(rows: &[&str], from: usize, slot: usize) -> Option<Vec<String>> {
    let mut p = stacked();
    let rows = ids(rows);
    let ids = TimelineIds {
        layers: &rows,
        cuts: &[],
    };
    let op = move_layer_op(from, slot, &ids, &p.compositions[0])?;
    apply(&mut p, op);
    Some(order(&p).into_iter().map(str::to_owned).collect())
}

const ALL_ROWS: [&str; 4] = ["l3", "l2", "l1", "l0"];

#[test]
fn 一番下の行を先頭の境目へ動かすと_一番前のレイヤーになる() {
    let after = moved(&ALL_ROWS, 3, 0).unwrap();
    assert_eq!(after, ["l1", "l2", "l3", "l0"]);
}

#[test]
fn 先頭の行を行の間の境目へ動かすと_その境目の上の行のすぐ後ろに入る() {
    let after = moved(&ALL_ROWS, 0, 3).unwrap();
    assert_eq!(after, ["l0", "l3", "l1", "l2"]);
    let end = moved(&ALL_ROWS, 0, 4).unwrap();
    assert_eq!(end, ["l3", "l0", "l1", "l2"], "末尾の境目は一番後ろ");
}

#[test]
fn 自分の上か下の境目は並びが変わらないので操作にしない() {
    assert_eq!(moved(&ALL_ROWS, 1, 1), None);
    assert_eq!(moved(&ALL_ROWS, 1, 2), None);
}

#[test]
fn 隠した行があっても_見えている行の境目を基準に入れる() {
    // l2 を Shy で隠した Timeline。l3 を l1 と l0 の間へ動かす。
    let after = moved(&["l3", "l1", "l0"], 0, 2).unwrap();
    assert_eq!(after, ["l0", "l3", "l1", "l2"]);
}

#[test]
fn 有効とロックのボタンは今の状態を反転する() {
    let mut p = stacked();
    let layer = p.compositions[0].layers[1].clone();
    apply(&mut p, header_button_op(HeaderButton::Enabled, &layer));
    assert!(!p.compositions[0].layers[1].enabled);
    apply(&mut p, header_button_op(HeaderButton::Lock, &layer));
    assert!(p.compositions[0].layers[1].lock);
    let again = p.compositions[0].layers[1].clone();
    apply(&mut p, header_button_op(HeaderButton::Enabled, &again));
    assert!(p.compositions[0].layers[1].enabled, "もう一度押すと戻る");
}

#[test]
fn mute_と_solo_は_mixer_と同じ_音の設定の全体を置く() {
    let mut p = stacked();
    let layer = p.compositions[0].layers[0].clone();
    assert!(layer.audio.is_none());
    apply(&mut p, header_button_op(HeaderButton::Mute, &layer));
    let audio = p.compositions[0].layers[0].audio.clone().unwrap();
    assert_eq!(
        audio,
        LayerAudio {
            mute: true,
            ..LayerAudio::new(DEFAULT_AUDIO_ROLE)
        }
    );
    let muted = p.compositions[0].layers[0].clone();
    apply(&mut p, header_button_op(HeaderButton::Solo, &muted));
    let audio = p.compositions[0].layers[0].audio.clone().unwrap();
    assert!(audio.mute && audio.solo, "Mute を残したまま Solo が入る");
    let both = p.compositions[0].layers[0].clone();
    apply(&mut p, header_button_op(HeaderButton::Mute, &both));
    let audio = p.compositions[0].layers[0].audio.clone().unwrap();
    assert!(!audio.mute && audio.solo);
}

/// Mixer が送るのと同じ形の操作。送る直前の l0 の音の設定に `change` を重ねて全体を置く（`ui/panels/mixer` の `trackOp`）。
fn mixer_op(p: &Project, change: impl FnOnce(&mut LayerAudio)) -> Op {
    let layer = &p.compositions[0].layers[0];
    let mut audio = layer.audio.clone();
    let audio_mut = audio.get_or_insert_with(|| LayerAudio::new(DEFAULT_AUDIO_ROLE));
    change(audio_mut);
    SetLayerAudio {
        layer: layer.id.clone(),
        audio,
    }
    .into()
}

#[test]
fn mixer_と見出しから交互に変えても_もう一方が変えた値を残す() {
    let mut p = stacked();
    let gain = Param::constant(ParamValue::Number(-6.0));
    let op = mixer_op(&p, |a| {
        a.role = "bgm".into();
        a.gain_db = gain.clone();
        a.fade_in = Some(Fade::linear(RationalTime::seconds(1)));
    });
    apply(&mut p, op);
    let layer = p.compositions[0].layers[0].clone();
    apply(&mut p, header_button_op(HeaderButton::Mute, &layer));
    let op = mixer_op(&p, |a| a.solo = !a.solo);
    apply(&mut p, op);
    let layer = p.compositions[0].layers[0].clone();
    apply(&mut p, header_button_op(HeaderButton::Mute, &layer));
    let audio = p.compositions[0].layers[0].audio.clone().unwrap();
    assert_eq!((audio.mute, audio.solo), (false, true));
    assert_eq!((audio.role.as_str(), &audio.gain_db), ("bgm", &gain));
    assert!(audio.fade_in.is_some(), "Mixer の外の設定も残す");
}
