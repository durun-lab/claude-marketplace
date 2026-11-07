//! 音の効果の種類（`audio.clap`、`audio.vst3`）。レイヤーの `effects` に置くと、レンダラーがレイヤーの音にかける。
//!
//! パラメータ: `plugin`（プラグインのファイル。規格の置き場所の中だけ）、`plugin_id`（空ならファイルの最初のプラグイン）、
//! `param.<パラメータの id>`（[`super::ParamDesc`] の値。キーフレームを打てる）。
//! 効果のインスタンス（`AudioEffectCx::instance`）ごとにホストを 1 つ持ち、区間が前の区間の続きでなければ
//! プラグインの状態を消す（ミキサーのダッキングと同じ扱い）。

use std::collections::{BTreeMap, HashMap};
use std::path::PathBuf;
use std::sync::{Mutex, PoisonError};
use std::time::Duration;

use mm_audio_api::AudioBlock;
use mm_host_api::HostSpec;
use mm_node_api::{AudioEffectCx, Cost, FrameCx, FrameRef, Isolation, NodeKind, Traits};
use mm_types::{
    MmError, ParamSchema, ParamSpec, ParamType, ParamValue, Params, RationalTime, Result,
};

use super::effect::{AudioEffect, ParamPoint};
use super::PluginStandard;

/// プラグインのファイルのパラメータの名前。
pub const PLUGIN_PARAM: &str = "plugin";
/// プラグインの `id` のパラメータの名前。
pub const PLUGIN_ID_PARAM: &str = "plugin_id";
/// プラグインのパラメータの名前の接頭辞。後ろにパラメータの `id` を 10 進で付ける。
pub const PARAM_PREFIX: &str = "param.";
const KIND_VERSION: u32 = 1;
/// 1 回の往復の既定の期限。プラグインの読み込みを含む。
const DEFAULT_TIMEOUT: Duration = Duration::from_secs(10);

/// 1 つの効果のインスタンス。
struct Instance<S: PluginStandard> {
    effect: AudioEffect<S>,
    path: String,
    plugin: Option<String>,
    /// 直前に処理した区間の終わり。次の区間がここから始まれば続き。
    next_start: Option<RationalTime>,
}

/// 音の効果の種類。組み立て（レンダラー）ごとに 1 つ作り、インスタンスの状態を持つ。`S` は規格。
pub struct PluginEffectKind<S: PluginStandard> {
    executable: Option<PathBuf>,
    timeout: Duration,
    /// `plugin` が指してよい置き場所。
    roots: Vec<PathBuf>,
    instances: Mutex<HashMap<String, Instance<S>>>,
}

impl<S: PluginStandard> PluginEffectKind<S> {
    /// `executable` のホストで処理する。`None` なら処理のたびに `Unsupported`。
    pub fn new(executable: Option<PathBuf>) -> Self {
        Self {
            executable,
            timeout: DEFAULT_TIMEOUT,
            roots: S::plugin_dirs(),
            instances: Mutex::new(HashMap::new()),
        }
    }

    /// 1 回の往復の期限を変える。
    pub fn with_timeout(mut self, timeout: Duration) -> Self {
        self.timeout = timeout;
        self
    }

    /// `plugin` が指してよい置き場所を `roots` に替える（テストのプラグインの置き場所など）。
    pub fn with_roots(mut self, roots: Vec<PathBuf>) -> Self {
        self.roots = roots;
        self
    }

    /// 動いているインスタンスの数。
    pub fn instance_count(&self) -> usize {
        self.lock().len()
    }

    /// インスタンスのホストのプロセス ID（試験でクラッシュを起こすため）。
    pub fn instance_pid(&self, instance: &str) -> Option<u32> {
        self.lock().get(instance).and_then(|i| i.effect.pid())
    }

