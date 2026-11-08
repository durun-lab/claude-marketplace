//! `trimLayer`。レイヤーのイン点かアウト点を動かす（Timeline の帯の端のつまみ）。

use serde::{Deserialize, Serialize};

use mm_doc::Project;
use mm_types::{LayerId, Result};

use crate::op::{Apply, Op};
use crate::ops::{invalid, negate, node_refs, Edge};
use crate::touches::Touches;

/// レイヤー `layer` の `edge` を、コンポジションの時間軸で `by` だけ動かす（素材の時間では `by × speed`）。
/// イン点は帯の左端と素材の位置を一緒に動かし（`start` と `in`）、画は動かない。アウト点は `out` だけ。
/// 正に動かすとイン点は短く、アウト点は長くなる。タイムリマップのあるレイヤーのイン点は拒否する（写像の起点が動くため）。
/// 動かせる範囲は素材の余白（`Layer::trim_room`）で、Timeline のつまみも同じ規則で止まる。
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize, schemars::JsonSchema)]
#[serde(deny_unknown_fields)]
pub struct TrimLayer {
    pub layer: LayerId,
    pub edge: Edge,
    pub by: mm_types::RationalTime,
}

impl Apply for TrimLayer {
    fn touches(&self, project: &Project) -> Result<Touches> {
        let (_, layer) = project.layer(&self.layer)?;
        let mut t = Touches::of(&self.layer);
        // 撮影素材の長さを読んで範囲を検査するので、素材の参照も読む所に入れる。
        node_refs(&layer.source, &mut t);
        Ok(t)
    }

    fn apply(&self, project: &mut Project) -> Result<Op> {
        self.check(project)?;
        let layer = project.layer_mut(&self.layer)?;
        let delta = self.by.scale(layer.speed)?;
        match self.edge {
            Edge::In => {
                let start = layer.start.checked_add(self.by)?;
                let in_point = layer.in_point.checked_add(delta)?;
                (layer.start, layer.in_point) = (start, in_point);
            }
            Edge::Out => layer.out_point = layer.out_point.checked_add(delta)?,
        }
        Ok(TrimLayer {
            layer: self.layer.clone(),
            edge: self.edge,
            by: negate(self.by)?,
        }
        .into())
    }
}

impl TrimLayer {
    /// 素材の余白を越える動かし方と、タイムリマップのあるレイヤーのイン点を拒否する。
    fn check(&self, project: &Project) -> Result<()> {
        let (_, layer) = project.layer(&self.layer)?;
        let room = layer.trim_room(project)?;
        let allowed = match self.edge {
            Edge::In if room.fixed_in => {
                return Err(invalid(format!(
                    "レイヤー {} はタイムリマップを持つのでイン点を動かせない",
                    self.layer
                )));
            }
            Edge::In => room.allows_in(self.by)?,
            Edge::Out => room.allows_out(self.by),
        };
        if allowed {
            Ok(())
        } else {
            Err(invalid(format!(
                "レイヤー {} が素材の外にはみ出す",
                self.layer
            )))
        }
    }
}
