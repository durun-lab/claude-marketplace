//! プロジェクトの書体の素材（拡張子が `ttf`、`otf`、`ttc` の素材）を、文字の実装とノードの種類に渡す。
//! 組が変わったフレームでだけ渡し直し、組の指紋をフレームのキャッシュのキーに入れる（同じ組なら同じ画素）。

use mm_doc::Project;
use mm_node_api::NodeRegistry;
use mm_render_api::digest_bytes;
use mm_text_api::{fonts_fingerprint, is_font_path, FontFile};

use crate::renderer::WgpuRenderer;

/// プロジェクトの書体の素材。場所の順に並べる。
pub(crate) fn project_fonts(project: &Project) -> Vec<FontFile> {
    let mut fonts: Vec<FontFile> = project
        .assets
        .iter()
        .filter(|a| is_font_path(std::path::Path::new(&a.path)))
        .map(|a| FontFile::from_asset(&a.path, &a.content_hash))
        .collect();
    fonts.sort();
    fonts
}

impl WgpuRenderer {
    /// 書体の組が前のフレームと違えば、文字の実装と全部のノードの種類に渡す。
    /// 渡せなかった実装は OS の書体で描き続けるので、描画は止めずに警告だけ残す。
    pub(crate) fn sync_fonts(&mut self, project: &Project, nodes: &dyn NodeRegistry) {
        let fonts = project_fonts(project);
        let fingerprint = fonts_fingerprint(&fonts);
        if fingerprint == self.fonts {
            return;
        }
        let _span = tracing::info_span!("render.use_fonts", count = fonts.len()).entered();
        let results = [
            self.services.text_shaper.use_fonts(&fonts),
            self.services.text_rasterizer.use_fonts(&fonts),
        ];
        let kinds = nodes.kinds().into_iter().filter_map(|id| nodes.get(id));
        let kind_results = kinds.map(|kind| kind.use_fonts(&fonts));
        for err in results
            .into_iter()
            .chain(kind_results)
            .filter_map(|r| r.err())
        {
            tracing::warn!(%err, "プロジェクトの書体を渡せない");
        }
        self.fonts = fingerprint;
    }

    /// フレームのキャッシュのキーに入れる、色の変換の設定と書体の組の指紋。書体が無ければ色の指紋のまま。
    pub(crate) fn config_fingerprint(&self) -> String {
        let color = self.services.color.config_fingerprint();
        if self.fonts.is_empty() {
            color
        } else {
            format!("{color}+fonts:{:016x}", digest_bytes(self.fonts.as_bytes()))
        }
    }
}
