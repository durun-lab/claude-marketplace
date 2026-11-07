//! 関門: MCP からの承認、承認の後の変更、上限の超過、自動の改善の回の中。

use std::process::Command;
use std::sync::Arc;

use mm_nodes_genai::{
    current_round, media_dir, FakePrices, FakeProvider, GenAi, GenError, Providers,
    IMPROVE_ROUND_ENV,
};
use mm_ops::{Entry, OpContext, ParamRef, SetParam, Transaction};
use mm_types::ParamValue;

use crate::common::{editor_with_layer, genai, genai_node};

/// 子のプロセスで走らせる試験の名前。
const CHILD_TEST: &str = "gate::子_回の印の下では承認できない";

#[test]
fn mcp_の経路からは承認できず_cli_からはできる() {
    let dir = tempfile::tempdir().unwrap();
    let (gen, _) = genai(&media_dir(dir.path()));
    let (ed, layer) = editor_with_layer(genai_node(&[]));
    let q = gen.quote(ed.project(), &layer).unwrap();
    assert!(matches!(
        gen.approve(&q, Entry::Mcp),
        Err(GenError::Forbidden(_))
    ));
    assert_eq!(gen.approve(&q, Entry::Cli).unwrap().entry(), Entry::Cli);
}

#[test]
fn 承認の後にプロンプトを変えたら生成しない() {
    let dir = tempfile::tempdir().unwrap();
    let (gen, fake) = genai(&media_dir(dir.path()));
    let (mut ed, layer) = editor_with_layer(genai_node(&[]));
    let approval = gen
        .approve(&gen.quote(ed.project(), &layer).unwrap(), Entry::Surface)
        .unwrap();
    let edit = SetParam {
        target: ParamRef::layer(&layer, "source.prompt"),
        value: ParamValue::Text("嵐の海".into()),
        keyframes: None,
    };
    ed.apply(
        Transaction::new("変える", vec![edit.into()]),
        &OpContext::human(),
    )
    .unwrap();
    let err = gen.generate(ed.project(), &approval).unwrap_err();
    assert!(matches!(err, GenError::NotApproved(_)), "{err}");
    assert_eq!(fake.calls(), 0);
}

#[test]
fn 見積もりが承認した上限を超えたら生成しない() {
    let dir = tempfile::tempdir().unwrap();
    let (cheap, _) = genai(&media_dir(dir.path()));
    let pricey = Arc::new(FakeProvider::new().with_prices(FakePrices {
        per_image_usd: 1.0,
        ..FakePrices::default()
    }));
    let raised = GenAi::new(
        media_dir(dir.path()),
        Providers::new().with(pricey.clone()).unwrap(),
    );
    let (ed, layer) = editor_with_layer(genai_node(&[]));
    let approval = cheap
        .approve(&cheap.quote(ed.project(), &layer).unwrap(), Entry::Surface)
        .unwrap();
    let err = raised.generate(ed.project(), &approval).unwrap_err();
    assert!(matches!(err, GenError::OverBudget { .. }), "{err}");
    assert_eq!(pricey.calls(), 0);
}

#[test]
fn 自動の改善の回の中では承認も生成もできない() {
    let dir = tempfile::tempdir().unwrap();
    let (gen, fake) = genai(&media_dir(dir.path()));
    let (ed, layer) = editor_with_layer(genai_node(&[]));
    let q = gen.quote(ed.project(), &layer).unwrap();
    let approval = gen.approve(&q, Entry::Surface).unwrap();
    let fake2 = Arc::new(FakeProvider::new());
    let in_round = GenAi::new(
        media_dir(dir.path()),
        Providers::new().with(fake2.clone()).unwrap(),
    )
    .in_round("r1");
    assert!(matches!(
        in_round.approve(&q, Entry::Surface),
        Err(GenError::Forbidden(_))
    ));
    let err = in_round.generate(ed.project(), &approval).unwrap_err();
    assert!(matches!(err, GenError::Forbidden(_)), "{err}");
    assert_eq!(fake.calls() + fake2.calls(), 0);
}

#[test]
fn 回の印の名前は_mm_improve_が渡すものと同じ() {
    assert_eq!(IMPROVE_ROUND_ENV, mm_improve::ROUND_ENV);
}

/// 環境変数はプロセスの全体に効くので、印を付けた子のプロセスでこの試験の片割れを走らせる。
#[test]
fn 回の印の環境変数があれば承認できない() {
    let exe = std::env::current_exe().unwrap();
    let out = Command::new(exe)
        .args([CHILD_TEST, "--exact", "--nocapture"])
        .env(IMPROVE_ROUND_ENV, "r-child")
        .output()
        .unwrap();
    let stdout = String::from_utf8_lossy(&out.stdout);
    assert!(out.status.success(), "{stdout}");
    assert!(
        stdout.contains("1 passed"),
        "子の試験が走っていない: {stdout}"
    );
}

/// 子のプロセスの片割れ。印が無いとき（普通の実行）は何もしない。
#[test]
fn 子_回の印の下では承認できない() {
    if current_round().is_none() {
        return;
    }
    let dir = tempfile::tempdir().unwrap();
    let (gen, fake) = genai(&media_dir(dir.path()));
    let (ed, layer) = editor_with_layer(genai_node(&[]));
    let q = gen.quote(ed.project(), &layer).unwrap();
    assert!(matches!(
        gen.approve(&q, Entry::Surface),
        Err(GenError::Forbidden(_))
    ));
    assert_eq!(fake.calls(), 0);
}
