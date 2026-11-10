//! Source のビューアー。Media で選んだ素材 1 本を、素材だけを載せた別のコンポジションとして別の再生スレッドで再生する。
//!
//! Program の再生スレッドとは `Shared`（時計、コマンド、フレームのスロット）も `Engine` も別なので、
//! Program の再生位置や編集に触れずに、見る・シークする・イン点とアウト点を付ける。
//! フレームは CPU の画像で受ける（再生スレッドごとに GPU のデバイスが別で、サーフェスのデバイスのテクスチャとして使えないため）。

use std::sync::atomic::{AtomicBool, AtomicU64, Ordering};
use std::sync::{Arc, Mutex};

use mm_doc::{Asset, ColorConfig, Composition, Layer, Node, Project};
use mm_types::{AssetId, CompositionId, Id, Rational, RationalTime};

use crate::win::playback::{Command, Playback, PlayerOptions};
use crate::win::state::Shared;

/// Source の素材だけを載せたコンポジションの ID。
const SOURCE_COMPOSITION: &str = "source";
/// その唯一のレイヤーの ID。
const SOURCE_LAYER: &str = "l-source";

/// 開いている素材と、その再生スレッド。落とすと再生スレッドを止めて待つ。
pub struct SourceHandle {
    /// Source の時計とフレーム。
    pub shared: Arc<Shared>,
    _playback: Playback,
    pub asset: AssetId,
    /// 素材のフレームレート。イン点・アウト点を付ける位置をフレームに丸めるのに使う。
    pub fps: Rational,
    pub duration: f64,
}

/// Source の置き場。`Shared` に 1 つ持つ。
#[derive(Default)]
pub struct SourceSlot {
    handle: Mutex<Option<Arc<SourceHandle>>>,
    /// 開くたびに進む。描画スレッドがフレームの番号を数え直す判定に使う。
    pub serial: AtomicU64,
    /// キー（再生、シーク）が Source に向くか。Source の領域をクリックすると真、Program か Timeline で偽。
    active: AtomicBool,
    /// 描画スレッドの `Ui` が Source の領域を出しているか。自動試験はこれで `Ui` と同じ割り付けを計算する。
    shown: AtomicBool,
    options: Mutex<Option<PlayerOptions>>,
}

impl SourceSlot {
    /// Source の再生スレッドの設定を置く。Program と同じ設定（音の有無、実装の組み合わせ）を使う。
    pub fn configure(&self, options: PlayerOptions) {
        *self.options.lock().expect("source options") = Some(options);
    }

    /// 開いている素材。
    pub fn handle(&self) -> Option<Arc<SourceHandle>> {
        self.handle.lock().expect("source handle").clone()
    }

    /// キーが Source に向くか。
    pub fn is_active(&self) -> bool {
        self.active.load(Ordering::SeqCst)
    }

    /// キーを向ける先を切り替える。
    pub fn set_active(&self, active: bool) {
        self.active.store(active, Ordering::SeqCst);
    }

    /// 描画スレッドの `Ui` が Source の領域を出しているか。
    pub fn is_shown(&self) -> bool {
        self.shown.load(Ordering::SeqCst)
    }

    /// 描画スレッドが `Ui` に Source のフレームを渡した・閉じたときに呼ぶ。
    pub fn set_shown(&self, shown: bool) {
        self.shown.store(shown, Ordering::SeqCst);
    }

