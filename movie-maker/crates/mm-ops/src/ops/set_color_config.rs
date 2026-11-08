//! `setColorConfig`。プロジェクトの色の設定（タイムラインの作業の明るさ）を丸ごと置き換える。

use serde::{Deserialize, Serialize};

use mm_doc::{ColorConfig, Project};
use mm_types::Result;

use crate::op::{Apply, Op};
use crate::touches::Touches;

/// 色の設定を触る操作どうしを重ねるための名前。素材やコンポジションの ID と紛れない。
pub const COLOR_CONFIG_TOUCH: &str = "colorConfig";

/// プロジェクトの色の設定を `color_config` にする。逆操作は前の値を置く同じ操作。
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize, schemars::JsonSchema)]
#[serde(deny_unknown_fields)]
pub struct SetColorConfig {
    #[serde(default)]
    pub color_config: ColorConfig,
}

impl Apply for SetColorConfig {
    fn touches(&self, _: &Project) -> Result<Touches> {
        let mut t = Touches::new();
        t.add_str(COLOR_CONFIG_TOUCH);
        Ok(t)
    }

    fn apply(&self, project: &mut Project) -> Result<Op> {
        let old = std::mem::replace(&mut project.color_config, self.color_config.clone());
        Ok(SetColorConfig { color_config: old }.into())
    }
}
