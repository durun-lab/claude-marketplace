//! `gen.ai` のノードの種類（docs/design/04-nodes.md の「gen.ai」）。
//!
//! - 種類（[`GenAiKind`]）はプロンプトと設定（種類、大きさ、長さ、シード）をパラメータに持ち、描くのは生成した素材だけ。
//! - 提供元は [`Provider`] で差し替える。この版は偽の提供元（[`FakeProvider`]）だけを持ち、実際のサービスは呼ばない。
//! - 生成は [`GenAi`] の 3 段（`quote` → `approve` → `generate`）でしか起きない。承認の印（[`Approval`]）は
//!   見積もりの鍵と費用の上限を持ち、MCP の経路と自動の改善の回の中（[`IMPROVE_ROUND_ENV`]）からは作れない。
//! - 同じ頼み（パラメータと提供元の版の BLAKE3）は `media/genai/` の素材を使い回し、提供元を呼ばない。

mod encode;
mod fake;
mod fit;
mod gate;
mod generate;
mod kind;
mod provider;
mod request;

pub use encode::{png_solid, wav_sine, y4m_solid, CHANNELS, SAMPLE_RATE};
pub use fake::{FakePrices, FakeProvider, FAKE_FPS, FAKE_PROVIDER};
pub use gate::{current_round, Approval, GenError, Quote, Reuse, IMPROVE_ROUND_ENV};
pub use generate::{asset_id_for, media_dir, GenAi, Generation, GENERATE_TXN_NAME};
pub use kind::{GenAiKind, GENAI_KIND, KIND_VERSION};
pub use provider::{Estimate, Generated, Provider, Providers};
pub use request::{
    GenRequest, Media, ASSET_PARAM, DURATION_PARAM, HEIGHT_PARAM, MEDIA_PARAM, PROMPT_PARAM,
    PROVIDER_PARAM, SEED_PARAM, WIDTH_PARAM,
};
