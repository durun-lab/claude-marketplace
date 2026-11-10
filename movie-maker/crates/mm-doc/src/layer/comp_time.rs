//! コンポジションの時刻でキーフレームを持つパラメータの一覧。`shiftLayer` が動かすキーフレームと、
//! Timeline の帯のマーカーとキーフレームの行が同じ一覧を使う。
//! 帯のマーカーは全部のレイヤーで毎回読むので、パスの文字を作らない一覧（`comp_time_params`）を別に持つ。

use mm_types::RationalTime;

use crate::node::Node;
use crate::param::Param;

use super::{Layer, Mask};

impl Layer {
    /// コンポジションの時刻でキーフレームを持つパラメータ（`mm-render` が `CompCx::t` で評価するもの）。
    /// 素材の時刻の再フレーミングと、レイヤーの始まりからの時刻のタイムリマップは含めない。
    /// 一覧を足すときは [`Layer::comp_time_param_paths`] と [`Layer::comp_time_params_mut`] にも足す
    /// （mm-ops の試験が JSON と操作のパスから突き合わせる）。
    pub fn comp_time_params(&self) -> Vec<&Param> {
        let mut out: Vec<&Param> = vec![
            &self.opacity,
            &self.transform.position,
            &self.transform.scale,
            &self.transform.rotation,
        ];
        out.extend(self.transform.anchor.as_ref());
        let crop = &self.crop;
        out.extend(
            [&crop.left, &crop.top, &crop.right, &crop.bottom]
                .into_iter()
                .flat_map(Option::as_ref),
        );
        for mask in &self.masks {
            out.extend([&mask.path, &mask.feather, &mask.opacity]);
            if let Some(rect) = &mask.rect {
                out.extend([&rect.position, &rect.size, &rect.radius]);
            }
        }
        if let Some(audio) = &self.audio {
            out.extend([&audio.gain_db, &audio.pan]);
        }
        if let Some(caption) = &self.framing.caption {
            out.extend([&caption.position, &caption.width]);
        }
        let mut nodes = vec![&self.source];
        nodes.extend(self.effects.iter().map(|e| &e.node));
        while let Some(node) = nodes.pop() {
            out.extend(node.params.values());
            nodes.extend(node.inputs.iter());
        }
        out
    }

    /// [`Layer::comp_time_params`] のうち、パラメータのパス（`mm-ops` の `ParamRef` の `path`）で指せるものと、そのパス。
    /// ノードの入力（`Node::inputs`）のノードのパラメータはパスで指せないので含めない。並びは Inspector の区画の順
    /// （不透明度、変形、切り抜き、ソース、音、効果、マスク）の後に字幕の置き場所。Timeline のキーフレームの行が使う。
    pub fn comp_time_param_paths(&self) -> Vec<(String, &Param)> {
        let t = &self.transform;
        let mut out: Vec<(String, &Param)> = vec![
            ("opacity".into(), &self.opacity),
            ("transform.position".into(), &t.position),
            ("transform.scale".into(), &t.scale),
            ("transform.rotation".into(), &t.rotation),
        ];
        out.extend(t.anchor.as_ref().map(|p| ("transform.anchor".into(), p)));
        let c = &self.crop;
        for (side, p) in [
            ("left", &c.left),
            ("top", &c.top),
            ("right", &c.right),
            ("bottom", &c.bottom),
        ] {
            out.extend(p.as_ref().map(|p| (format!("crop.{side}"), p)));
        }
        out.extend(node_paths("source", &self.source));
        if let Some(audio) = &self.audio {
            out.push(("audio.gain_db".into(), &audio.gain_db));
            out.push(("audio.pan".into(), &audio.pan));
        }
        for effect in &self.effects {
            out.extend(node_paths(&format!("effects.{}", effect.id), &effect.node));
        }
        out.extend(self.masks.iter().flat_map(mask_paths));
        if let Some(caption) = &self.framing.caption {
            out.push(("caption.position".into(), &caption.position));
            out.push(("caption.width".into(), &caption.width));
        }
        out
    }

    /// [`Layer::comp_time_params`] と同じ一覧の、書き換えられる参照。`shiftLayer` がキーフレームを動かす。
    pub fn comp_time_params_mut(&mut self) -> Vec<&mut Param> {
        let mut out: Vec<&mut Param> = vec![
            &mut self.opacity,
            &mut self.transform.position,
            &mut self.transform.scale,
            &mut self.transform.rotation,
        ];
        out.extend(self.transform.anchor.as_mut());
        let crop = &mut self.crop;
        out.extend(
            [
                &mut crop.left,
                &mut crop.top,
                &mut crop.right,
                &mut crop.bottom,
            ]
            .into_iter()
            .flat_map(Option::as_mut),
        );
        for mask in &mut self.masks {
            out.extend([&mut mask.path, &mut mask.feather, &mut mask.opacity]);
            if let Some(rect) = &mut mask.rect {
                out.extend([&mut rect.position, &mut rect.size, &mut rect.radius]);
            }
        }
        if let Some(audio) = &mut self.audio {
            out.extend([&mut audio.gain_db, &mut audio.pan]);
        }
        if let Some(caption) = &mut self.framing.caption {
            out.extend([&mut caption.position, &mut caption.width]);
        }
        let mut nodes = vec![&mut self.source];
        nodes.extend(self.effects.iter_mut().map(|e| &mut e.node));
        while let Some(node) = nodes.pop() {
            out.extend(node.params.values_mut());
            nodes.extend(node.inputs.iter_mut());
        }
        out
    }

    /// [`Layer::comp_time_params`] のキーフレームの時刻（コンポジションの時刻）。前から順で、重複は 1 つにする。
    pub fn keyframe_times(&self) -> Vec<RationalTime> {
        let mut times: Vec<RationalTime> = self
            .comp_time_params()
            .into_iter()
            .flat_map(|p| p.keyframes.iter().map(|k| k.time))
            .collect();
        times.sort();
        times.dedup();
        times
    }
}

/// ノードのパラメータと、そのパス（`<prefix>.<名前>`）。名前の順。
fn node_paths<'a>(prefix: &str, node: &'a Node) -> impl Iterator<Item = (String, &'a Param)> {
    let prefix = prefix.to_owned();
    node.params
        .iter()
        .map(move |(name, p)| (format!("{prefix}.{name}"), p))
}

/// マスクのパラメータと、そのパス（`masks.<マスクの ID>.<名前>`）。
fn mask_paths(mask: &Mask) -> Vec<(String, &Param)> {
    let id = &mask.id;
    let mut out = vec![
        (format!("masks.{id}.path"), &mask.path),
        (format!("masks.{id}.feather"), &mask.feather),
        (format!("masks.{id}.opacity"), &mask.opacity),
    ];
    if let Some(rect) = &mask.rect {
        out.push((format!("masks.{id}.rect.position"), &rect.position));
        out.push((format!("masks.{id}.rect.size"), &rect.size));
        out.push((format!("masks.{id}.rect.radius"), &rect.radius));
    }
    out
}
