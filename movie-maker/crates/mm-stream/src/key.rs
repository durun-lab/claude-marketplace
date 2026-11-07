//! 配信の区間のキー（docs/design/12-mobile.md の「映像の届け方」）。
//!
//! 区間の時間の範囲に掛かるものだけの指紋から作る。編集で変わらない区間はキーが同じなので作り直さず、端末も
//! `immutable` のまま使い続ける。指紋に入れるものは RFC 0013 のフレームのキャッシュのキーと同じ考え方で、
//! ノードの JSON、種類のバージョンと指紋（`NodeKind::cache_fingerprint`、RFC 0132）、素材の `contentHash`。
//! レイヤーとカットの位置は区間の始まりからの相対で入れるので、後ろの編集は前の区間のキーを変えない。

use std::fmt;
use std::path::PathBuf;
use std::str::FromStr;

use mm_doc::{
    Composition, Layer, Node, Project, Sequence, NESTED_KIND, NESTED_PARAM, SEQUENCE_KIND,
    SEQUENCE_PARAM,
};
use mm_eval::{node_params_at, place_cuts, source_time, PlacedCut};
use mm_node_api::NodeRegistry;
use mm_render_api::digest_bytes;
use mm_types::{AssetId, Id, MmError, Params, RationalTime, Result, TimeRange};
use serde::Serialize;

use crate::hls::Plan;

/// 区間のバイトの作り方（コンテナ、符号化の設定）の印。作り方を変えたら上げ、前のキーを使わせない。
const FORMAT: &str = "mm-stream/ts-1";
/// 入れ子をたどる深さの上限。レンダラーと同じ。
const MAX_DEPTH: u32 = 16;
/// 区間のファイルの拡張子。
pub(crate) const SEGMENT_EXT: &str = "ts";

/// 配信の区間のキー。URL の `seg/{key}.ts` とキャッシュのファイル名に入る。
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub struct SegmentKey(u64);

impl SegmentKey {
    /// プレイリストとキャッシュに使うファイル名（`<16 桁の 16 進>.ts`）。
    pub fn file_name(self) -> String {
        format!("{self}.{SEGMENT_EXT}")
    }
}

impl fmt::Display for SegmentKey {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        write!(f, "{:016x}", self.0)
    }
}

impl FromStr for SegmentKey {
    type Err = MmError;

    /// 16 桁の 16 進。URL の最後の部分をそのまま渡せるよう、末尾の `.ts` は省ける。
    fn from_str(s: &str) -> Result<Self> {
        let bad = || MmError::InvalidArgument(format!("区間のキーとして読めない: {s}"));
        let hex = s.strip_suffix(&format!(".{SEGMENT_EXT}")).unwrap_or(s);
        if hex.len() != 16 {
            return Err(bad());
        }
        u64::from_str_radix(hex, 16).map(Self).map_err(|_| bad())
    }
}

/// キーを作る材料のうち、区間によらないもの。
pub(crate) struct KeyEnv<'a> {
    pub project: &'a Project,
    pub nodes: &'a dyn NodeRegistry,
    /// 描画とエンコードの実装の名前。実装が変わると同じ編集でも出力が変わる。
    pub backends: &'a str,
    /// プロジェクトのバージョン。決定的でない種類や取り込み前の素材に頼る区間だけ、キーに入れる。
    pub version: u64,
}

/// 区間 `index` のキー。
pub(crate) fn segment_key(env: &KeyEnv<'_>, plan: &Plan, index: u64) -> Result<SegmentKey> {
    let range = plan.range(index)?;
    let comp = env.project.composition(&plan.composition)?;
    let mut walk = Walk {
        env,
        text: format!(
            "{FORMAT}|{}|{}|{}x{}@{}|{}-{}",
            env.backends,
            plan.quality.as_str(),
            plan.width,
            plan.height,
            plan.fps.to_f64(),
            range.start,
            range.end
        ),
        volatile: false,
    };
    walk.composition(comp, range, 0);
    if walk.volatile {
        walk.push(&format!("|version={}", env.version));
    }
    Ok(SegmentKey(digest_bytes(walk.text.as_bytes())))
}

