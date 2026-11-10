//! 帯の波形の元。撮影素材のレイヤーとカットに、コンポジションの時刻から波形の時刻への換算を付ける。

use std::collections::HashMap;
use std::sync::Arc;

use mm_doc::{Asset, Cut, Layer, Node, Project, FOOTAGE_KIND, FOOTAGE_PARAM};
use mm_types::AssetId;
use mm_ui_layout::WaveSource;

use super::comp_secs;

/// レイヤーとカットから帯の波形の元を引くもの。素材を ID から引く表を 1 回だけ作り、
/// コンテンツハッシュの文字列は同じ素材の帯どうしで共有する。
pub(super) struct WaveResolver<'a> {
    assets: HashMap<&'a AssetId, &'a Asset>,
    hashes: HashMap<&'a AssetId, Arc<str>>,
}

impl<'a> WaveResolver<'a> {
    pub(super) fn new(project: &'a Project) -> Self {
        Self {
            assets: project.assets.iter().map(|a| (&a.id, a)).collect(),
            hashes: HashMap::new(),
        }
    }

    /// 撮影素材のレイヤー。コンポジションの `start` に素材の `in` が当たる（`mm_eval::source_time` と同じ換算）。
    /// タイムリマップのあるレイヤーは出さない。
    pub(super) fn layer(&mut self, layer: &Layer) -> Option<WaveSource> {
        if layer.time_remap.is_some() {
            return None;
        }
        let at = (layer.start.to_f64(), layer.in_point.to_f64());
        self.source(&layer.source, at, layer.speed.to_f64())
    }

    /// sequence のカット。`seq_start` はカットの始まり（sequence の秒）で、そこにカットの `in` が当たる。
    /// 波形の速さは、カットの速さにレイヤーの速さを掛けたもの。
    pub(super) fn cut(&mut self, layer: &Layer, cut: &Cut, seq_start: f64) -> Option<WaveSource> {
        if layer.time_remap.is_some() {
            return None;
        }
        let at = (comp_secs(layer, seq_start), cut.in_point.to_f64());
        self.source(&cut.source, at, cut.speed.to_f64() * layer.speed.to_f64())
    }

    /// 撮影素材のノード `node` の波形の元。`(comp_time, media_time)` はコンポジションの時刻と、そこに当たる素材の時刻。
    /// キャッシュの波形は音の最初のサンプルから数え、再生の音は映像の先頭を素材の時刻 0 に置くので、
    /// 素材の時刻に音と映像の先頭のずれを足して波形の時刻にする。素材がプロジェクトに無ければ出さない。
    fn source(
        &mut self,
        node: &Node,
        (comp_time, media_time): (f64, f64),
        speed: f64,
    ) -> Option<WaveSource> {
        if node.kind != FOOTAGE_KIND {
            return None;
        }
        let id = node.params.get(FOOTAGE_PARAM)?.value.as_asset()?;
        let asset = *self.assets.get(id)?;
        if asset.content_hash.is_empty() {
            return None;
        }
        let content_hash = self
            .hashes
            .entry(&asset.id)
            .or_insert_with(|| asset.content_hash.as_str().into())
            .clone();
        Some(WaveSource {
            content_hash,
            comp_time,
            wave_time: media_time + asset.timebase_offset.to_f64(),
            speed,
        })
    }
}

#[cfg(test)]
mod tests {
    use mm_doc::{Asset, Composition, Cut, Layer, Node, Project, Sequence, TimeRemap};
    use mm_doc::{SEQUENCE_KIND, SEQUENCE_PARAM};
    use mm_types::{ColorSpace, Id, ParamValue, Rational, RationalTime};

    use super::super::timeline_model;

    /// 素材 `id`。音は映像より `offset_ms` ミリ秒早く始まる。
    fn asset(id: &str, offset_ms: i128) -> Asset {
        Asset {
            id: Id::new(id),
            path: format!("{id}.mkv"),
            content_hash: format!("hash-{id}"),
            color_space: ColorSpace::bt709(),
            duration: RationalTime::seconds(60),
            fps: Some(Rational::integer(30)),
            timebase_offset: RationalTime::new(offset_ms, 1000).unwrap(),
        }
    }

