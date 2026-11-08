//! 登録した端末と登録コード。PC には端末の資格の BLAKE3 のハッシュだけを置く（`devices.json`）。
//!
//! 登録コードは同時に 1 つだけ持ち、期限を過ぎるか、1 回使うか、決まった回数を間違えると消す。
//! 登録と登録を外すのは PC のアプリの Rust の呼び出しからだけ受け、HTTP には出さない。

use std::path::PathBuf;
use std::sync::{Mutex, MutexGuard};
use std::time::{Duration, Instant};

use serde::{Deserialize, Serialize};
use tokio::sync::watch;

use crate::core::{now_ms, ServeError};
use crate::http::constant_time_eq;

/// 端末の資格の長さ（バイト）。クッキーには 16 進で 2 倍の文字数で入る。
pub const CREDENTIAL_BYTES: usize = 32;
/// 登録コードの文字数。
pub const PAIRING_CODE_LEN: usize = 8;
/// 登録コードを間違えてよい回数。超えたら登録コードを消し、PC のアプリで出し直す。
pub const PAIRING_MAX_FAILURES: u32 = 5;
/// 登録コードの既定の期限。
pub const PAIRING_TTL: Duration = Duration::from_secs(5 * 60);
/// 端末の名前の最大の文字数。
const NAME_MAX_CHARS: usize = 64;
/// 端末の ID の長さ（バイト）。ID は登録ごとの乱数で、操作の記録の発行元（`Actor::device`）に残り取り消しで端末を見分けるので、
/// 登録を外した端末の ID と偶然そろわない長さにする。
const DEVICE_ID_BYTES: usize = 8;
/// 登録コードの文字。読み違えやすい I、L、O、U を除いた 32 文字（Crockford の Base32）で、1 バイトの下位 5 ビットで偏りなく選べる。
const CODE_ALPHABET: &[u8; 32] = b"0123456789ABCDEFGHJKMNPQRSTVWXYZ";
const CODE_MASK: u8 = 0b1_1111;

/// 登録した端末。画面と応答に出す形で、資格のハッシュは含まない。
#[derive(Debug, Clone, PartialEq, Eq, Serialize)]
pub struct DeviceInfo {
    pub id: String,
    pub name: String,
    /// 登録した時の `Tailscale-User-Login`。以後の要求も同じ人でなければ断る。
    pub login: String,
    pub paired_at_ms: i64,
}

/// PC のアプリが出す登録コード。
#[derive(Debug, Clone, PartialEq, Eq, Serialize)]
pub struct PairingCode {
    pub code: String,
    pub expires_at_ms: i64,
}

