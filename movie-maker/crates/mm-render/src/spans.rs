//! 描画のスパンに載せるフィールド（計装の決まり。`docs/design/17-instrumentation.md`）。

use mm_doc::{Layer, Project};

/// スパンに載せる素材の `contentHash` の字数。
const HASH_PREFIX: usize = 8;

/// レイヤー 1 枚（カットはカットの ID のレイヤー）を描くスパン。レイヤーの ID、ノードの種類、素材を載せる。
/// フィールドの値はプロジェクトのデータを借りるだけで、スパンを作らないときは引かない。
pub(crate) fn layer_span(project: &Project, layer: &Layer) -> tracing::Span {
    tracing::debug_span!(
        "render.layer",
        layer = layer.id.as_str(),
        kind = layer.source.kind.as_str(),
        asset = asset_hash(project, layer),
    )
}

/// レイヤーの素材の `contentHash` の先頭 [`HASH_PREFIX`] 字。素材を参照しないレイヤーは `None`。
/// 素材のパスやファイル名には個人のフォルダーの名前が入るので、スパンにはハッシュの先頭だけを載せる。
fn asset_hash<'p>(project: &'p Project, layer: &Layer) -> Option<&'p str> {
    let id = layer
        .source
        .params
        .values()
        .find_map(|p| p.value.as_asset())?;
    let hash = project.asset(id).ok()?.content_hash.as_str();
    let end = hash
        .char_indices()
        .nth(HASH_PREFIX)
        .map_or(hash.len(), |(i, _)| i);
    Some(&hash[..end])
}

#[cfg(test)]
mod tests {
    use mm_doc::{Asset, Node, Param};
    use mm_trace::ChromeTraceLayer;
    use mm_types::{ColorSpace, Id, ParamValue, RationalTime};
    use tracing_subscriber::prelude::*;

    use super::*;

    fn project_with(hash: &str) -> Project {
        let mut project = Project::new();
        project.assets.push(Asset {
            id: Id::new("clip"),
            path: "C:/Users/someone/clip.mkv".into(),
            content_hash: hash.into(),
            color_space: ColorSpace::bt709(),
            duration: RationalTime::new(10, 1).unwrap(),
            fps: None,
            timebase_offset: RationalTime::ZERO,
        });
        project
    }

    fn layer(kind: &str, asset: Option<&str>) -> Layer {
        let mut source = Node::new(kind, 1);
        if let Some(a) = asset {
            let value = ParamValue::AssetRef(Id::new(a));
            source.params.insert("asset".into(), Param::constant(value));
        }
        let (zero, one) = (RationalTime::ZERO, RationalTime::new(1, 1).unwrap());
        Layer::new("video-314", source, zero, zero, one)
    }

    /// `f` の中で作ったスパンのフィールドを、閉じた順に返す。
    fn recorded(f: impl FnOnce()) -> Vec<Vec<(&'static str, String)>> {
        let (chrome, handle) = ChromeTraceLayer::new();
        tracing::subscriber::with_default(tracing_subscriber::registry().with(chrome), f);
        handle.events().into_iter().map(|e| e.args).collect()
    }

    #[test]
    fn レイヤーのスパンにレイヤーの_id_とノードの種類と素材のハッシュの先頭を載せる() {
        let project = project_with("0123456789abcdef");
        let args = recorded(|| drop(layer_span(&project, &layer("footage", Some("clip")))));
        assert_eq!(
            args,
            vec![vec![
                ("layer", "video-314".to_owned()),
                ("kind", "footage".to_owned()),
                ("asset", "01234567".to_owned()),
            ]]
        );
    }

    #[test]
    fn 素材を参照しないレイヤーと見つからない素材は素材を載せない() {
        let project = project_with("0123456789abcdef");
        let args = recorded(|| {
            drop(layer_span(&project, &layer("solid", None)));
            drop(layer_span(&project, &layer("footage", Some("missing"))));
        });
        for fields in args {
            assert!(
                fields.iter().all(|(name, _)| *name != "asset"),
                "{fields:?}"
            );
        }
    }

    #[test]
    fn ハッシュの先頭は文字の途中で切らない() {
        let project = project_with("あいうえおかきくけこ");
        let layer = layer("footage", Some("clip"));
        assert_eq!(asset_hash(&project, &layer), Some("あいうえおかきく"));
    }
}
