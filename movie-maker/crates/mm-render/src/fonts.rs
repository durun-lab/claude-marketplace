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

    /// フレームのキャッシュのキーに入れる、色の変換の設定、文字の実装、書体の組の指紋。書体が無ければ書体の分は付けない。
    /// 文字の実装（glyphon と cef など）は同じパラメータで違う画素を描くので、切り替えた後にディスクの退避を引かないよう入れる。
    pub(crate) fn config_fingerprint(&self) -> String {
        let color = self.services.color.config_fingerprint();
        let text = format!(
            "{color}+text:{}/{}",
            self.services.text_shaper.id(),
            self.services.text_rasterizer.id()
        );
        if self.fonts.is_empty() {
            text
        } else {
            format!("{text}+fonts:{:016x}", digest_bytes(self.fonts.as_bytes()))
        }
    }
}

#[cfg(test)]
mod tests {
    use std::sync::Arc;

    use mm_gpu::{GpuContext, GpuRequest};
    use mm_node_api::Services;
    use mm_testkit::audio::SampleMixerFactory;
    use mm_testkit::decode::SampleDecoderFactory;
    use mm_testkit::fixtures::sample_services;
    use mm_text_api::{ShapedText, TextRasterizer, TextStyle};
    use mm_types::{Result, RgbaImage};

    use super::*;

    /// モックと同じ画素を描くが、実装の名前だけが違うレンダラー。
    struct Renamed(Arc<dyn TextRasterizer>);

    impl TextRasterizer for Renamed {
        fn id(&self) -> &'static str {
            "renamed"
        }
        fn rasterize(
            &self,
            shaped: &ShapedText,
            style: &TextStyle,
            scale: f32,
        ) -> Result<RgbaImage> {
            self.0.rasterize(shaped, style, scale)
        }
    }

    fn renderer(ctx: &Arc<GpuContext>, services: Services) -> WgpuRenderer {
        WgpuRenderer::new(
            ctx.clone(),
            Arc::new(SampleDecoderFactory::default()),
            Arc::new(SampleMixerFactory),
            services,
        )
    }

    #[test]
    fn 文字のレンダラーが違えばキャッシュのキーの指紋が違う() {
        let ctx = match GpuContext::shared(&GpuRequest::default()) {
            Ok(ctx) => ctx,
            Err(e) if std::env::var_os("MM_REQUIRE_GPU").is_none() => {
                eprintln!("GPU が無いのでスキップする: {e}");
                return;
            }
            Err(e) => panic!("MM_REQUIRE_GPU があるのに GPU が無い: {e}"),
        };
        let base = sample_services();
        let renamed = Services {
            text_rasterizer: Arc::new(Renamed(base.text_rasterizer.clone())),
            ..base.clone()
        };
        let a = renderer(&ctx, base.clone()).config_fingerprint();
        assert_eq!(a, renderer(&ctx, base).config_fingerprint());
        assert_ne!(a, renderer(&ctx, renamed).config_fingerprint());
    }
}
