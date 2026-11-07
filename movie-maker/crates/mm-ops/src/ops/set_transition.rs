//! `setTransition`。カットの先頭（前のカットとの編集点）のトランジションを置く・差し替える・外す。

use serde::{Deserialize, Serialize};

use mm_doc::{CutId, Project, Transition};
use mm_types::Result;

use crate::op::{Apply, Op};
use crate::ops::{add_nesting_reach, cut_refs_reach, node_refs};
use crate::touches::Touches;

/// カット `cut` の `transition_in` を `transition` にする（`None` で外す）。種類（`node.kind`）、パラメータ、長さの
/// 変更も、この操作で丸ごと差し替える。sequence の先頭のカットに置いたトランジションは描かない。
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize, schemars::JsonSchema)]
#[serde(deny_unknown_fields)]
pub struct SetTransition {
    pub cut: CutId,
    #[serde(default)]
    pub transition: Option<Transition>,
}

impl Apply for SetTransition {
    fn touches(&self, project: &Project) -> Result<Touches> {
        let (_, cut) = project.cut(&self.cut)?;
        let mut t = Touches::of(&self.cut);
        cut_refs_reach(project, cut, &mut t);
        if let Some(tr) = &self.transition {
            let mut roots = Touches::new();
            node_refs(&tr.node, &mut roots);
            add_nesting_reach(project, &roots, &mut t);
        }
        Ok(t)
    }

    fn apply(&self, project: &mut Project) -> Result<Op> {
        let cut = project.cut_mut(&self.cut)?;
        let old = std::mem::replace(&mut cut.transition_in, self.transition.clone());
        Ok(SetTransition {
            cut: self.cut.clone(),
            transition: old,
        }
        .into())
    }
}
