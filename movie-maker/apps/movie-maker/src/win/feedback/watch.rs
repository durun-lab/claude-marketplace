//! 計装の見張り。予算を超えたスパンと、ホストのクラッシュの回数のカウンタ（`mm_trace::names::host_crash_counter`）を
//! 自動の起票に回す `Layer`。同じものは mm-improve がまとめて数えるが、記録はファイルの読み書きなので、ここで間を空けて送る。

use std::collections::HashMap;
use std::sync::{Arc, Mutex};
use std::time::{Duration, Instant};

use mm_improve::{ErrorInput, ErrorSource};
use tracing::field::{Field, Visit};
use tracing::span::{Attributes, Id};
use tracing::{Event, Subscriber};
use tracing_subscriber::layer::{Context, Layer};
use tracing_subscriber::registry::LookupSpan;

/// 60 fps の 1 フレーム（ms）。
const FRAME_MS: f64 = 1000.0 / 60.0;

/// 予算（ms）。docs/design/17-instrumentation.md の「予算の表」の行のうち、アプリで 1 回ごとに測れるもの。
/// プレビューの 1 フレームはデコードと表示を含めて 1 フレームに収める目標、GPU は 6 ms、シークは原本の 99%。
pub const BUDGETS: &[(&str, f64)] = &[
    ("decode.seek", 150.0),
    ("preview.render_frame", FRAME_MS),
    ("render.gpu_frame", 6.0),
];

/// 起動の直後はシェーダーのコンパイルや素材を開く待ちで遅いのが普通なので、遅い処理を数えない。
const WARMUP: Duration = Duration::from_secs(10);
/// 同じスパンかホストを続けて送らない間隔。
const THROTTLE: Duration = Duration::from_secs(10);

/// ホストの種類（`mm-host-pipe` の `PipeKind::name`）と、起票の出どころ。
const HOSTS: &[(&str, ErrorSource)] = &[
    ("host_cef", ErrorSource::HostCef),
    ("host_ofx", ErrorSource::HostOfx),
];

/// 起票の送り先。
pub type Sink = Arc<dyn Fn(ErrorInput) + Send + Sync>;

/// スパンの開いた時刻。
struct Opened(Instant);

/// 見張りの `Layer`。
pub struct Watch {
    sink: Sink,
    budgets: &'static [(&'static str, f64)],
    since: Instant,
    warmup: Duration,
    last_sent: Mutex<HashMap<String, Instant>>,
}

/// アプリの見張り。起票はアプリのサービス（[`super::service`]）へ送り、サービスが無ければ捨てる。
pub fn watch_layer() -> Watch {
    let sink: Sink = Arc::new(|input| {
        if let Some(svc) = super::service() {
            svc.report(input);
        }
    });
    Watch::new(sink, BUDGETS, WARMUP)
}

impl Watch {
    /// 送り先、予算、起動の直後に数えない長さを決めて作る。
    pub fn new(sink: Sink, budgets: &'static [(&'static str, f64)], warmup: Duration) -> Self {
        Self {
            sink,
            budgets,
            since: Instant::now(),
            warmup,
            last_sent: Mutex::new(HashMap::new()),
        }
    }

    fn budget_of(&self, name: &str) -> Option<f64> {
        self.budgets
            .iter()
            .find(|(n, _)| *n == name)
            .map(|(_, ms)| *ms)
    }

    /// `key` を前に送ってから `THROTTLE` 経っていれば、送ったことにして `true`。
    fn allow(&self, key: &str) -> bool {
        let now = Instant::now();
        let mut last = self.last_sent.lock().unwrap_or_else(|e| e.into_inner());
        match last.get(key) {
            Some(at) if now.duration_since(*at) < THROTTLE => false,
            _ => {
                last.insert(key.to_owned(), now);
                true
            }
        }
    }

    fn slow(&self, name: &str, ms: f64) {
        let Some(budget) = self.budget_of(name) else {
            return;
        };
        if ms <= budget || self.since.elapsed() < self.warmup || !self.allow(name) {
            return;
        }
        (self.sink)(ErrorInput::slow(ErrorSource::App, name, ms, budget));
    }

    fn crash(&self, host: &str) {
        let Some(source) = host_source(host) else {
            return;
        };
        if !self.allow(host) {
            return;
        }
        let mut input = ErrorInput::error(
            source,
            format!("{host} のホストがクラッシュした（応答の途中で終わったか、要求の合間に終わっていた）"),
        );
        input.where_ = Some(host.to_owned());
        (self.sink)(input);
    }
}

fn host_source(host: &str) -> Option<ErrorSource> {
    HOSTS.iter().find(|(h, _)| *h == host).map(|(_, s)| *s)
}

/// イベントのうち見るフィールド。
#[derive(Default)]
struct Fields {
    metric: Option<String>,
    name: Option<String>,
    value: Option<f64>,
}

impl Visit for Fields {
    fn record_f64(&mut self, field: &Field, value: f64) {
        if field.name() == "value" {
            self.value = Some(value);
        }
    }

    fn record_str(&mut self, field: &Field, value: &str) {
        match field.name() {
            "metric" => self.metric = Some(value.to_owned()),
            "name" => self.name = Some(value.to_owned()),
            _ => {}
        }
    }