/// 登録の失敗。応答の `code` に対応する。
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum PairError {
    /// 登録コードが無い、期限切れ、回数を使い切った。
    NoCode,
    /// 登録コードが違う。
    WrongCode,
    /// 名前が空か長すぎる。
    BadName,
    /// 同じ名前の端末がある（履歴と提案の一覧で、どの端末の操作かを名前で読み分ける）。
    NameTaken,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
struct DeviceRecord {
    id: String,
    name: String,
    login: String,
    paired_at_ms: i64,
    /// 端末の資格の BLAKE3（16 進）。
    hash: String,
    /// 資格がまだ一度も使われていない。端末がクッキーを保存しなかった登録は、名前を塞がず期限で一覧から消える。
    #[serde(default, skip_serializing_if = "std::ops::Not::not")]
    unused: bool,
}

impl DeviceRecord {
    /// 一覧に残る登録か。資格を一度も使わないまま登録コードの期限（`ttl_ms`）を過ぎた登録は無いものとみなす。
    fn live(&self, now_ms: i64, ttl_ms: i64) -> bool {
        !self.unused || now_ms.saturating_sub(self.paired_at_ms) <= ttl_ms
    }

    fn same_name(&self, name: &str) -> bool {
        self.name.to_lowercase() == name.to_lowercase()
    }
}

#[derive(Debug, Default, Serialize, Deserialize)]
struct DevicesFile {
    #[serde(default)]
    devices: Vec<DeviceRecord>,
}

struct Pending {
    code: String,
    /// 期限は単調な時計で確かめ、PC の時計を戻しても延びない。
    deadline: Instant,
    failures: u32,
}

struct State {
    file: DevicesFile,
    pending: Option<Pending>,
}

/// 登録した端末の一覧と、今の登録コード。
pub struct Devices {
    path: PathBuf,
    ttl: Duration,
    state: Mutex<State>,
    /// 登録を外すたびに増える番号。開いている SSE が見張り、自分の端末が外されたら閉じる。
    revocations: watch::Sender<u64>,
}

impl Devices {
    /// `path` の `devices.json` を読む。無ければ空から始める。
    pub fn open(path: PathBuf, ttl: Duration) -> Result<Self, ServeError> {
        let file = match std::fs::read(&path) {
            Ok(bytes) => serde_json::from_slice(&bytes).map_err(|e| {
                ServeError::Internal(format!("端末の一覧 {path:?} が読めない: {e}"))
            })?,
            Err(e) if e.kind() == std::io::ErrorKind::NotFound => DevicesFile::default(),
            Err(e) => return Err(ServeError::Internal(format!("{path:?} が開けない: {e}"))),
        };
        Ok(Self {
            path,
            ttl,
            state: Mutex::new(State {
                file,
                pending: None,
            }),
            revocations: watch::Sender::new(0),
        })
    }

    fn lock(&self) -> Result<MutexGuard<'_, State>, ServeError> {
        self.state
            .lock()
            .map_err(|_| ServeError::Internal("端末の一覧のロックが壊れた".into()))
    }

    /// 新しい登録コードを出す。前の登録コードは使えなくなる。
    pub fn issue_code(&self) -> Result<PairingCode, ServeError> {
        let mut raw = [0u8; PAIRING_CODE_LEN];
        fill_random(&mut raw)?;
        let code: String = raw
            .iter()
            .map(|b| char::from(CODE_ALPHABET[usize::from(b & CODE_MASK)]))
            .collect();
        let expires_at_ms = now_ms().saturating_add(self.ttl_ms());
        self.lock()?.pending = Some(Pending {
            code: code.clone(),
            deadline: Instant::now() + self.ttl,
            failures: 0,
        });
        Ok(PairingCode {
            code,
            expires_at_ms,
        })
    }

    /// 登録コードを確かめて端末を登録し、端末と資格（16 進）を返す。
    pub(crate) fn pair(
        &self,
        code: &str,
        name: &str,
        login: &str,
    ) -> Result<Result<(DeviceInfo, String), PairError>, ServeError> {
        let mut st = self.lock()?;
        // 登録コードを先に確かめ、登録コードを知らない者に登録した名前を探らせない。名前の誤りでは登録コードを消さない。
        if let Err(e) = check_code(&mut st.pending, code) {
            return Ok(Err(e));
        }
        let name = name.trim();
        let now = now_ms();
        if let Err(e) = make_room(&mut st.file.devices, name, now, self.ttl_ms()) {
            return Ok(Err(e));
        }
        st.pending = None;
        let credential = random_hex(CREDENTIAL_BYTES)?;
        let record = DeviceRecord {
            id: random_hex(DEVICE_ID_BYTES)?,
            name: name.to_owned(),
            login: login.to_owned(),
            paired_at_ms: now,
            hash: hash_credential(&credential),
            unused: true,
        };
        st.file.devices.push(record.clone());
        if let Err(e) = self.save(&st.file) {
            st.file.devices.pop();
            return Err(e);
        }
        Ok(Ok((info(&record), credential)))
    }

    /// 資格（クッキーの値）の端末。無いか登録を外した端末なら `None`。初めて使われた資格は `unused` のフラグを下ろす。
    pub(crate) fn find(&self, credential: &str) -> Result<Option<DeviceInfo>, ServeError> {
        let hash = hash_credential(credential);
        let (now, ttl_ms) = (now_ms(), self.ttl_ms());
        let mut st = self.lock()?;
        let Some(pos) = st.file.devices.iter().position(|d| {
            d.live(now, ttl_ms) && constant_time_eq(d.hash.as_bytes(), hash.as_bytes())
        }) else {
            return Ok(None);
        };
        if st.file.devices[pos].unused {
            st.file.devices[pos].unused = false;
            if let Err(e) = self.save(&st.file) {
                st.file.devices[pos].unused = true;
                return Err(e);
            }
        }
        Ok(Some(info(&st.file.devices[pos])))
    }

    /// 登録した端末の一覧（登録した順）。資格を一度も使わないまま期限を過ぎた登録は出さない。
    pub fn list(&self) -> Result<Vec<DeviceInfo>, ServeError> {
        let (now, ttl_ms) = (now_ms(), self.ttl_ms());
        Ok(self
            .lock()?
            .file
            .devices
            .iter()
            .filter(|d| d.live(now, ttl_ms))
            .map(info)
            .collect())
    }

    fn ttl_ms(&self) -> i64 {
        i64::try_from(self.ttl.as_millis()).unwrap_or(i64::MAX)
    }

    /// 端末の登録を外す。以後その資格の要求は 401 で、開いている SSE も閉じる。無い ID なら `false`。
    pub fn revoke(&self, id: &str) -> Result<bool, ServeError> {
        let mut st = self.lock()?;
        let Some(pos) = st.file.devices.iter().position(|d| d.id == id) else {
            return Ok(false);
        };
        let removed = st.file.devices.remove(pos);
        if let Err(e) = self.save(&st.file) {
            st.file.devices.insert(pos, removed);
            return Err(e);
        }
        drop(st);
        self.revocations.send_modify(|n| *n = n.wrapping_add(1));
        Ok(true)
    }

    /// `id` の端末が今も登録されているか。
    pub(crate) fn is_paired(&self, id: &str) -> Result<bool, ServeError> {
        Ok(self.lock()?.file.devices.iter().any(|d| d.id == id))
    }

    /// 登録を外した知らせ。値は外すたびに変わるので、受けたら [`Self::is_paired`] で確かめ直す。
    pub(crate) fn subscribe_revocations(&self) -> watch::Receiver<u64> {
        self.revocations.subscribe()
    }

    fn save(&self, file: &DevicesFile) -> Result<(), ServeError> {
        let bytes =
            serde_json::to_vec_pretty(file).map_err(|e| ServeError::Internal(e.to_string()))?;
        mm_store::write_private_file(&self.path, &bytes)
            .map_err(|e| ServeError::Internal(format!("端末の一覧を書けない: {e}")))
    }
}

