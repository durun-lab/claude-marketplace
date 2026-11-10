//! プロジェクトのデータから Timeline のモデルを作る。

use std::sync::Arc;

use mm_doc::{
    Composition, Layer, Project, Sequence, SequenceId, FOOTAGE_KIND, NESTED_KIND, SEQUENCE_KIND,
    SEQUENCE_PARAM,
};
use mm_eval::{place_cuts, PlacedCut};
use mm_types::Id;
use mm_ui_layout::{AudioState, CutBar, Item, ItemKind, TimelineModel, TrimBounds};

use crate::names::Namer;

mod wave;

use wave::WaveResolver;

/// 音を持てるレイヤーの種類。Mixer のトラック（`apps/movie-maker` の `mixer.rs`）と同じで、見出しに Mute と Solo を出す。
const AUDIO_KINDS: [&str; 3] = [FOOTAGE_KIND, NESTED_KIND, SEQUENCE_KIND];

/// 音を持てるレイヤーの Mute と Solo。音の設定が無いレイヤーは既定（どちらも切）。音を持てないレイヤーは `None`。
fn audio_state(layer: &Layer) -> Option<AudioState> {
    AUDIO_KINDS.contains(&layer.source.kind.as_str()).then(|| {
        let audio = layer.audio.as_ref();
        AudioState {
            mute: audio.is_some_and(|a| a.mute),
            solo: audio.is_some_and(|a| a.solo),
        }
    })
}

fn kind_of(layer: &Layer) -> ItemKind {
    match layer.source.kind.as_str() {
        "footage" => ItemKind::Footage,
        "image" => ItemKind::Image,
        "solid" => ItemKind::Solid,
        "text" => ItemKind::Text,
        "composition" => ItemKind::Composition,
        SEQUENCE_KIND => ItemKind::Sequence,
        _ => ItemKind::Other,
    }
}

/// Timeline に並ぶレイヤーを、上が先頭の順に返す。時間軸が壊れたレイヤーはスキップする。
/// 並びの規則はモデルとレイヤーの ID の対応の両方が使う 1 つだけにする。
fn visible_layers(comp: &Composition) -> impl Iterator<Item = (&Layer, mm_types::TimeRange)> {
    comp.layers
        .iter()
        .rev()
        .filter_map(|layer| Some((layer, layer.range().ok()?)))
}

/// Timeline の `Item` と同じ並びのレイヤーの ID。Timeline の選択（添字）をレイヤーに引き直すのに使う。
pub fn timeline_layer_ids(comp: &Composition) -> Vec<String> {
    visible_layers(comp)
        .map(|(layer, _)| layer.id.to_string())
        .collect()
}

/// シーケンスのレイヤーが指すシーケンス。シーケンスのレイヤーでないか、シーケンスが無ければ `None`。
pub(crate) fn layer_sequence<'a>(project: &'a Project, layer: &Layer) -> Option<&'a Sequence> {
    if layer.source.kind != SEQUENCE_KIND {
        return None;
    }
    let id: SequenceId = Id::new(layer.source.text_param(SEQUENCE_PARAM).unwrap_or(""));
    project.sequence(&id).ok()
}

/// シーケンスのレイヤーが指すシーケンスと、有効なカットの配置。シーケンスのレイヤーでなければ `None`。
fn layer_cuts<'a>(project: &'a Project, layer: &Layer) -> Option<(&'a Sequence, Vec<PlacedCut>)> {
    let seq = layer_sequence(project, layer)?;
    Some((seq, place_cuts(seq).ok()?))
}

/// シーケンスの時刻を、レイヤーを通したコンポジションの時刻にする（`start + (t - in) / speed`）。
fn comp_secs(layer: &Layer, seq_secs: f64) -> f64 {
    layer.start.to_f64() + (seq_secs - layer.in_point.to_f64()) / layer.speed.to_f64()
}

/// Timeline の `Item` と同じ並びの、シーケンスのカットの ID。シーケンスでないレイヤーは空。
/// Timeline で選んだカット（添字）を操作の ID に引き直すのに使う。
pub fn timeline_cut_ids(project: &Project, comp: &Composition) -> Vec<Vec<String>> {
    visible_layers(comp)
        .map(|(layer, _)| {
            layer_cuts(project, layer)
                .map(|(_, placed)| placed.into_iter().map(|p| p.id.to_string()).collect())
                .unwrap_or_default()
        })
        .collect()
}

