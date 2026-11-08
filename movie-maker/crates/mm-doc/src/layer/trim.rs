//! レイヤーの端のつまみで動かせる量の規則。`trimLayer` の検査と Timeline のつまみが同じ値を使う。

use mm_types::{AssetId, RationalTime, Result};

use crate::node::{FOOTAGE_KIND, FOOTAGE_PARAM};
use crate::Project;

use super::Layer;

/// レイヤーの素材の余白（コンポジションの時間軸）。イン点の前とアウト点の後ろに残る素材の長さ。
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct TrimRoom {
    /// イン点の前の素材の余白。`None` は素材の長さが無く、長さを 1 フレーム残す限り前へ動かせる。
    pub head: Option<RationalTime>,
    /// アウト点の後ろの素材の余白。`None` は素材の長さが無く、後ろへいくらでも動かせる。
    pub tail: Option<RationalTime>,
    /// イン点を動かせない。タイムリマップの写像の起点がイン点にあるため。
    pub fixed_in: bool,
}

impl TrimRoom {
    /// イン点を `by`（正で後ろへ）動かしても素材の先頭を越えないか。
    pub fn allows_in(&self, by: RationalTime) -> Result<bool> {
        if self.fixed_in {
            return Ok(false);
        }
        Ok(match self.head {
            Some(head) => by.checked_add(head)? >= RationalTime::ZERO,
            None => true,
        })
    }

    /// アウト点を `by`（正で後ろへ）動かしても素材の終わりを越えないか。
    pub fn allows_out(&self, by: RationalTime) -> bool {
        self.tail.is_none_or(|tail| by <= tail)
    }
}

impl Layer {
    /// 撮影素材のレイヤーが指す素材。撮影素材でなければ `None`。
    fn footage_asset(&self) -> Option<&AssetId> {
        if self.source.kind != FOOTAGE_KIND {
            return None;
        }
        self.source
            .params
            .get(FOOTAGE_PARAM)
            .and_then(|p| p.value.as_asset())
    }

    /// 素材の余白。撮影素材だけが素材の長さを持ち、静止画、文字、単色、入れ子は長さで縛らない（After Effects の静止画と同じ）。
    pub fn trim_room(&self, project: &Project) -> Result<TrimRoom> {
        let fixed_in = self.time_remap.is_some();
        let Some(id) = self.footage_asset() else {
            return Ok(TrimRoom {
                head: None,
                tail: None,
                fixed_in,
            });
        };
        let duration = project.asset(id)?.duration;
        Ok(TrimRoom {
            head: Some(self.in_point.div_by(self.speed)?),
            tail: Some(duration.checked_sub(self.out_point)?.div_by(self.speed)?),
            fixed_in,
        })
    }
}

#[cfg(test)]
mod tests {
    use mm_types::{ColorSpace, Id, Rational};

    use crate::{Asset, Node};

    use super::*;

    fn secs(n: i64) -> RationalTime {
        RationalTime::seconds(n)
    }

    fn footage_project() -> Project {
        let mut p = Project::new();
        p.assets.push(Asset {
            id: Id::new("a"),
            path: "a.mp4".into(),
            content_hash: "hash-a".into(),
            color_space: ColorSpace::bt709(),
            duration: secs(20),
            fps: Some(Rational::integer(30)),
            timebase_offset: RationalTime::ZERO,
        });
        p
    }

    fn layer(source: Node, in_s: i64, out_s: i64, speed: u32) -> Layer {
        Layer {
            speed: Rational::integer(speed),
            ..Layer::new("l", source, secs(0), secs(in_s), secs(out_s))
        }
    }

    #[test]
    fn 撮影素材の余白は速さで割ったコンポジションの秒() {
        let p = footage_project();
        let room = layer(Node::footage(&Id::new("a")), 4, 10, 2)
            .trim_room(&p)
            .unwrap();
        assert_eq!((room.head, room.tail), (Some(secs(2)), Some(secs(5))));
        assert!(room.allows_in(secs(-2)).unwrap() && !room.allows_in(secs(-3)).unwrap());
        assert!(room.allows_out(secs(5)) && !room.allows_out(secs(6)));
    }

    #[test]
    fn 素材の長さが無いレイヤーは縛らず_タイムリマップはイン点を止める() {
        let p = footage_project();
        let mut solid = layer(Node::new("solid", 1), 0, 5, 1);
        let room = solid.trim_room(&p).unwrap();
        assert!(room.allows_in(secs(-99)).unwrap() && room.allows_out(secs(99)));
        solid.time_remap = Some(crate::TimeRemap {
            keyframes: Vec::new(),
        });
        let room = solid.trim_room(&p).unwrap();
        assert!(!room.allows_in(secs(1)).unwrap() && room.allows_out(secs(1)));
    }
}
