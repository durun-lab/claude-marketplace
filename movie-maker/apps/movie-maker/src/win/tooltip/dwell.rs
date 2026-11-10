//! ツールチップを出す時機の状態機械。ポインターの下の部品が変わったら待ち時間を数え直し、止まって待つと出し、
//! 出ている間に別の部品へ移れば短い待ち（reshow）で差し替え、出しっぱなしにせず autopop で消す。待ち時間は Windows の
//! ツールチップの既定（`TTDT_INITIAL`、`TTDT_RESHOW`、`TTDT_AUTOPOP`）を呼び出し側が読んで渡す（トラッキングのツールチップは
//! Windows が待ち時間を数えないので、自前のタイマーで同じ動きにする）。先例は Windows の標準のツールチップの動き。

use mm_ui_layout::TooltipContent;

/// 待ち時間（ミリ秒）。
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct Delays {
    /// ポインターが止まってから最初に出すまで。
    pub initial: u32,
    /// 出ている間に別の部品へ移ってから、差し替えるまで。
    pub reshow: u32,
    /// 出してから消すまで。
    pub autopop: u32,
}

/// UI のスレッドがすること。
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum TooltipAction {
    /// ツールチップを出す（文と位置の差し替えを含む）。
    Show(TooltipContent),
    /// ツールチップを隠す。
    Hide,
    /// 出すまでのタイマーを `ms` で掛け直す。
    ArmShow(u32),
    /// 消すまでのタイマーを `ms` で掛ける。
    ArmPop(u32),
    /// 2 つのタイマーを止める。
    Disarm,
}

/// ツールチップの状態。
#[derive(Debug, Default)]
pub struct Dwell {
    target: Option<TooltipContent>,
    visible: bool,
    /// 押下やホイール、autopop で消した。ポインターが別の部品へ移るまで出し直さない。
    dismissed: bool,
}

impl Dwell {
    /// ポインターの下の部品のツールチップが `tip` になった（部品が無ければ `None`）。同じ部品なら何もしない。
    pub fn target(&mut self, tip: Option<TooltipContent>, delays: Delays) -> Vec<TooltipAction> {
        if tip == self.target {
            return Vec::new();
        }
        let was_visible = std::mem::take(&mut self.visible);
        self.dismissed = false;
        self.target = tip;
        let mut out = Vec::new();
        if was_visible {
            out.push(TooltipAction::Hide);
        }
        match (&self.target, was_visible) {
            (Some(_), true) => out.push(TooltipAction::ArmShow(delays.reshow)),
            (Some(_), false) => out.push(TooltipAction::ArmShow(delays.initial)),
            (None, _) => out.push(TooltipAction::Disarm),
        }
        out
    }

    /// 出すまでのタイマーの時間が来た。
    pub fn show_timer(&mut self, delays: Delays) -> Vec<TooltipAction> {
        match (&self.target, self.visible, self.dismissed) {
            (Some(tip), false, false) => {
                self.visible = true;
                vec![
                    TooltipAction::Show(tip.clone()),
                    TooltipAction::ArmPop(delays.autopop),
                ]
            }
            _ => vec![TooltipAction::Disarm],
        }
    }

    /// 消すまでのタイマーの時間が来た（autopop）。同じ部品の上では出し直さない。
    pub fn pop_timer(&mut self) -> Vec<TooltipAction> {
        self.dismiss()
    }

    /// 押した、ホイールを回した、キーを押した。出ていれば隠し、同じ部品の上では出し直さない。
    pub fn dismiss(&mut self) -> Vec<TooltipAction> {
        self.dismissed = true;
        let was_visible = std::mem::take(&mut self.visible);
        let mut out = vec![TooltipAction::Disarm];
        if was_visible {
            out.push(TooltipAction::Hide);
        }
        out
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    const D: Delays = Delays {
        initial: 500,
        reshow: 100,
        autopop: 5000,
    };

    fn tip(id: &'static str) -> Option<TooltipContent> {
        Some(TooltipContent::Command(id))
    }

    #[test]
    fn 部品の上で止まって_initial_待つと出し_autopop_で消す() {
        let mut d = Dwell::default();
        assert_eq!(d.target(tip("a"), D), [TooltipAction::ArmShow(500)]);
        assert_eq!(
            d.show_timer(D),
            [
                TooltipAction::Show(TooltipContent::Command("a")),
                TooltipAction::ArmPop(5000)
            ]
        );
        assert_eq!(d.pop_timer(), [TooltipAction::Disarm, TooltipAction::Hide]);
        assert_eq!(
            d.target(tip("a"), D),
            Vec::<TooltipAction>::new(),
            "同じ部品の上では出し直さない"
        );
        assert_eq!(d.show_timer(D), [TooltipAction::Disarm]);
    }

    #[test]
    fn 出ている間に別の部品へ移ると_隠して_reshow_で差し替える() {
        let mut d = Dwell::default();
        d.target(tip("a"), D);
        d.show_timer(D);
        assert_eq!(
            d.target(tip("b"), D),
            [TooltipAction::Hide, TooltipAction::ArmShow(100)]
        );
        assert_eq!(
            d.show_timer(D),
            [
                TooltipAction::Show(TooltipContent::Command("b")),
                TooltipAction::ArmPop(5000)
            ]
        );
    }

    #[test]
    fn 出る前に別の部品へ移れば_initial_から数え直し_部品の外へ出れば止める() {
        let mut d = Dwell::default();
        d.target(tip("a"), D);
        assert_eq!(d.target(tip("b"), D), [TooltipAction::ArmShow(500)]);
        assert_eq!(d.target(None, D), [TooltipAction::Disarm]);
        assert_eq!(
            d.show_timer(D),
            [TooltipAction::Disarm],
            "対象が無ければ出さない"
        );
    }

    #[test]
    fn 押したら隠し_別の部品へ移るまで出し直さない() {
        let mut d = Dwell::default();
        d.target(tip("a"), D);
        d.show_timer(D);
        assert_eq!(d.dismiss(), [TooltipAction::Disarm, TooltipAction::Hide]);
        assert_eq!(d.show_timer(D), [TooltipAction::Disarm]);
        assert_eq!(d.target(tip("b"), D), [TooltipAction::ArmShow(500)]);
        assert_eq!(d.show_timer(D).len(), 2, "別の部品では出す");
    }
}
