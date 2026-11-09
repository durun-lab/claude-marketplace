//! `Ui` の再生の操作のバー。状態を受け、ポインターの入力からボタンとジョグバーの操作を返し、矩形と文字を作る。

use mm_ui_layout::{transport_quads, PointerEvent, Quad, TransportAction, ViewerState};

use super::Ui;

impl Ui {
    /// 再生の操作のバーの状態。`program` は再生中か、押せないコマンドの ID（再生位置、長さ、フレームレートは `Ui` が埋める）、
    /// `source` は Source を開いているときの状態（Source のバーは `Some` の間だけ出る）。
    pub fn set_transport(&mut self, program: ViewerState, source: Option<ViewerState>) {
        self.transport.program = program;
        self.transport.source = source;
    }

    /// サーフェスの座標（物理ピクセル）のポインターの入力を再生の操作のバーに渡し、起きた操作を返す。
    pub fn transport_pointer(&mut self, ev: PointerEvent) -> Vec<TransportAction> {
        self.sync_program();
        let logical = self.scale.pointer_to_logical(ev);
        self.transport_controller
            .handle(logical, &self.layout, &self.transport)
    }

    /// Program のバーの再生位置、長さ、フレームレートを今の値にする。
    fn sync_program(&mut self) {
        let p = &mut self.transport.program;
        p.t = self.playhead;
        p.duration = self.model.duration;
        p.fps = self.model.fps;
    }

    /// バーの矩形。サーフェスの座標（物理ピクセル）。
    pub(super) fn transport_quads(&mut self) -> Vec<Quad> {
        self.sync_program();
        let mut quads = transport_quads(
            &self.layout,
            &self.transport,
            &self.transport_controller,
            &self.palette,
        );
        for q in &mut quads {
            q.rect = self.scale.rect_to_physical(q.rect);
        }
        quads
    }
}