/// 名前を確かめ、期限を過ぎた登録と、同じ名前で資格を使っていない登録を外す。
fn make_room(
    devices: &mut Vec<DeviceRecord>,
    name: &str,
    now_ms: i64,
    ttl_ms: i64,
) -> Result<(), PairError> {
    if name.is_empty()
        || name.chars().count() > NAME_MAX_CHARS
        || name.chars().any(char::is_control)
    {
        return Err(PairError::BadName);
    }
    // 画面に出す発行元は名前なので、大文字と小文字の違いだけの名前も同じとみなす。取り消しの判定は端末の ID でする。
    // 資格を一度も使っていない登録は操作を出していないので、同じ名前の登録し直しで置き換える。
    if devices.iter().any(|d| !d.unused && d.same_name(name)) {
        return Err(PairError::NameTaken);
    }
    devices.retain(|d| d.live(now_ms, ttl_ms) && !d.same_name(name));
    Ok(())
}

/// 登録コードを確かめる。違えば数え、期限か回数を超えたら消す。合った登録コードは登録を決めてから消す。
fn check_code(pending: &mut Option<Pending>, given: &str) -> Result<(), PairError> {
    let Some(p) = pending.as_mut() else {
        return Err(PairError::NoCode);
    };
    if Instant::now() > p.deadline {
        *pending = None;
        return Err(PairError::NoCode);
    }
    let given = normalize_code(given);
    if constant_time_eq(given.as_bytes(), p.code.as_bytes()) {
        return Ok(());
    }
    p.failures += 1;
    if p.failures >= PAIRING_MAX_FAILURES {
        *pending = None;
    }
    Err(PairError::WrongCode)
}

/// 手で入れた登録コードの区切り（空白とハイフン）を除いて大文字にそろえ、Crockford の Base32 の読み替え
/// （O を 0、I と L を 1）をする。
fn normalize_code(raw: &str) -> String {
    raw.chars()
        .filter(|c| !c.is_whitespace() && *c != '-')
        .map(|c| match c.to_ascii_uppercase() {
            'O' => '0',
            'I' | 'L' => '1',
            other => other,
        })
        .collect()
}

fn info(d: &DeviceRecord) -> DeviceInfo {
    DeviceInfo {
        id: d.id.clone(),
        name: d.name.clone(),
        login: d.login.clone(),
        paired_at_ms: d.paired_at_ms,
    }
}

fn hash_credential(credential: &str) -> String {
    blake3::hash(credential.as_bytes()).to_hex().to_string()
}

fn fill_random(buf: &mut [u8]) -> Result<(), ServeError> {
    getrandom::fill(buf).map_err(|e| ServeError::Internal(format!("乱数を取れない: {e}")))
}

