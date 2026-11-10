use super::*;

fn over(hit: Hit, tool: Tool) -> Pointee {
    Pointee {
        under: Under::Timeline(hit),
        tool,
        ..Pointee::default()
    }
}

const ALT: Mods = Mods {
    shift: false,
    alt: true,
};
const SHIFT: Mods = Mods {
    shift: true,
    alt: false,
};
const NONE: Mods = Mods {
    shift: false,
    alt: false,
};

fn cut(part: Part) -> Hit {
    Hit::Cut {
        item: 0,
        cut: 1,
        part,
    }
}

fn layer(part: Part) -> Hit {
    Hit::Layer { index: 0, part }
}

#[test]
fn カットの端は左右の矢印で_alt_では_roll_の形() {
    for tool in [Tool::Selection, Tool::Trim] {
        let p = over(cut(Part::OutHandle), tool);
        assert_eq!(cursor_for(&p, NONE), CursorKind::SizeWE, "{tool:?}");
        assert_eq!(cursor_for(&p, ALT), CursorKind::Roll, "{tool:?}");
    }
}

#[test]
fn レイヤーの端は_alt_でも左右の矢印() {
    let p = over(layer(Part::InHandle), Tool::Selection);
    assert_eq!(cursor_for(&p, ALT), CursorKind::SizeWE);
}

#[test]
fn trim_のカットの中は_slip_で_shift_では_slide() {
    let p = over(cut(Part::Body), Tool::Trim);
    assert_eq!(cursor_for(&p, NONE), CursorKind::Slip);
    assert_eq!(cursor_for(&p, SHIFT), CursorKind::Slide);
    let gap = Pointee { gap: true, ..p };
    assert_eq!(
        cursor_for(&gap, NONE),
        CursorKind::Arrow,
        "隙間は slip しない"
    );
    assert_eq!(
        cursor_for(&gap, SHIFT),
        CursorKind::Slide,
        "隙間も slide する"
    );
}

#[test]
fn 仕切りは縦が左右_横が上下の矢印() {
    assert_eq!(splitter_cursor(Splitter::Vertical), CursorKind::SizeWE);
    assert_eq!(splitter_cursor(Splitter::Horizontal), CursorKind::SizeNS);
}

#[test]
fn selection_の中は矢印() {
    assert_eq!(
        cursor_for(&over(cut(Part::Body), Tool::Selection), NONE),
        CursorKind::Arrow
    );
    assert_eq!(
        cursor_for(&over(layer(Part::Body), Tool::Trim), SHIFT),
        CursorKind::Arrow
    );
}

#[test]
fn blade_は割れるカットの上で刃_レイヤーと隙間と割れない所は禁止() {
    let ready = Pointee {
        blade_ready: true,
        ..over(cut(Part::Body), Tool::Blade)
    };
    assert_eq!(cursor_for(&ready, NONE), CursorKind::Blade);
    let edge = Pointee {
        blade_ready: true,
        ..over(cut(Part::InHandle), Tool::Blade)
    };
    assert_eq!(
        cursor_for(&edge, NONE),
        CursorKind::Blade,
        "端のつまみの上も割る"
    );
    let gap = Pointee { gap: true, ..ready };
    assert_eq!(cursor_for(&gap, NONE), CursorKind::No);
    let early = over(cut(Part::Body), Tool::Blade);
    assert_eq!(cursor_for(&early, NONE), CursorKind::No);
    assert_eq!(
        cursor_for(&over(layer(Part::Body), Tool::Blade), NONE),
        CursorKind::No
    );
}

#[test]
fn ドラッグ中は押したときの種類のままで_修飾キーの変化に従わない() {
    let drag = |kind| Pointee {
        dragging: Some(Dragging {
            kind,
            by: None,
            snap: None,
        }),
        ..over(cut(Part::OutHandle), Tool::Selection)
    };
    assert_eq!(cursor_for(&drag(EditDrag::Ripple), ALT), CursorKind::SizeWE);
    assert_eq!(cursor_for(&drag(EditDrag::Roll), NONE), CursorKind::Roll);
    assert_eq!(cursor_for(&drag(EditDrag::Slide), NONE), CursorKind::Slide);
    assert_eq!(
        cursor_for(&drag(EditDrag::MoveKeyframe), ALT),
        CursorKind::Arrow
    );
}

#[test]
fn ズームバーの端は左右の矢印_ツールバーと映像は矢印() {
    let end = Hit::ZoomBar {
        part: ZoomBarPart::EndEdge,
    };
    assert_eq!(
        cursor_for(&over(end, Tool::Selection), NONE),
        CursorKind::SizeWE
    );
    assert_eq!(cursor_for(&Pointee::default(), NONE), CursorKind::Arrow);
    let bar = Pointee {
        under: Under::Toolbar(ToolbarPart::Slider),
        ..Pointee::default()
    };
    assert_eq!(cursor_for(&bar, NONE), CursorKind::Arrow);
}