    /// 素材 `asset` を Source で開く。前の素材は閉じる。
    pub fn open(&self, project: &Project, comp: &Composition, asset: &str) -> Result<(), String> {
        let asset: AssetId = Id::new(asset);
        let found = project.asset(&asset).map_err(|e| e.to_string())?;
        let options = self
            .options
            .lock()
            .expect("source options")
            .clone()
            .unwrap_or_default();
        let (synthetic, fps) = source_project(found, comp, &project.color_config)?;
        let shared = Arc::new(Shared::new());
        shared.cpu_frames.store(true, Ordering::SeqCst);
        let comp_id: CompositionId = Id::new(SOURCE_COMPOSITION);
        let (playback, _) = Playback::spawn(shared.clone(), Arc::new(synthetic), comp_id, options)
            .map_err(|e| e.to_string())?;
        let handle = Arc::new(SourceHandle {
            shared,
            _playback: playback,
            asset,
            fps,
            duration: found.duration.to_f64(),
        });
        // 古い素材の再生スレッドは、ロックの外で落とす（止めて待つため）。
        let old = self.handle.lock().expect("source handle").replace(handle);
        self.serial.fetch_add(1, Ordering::SeqCst);
        self.set_active(true);
        drop(old);
        Ok(())
    }

    /// Source の再生スレッドへコマンドを送る。素材が開いていなければ捨てる。
    pub fn send(&self, command: Command) {
        if let Some(h) = self.handle() {
            h.shared.send(command);
        }
    }
}

/// 素材 1 本を、素材の長さ全体を映すコンポジションにしたプロジェクト。大きさは Program のコンポジションに合わせる。
/// Program と Source は同じ合成の後のトーンマップ（`Ui::set_hdr_peak`）を受けるので、作業の明るさを本体に揃える。
fn source_project(
    asset: &Asset,
    program: &Composition,
    color_config: &ColorConfig,
) -> Result<(Project, Rational), String> {
    let fps = asset.fps.unwrap_or(program.fps);
    let mut project = Project::new();
    project.color_config = color_config.clone();
    project.assets.push(asset.clone());
    project.compositions.push(
        Composition::new(
            Id::new(SOURCE_COMPOSITION),
            program.width,
            program.height,
            fps,
            asset.duration,
        )
        .with_layers(vec![Layer::new(
            SOURCE_LAYER,
            Node::footage(&asset.id),
            RationalTime::ZERO,
            RationalTime::ZERO,
            asset.duration,
        )]),
    );
    project.validate().map_err(|e| e.to_string())?;
    Ok((project, fps))
}

/// 素材 `asset` のイン点とアウト点（素材の秒）。開いているプロジェクトのデータ（`bins.marks`）から読む。
pub fn marks_of(shared: &Shared, asset: &AssetId) -> (Option<f64>, Option<f64>) {
    let Some(doc) = shared.doc() else {
        return (None, None);
    };
    let marks = doc.project.bins.marks.iter().find(|m| &m.asset == asset);
    (
        marks.and_then(|m| m.in_point.map(RationalTime::to_f64)),
        marks.and_then(|m| m.out_point.map(RationalTime::to_f64)),
    )
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::win::sequence_edit::tests::sample_project;

    #[test]
    fn 素材だけを載せたコンポジションは素材の長さ全体を映す() {
        let p = sample_project();
        let (src, fps) = source_project(&p.assets[0], &p.compositions[0], &p.color_config).unwrap();
        let comp = src.composition(&Id::new(SOURCE_COMPOSITION)).unwrap();
        assert_eq!(comp.duration, p.assets[0].duration);
        assert_eq!(comp.layers.len(), 1);
        assert_eq!(comp.layers[0].source.kind, "footage");
        assert_eq!(fps, Rational::integer(30));
        assert_eq!((comp.width, comp.height), (64, 36));
    }

    /// Program と同じ合成の後のトーンマップを受けるので、HDR の素材に入力の側と合成の後の二重に掛けない。
    #[test]
    fn source_のプロジェクトは本体の作業の明るさを写す() {
        let p = sample_project();
        let hdr = ColorConfig {
            working_range: mm_doc::WorkingRange::Hdr,
        };
        let (src, _) = source_project(&p.assets[0], &p.compositions[0], &hdr).unwrap();
        assert_eq!(src.color_config, hdr);
    }

    #[test]
    fn 開いていない_source_はコマンドを捨てる() {
        let shared = Shared::new();
        assert!(shared.source.handle().is_none());
        shared.source.send(Command::Play);
        assert!(!shared.source.is_active());
    }
}
