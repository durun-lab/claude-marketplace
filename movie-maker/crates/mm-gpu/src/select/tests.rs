use super::*;

fn gpus() -> Vec<AdapterCandidate> {
    vec![
        AdapterCandidate {
            name: "NVIDIA GeForce RTX 5090".into(),
            luid: Some(0x0000_0000_0001_2345),
        },
        AdapterCandidate {
            name: "AMD Radeon(TM) Graphics".into(),
            luid: Some(0x0000_0001_0000_abcd),
        },
        AdapterCandidate {
            name: "Microsoft Basic Render Driver".into(),
            luid: None,
        },
    ]
}

fn pick(text: &str) -> Result<usize> {
    let selector = GpuSelector::parse(text)?.expect("指定あり");
    choose(GPU_ENV, &selector, &gpus())
}

#[test]
fn empty_means_no_selection() {
    assert_eq!(GpuSelector::parse("").unwrap(), None);
    assert_eq!(GpuSelector::parse("  ").unwrap(), None);
}

#[test]
fn name_part_ignores_case() {
    assert_eq!(pick("5090").unwrap(), 0);
    assert_eq!(pick("radeon").unwrap(), 1);
    assert_eq!(pick(" basic render ").unwrap(), 2);
}

#[test]
fn digits_are_a_name_not_an_index() {
    assert_eq!(
        GpuSelector::parse("5090").unwrap(),
        Some(GpuSelector::Name("5090".into()))
    );
}

#[test]
fn luid_is_hex_with_optional_0x() {
    assert_eq!(pick("luid:12345").unwrap(), 0);
    assert_eq!(pick("LUID:0x000000010000ABCD").unwrap(), 1);
}

#[test]
fn index_counts_from_zero() {
    assert_eq!(pick("index:0").unwrap(), 0);
    assert_eq!(pick("index:2").unwrap(), 2);
}

#[test]
fn first_match_wins() {
    assert_eq!(pick("r").unwrap(), 0);
}

#[test]
fn unreadable_values_explain_the_forms() {
    for text in ["luid:xyz", "index:-1", "index:a"] {
        let err = GpuSelector::parse(text).unwrap_err().to_string();
        assert!(err.contains(text), "{err}");
        assert!(err.contains("名前の一部"), "{err}");
    }
}

#[test]
fn no_match_lists_what_is_there() {
    let err = pick("intel").unwrap_err().to_string();
    assert!(err.contains("MM_GPU=intel"), "{err}");
    assert!(
        err.contains("index:0 NVIDIA GeForce RTX 5090 (luid:0000000000012345)"),
        "{err}"
    );
    assert!(
        err.contains("index:2 Microsoft Basic Render Driver"),
        "{err}"
    );
    let err = pick("index:3").unwrap_err().to_string();
    assert!(err.contains("MM_GPU=index:3"), "{err}");
}

#[test]
fn display_round_trips() {
    for text in ["radeon", "luid:000000010000abcd", "index:1"] {
        let selector = GpuSelector::parse(text).unwrap().unwrap();
        assert_eq!(selector.to_string(), text);
        assert_eq!(
            GpuSelector::parse(&selector.to_string()).unwrap(),
            Some(selector)
        );
    }
}