    fn lock(&self) -> std::sync::MutexGuard<'_, HashMap<String, Instance<S>>> {
        self.instances
            .lock()
            .unwrap_or_else(PoisonError::into_inner)
    }

    fn spec(&self) -> Result<HostSpec> {
        let executable = self.executable.clone().ok_or_else(|| {
            MmError::Unsupported(format!("{} の実行ファイルが見つからない", S::HOST_STEM))
        })?;
        Ok(HostSpec {
            executable,
            args: Vec::new(),
            default_timeout: self.timeout,
            gpu: None,
            client_pid: None,
        })
    }
}

/// プラグインのパラメータの名前（`param.<id>`）。
pub fn param_key(id: u32) -> String {
    format!("{PARAM_PREFIX}{id}")
}

/// 評価済みのパラメータから、プラグインのパラメータの値を取り出す。
fn plugin_values(params: &Params) -> BTreeMap<u32, f64> {
    params
        .0
        .iter()
        .filter_map(|(k, v)| {
            let id = k.strip_prefix(PARAM_PREFIX)?.parse().ok()?;
            Some((id, v.as_number()?))
        })
        .collect()
}

fn text_param(params: &Params, name: &str) -> String {
    params
        .get(name)
        .and_then(ParamValue::as_text)
        .unwrap_or("")
        .to_owned()
}

impl<S: PluginStandard> NodeKind for PluginEffectKind<S> {
    fn id(&self) -> &'static str {
        S::KIND_ID
    }

    fn version(&self) -> u32 {
        KIND_VERSION
    }

    fn params(&self) -> ParamSchema {
        let text =
            |name: &str| ParamSpec::new(name, ParamType::Text, ParamValue::Text(String::new()));
        ParamSchema::new(vec![text(PLUGIN_PARAM), text(PLUGIN_ID_PARAM)])
    }

    fn traits(&self) -> Traits {
        Traits {
            realtime: true,
            deterministic: true,
            offline: false,
            time_varying: false,
            audio: true,
            vector: false,
            pixel: false,
            cost: Cost::Medium,
            isolation: Isolation::OutOfProcess,
        }
    }

    fn migrate(&self, old: serde_json::Value, from: u32) -> Result<Params> {
        if from != KIND_VERSION {
            return Err(MmError::Unsupported(format!(
                "{} のバージョン {from} からの移行が無い",
                S::KIND_ID
            )));
        }
        Ok(serde_json::from_value(old)?)
    }

    /// 画像には何もしない。
    fn is_identity(&self, _cx: &FrameCx<'_>) -> bool {
        true
    }

    fn render(&self, cx: &mut FrameCx<'_>) -> Result<FrameRef> {
        if cx.inputs.is_empty() {
            return Ok(FrameRef::Transparent { size: cx.size });
        }
        cx.inputs.frame(0, cx.time)
    }

    fn process_audio(&self, cx: &AudioEffectCx<'_>, block: &mut AudioBlock) -> Result<()> {
        let Some((_, first)) = cx.params.first() else {
            return Ok(());
        };
        let path = text_param(first, PLUGIN_PARAM);
        if path.is_empty() {
            return Ok(());
        }
        let plugin = Some(text_param(first, PLUGIN_ID_PARAM)).filter(|p| !p.is_empty());
        let mut instances = self.lock();
        let stale = instances.get(cx.instance).is_some_and(|i| {
            i.path != path || i.plugin != plugin || i.effect.sample_rate() != cx.sample_rate
        });
        if stale {
            instances.remove(cx.instance);
        }
        if !instances.contains_key(cx.instance) {
            let real = crate::resolve_in_roots(&self.roots, &path, S::is_plugin_name)?;
            let effect = AudioEffect::start(self.spec()?, real, plugin.clone(), cx.sample_rate)?;
            let instance = Instance {
                effect,
                path,
                plugin,
                next_start: None,
            };
            instances.insert(cx.instance.to_owned(), instance);
        }
        let instance = instances
            .get_mut(cx.instance)
            .ok_or_else(|| MmError::Internal(format!("{} の効果のインスタンスが無い", S::NAME)))?;
        run(instance, cx, block)
    }
}

