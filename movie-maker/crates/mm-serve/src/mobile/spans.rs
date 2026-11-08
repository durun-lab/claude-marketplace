//! 提案の範囲（`scope.sequences` の秒はシーケンスの時間軸）を、そのシーケンスを置いたレイヤーの
//! コンポジションの時刻に変換する。モバイル版は静止画のフレーム（`GET /m/api/frame?comp=&t=`）とスクラブバーをコンポジションの
//! 時間軸で持つので、提案の応答に `comp_spans` として足す。

use mm_doc::{Composition, Layer, Project};
use mm_ops::{LayerRef, ProposalScope, SequenceSpan};
use mm_story::layer_sequence;
use serde::Serialize;
use serde_json::Value;

use super::cuts::Placement;
use crate::core::{Core, ServeError};
use crate::review::ProposalView;

/// 提案の範囲 1 つの、コンポジションの上の位置。1 つのシーケンスを複数のレイヤーが置けば、その数だけ並ぶ。
#[derive(Debug, Clone, PartialEq, Serialize)]
pub(super) struct CompSpan {
    pub comp: String,
    pub layer: String,
    pub sequence: String,
    /// コンポジションの時間軸の秒。レイヤーの見えている区間に切り詰める。
    pub start: f64,
    pub end: f64,
}

/// `scope` の範囲を、今のプロジェクトのデータのレイヤーの置き方で変換する。変換できない範囲（時間の写像を持つ
/// レイヤーだけが置く、レイヤーの見えている区間の外）は入れない。
pub(super) fn comp_spans(project: &Project, scope: &ProposalScope) -> Vec<CompSpan> {
    project
        .compositions
        .iter()
        .flat_map(|comp| comp.layers.iter().map(move |layer| (comp, layer)))
        .filter_map(|(comp, layer)| {
            let seq = layer_sequence(project, layer)?;
            Some((comp, layer, seq.id.as_str(), Placement::of(layer)?))
        })
        .flat_map(|(comp, layer, seq, place)| {
            scope
                .sequences
                .iter()
                .filter(move |s| s.id == seq)
                .filter_map(move |s| on_layer(comp, layer, place, s))
        })
        .collect()
}

fn on_layer(
    comp: &Composition,
    layer: &Layer,
    place: Placement,
    span: &SequenceSpan,
) -> Option<CompSpan> {
    let (first, last) = place.visible(layer);
    let start = place.to_comp(span.start).max(first);
    let end = place.to_comp(span.end).min(last);
    (start <= end).then(|| CompSpan {
        comp: comp.id.as_str().to_owned(),
        layer: layer.id.as_str().to_owned(),
        sequence: span.id.clone(),
        start,
        end,
    })
}

/// 提案の区間の短い動画の範囲（コンポジション、始まりと終わりの秒）。`comp`（省くと最初に見つかったコンポジション）の上の、
/// `comp_spans` と触るレイヤーの見えている区間を全部含む範囲。どちらも無ければ `None`。
/// 触るレイヤーは保存したデータ（`saved`）と提案を適用したもの（`applied`）の両方で引き、提案が足すレイヤーと動かすレイヤーの行き先も含める。
pub(super) fn clip_span(
    saved: &Project,
    applied: &Project,
    scope: &ProposalScope,
    comp: Option<&str>,
) -> Option<(String, f64, f64)> {
    let spans = comp_spans(saved, scope)
        .into_iter()
        .map(|s| (s.comp, s.start, s.end));
    let layers = [saved, applied]
        .into_iter()
        .flat_map(|p| scope.layers.iter().filter_map(move |r| layer_span(p, r)));
    let all: Vec<(String, f64, f64)> = spans.chain(layers).collect();
    let want = comp.map_or_else(|| all.first().map(|s| s.0.clone()), |c| Some(c.to_owned()))?;
    all.into_iter()
        .filter(|s| s.0 == want)
        .map(|s| (s.1, s.2))
        .reduce(|a, b| (a.0.min(b.0), a.1.max(b.1)))
        .map(|(start, end)| (want, start, end))
}

/// `project` の中のレイヤー `r` の区間（コンポジション、始まりと終わりの秒）。無いレイヤーは `None`。
fn layer_span(project: &Project, r: &LayerRef) -> Option<(String, f64, f64)> {
    let range = project
        .compositions
        .iter()
        .find(|c| c.id.as_str() == r.comp)?
        .layers
        .iter()
        .find(|l| l.id.as_str() == r.id)?
        .range()
        .ok()?;
    Some((r.comp.clone(), range.start.to_f64(), range.end.to_f64()))
}

/// 提案の応答の JSON に `comp_spans` を足す。
pub(super) fn with_comp_spans(project: &Project, view: &ProposalView) -> Result<Value, ServeError> {
    let spans = comp_spans(project, &view.scope);
    let mut value = serde_json::to_value(view).map_err(internal)?;
    // `ProposalView` は構造体なので JSON はオブジェクトになり、キーを足せる。
    value["comp_spans"] = serde_json::to_value(spans).map_err(internal)?;
    Ok(value)
}

