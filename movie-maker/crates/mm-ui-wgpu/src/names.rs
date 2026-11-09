//! Timeline の帯に出す名前。レイヤーにもカットにも名前の欄が無いので、素材の名前（撮影素材のファイル名、文字の最初の行など）
//! を名前にし、種類は帯の色に任せる（Premiere と Resolve の帯もクリップの名前だけを出す）。素材の名前が無いもの（単色）は
//! 種類の名前を英語で出す。

use std::collections::HashMap;
use std::sync::Arc;

use mm_doc::{
    Asset, Node, Project, FOOTAGE_KIND, FOOTAGE_PARAM, NESTED_KIND, NESTED_PARAM, SEQUENCE_KIND,
    SEQUENCE_PARAM,
};
use mm_text_api::parse_ruby;
use mm_types::AssetId;

/// 静止画のノードの種類の名前。素材の参照は撮影素材と同じパラメータに持つ。
const IMAGE_KIND: &str = "image";
/// 文字のノードの種類の名前と、文字列のパラメータの名前。
const TEXT_KIND: &str = "text";
const TEXT_PARAM: &str = "text";

/// 帯の名前を作るもの。素材を ID から引く表を 1 回だけ作る（大きなプロジェクトは素材もレイヤーも数千ある）。
pub(crate) struct Namer<'a> {
    assets: HashMap<&'a AssetId, &'a Asset>,
}

impl<'a> Namer<'a> {
    pub(crate) fn new(project: &'a Project) -> Self {
        Self {
            assets: project.assets.iter().map(|a| (&a.id, a)).collect(),
        }
    }

    /// ノードの帯の名前。素材の名前があればそれ、無ければ種類の名前。隙間は空（名前を出さない）。
    pub(crate) fn label(&self, node: &Node) -> Arc<str> {
        if node.is_gap() {
            return Arc::default();
        }
        match self.source_name(node) {
            Some(name) => name.into(),
            None => kind_label(&node.kind).into(),
        }
    }

    /// 素材の名前。撮影素材と静止画は素材のファイル名（素材が無ければ ID）、文字は最初の空でない行（ルビの注記を外す）、
    /// 入れ子と sequence は指す先の ID。
    fn source_name(&self, node: &Node) -> Option<String> {
        match node.kind.as_str() {
            FOOTAGE_KIND | IMAGE_KIND => {
                let id = node.params.get(FOOTAGE_PARAM)?.value.as_asset()?;
                Some(
                    self.assets
                        .get(id)
                        .map_or_else(|| id.to_string(), |a| file_name(&a.path)),
                )
            }
            TEXT_KIND => node.text_param(TEXT_PARAM).and_then(first_line),
            NESTED_KIND => non_empty(node.text_param(NESTED_PARAM)?),
            SEQUENCE_KIND => non_empty(node.text_param(SEQUENCE_PARAM)?),
            _ => None,
        }
    }
}

/// ノードの種類の英語の名前。知らない種類は種類の名前のまま。
fn kind_label(kind: &str) -> &str {
    match kind {
        FOOTAGE_KIND => "Footage",
        IMAGE_KIND => "Image",
        "solid" => "Solid",
        TEXT_KIND => "Text",
        NESTED_KIND => "Composition",
        SEQUENCE_KIND => "Sequence",
        other => other,
    }
}

/// パスの最後の要素。区切りは `/` と `\` のどちらも受ける。
fn file_name(path: &str) -> String {
    path.rsplit(['/', '\\']).next().unwrap_or(path).to_owned()
}

/// 最初の空でない行の、ルビの注記を外した親文字。帯は 1 行なので 2 行目からは出さない。
fn first_line(text: &str) -> Option<String> {
    let line = text.lines().map(str::trim).find(|l| !l.is_empty())?;
    Some(
        parse_ruby(line)
            .into_iter()
            .map(|s| &line[s.base])
            .collect(),
    )
}

fn non_empty(text: &str) -> Option<String> {
    (!text.is_empty()).then(|| text.to_owned())
}

#[cfg(test)]
mod tests {
    use super::*;
    use mm_types::{ColorSpace, Id, ParamValue, Rational, RationalTime};

    fn project() -> Project {
        let mut p = Project::new();
        p.assets.push(Asset {
            id: Id::new("a"),
            path: r"D:\clips\day1/take 3.mp4".into(),
            content_hash: "h".into(),
            color_space: ColorSpace::bt709(),
            duration: RationalTime::seconds(10),
            fps: Some(Rational::integer(30)),
            timebase_offset: RationalTime::ZERO,
        });
        p
    }

    fn text(value: &str) -> Node {
        Node::new(TEXT_KIND, 1).with_param(TEXT_PARAM, ParamValue::Text(value.into()))
    }

    #[test]
    fn 素材を持つノードは素材のファイル名で_種類を付けない() {
        let p = project();
        let names = Namer::new(&p);
        let footage = Node::footage(&Id::new("a"));
        assert_eq!(&*names.label(&footage), "take 3.mp4");
        let image = Node::new(IMAGE_KIND, 1)
            .with_param(FOOTAGE_PARAM, ParamValue::AssetRef(Id::new("missing")));
        assert_eq!(&*names.label(&image), "missing", "素材が無ければ ID");
    }

    #[test]
    fn 文字は最初の行の親文字で_入れ子と_sequence_は指す先の_id() {
        let p = project();
        let names = Namer::new(&p);
        assert_eq!(
            &*names.label(&text("\n  ｜東京《とうきょう》へ  \n2 行目")),
            "東京へ"
        );
        assert_eq!(&*names.label(&text("")), "Text", "空の文字は種類の名前");
        let nested =
            Node::new(NESTED_KIND, 1).with_param(NESTED_PARAM, ParamValue::Text("scene".into()));
        assert_eq!(&*names.label(&nested), "scene");
        let seq =
            Node::new(SEQUENCE_KIND, 1).with_param(SEQUENCE_PARAM, ParamValue::Text("talk".into()));
        assert_eq!(&*names.label(&seq), "talk");
    }

    #[test]
    fn 単色は種類だけで_隙間と知らない種類はそれぞれ空と種類の名前() {
        let p = project();
        let names = Namer::new(&p);
        assert_eq!(&*names.label(&Node::new("solid", 1)), "Solid");
        assert_eq!(&*names.label(&Node::gap()), "");
        assert_eq!(&*names.label(&Node::new("web.cef", 1)), "web.cef");
    }
}
