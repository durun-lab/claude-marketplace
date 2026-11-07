use mm_ops::Op;
use mm_story::{is_mobile_op, MOBILE_OPS};

#[test]
fn モバイルの操作は全部_mm_ops_に実在する() {
    for name in MOBILE_OPS {
        assert!(Op::NAMES.contains(name), "mm-ops に無い操作: {name}");
    }
}

#[test]
fn モバイルの操作に重複が無い() {
    let mut names = MOBILE_OPS.to_vec();
    names.sort_unstable();
    names.dedup();
    assert_eq!(names.len(), MOBILE_OPS.len());
}

#[test]
fn 端の詰めやレイヤーの操作は断る() {
    assert!(is_mobile_op("cutWords"));
    assert!(is_mobile_op("insertTopics"));
    for name in ["ripple", "trimLayer", "addLayer", "setParam", "removeAsset"] {
        assert!(!is_mobile_op(name), "{name}");
    }
}
