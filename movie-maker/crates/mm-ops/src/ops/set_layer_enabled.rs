//! `setLayerEnabled`。レイヤーの有効と無効を切り替える。

use serde::{Deserialize, Serialize};

use mm_doc::Project;
use mm_types::{LayerId, Result};

use crate::op::{Apply, Op};
use crate::touches::Touches;

/// 有効を `enabled` にする。無効のレイヤーは描かず、鳴らさない（Timeline の見出しの目のボタン）。
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize, schemars::JsonSchema)]
#[serde(deny_unknown_fields)]
pub struct SetLayerEnabled {
    pub layer: LayerId,
    pub enabled: bool,
}

impl Apply for SetLayerEnabled {
    fn touches(&self, _: &Project) -> Result<Touches> {
        Ok(Touches::of(&self.layer))
    }

    fn apply(&self, project: &mut Project) -> Result<Op> {
        let layer = project.layer_mut(&self.layer)?;
        let enabled = std::mem::replace(&mut layer.enabled, self.enabled);
        Ok(SetLayerEnabled {
            layer: self.layer.clone(),
            enabled,
        }
        .into())
    }
}
