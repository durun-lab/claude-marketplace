//! スクラブバーの目盛りに使う、コンポジションのカットの境の秒。シーケンスの時刻をコンポジションの時刻に変換する
//! [`Placement`] は、提案の範囲の変換（`spans`）と共有する。

use mm_doc::{Composition, Layer, Project};
use mm_eval::place_cuts;
use mm_story::layer_sequence;

/// シーケンスを置いたレイヤーの、シーケンスの時刻からコンポジションの時刻への変換。
#[derive(Debug, Clone, Copy)]
pub(super) struct Placement {
    start: f64,
    offset: f64,
    speed: f64,
}

impl Placement {
    /// 時間の写像を持つレイヤーと逆再生のレイヤーは、シーケンスの時刻が一意に戻らないので `None`。
    pub(super) fn of(layer: &Layer) -> Option<Self> {
        let speed = layer.speed.to_f64();
        (layer.time_remap.is_none() && speed > 0.0).then(|| Self {
            start: layer.start.to_f64(),
            offset: layer.in_point.to_f64(),
            speed,
        })
    }

    /// シーケンスの時刻 `t`（秒）のコンポジションの時刻。
    pub(super) fn to_comp(self, t: f64) -> f64 {
        self.start + (t - self.offset) / self.speed
    }

    /// レイヤーが見えている区間（コンポジションの時刻の始まりと終わり）。
    pub(super) fn visible(self, layer: &Layer) -> (f64, f64) {
        (self.start, self.to_comp(layer.out_point.to_f64()))
    }
}

/// `comp` のシーケンスのレイヤーが並べるカットの境（コンポジションの時間軸の秒、昇順、重複なし）。
/// 0 とコンポジションの終わりは境に数えない。
pub(super) fn boundaries(project: &Project, comp: &Composition) -> Vec<f64> {
    let end = comp.duration.to_f64();
    let mut out: Vec<f64> = Vec::new();
    for layer in &comp.layers {
        let (Some(seq), Some(place)) = (layer_sequence(project, layer), Placement::of(layer))
        else {
            continue;
        };
        let Ok(placed) = place_cuts(seq) else {
            continue;
        };
        let (first, last) = place.visible(layer);
        out.extend(
            placed
                .iter()
                .flat_map(|p| [p.range.start, p.range.end])
                .map(|t| place.to_comp(t.to_f64()))
                .filter(|t| *t > first && *t < last && *t > 0.0 && *t < end),
        );
    }
    out.sort_by(f64::total_cmp);
    out.dedup_by(|a, b| (*a - *b).abs() < 1e-9);
    out
}

#[cfg(test)]
mod tests {
    use super::*;
    use mm_doc::{Cut, Layer, Node, Sequence, SEQUENCE_KIND, SEQUENCE_PARAM};
    use mm_types::{Id, ParamValue, Rational, RationalTime};

    fn project(cut_seconds: &[i64], layer_start: i64) -> Project {
        let footage = Node::new("footage", 1);
        let cuts = cut_seconds
            .iter()
            .enumerate()
            .map(|(i, s)| {
                Cut::new(
                    format!("c{i}"),
                    footage.clone(),
                    RationalTime::ZERO,
                    RationalTime::seconds(*s),
                )
            })
            .collect();
        let total: i64 = cut_seconds.iter().sum();
        let mut p = Project::new();
        p.sequences.push(Sequence {
            id: Id::new("s"),
            cuts,
            origin: None,
        });
        let node =
            Node::new(SEQUENCE_KIND, 1).with_param(SEQUENCE_PARAM, ParamValue::Text("s".into()));
        let layer = Layer::new(
            "l",
            node,
            RationalTime::seconds(layer_start),
            RationalTime::ZERO,
            RationalTime::seconds(total),
        );
        p.compositions.push(
            Composition::new(
                Id::new("main"),
                1280,
                720,
                Rational::integer(30),
                RationalTime::seconds(layer_start + total),
            )
            .with_layers(vec![layer]),
        );
        p
    }

    #[test]
    fn カットの境はレイヤーの始まりを足した秒で昇順に並ぶ() {
        let p = project(&[3, 2, 4], 1);
        assert_eq!(boundaries(&p, &p.compositions[0]), vec![4.0, 6.0]);
    }

    #[test]
    fn 外したカットは境に数えない() {
        let mut p = project(&[3, 2, 4], 0);
        p.sequences[0].cuts[1].enabled = false;
        p.compositions[0].layers[0].out_point = RationalTime::seconds(7);
        assert_eq!(boundaries(&p, &p.compositions[0]), vec![3.0]);
    }

    #[test]
    fn カットが_1_つなら境は無い() {
        let p = project(&[5], 0);
        assert!(boundaries(&p, &p.compositions[0]).is_empty());
    }
}