    fn footage(id: &str, asset: &str, (start, in_s, out_s): (i64, i64, i64)) -> Layer {
        Layer::new(
            id,
            Node::footage(&Id::new(asset)),
            RationalTime::seconds(start),
            RationalTime::seconds(in_s),
            RationalTime::seconds(out_s),
        )
    }

    fn cut(id: &str, source: Node, (in_s, out_s): (i64, i64)) -> Cut {
        let (a, b) = (RationalTime::seconds(in_s), RationalTime::seconds(out_s));
        Cut::new(id, source, a, b)
    }

    /// 2 倍速の k1（素材 a）、隙間 k2、k3（素材 b）を、コンポジションの 1 秒から並べる sequence のレイヤー。
    fn sequence(project: &mut Project) -> Layer {
        let mut fast = cut("k1", Node::footage(&Id::new("a")), (10, 13));
        fast.speed = Rational::integer(2);
        let gap = cut("k2", Node::gap(), (0, 1));
        let normal = cut("k3", Node::footage(&Id::new("b")), (5, 7));
        project.sequences.push(Sequence {
            id: Id::new("s"),
            cuts: vec![fast, gap, normal],
            origin: None,
        });
        let mut seq = footage("seq", "none", (1, 0, 9));
        seq.source =
            Node::new(SEQUENCE_KIND, 1).with_param(SEQUENCE_PARAM, ParamValue::Text("s".into()));
        seq
    }

    #[test]
    fn 撮影素材のレイヤーとカットに付き_波形の時刻はイン点と速さと音と映像の先頭のずれで決まる() {
        let mut project = Project::new();
        project.assets = vec![asset("a", 250), asset("b", 0)];
        let seq = sequence(&mut project);
        let mut talk = footage("talk", "a", (2, 4, 10));
        talk.speed = Rational::integer(2);
        let mut remapped = talk.clone();
        remapped.id = Id::new("remapped");
        remapped.time_remap = Some(TimeRemap {
            keyframes: Vec::new(),
        });
        let lost = footage("lost", "gone", (0, 0, 5));
        let comp = Composition::new(
            Id::new("c"),
            64,
            36,
            Rational::integer(30),
            RationalTime::seconds(10),
        )
        .with_layers(vec![talk, remapped, lost, seq]);
        let m = timeline_model(&project, &comp);
        let of = |w: Option<&mm_ui_layout::WaveSource>| {
            w.map(|w| {
                (
                    w.content_hash.to_string(),
                    w.comp_time,
                    w.wave_time,
                    w.speed,
                )
            })
        };
        let wave = |i: usize| of(m.items[i].wave.as_ref());
        let some = |h: &str, at, src, speed| Some((h.to_string(), at, src, speed));
        assert_eq!(wave(0), None, "sequence のレイヤーは持たず、カットが持つ");
        assert_eq!(wave(1), None, "素材がプロジェクトに無い");
        assert_eq!(wave(2), None, "タイムリマップのあるレイヤーは出さない");
        assert_eq!(
            wave(3),
            some("hash-a", 2.0, 4.25, 2.0),
            "始まりにイン点と先頭のずれ"
        );
        let cuts: Vec<_> = m.items[0]
            .cuts
            .iter()
            .map(|c| of(c.wave.as_ref()))
            .collect();
        assert_eq!(
            cuts,
            [
                some("hash-a", 1.0, 10.25, 2.0),
                None,
                some("hash-b", 3.5, 5.0, 1.0)
            ],
            "カットの始まりにカットのイン点。隙間は無し"
        );
        let hashes: Vec<String> = m.wave_hashes().iter().map(|h| h.to_string()).collect();
        assert_eq!(hashes, ["hash-a", "hash-b"], "重複なし、出てきた順");
    }
}
