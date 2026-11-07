//! スクラブの棒の目盛りに使う、コンポジションのカットの境の秒。

use mm_doc::{Composition, Project};
use mm_eval::place_cuts;
use mm_story::layer_sequence;

/// `comp` のシーケンスのレイヤーが並べるカットの境（コンポジションの時間軸の秒、昇順、重複なし）。
/// 0 とコンポジションの終わりは境に数えない。時間の写像を持つレイヤーは素材の時刻が一意に戻らないので除く。
pub(super) fn boundaries(project: &Project, comp: &Composition) -> Vec<f64> {
    let end = comp.duration.to_f64();
    let mut out: Vec<f64> = Vec::new();
    for layer in &comp.layers {
        let (Some(seq), None) = (layer_sequence(project, layer), &layer.time_remap) else {
            continue;
        };
        let Ok(placed) = place_cuts(seq) else {
            continue;
        };
        let (start, offset) = (layer.start.to_f64(), layer.in_point.to_f64());
        let speed = layer.speed.to_f64();
        let last = start + (layer.out_point.to_f64() - offset) / speed;
        out.extend(
            placed
                .iter()
                .flat_map(|p| [p.range.start, p.range.end])
                .map(|t| start + (t.to_f64() - offset) / speed)
                .filter(|t| *t > start && *t < last && *t > 0.0 && *t < end),
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