/// 区間に掛かるものをたどって、指紋の文字列を作る。
struct Walk<'a> {
    env: &'a KeyEnv<'a>,
    text: String,
    /// 中身から決まらない入力に頼るか。真ならプロジェクトのバージョンもキーに入れる。
    volatile: bool,
}

impl Walk<'_> {
    fn push(&mut self, s: &str) {
        self.text.push('|');
        self.text.push_str(s);
    }

    /// `value` の JSON から `skip` のフィールドを除いて足す。
    fn json<T: Serialize>(&mut self, value: &T, skip: &[&str]) {
        match serde_json::to_value(value) {
            Ok(mut v) => {
                if let Some(map) = v.as_object_mut() {
                    for name in skip {
                        map.remove(*name);
                    }
                }
                self.push(&v.to_string());
            }
            Err(_) => self.volatile = true,
        }
    }

    fn time(&mut self, t: Result<RationalTime>) {
        match t {
            Ok(t) => self.push(&t.to_string()),
            Err(_) => self.volatile = true,
        }
    }

    /// コンポジションの `range` に掛かるレイヤー。長さとマーカーは区間の中身を変えないので入れない。
    fn composition(&mut self, comp: &Composition, range: TimeRange, depth: u32) {
        if depth > MAX_DEPTH {
            self.volatile = true;
            return;
        }
        self.json(comp, &["layers", "duration", "markers"]);
        for layer in comp.layers.iter().filter(|l| l.enabled) {
            let Ok(span) = layer.range() else {
                self.volatile = true;
                continue;
            };
            if let Some(visible) = span.intersect(&range) {
                self.layer(comp, layer, range, visible, depth);
            }
        }
    }

    /// レイヤーは位置の代わりに、区間の中で見える範囲と、その両端の素材の時刻を入れる。
    fn layer(
        &mut self,
        comp: &Composition,
        layer: &Layer,
        range: TimeRange,
        visible: TimeRange,
        depth: u32,
    ) {
        self.json(layer, &["start", "in", "out", "framing"]);
        self.time(visible.start.checked_sub(range.start));
        self.time(visible.end.checked_sub(range.start));
        let inner = source_time(layer, visible.start)
            .and_then(|a| TimeRange::new(a, source_time(layer, visible.end)?));
        let Ok(inner) = inner else {
            self.volatile = true;
            return;
        };
        self.time(Ok(inner.start));
        self.time(Ok(inner.end));
        self.json(&layer.framing.caption, &[]);
        self.related(comp, layer);
        self.node(&layer.source);
        for effect in layer.effects.iter().filter(|e| e.enabled) {
            self.node(&effect.node);
        }
        match layer.source.kind.as_str() {
            SEQUENCE_KIND => self.sequence(layer, inner, depth),
            NESTED_KIND => self.nested(&layer.source, Some(inner), depth),
            _ => {}
        }
    }

    /// 親とトラックマットのレイヤーは、変形と形を全部引き継ぐので丸ごと入れる。
    fn related(&mut self, comp: &Composition, layer: &Layer) {
        let mut next = layer.parent.as_ref();
        let mut hops = 0;
        while let Some(id) = next {
            let Some(parent) = comp.layers.iter().find(|l| &l.id == id) else {
                break;
            };
            self.json(parent, &[]);
            next = parent.parent.as_ref();
            hops += 1;
            if hops > comp.layers.len() {
                break;
            }
        }
        if let Some(matte) = &layer.track_matte {
            if let Some(m) = comp.layers.iter().find(|l| l.id == matte.layer) {
                self.json(m, &[]);
                self.node(&m.source);
            }
        }
    }

    /// 入れ子のコンポジション。`inner` が無ければ全長（範囲を写せないカットの中身）。
    fn nested(&mut self, node: &Node, inner: Option<TimeRange>, depth: u32) {
        let Some(id) = node.text_param(NESTED_PARAM) else {
            return;
        };
        let Ok(comp) = self.env.project.composition(&Id::new(id)) else {
            self.volatile = true;
            return;
        };
        let range = inner.or_else(|| TimeRange::new(RationalTime::ZERO, comp.duration).ok());
        match range {
            Some(r) => self.composition(comp, r, depth + 1),
            None => self.volatile = true,
        }
    }

    /// シーケンスの `inner` に掛かるカットと、トランジションで混ざる隣のカット。
    fn sequence(&mut self, layer: &Layer, inner: TimeRange, depth: u32) {
        let project = self.env.project;
        let seq = layer
            .source
            .text_param(SEQUENCE_PARAM)
            .and_then(|name| project.sequence(&Id::new(name)).ok());
        let Some((seq, placed)) = seq.and_then(|s| Some((s, place_cuts(s).ok()?))) else {
            self.volatile = true;
            return;
        };
        self.json(seq, &["cuts"]);
        if let Some(origin) = &seq.origin {
            match project.sequence(&origin.sequence) {
                Ok(o) => self.json(o, &[]),
                Err(_) => self.volatile = true,
            }
        }
        for i in touching(seq, &placed, inner) {
            let p = &placed[i];
            let cut = &seq.cuts[p.index];
            self.json(cut, &["transition_in"]);
            self.time(p.range.start.checked_sub(inner.start));
            self.time(p.range.end.checked_sub(inner.start));
            if let Some(r) = layer.framing.cuts.iter().find(|r| r.cut == cut.id) {
                self.json(r, &[]);
            }
            if let Some(tr) = cut
                .transition_in
                .as_ref()
                .filter(|_| transition_reaches(seq, p, inner))
            {
                self.json(tr, &[]);
                self.node(&tr.node);
            }
            self.node(&cut.source);
            if cut.source.kind == NESTED_KIND {
                self.nested(&cut.source, None, depth);
            }
        }
    }

    /// ノードの種類のバージョンと指紋、パラメータの素材。決定的でない種類は中身から決まらない。
    fn node(&mut self, node: &Node) {
        let project = self.env.project;
        let mut params = node_params_at(node, RationalTime::ZERO);
        match self.env.nodes.get(&node.kind) {
            None => self.push(&format!("{}=unregistered", node.kind)),
            Some(kind) => {
                params = params.with_defaults(&kind.params());
                if !kind.traits().deterministic {
                    self.volatile = true;
                }
                let asset_path =
                    |id: &AssetId| project.asset(id).ok().map(|a| PathBuf::from(&a.path));
                match kind.cache_fingerprint(&params, &asset_path) {
                    Some(fp) => self.push(&format!("{}@{}:{fp}", node.kind, kind.version())),
                    None => self.volatile = true,
                }
            }
        }
        self.assets(&params);
        for input in &node.inputs {
            self.node(input);
        }
    }

    /// 素材の中身は `contentHash` で入れる。取り込みの前（ハッシュが空）は中身から決まらない。
    fn assets(&mut self, params: &Params) {
        let project = self.env.project;
        for id in params.0.values().filter_map(|v| v.as_asset()) {
            match project.asset(id) {
                Ok(asset) if !asset.content_hash.is_empty() => self.json(asset, &[]),
                _ => self.volatile = true,
            }
        }
    }
}

/// `inner` に掛かるカット（`placed` の中の番号）。重なるカットと、`inner` に掛かるトランジションの両側のカット。
fn touching(seq: &Sequence, placed: &[PlacedCut], inner: TimeRange) -> Vec<usize> {
    let mixes = |i: usize| i < placed.len() && transition_reaches(seq, &placed[i], inner);
    (0..placed.len())
        .filter(|&i| placed[i].range.intersect(&inner).is_some() || mixes(i) || mixes(i + 1))
        .collect()
}

/// カットの頭のトランジションが `inner` に掛かるか。トランジションは境目の前後に長さの半分まで掛かるので、
/// 前後に長さの分を見れば足りる。範囲を計算できなければ掛かるとみなす。
fn transition_reaches(seq: &Sequence, placed: &PlacedCut, inner: TimeRange) -> bool {
    let Some(tr) = &seq.cuts[placed.index].transition_in else {
        return false;
    };
    let edge = placed.range.start;
    edge.checked_sub(tr.duration)
        .and_then(|a| TimeRange::new(a, edge.checked_add(tr.duration)?))
        .map_or(true, |r| r.intersect(&inner).is_some())
}