/// コンポジションのレイヤーを Timeline に並べる。上のレイヤーが先頭。時間軸が壊れたレイヤーはスキップする。
/// シーケンスのレイヤーには、有効なカットを先頭から並べる（隙間は空きとして区別する）。
pub fn timeline_model(project: &Project, comp: &Composition) -> TimelineModel {
    let names = Namer::new(project);
    let mut waves = WaveResolver::new(project);
    let items = visible_layers(comp)
        .map(|(layer, range)| Item {
            id: layer.id.as_str().into(),
            start: range.start.to_f64(),
            end: range.end.to_f64(),
            kind: kind_of(layer),
            enabled: layer.enabled,
            locked: layer.lock,
            shy: false,
            audio: audio_state(layer),
            label: names.label(&layer.source),
            keyframes: keyframe_secs(layer),
            cuts: cut_bars(project, layer, &names, &mut waves),
            trim: trim_bounds(project, layer, range),
            wave: waves.layer(layer),
        })
        .collect();
    TimelineModel {
        duration: comp.duration.to_f64(),
        fps: comp.fps.to_f64(),
        items,
        markers: comp.markers.iter().map(|m| m.time.to_f64()).collect(),
        ..TimelineModel::default()
    }
}

/// レイヤーのキーフレームの時刻（コンポジションの秒、前から順）。`shiftLayer` が動かすパラメータと同じ一覧から引く。
fn keyframe_secs(layer: &Layer) -> Arc<[f64]> {
    layer
        .keyframe_times()
        .into_iter()
        .map(|t| t.to_f64())
        .collect()
}

/// 端のつまみで動かせる範囲。`trimLayer` の検査と同じ素材の余白（`Layer::trim_room`）から作る。
/// 素材が引けないレイヤーは長さで止めず、離したときの操作の拒否に任せる。
fn trim_bounds(project: &Project, layer: &Layer, range: mm_types::TimeRange) -> TrimBounds {
    let Ok(room) = layer.trim_room(project) else {
        return TrimBounds {
            fixed_in: layer.time_remap.is_some(),
            ..TrimBounds::FREE
        };
    };
    TrimBounds {
        min_start: room.head.map_or(f64::NEG_INFINITY, |head| {
            range.start.to_f64() - head.to_f64()
        }),
        max_end: room
            .tail
            .map_or(f64::INFINITY, |tail| range.end.to_f64() + tail.to_f64()),
        fixed_in: room.fixed_in,
    }
}

fn cut_bars(
    project: &Project,
    layer: &Layer,
    names: &Namer,
    waves: &mut WaveResolver<'_>,
) -> Vec<CutBar> {
    let Some((seq, placed)) = layer_cuts(project, layer) else {
        return Vec::new();
    };
    placed
        .iter()
        .map(|p| CutBar {
            start: comp_secs(layer, p.range.start.to_f64()),
            end: comp_secs(layer, p.range.end.to_f64()),
            label: names.label(&seq.cuts[p.index].source),
            gap: seq.cuts[p.index].source.is_gap(),
            disabled_before: p.index.checked_sub(1).is_some_and(|i| !seq.cuts[i].enabled),
            disabled_after: seq.cuts.get(p.index + 1).is_some_and(|c| !c.enabled),
            wave: waves.cut(layer, &seq.cuts[p.index], p.range.start.to_f64()),
        })
        .collect()
}

#[cfg(test)]
mod tests {
    use super::*;
    use mm_doc::Node;
    use mm_types::{Rational, RationalTime};

    fn layer(id: &str, kind: &str, start: i64, in_s: i64, out_s: i64) -> Layer {
        Layer::new(
            id,
            Node::new(kind, 1),
            RationalTime::seconds(start),
            RationalTime::seconds(in_s),
            RationalTime::seconds(out_s),
        )
    }

    fn comp() -> Composition {
        Composition::new(
            mm_types::Id::new("c"),
            64,
            36,
            Rational::new(30, 1).unwrap(),
            RationalTime::seconds(10),
        )
        .with_layers(vec![
            layer("bottom", "footage", 0, 0, 4),
            layer("top", "text", 2, 0, 5),
        ])
    }

    #[test]
    fn 上のレイヤーが先頭に来て_範囲は始まりから長さ() {
        let m = timeline_model(&Project::new(), &comp());
        assert_eq!(m.items.len(), 2);
        assert_eq!(m.items[0].kind, ItemKind::Text);
        assert_eq!((m.items[0].start, m.items[0].end), (2.0, 7.0));
        assert_eq!(m.items[1].kind, ItemKind::Footage);
        assert_eq!((m.duration, m.fps), (10.0, 30.0));
    }

