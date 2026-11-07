use super::*;

#[test]
fn 何もしていなければ届いたパッチをすぐ当てる() {
    let mut gate = Gate::new();
    assert_eq!(gate.offer(1), Some(1));
    assert!(!gate.held(), "当てたら印を出さない");
}

#[test]
fn 入力中に届いたパッチは入力が終わるまで持つ() {
    let mut gate = Gate::new();
    assert_eq!(gate.set(Busy::Input, true), None);
    assert_eq!(gate.offer(1), None, "ドラッグの途中では当てない");
    assert!(gate.held(), "待たせている間は印を出す");
    assert_eq!(gate.set(Busy::Input, false), Some(1));
    assert!(!gate.held());
}

#[test]
fn 入力と書き出しの両方が終わってから当てる() {
    let mut gate = Gate::new();
    gate.set(Busy::Export, true);
    gate.set(Busy::Input, true);
    assert_eq!(gate.offer(1), None);
    assert_eq!(
        gate.set(Busy::Input, false),
        None,
        "書き出しはまだ動いている"
    );
    assert_eq!(gate.set(Busy::Export, false), Some(1));
}

#[test]
fn 待たせている間に次のパッチが届いたら新しい方だけ当てる() {
    let mut gate = Gate::new();
    gate.set(Busy::Export, true);
    assert_eq!(gate.offer(1), None);
    assert_eq!(gate.offer(2), None);
    assert_eq!(gate.set(Busy::Export, false), Some(2));
    assert_eq!(
        gate.set(Busy::Export, false),
        None,
        "当てたものは二度当てない"
    );
}

#[test]
fn 終わりの知らせが重なっても待たせていなければ何も当てない() {
    let mut gate: Gate<u8> = Gate::new();
    assert_eq!(gate.set(Busy::Input, false), None);
    assert_eq!(gate.set(Busy::Input, false), None);
}

#[test]
fn 入口は渡した関数の値を返す() {
    let base = 40;
    let step = 2;
    assert_eq!(call(|| base + step), 42);
}
