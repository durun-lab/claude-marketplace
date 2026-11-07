//! 色空間の変換のインターフェース。レンダラーは [`ColorTransform::program`] で GPU の形を受け取り、
//! 契約テストと CPU の経路は [`ColorTransform::apply`] で同じ変換を数で確かめる。
//!
//! # 色空間の名前の決まり
//!
//! 小文字のスネークケース。レンジ（tv / full）は名前に入れない（デコーダーが RGB へ変換する時点で展開済み）。
//!
//! | 名前 | 伝達関数 | 原色 |
//! |---|---|---|
//! | `lin_rec709` | 線形（作業色空間） | Rec.709 |
//! | `srgb` | IEC 61966-2-1 の区分（静止画の既定） | Rec.709 |
//! | `bt709` | Rec.1886 の表示の式（`linear = v^2.4`、映像の既定。OCIO の `Rec.1886 Rec.709 - Display`） | Rec.709 |
//! | `bt709_oetf` | BT.709 のカメラの OETF（シーン参照の素材だけ） | Rec.709 |
//! | `bt601` | Rec.1886 の表示の式 | Rec.601（SMPTE 170M）。作業色空間には Rec.709 と同じ原色として扱う |
//! | `bt2020` | Rec.1886 の表示の式 | Rec.2020（SDR）。作業色空間へは [`REC2020_TO_REC709`] で変換する |
//! | `bt2100_pq` | PQ（SMPTE ST 2084）。線形の 1.0 が 203 cd/m²（`mm_types::transfer`） | Rec.2020 |
//! | `bt2100_hlg` | HLG（ARIB STD-B67）。表示の上限 1000 cd/m² の OOTF を含む | Rec.2020 |
//!
//! 原色の扱いの理由は RFC 0005 の「bt601 と bt2020 の扱い」。HDR の 2 つは符号化値 (1,1,1) が白にならない
//! （PQ は 10000 cd/m²）ので [`UNIT_WHITE_SPACES`] に入れない。
//!
//! 契約（`mm_testkit::color::contract`）:
//! - 往復（from → to → from）が許容差 1e-3 に入る。
//! - 作業色空間から作業色空間への変換は恒等。
//! - 符号化値 (1,1,1) が白になる空間（名前の表の全部、[`UNIT_WHITE_SPACES`]）では、(1,1,1) は作業色空間でも白。
//!   それ以外の空間（`acescg` など）は白の確認をスキップし、往復と `program` だけ確かめる。
//! - `program` と `apply` は同じ変換を表す（実装の組み合わせごとに期待画像が守る）。

use serde::{Deserialize, Serialize};

use mm_types::{ColorSpace, Result};

/// 符号化値 (1,1,1) が白になる空間の名前（名前の表の全部）。契約テストはこの空間だけ白を確かめる。
pub const UNIT_WHITE_SPACES: &[&str] = &[
    "lin_rec709",
    "srgb",
    "bt709",
    "bt709_oetf",
    "bt601",
    "bt2020",
];

/// 原色の行列。デコーダーも同じ値を使うので `mm_types::transfer` に置き、ここから引けるようにする。
pub use mm_types::transfer::{REC2020_TO_REC709, REC709_TO_REC2020};

/// GPU で適用する変換の形。
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[serde(tag = "type", rename_all = "snake_case")]
pub enum TransformProgram {
    /// 何もしない。レンダラーはパスを飛ばせる。
    Identity,
    /// WGSL の関数 `fn mm_color_transform(c: vec3<f32>) -> vec3<f32>` を含むソース。
    Wgsl { source: String },
    /// 3D LUT。`size^3 * 3` の f32、R が最も速く変わる。格子の入力は `shaper` を通した後の 0..=1。
    Lut3d {
        size: u32,
        data: Vec<f32>,
        #[serde(default)]
        shaper: LutShaper,
    },
}

/// 3D LUT を引く前に入力に掛ける 1D の曲線。線形の光（1 を超えるハイライトを含む）を 0..=1 の格子に収める。
#[derive(Debug, Clone, Copy, Default, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum LutShaper {
    /// 何も掛けない。入力は符号化値で、0..=1 に切る。
    #[default]
    None,
    /// 線形の光を PQ の符号化値にする（`mm_types::transfer::linear_to_pq`）。0..10000 cd/m² が 0..=1 に入る。
    /// 負の値は 0 に切る。
    Pq,
}

impl LutShaper {
    /// 入力の 1 成分を格子の座標（0..=1）にする。
    pub fn apply(self, v: f32) -> f32 {
        match self {
            Self::None => v,
            Self::Pq => mm_types::transfer::linear_to_pq(v.max(0.0)),
        }
        .clamp(0.0, 1.0)
    }

    /// 格子の座標（0..=1）を入力の値に戻す。ベイクで格子の各点に当てる入力を決める。
    pub fn invert(self, g: f32) -> f32 {
        match self {
            Self::None => g,
            Self::Pq => mm_types::transfer::pq_to_linear(g),
        }
    }

    /// 同じ曲線の WGSL の式（`arg` は `vec3<f32>`）。`mm_types::transfer::TRANSFER_WGSL` を前に連結する。
    pub fn wgsl(self, arg: &str) -> String {
        match self {
            Self::None => arg.to_owned(),
            Self::Pq => format!("mm_linear_to_pq3(max({arg}, vec3<f32>(0.0)))"),
        }
    }
}

/// 色の変換。入力の色空間 → 作業色空間 → 表示・出力。
pub trait ColorTransform: Send + Sync {
    /// 実装の名前（`ocio`、`lut`、モックは `sample`）。
    fn id(&self) -> &'static str;
    /// `from` から `to` への GPU の形。知らない色空間は `Err(Unsupported)`。
    fn program(&self, from: &ColorSpace, to: &ColorSpace) -> Result<TransformProgram>;
    /// 同じ変換を CPU で 1 画素に適用する。線形の RGB（アルファは触らない）。
    fn apply(&self, from: &ColorSpace, to: &ColorSpace, rgb: [f32; 3]) -> Result<[f32; 3]>;
    /// この実装が知っている色空間の名前。
    fn known_spaces(&self) -> Vec<ColorSpace>;
    /// 実装と設定の指紋。同じ指紋なら同じ変換になる（OCIO なら設定の場所とライブラリのバージョンも含める）。
    /// 変換した画素はこの指紋の下でフレームのキャッシュに覚える。設定を持たない実装は `id` で足りる。
    fn config_fingerprint(&self) -> String {
        self.id().to_owned()
    }
}