    #[test]
    fn マーカーの時刻をスナップの目標に渡す() {
        let mut c = comp();
        c.markers = vec![mm_doc::Marker {
            id: mm_types::Id::new("m1"),
            time: RationalTime::new(5, 2).unwrap(),
            duration: RationalTime::zero(),
            name: "Chapter".into(),
        }];
        assert_eq!(timeline_model(&Project::new(), &c).markers, [2.5]);
    }

    #[test]
    fn 選択の添字はモデルと同じ並びでレイヤーの_id_に引ける() {
        let mut c = comp();
        // 時間軸が壊れた（出が入より前の）レイヤーはモデルからスキップし、添字がずれない。
        c.layers.insert(1, layer("broken", "text", 0, 5, 1));
        let ids = timeline_layer_ids(&c);
        assert_eq!(ids, ["top", "bottom"]);
        let model = timeline_model(&Project::new(), &c);
        let in_model: Vec<&str> = model.items.iter().map(|i| &*i.id).collect();
        assert_eq!(in_model, ids, "行の高さをレイヤーごとに引く ID");
    }

    #[test]
    fn シーケンスのレイヤーはカットを並べ_隙間に印を付ける() {
        use mm_doc::Cut;
        let cut = |id: &str, node: Node, a: i64, b: i64| {
            Cut::new(id, node, RationalTime::seconds(a), RationalTime::seconds(b))
        };
        let mut project = Project::new();
        project.sequences.push(Sequence {
            id: Id::new("s"),
            cuts: vec![
                cut("k1", Node::footage(&Id::new("a")), 0, 3),
                cut("k2", Node::gap(), 0, 1),
                cut("k3", Node::footage(&Id::new("a")), 5, 7),
            ],
            origin: None,
        });
        project.sequences[0].cuts[2].enabled = false;
        let mut seq = layer("seq", SEQUENCE_KIND, 1, 0, 9);
        seq.source = Node::new(SEQUENCE_KIND, 1)
            .with_param(SEQUENCE_PARAM, mm_types::ParamValue::Text("s".into()));
        let mut c = comp();
        c.layers.push(seq);
        let m = timeline_model(&project, &c);
        let item = &m.items[0];
        assert_eq!(item.kind, ItemKind::Sequence);
        let bars: Vec<_> = item.cuts.iter().map(|b| (b.start, b.end, b.gap)).collect();
        assert_eq!(
            bars,
            [(1.0, 4.0, false), (4.0, 5.0, true)],
            "レイヤーの開始分ずらし、無効なカットは出さない"
        );
        assert_eq!(timeline_cut_ids(&project, &c)[0], ["k1", "k2"]);
        assert!(timeline_cut_ids(&project, &c)[1].is_empty());
        let labels: Vec<&str> = item.cuts.iter().map(|b| &*b.label).collect();
        assert_eq!(
            labels,
            ["a", ""],
            "カットの名前は素材の ID（素材が無い）。隙間は空"
        );
        assert_eq!(&*item.label, "s");
    }

    /// k1、無効な k2、k3、k4 の順に並べたシーケンスのレイヤーを持つコンポジション。
    fn disabled_between() -> (Project, Composition) {
        use mm_doc::Cut;
        let cut = |id: &str, a: i64, b: i64| {
            Cut::new(
                id,
                Node::footage(&Id::new("a")),
                RationalTime::seconds(a),
                RationalTime::seconds(b),
            )
        };
        let mut project = Project::new();
        project.sequences.push(Sequence {
            id: Id::new("s"),
            cuts: vec![
                cut("k1", 0, 3),
                cut("k2", 0, 2),
                cut("k3", 0, 3),
                cut("k4", 0, 3),
            ],
            origin: None,
        });
        project.sequences[0].cuts[1].enabled = false;
        let mut seq = layer("seq", SEQUENCE_KIND, 0, 0, 9);
        seq.source = Node::new(SEQUENCE_KIND, 1)
            .with_param(SEQUENCE_PARAM, mm_types::ParamValue::Text("s".into()));
        let mut c = comp();
        c.layers.push(seq);
        (project, c)
    }

