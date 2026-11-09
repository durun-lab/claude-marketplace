//! コンポジションの時刻でキーフレームを持つパラメータの一覧。`shiftLayer` が動かすキーフレームと、
//! Timeline の帯のマーカーが同じ一覧を使う。

use mm_types::RationalTime;

use crate::param::Param;

use super::Layer;

impl Layer {
    /// コンポジションの時刻でキーフレームを持つパラメータ（`mm-render` が `CompCx::t` で評価するもの）。
    /// 素材の時刻の再フレーミングと、レイヤーの始まりからの時刻のタイムリマップは含めない。
    /// 一覧を足すときは [`Layer::comp_time_params_mut`] にも足す（mm-ops の試験が JSON から突き合わせる）。
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