/// 今のプロジェクトのデータを読んで `with_comp_spans` する（提案 1 つの応答の用）。
pub(super) fn view_json(core: &Core, view: &ProposalView) -> Result<Value, ServeError> {
    with_comp_spans(&core.project()?.project, view)
}

fn internal(e: serde_json::Error) -> ServeError {
    ServeError::Internal(format!("提案を JSON にできない: {e}"))
}

#[cfg(test)]
mod tests {
    use super::*;
    use mm_doc::{Cut, Node, Sequence, SEQUENCE_KIND, SEQUENCE_PARAM};
    use mm_types::{Id, ParamValue, Rational, RationalTime};

    /// 10 秒のシーケンスの 2〜10 秒を、コンポジションの 5 秒から 2 倍速で置く（見えるのは 5〜9 秒）。
    fn project() -> Project {
        let cut = Cut::new(
            "c",
            Node::new("footage", 1),
            RationalTime::ZERO,
            RationalTime::seconds(10),
        );
        let mut p = Project::new();
        p.sequences.push(Sequence {
            id: Id::new("s"),
            cuts: vec![cut],
            origin: None,
        });
        let node =
            Node::new(SEQUENCE_KIND, 1).with_param(SEQUENCE_PARAM, ParamValue::Text("s".into()));
        let mut layer = Layer::new(
            "l",
            node,
            RationalTime::seconds(5),
            RationalTime::seconds(2),
            RationalTime::seconds(10),
        );
        layer.speed = Rational::integer(2);
        p.compositions.push(
            Composition::new(
                Id::new("main"),
                1280,
                720,
                Rational::integer(30),
                RationalTime::seconds(12),
            )
            .with_layers(vec![layer]),
        );
        p
    }

    fn scope(start: f64, end: f64) -> ProposalScope {
        ProposalScope {
            sequences: vec![SequenceSpan {
                id: "s".into(),
                cuts: vec!["c".into()],
                start,
                end,
            }],
            ..ProposalScope::default()
        }
    }

    #[test]
    fn シーケンスの秒をレイヤーの始まりと速さでコンポジションの秒に変換する() {
        let spans = comp_spans(&project(), &scope(4.0, 6.0));
        assert_eq!(spans.len(), 1);
        assert_eq!(
            (spans[0].comp.as_str(), spans[0].layer.as_str()),
            ("main", "l")
        );
        assert_eq!((spans[0].start, spans[0].end), (6.0, 7.0));
    }

    #[test]
    fn レイヤーの見えている区間に切り詰め_外の範囲は入れない() {
        let spans = comp_spans(&project(), &scope(0.0, 12.0));
        assert_eq!((spans[0].start, spans[0].end), (5.0, 9.0));
        assert!(comp_spans(&project(), &scope(0.0, 1.0)).is_empty());
    }

    #[test]
    fn 短い動画の範囲はシーケンスの範囲と触るレイヤーの区間を合わせる() {
        let p = project();
        assert_eq!(
            clip_span(&p, &p, &scope(4.0, 6.0), None),
            Some(("main".into(), 6.0, 7.0))
        );
        let mut both = scope(4.0, 6.0);
        both.layers.push(layer_ref("l"));
        // レイヤー l は 5〜9 秒に見える。
        assert_eq!(
            clip_span(&p, &p, &both, None),
            Some(("main".into(), 5.0, 9.0))
        );
        assert_eq!(clip_span(&p, &p, &both, Some("other")), None);
        assert_eq!(clip_span(&p, &p, &ProposalScope::default(), None), None);
    }

    fn layer_ref(id: &str) -> LayerRef {
        LayerRef {
            comp: "main".into(),
            id: id.into(),
        }
    }

    #[test]
    fn 短い動画の範囲は提案が足すレイヤーと動かしたレイヤーの行き先も含める() {
        let saved = project();
        let mut applied = saved.clone();
        let mut added = applied.compositions[0].layers[0].clone();
        added.id = Id::new("new");
        added.start = RationalTime::seconds(1);
        applied.compositions[0].layers.push(added);
        let adds = ProposalScope {
            layers: vec![layer_ref("new")],
            ..ProposalScope::default()
        };
        // 足したレイヤー new は 1〜5 秒に見える。保存したデータには無い。
        assert_eq!(
            clip_span(&saved, &applied, &adds, None),
            Some(("main".into(), 1.0, 5.0))
        );
        applied.compositions[0].layers[0].start = RationalTime::seconds(7);
        let moves = ProposalScope {
            layers: vec![layer_ref("l")],
            ..ProposalScope::default()
        };
        // l は 5〜9 秒から 7〜11 秒へ動く。
        assert_eq!(
            clip_span(&saved, &applied, &moves, None),
            Some(("main".into(), 5.0, 11.0))
        );
    }

    #[test]
    fn 時間の写像を持つレイヤーは変換しない() {
        let mut p = project();
        p.compositions[0].layers[0].time_remap = Some(mm_doc::TimeRemap { keyframes: vec![] });
        assert!(comp_spans(&p, &scope(4.0, 6.0)).is_empty());
    }
}