    #[test]
    fn 隣に無効なカットがあると_roll_と_slide_は出さない() {
        use mm_ui_layout::{body_edit, edge_edit, Mods, Part};
        let (project, c) = disabled_between();
        let m = timeline_model(&project, &c);
        let alt = Mods {
            shift: false,
            alt: true,
        };
        let shift = Mods {
            shift: true,
            alt: false,
        };
        // 見えるカットは k1(0)、k3(1)、k4(2)。mm-ops の roll と slide は seq.cuts の隣（無効な k2）を動かす。
        assert!(edge_edit(&m, 0, Some(0), Part::OutHandle, alt, 2.5).is_none());
        assert!(edge_edit(&m, 0, Some(1), Part::InHandle, alt, 3.5).is_none());
        assert!(body_edit(&m, 0, 1, shift, 0.5).is_none());
        assert!(edge_edit(&m, 0, Some(1), Part::OutHandle, alt, 5.5).is_some());
        assert!(body_edit(&m, 0, 1, Mods::default(), 0.5).is_some());
    }

    #[test]
    fn 撮影素材のつまみは素材の先頭と終わりまで_素材の長さが無いレイヤーは縛らない() {
        let mut project = Project::new();
        project.assets.push(mm_doc::Asset {
            id: Id::new("a"),
            path: "a.mp4".into(),
            content_hash: "hash-a".into(),
            color_space: mm_types::ColorSpace::bt709(),
            duration: RationalTime::seconds(20),
            fps: Some(Rational::integer(30)),
            timebase_offset: RationalTime::ZERO,
        });
        // 2 倍速で素材の 4 秒から 10 秒を使う帯 [3, 6)。先頭まで 2 秒、終わりまで 5 秒（コンポジションの秒）。
        let mut fast = layer("fast", "footage", 3, 4, 10);
        fast.source = Node::footage(&Id::new("a"));
        fast.speed = Rational::integer(2);
        let mut c = comp();
        c.layers = vec![fast, layer("text", "text", 0, 0, 5)];
        let m = timeline_model(&project, &c);
        let trim = m.items[1].trim;
        assert_eq!((trim.min_start, trim.max_end), (1.0, 11.0));
        assert!(!trim.fixed_in);
        assert_eq!(m.items[0].trim, TrimBounds::FREE);
    }

    #[test]
    fn キーフレームの時刻は全部のパラメータから集め_重複なしで前から順() {
        use mm_doc::{Interpolation, Keyframe};
        use mm_types::ParamValue;
        let key = |s: i64| {
            Keyframe::new(
                RationalTime::seconds(s),
                ParamValue::Number(1.0),
                Interpolation::Linear,
            )
        };
        let mut c = comp();
        let top = c
            .layers
            .iter_mut()
            .find(|l| l.id.as_str() == "top")
            .unwrap();
        top.opacity.keyframes = vec![key(2), key(4)];
        top.transform.rotation.keyframes = vec![key(2), key(3)];
        let m = timeline_model(&Project::new(), &c);
        assert_eq!(&*m.items[0].keyframes, [2.0, 3.0, 4.0]);
        assert!(m.items[0].has_keyframes() && !m.items[1].has_keyframes());
    }

    #[test]
    fn 音を持てるレイヤーは音の設定が無くても_mute_と_solo_を持ち_ロックを写す() {
        let mut talk = layer("talk", "footage", 0, 0, 4);
        talk.audio = Some(mm_doc::LayerAudio {
            solo: true,
            ..mm_doc::LayerAudio::new("voice")
        });
        talk.lock = true;
        let c = comp().with_layers(vec![
            talk,
            layer("seq", SEQUENCE_KIND, 0, 0, 4),
            layer("bg", "solid", 0, 0, 4),
        ]);
        let m = timeline_model(&Project::new(), &c);
        let audio: Vec<_> = m.items.iter().map(|i| i.audio).collect();
        let off = AudioState::default();
        let solo = AudioState { solo: true, ..off };
        assert_eq!(
            audio,
            [None, Some(off), Some(solo)],
            "上から単色、sequence、撮影素材"
        );
        let locked: Vec<bool> = m.items.iter().map(|i| i.locked).collect();
        assert_eq!(locked, [false, false, true]);
    }

    #[test]
    fn 知らない種類は_other() {
        let mut c = comp();
        c.layers.push(layer("x", "web.cef", 0, 0, 1));
        assert_eq!(
            timeline_model(&Project::new(), &c).items[0].kind,
            ItemKind::Other
        );
    }
}