fn random_hex(len: usize) -> Result<String, ServeError> {
    let mut bytes = vec![0u8; len];
    fill_random(&mut bytes)?;
    Ok(bytes.iter().map(|b| format!("{b:02x}")).collect())
}

#[cfg(test)]
mod tests {
    use super::*;

    fn devices(ttl: Duration) -> (tempfile::TempDir, Devices) {
        let tmp = tempfile::tempdir().unwrap();
        let d = Devices::open(tmp.path().join("devices.json"), ttl).unwrap();
        (tmp, d)
    }

    #[test]
    fn 登録コードは決まった文字から決まった長さで出る() {
        let (_tmp, d) = devices(PAIRING_TTL);
        let code = d.issue_code().unwrap().code;
        assert_eq!(code.len(), PAIRING_CODE_LEN);
        assert!(code.bytes().all(|c| CODE_ALPHABET.contains(&c)));
    }

    #[test]
    fn 登録コードは小文字と区切りを許し_1_回で消える() {
        let (_tmp, d) = devices(PAIRING_TTL);
        let code = d.issue_code().unwrap().code;
        let typed = format!("{}-{}", &code[..4], &code[4..]).to_lowercase();
        assert!(d.pair(&typed, "phone", "me@x").unwrap().is_ok());
        assert_eq!(
            d.pair(&code, "phone2", "me@x").unwrap().unwrap_err(),
            PairError::NoCode
        );
    }

    #[test]
    fn ファイルには資格でなくハッシュだけが残る() {
        let (tmp, d) = devices(PAIRING_TTL);
        let code = d.issue_code().unwrap().code;
        let (_, credential) = d.pair(&code, "phone", "me@x").unwrap().unwrap();
        let text = std::fs::read_to_string(tmp.path().join("devices.json")).unwrap();
        assert!(!text.contains(&credential));
        assert!(text.contains(&hash_credential(&credential)));
        let reopened = Devices::open(tmp.path().join("devices.json"), PAIRING_TTL).unwrap();
        assert_eq!(reopened.find(&credential).unwrap().unwrap().name, "phone");
    }

    fn pair_new(d: &Devices, name: &str) -> String {
        let code = d.issue_code().unwrap().code;
        d.pair(&code, name, "me@x").unwrap().unwrap().1
    }

    #[test]
    fn 資格を使わなかった登録は同じ名前の登録し直しで置き換わる() {
        let (_tmp, d) = devices(PAIRING_TTL);
        let lost = pair_new(&d, "phone");
        let kept = pair_new(&d, "Phone");
        assert_eq!(d.list().unwrap().len(), 1);
        assert!(d.find(&lost).unwrap().is_none());
        assert_eq!(d.find(&kept).unwrap().unwrap().name, "Phone");
        let code = d.issue_code().unwrap().code;
        assert_eq!(
            d.pair(&code, "phone", "me@x").unwrap().unwrap_err(),
            PairError::NameTaken
        );
    }

    #[test]
    fn 資格を使わないまま期限を過ぎた登録は無いものとみなす() {
        let (_tmp, d) = devices(PAIRING_TTL);
        let stale = pair_new(&d, "phone");
        let used = pair_new(&d, "tablet");
        assert!(d.find(&used).unwrap().is_some());
        let past = i64::try_from(PAIRING_TTL.as_millis()).unwrap() + 1;
        for r in &mut d.lock().unwrap().file.devices {
            r.paired_at_ms -= past;
        }
        let names: Vec<_> = d.list().unwrap().into_iter().map(|i| i.name).collect();
        assert_eq!(names, ["tablet"]);
        assert!(d.find(&stale).unwrap().is_none());
        assert!(d.find(&used).unwrap().is_some());
    }

    #[test]
    fn 未使用のフラグはファイルに残り_フラグの無い古い記録は使ったものとみなす() {
        let (tmp, d) = devices(PAIRING_TTL);
        let path = tmp.path().join("devices.json");
        let credential = pair_new(&d, "phone");
        assert!(std::fs::read_to_string(&path).unwrap().contains("unused"));
        d.find(&credential).unwrap().unwrap();
        assert!(!std::fs::read_to_string(&path).unwrap().contains("unused"));
        let old =
            r#"{"devices":[{"id":"a","name":"old","login":"me@x","paired_at_ms":0,"hash":"h"}]}"#;
        std::fs::write(&path, old).unwrap();
        let reopened = Devices::open(path, PAIRING_TTL).unwrap();
        assert_eq!(reopened.list().unwrap().len(), 1);
    }
}
