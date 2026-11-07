//! 今のプレイリストの表。区間の URL はキーだけを持つので、キーから描き方（プロジェクト、割り方、番号）を引く。

use std::collections::HashMap;
use std::sync::Arc;

use mm_doc::Project;

use crate::hls::{HlsRequest, Plan};
use crate::key::SegmentKey;
use crate::quality::HlsQuality;

/// 1 つのプレイリスト。プロジェクトはそのバージョンの写しで、キーが同じ区間はどの写しで描いても同じになる。
pub(crate) struct Listing {
    pub version: u64,
    pub project: Arc<Project>,
    pub plan: Arc<Plan>,
    /// 区間の順のキー。
    pub keys: Vec<SegmentKey>,
}

/// コンポジションと画質ごとの最新のプレイリストと、キーの索引。
#[derive(Default)]
pub(crate) struct Listings {
    latest: HashMap<(String, HlsQuality), Arc<Listing>>,
    index: HashMap<SegmentKey, (Arc<Listing>, u64)>,
}

impl Listings {
    /// `version` のときに作った `req` のプレイリスト。
    pub(crate) fn get(&self, req: &HlsRequest, version: u64) -> Option<Arc<Listing>> {
        self.latest
            .get(&(req.composition.clone(), req.quality))
            .filter(|l| l.version == version)
            .cloned()
    }

    /// `req` の最新を置き換え、どのプレイリストにも無くなったキーを索引から外す。
    pub(crate) fn insert(&mut self, req: &HlsRequest, listing: Arc<Listing>) {
        self.latest
            .insert((req.composition.clone(), req.quality), listing);
        self.index.clear();
        for l in self.latest.values() {
            for (i, key) in l.keys.iter().enumerate() {
                self.index.entry(*key).or_insert((l.clone(), i as u64));
            }
        }
    }

    /// キーの区間を描くプレイリストと、その中の番号。
    pub(crate) fn find(&self, key: SegmentKey) -> Option<(Arc<Listing>, u64)> {
        self.index.get(&key).cloned()
    }

    pub(crate) fn clear(&mut self) {
        *self = Self::default();
    }
}