    fn record_debug(&mut self, field: &Field, value: &dyn std::fmt::Debug) {
        let text = format!("{value:?}");
        match field.name() {
            "name" => self.name = Some(text.trim_matches('"').to_owned()),
            "metric" => self.metric = Some(text.trim_matches('"').to_owned()),
            _ => {}
        }
    }
}

impl<S> Layer<S> for Watch
where
    S: Subscriber + for<'a> LookupSpan<'a>,
{
    fn on_new_span(&self, attrs: &Attributes<'_>, id: &Id, ctx: Context<'_, S>) {
        if self.budget_of(attrs.metadata().name()).is_none() {
            return;
        }
        if let Some(span) = ctx.span(id) {
            span.extensions_mut().insert(Opened(Instant::now()));
        }
    }

    fn on_close(&self, id: Id, ctx: Context<'_, S>) {
        let Some(span) = ctx.span(&id) else {
            return;
        };
        let opened = span.extensions().get::<Opened>().map(|o| o.0);
        if let Some(at) = opened {
            self.slow(span.name(), at.elapsed().as_secs_f64() * 1000.0);
        }
    }

    fn on_event(&self, event: &Event<'_>, _ctx: Context<'_, S>) {
        if event.metadata().target() != mm_trace::METRICS_TARGET {
            return;
        }
        let mut fields = Fields::default();
        event.record(&mut fields);
        let Some(name) = &fields.name else {
            return;
        };
        match (fields.metric.as_deref(), fields.value) {
            // 外で測った時間（GPU のタイムスタンプ）は `record_span` のイベントでマイクロ秒が届く。
            (Some("span"), Some(us)) => self.slow(name, us / 1000.0),
            (Some("counter"), _) => {
                if let Some(host) = mm_trace::names::crashed_host(name) {
                    self.crash(host);
                }
            }
            _ => {}
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use tracing_subscriber::prelude::*;

    const TEST_BUDGETS: &[(&str, f64)] = &[("test.slow", 1.0)];

    fn capture(f: impl FnOnce()) -> Vec<ErrorInput> {
        let seen = Arc::new(Mutex::new(Vec::new()));
        let sink_seen = seen.clone();
        let sink: Sink = Arc::new(move |i| sink_seen.lock().unwrap().push(i));
        let layer = Watch::new(sink, TEST_BUDGETS, Duration::ZERO);
        let subscriber = tracing_subscriber::registry().with(layer);
        tracing::subscriber::with_default(subscriber, f);
        let out = seen.lock().unwrap().clone();
        out
    }

    #[test]
    fn 予算を超えたスパンを遅い処理として_1_回だけ送る() {
        let seen = capture(|| {
            for _ in 0..2 {
                let _s = tracing::info_span!("test.slow").entered();
                std::thread::sleep(Duration::from_millis(5));
            }
            let _fast = tracing::info_span!("test.other").entered();
        });
        assert_eq!(seen.len(), 1, "{seen:?}");
        assert_eq!(seen[0].span.as_deref(), Some("test.slow"));
        assert!(seen[0].ms.unwrap() > 1.0);
        assert_eq!(seen[0].budget_ms, Some(1.0));
    }

    #[test]
    fn record_span_で外から測った時間も予算と比べる() {
        let seen = capture(|| {
            mm_trace::record_span("test.slow", Duration::from_micros(500));
            mm_trace::record_span("test.slow", Duration::from_millis(3));
        });
        assert_eq!(seen.len(), 1, "{seen:?}");
        assert_eq!(seen[0].ms, Some(3.0));
    }

    #[test]
    fn ホストのクラッシュの回数のカウンタをクラッシュとして送る() {
        let cef = mm_trace::names::host_crash_counter("host_cef");
        let ofx = mm_trace::names::host_crash_counter("host_ofx");
        let seen = capture(|| {
            mm_trace::counter!(cef.as_str(), 1);
            mm_trace::counter!(cef.as_str(), 1);
            mm_trace::counter!(ofx.as_str(), 1);
            mm_trace::counter!("cache.hit", 1);
            tracing::warn!(target: "mm_nodes_cef::hosts", why = "ログの文", "web.cef のホストを起こし直す");
        });
        let sources: Vec<ErrorSource> = seen.iter().map(|i| i.source).collect();
        assert_eq!(
            sources,
            vec![ErrorSource::HostCef, ErrorSource::HostOfx],
            "{seen:?}"
        );
        assert_eq!(seen[0].where_.as_deref(), Some("host_cef"));
    }

    #[test]
    fn 起動の直後は遅い処理を数えない() {
        let seen = Arc::new(Mutex::new(Vec::new()));
        let sink_seen = seen.clone();
        let sink: Sink = Arc::new(move |i| sink_seen.lock().unwrap().push(i));
        let layer = Watch::new(sink, TEST_BUDGETS, Duration::from_secs(60));
        tracing::subscriber::with_default(tracing_subscriber::registry().with(layer), || {
            mm_trace::record_span("test.slow", Duration::from_millis(3));
        });
        assert!(seen.lock().unwrap().is_empty());
    }
}
