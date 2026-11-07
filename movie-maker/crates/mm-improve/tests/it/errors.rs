//! 自動の起票。同じものは 1 件、未対応の上限、同じものの起票の上限、1 時間あたりの上限。

use mm_improve::{
    ErrorBook, ErrorInput, ErrorSource, FeedbackKind, FeedbackStatus, FeedbackStore, Load,
    RecentLanding, MAX_FILINGS_PER_HOUR, MAX_PENDING,
};

const NOW: u64 = 1_800_000_000_000;

fn store() -> (tempfile::TempDir, FeedbackStore) {
    let dir = tempfile::tempdir().unwrap();
    let store = FeedbackStore::new(dir.path().join("feedback"));
    (dir, store)
}

fn err(msg: &str) -> ErrorInput {
    ErrorInput::error(ErrorSource::App, msg)
}

#[test]
fn same_error_with_different_numbers_is_one_item() {
    let (_d, store) = store();
    let mut book = ErrorBook::default();
    let first = book
        .record(&err("frame 12 failed"), &store, NOW, None)
        .unwrap();
    let second = book
        .record(&err("frame 99 failed"), &store, NOW + 1, None)
        .unwrap();

    assert!(first.is_some());
    assert_eq!(second, None);
    assert_eq!(store.pending().unwrap().len(), 1);
    assert_eq!(book.records.values().next().unwrap().count, 2);
}

#[test]
fn pending_auto_items_are_capped_per_kind() {
    let (_d, store) = store();
    let mut book = ErrorBook::default();
    for name in ["a", "b", "c", "d"] {
        book.record(&err(&format!("{name} broke")), &store, NOW, None)
            .unwrap();
    }
    let slow = ErrorInput::slow(ErrorSource::Webview, "panel.render", 80.0, 16.0);
    let filed_slow = book.record(&slow, &store, NOW, None).unwrap();

    let pending = store.pending().unwrap();
    assert_eq!(
        pending
            .iter()
            .filter(|i| i.kind == FeedbackKind::Error)
            .count(),
        MAX_PENDING
    );
    assert!(filed_slow.is_some(), "遅い処理はエラーと別に数える");
}

#[test]
fn recurrence_after_done_is_filed_again_up_to_the_limit() {
    let (_d, store) = store();
    let mut book = ErrorBook::default();
    for round in 0..3u64 {
        let filed = book
            .record(&err("host crashed"), &store, NOW + round, None)
            .unwrap();
        assert_eq!(
            filed.is_some(),
            round < 2,
            "起票は 2 回まで（{round} 回目）"
        );
        if let Some(id) = filed {
            let mut item = store.get(&id).unwrap();
            item.status = FeedbackStatus::Done;
            store.put(&item).unwrap();
            book.reset_item(&id);
        }
    }
    let text = &store.all().unwrap()[0].text;
    assert!(text.contains("前回の要望"), "{text}");
}

#[test]
fn hourly_limit_stops_auto_filing() {
    let (_d, store) = store();
    let mut book = ErrorBook {
        filed_at: vec![NOW - 1000; MAX_FILINGS_PER_HOUR],
        ..Default::default()
    };
    assert_eq!(
        book.record(&err("x broke"), &store, NOW, None).unwrap(),
        None
    );
    book.filed_at = vec![NOW - 3_700_000; MAX_FILINGS_PER_HOUR];
    assert!(
        book.record(&err("y broke"), &store, NOW, None)
            .unwrap()
            .is_some(),
        "1 時間より前は数えない"
    );
}

#[test]
fn slow_item_has_load_trace_query_and_suspect_run() {
    let (_d, store) = store();
    let mut book = ErrorBook::default();
    let mut slow = ErrorInput::slow(ErrorSource::App, "render.frame", 41.5, 16.0);
    slow.load = Some(Load {
        cpu: 3.2,
        gpu: Some(97.0),
    });
    slow.trace = Some("C:/t/traces/1.json".into());
    let landing = RecentLanding {
        run_id: "20261107-120000-abcd",
        at: NOW - 60_000,
    };

    let id = book
        .record(&slow, &store, NOW, Some(landing))
        .unwrap()
        .unwrap();

    let item = store.get(&id).unwrap();
    assert_eq!(item.kind, FeedbackKind::Slow);
    for want in [
        "最長 41.5 ms（予算 16 ms）",
        "GPU 97%",
        "mm trace query \"C:/t/traces/1.json\"",
        "name = 'render.frame'",
        "20261107-120000-abcd",
    ] {
        assert!(item.text.contains(want), "{want} が無い: {}", item.text);
    }
    assert_eq!(item.context["error"]["slowestMs"], 41.5);
}