/// 続きでなければ状態を消してから処理する。失敗したら次の呼び出しを続きと見なさない。
fn run<S: PluginStandard>(
    instance: &mut Instance<S>,
    cx: &AudioEffectCx<'_>,
    block: &mut AudioBlock,
) -> Result<()> {
    let points: Vec<ParamPoint> = cx
        .params
        .iter()
        .map(|(offset, p)| ParamPoint {
            offset: *offset,
            values: plugin_values(p),
        })
        .collect();
    let result = (|| {
        if instance.next_start.is_some_and(|t| t != cx.range.start) {
            instance.effect.reset()?;
        }
        instance.effect.process(block, &points)
    })();
    instance.next_start = result.is_ok().then_some(cx.range.end);
    result
}

#[cfg(test)]
mod tests {
    use super::*;

    /// ホストを起こさないテストに使う規格。
    struct TestStandard;

    impl PluginStandard for TestStandard {
        const NAME: &'static str = "TEST";
        const KIND_ID: &'static str = "audio.test";
        const PIPE: crate::PipeKind = crate::PipeKind {
            name: "host_test",
            startup_timeout: DEFAULT_TIMEOUT,
            work_dir_env: None,
            stale_handles: crate::StaleHandles::Count,
            configure: |_| {},
        };
        const HOST_STEM: &'static str = "mm-host-test";
        const HOST_ENV: &'static str = "MM_TEST_HOST";
        const LINK_PREFIX: &'static str = "mm-test";
        const EXTENSION: &'static str = "test";

        fn plugin_dirs() -> Vec<PathBuf> {
            Vec::new()
        }
    }

    #[test]
    fn プラグインのパラメータは名前の頭で見分ける() {
        let mut p = Params::new();
        p.set(param_key(7), ParamValue::Number(0.5));
        p.set("param.x", ParamValue::Number(1.0));
        p.set(PLUGIN_PARAM, ParamValue::Text("a.clap".into()));
        assert_eq!(plugin_values(&p), BTreeMap::from([(7, 0.5)]));
    }

    #[test]
    fn プラグインが空なら何もしない() {
        let kind = PluginEffectKind::<TestStandard>::new(None);
        assert_eq!(kind.id(), "audio.test");
        let params = [(0, kind.params().defaults())];
        let cx = AudioEffectCx {
            range: mm_types::TimeRange::new(RationalTime::ZERO, RationalTime::seconds(1)).unwrap(),
            instance: "c/l/e",
            params: &params,
            sample_rate: 48_000,
            channels: 2,
        };
        let mut block = AudioBlock::silence(48_000, 2, 4);
        block.samples[0] = 0.25;
        let before = block.clone();
        kind.process_audio(&cx, &mut block).expect("素通し");
        assert_eq!(block, before);
        assert_eq!(kind.instance_count(), 0);
    }

    #[test]
    fn 置き場所の外のプラグインはホストを起こさずに拒否する() {
        let tmp = std::env::temp_dir().join(format!("mm-audio-kind-{}", std::process::id()));
        std::fs::create_dir_all(&tmp).unwrap();
        let outside = tmp.join("evil.test");
        std::fs::write(&outside, b"").unwrap();
        let kind = PluginEffectKind::<TestStandard>::new(Some("ホストは起こさない.exe".into()))
            .with_roots(vec![tmp.join("plugins")]);
        let mut params = kind.params().defaults();
        params.set(
            PLUGIN_PARAM,
            ParamValue::Text(outside.display().to_string()),
        );
        let params = [(0, params)];
        let cx = AudioEffectCx {
            range: mm_types::TimeRange::new(RationalTime::ZERO, RationalTime::seconds(1)).unwrap(),
            instance: "c/l/e",
            params: &params,
            sample_rate: 48_000,
            channels: 2,
        };
        let mut block = AudioBlock::silence(48_000, 2, 4);
        let err = kind.process_audio(&cx, &mut block);
        assert!(
            matches!(err, Err(MmError::NotFound(_) | MmError::InvalidArgument(_))),
            "{err:?}"
        );
        assert_eq!(kind.instance_count(), 0);
        std::fs::remove_dir_all(&tmp).unwrap();
    }
}
