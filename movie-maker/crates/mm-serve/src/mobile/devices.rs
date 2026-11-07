//! 登録した端末と登録の符号。PC には端末の資格の BLAKE3 のハッシュだけを置く（`devices.json`）。
//!
//! 登録の符号は同時に 1 つだけ持ち、期限を過ぎるか、1 回使うか、決まった回数を間違えると消す。
//! 登録と取り消しは PC のアプリの Rust の呼び出しからだけ受け、HTTP には出さない。

use std::path::PathBuf;
use std::sync::{Mutex, MutexGuard};
use std::time::Duration;

use serde::{Deserialize, Serialize};

use crate::core::{now_ms, ServeError};
use crate::http::constant_time_eq;

/// 端末の資格の長さ（バイト）。クッキーには 16 進で 2 倍の文字数で入る。
pub const CREDENTIAL_BYTES: usize = 32;
/// 登録の符号の文字数。
pub const PAIRING_CODE_LEN: usize = 8;
/// 登録の符号を間違えてよい回数。超えたら符号を消し、PC のアプリで出し直す。
pub const PAIRING_MAX_FAILURES: u32 = 5;
/// 登録の符号の既定の期限。
pub const PAIRING_TTL: Duration = Duration::from_secs(5 * 60);
/// 端末の名前の最大の文字数。
const NAME_MAX_CHARS: usize = 64;
/// 端末の ID の長さ（バイト）。
const DEVICE_ID_BYTES: usize = 8;
/// 符号の文字。読み違えやすい I、L、O、U を除いた 32 文字（Crockford の Base32）で、1 バイトの下位 5 ビットで偏りなく選べる。
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

/// PC のアプリが出す登録の符号。
#[derive(Debug, Clone, PartialEq, Eq, Serialize)]
pub struct PairingCode {
    pub code: String,
    pub expires_at_ms: i64,
}

/// 登録の失敗。応答の `code` に対応する。
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum PairError {
    /// 符号が無い、期限切れ、回数を使い切った。
    NoCode,
    /// 符号が違う。
    WrongCode,
    /// 名前が空か長すぎる。
    BadName,
    /// 同じ名前の端末がある（取り消しの判定に端末の名前を使う）。
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
}

#[derive(Debug, Default, Serialize, Deserialize)]
struct DevicesFile {
    #[serde(default)]
    devices: Vec<DeviceRecord>,
}

struct Pending {
    code: String,
    expires_at_ms: i64,
    failures: u32,
}

struct State {
    file: DevicesFile,
    pending: Option<Pending>,
}

/// 登録した端末の一覧と、今の登録の符号。
pub struct Devices {
    path: PathBuf,
    ttl: Duration,
    state: Mutex<State>,
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
        })
    }

    fn lock(&self) -> Result<MutexGuard<'_, State>, ServeError> {
        self.state
            .lock()
            .map_err(|_| ServeError::Internal("端末の一覧のロックが壊れた".into()))
    }

    /// 新しい登録の符号を出す。前の符号は使えなくなる。
    pub fn issue_code(&self) -> Result<PairingCode, ServeError> {
        let mut raw = [0u8; PAIRING_CODE_LEN];
        fill_random(&mut raw)?;
        let code: String = raw
            .iter()
            .map(|b| char::from(CODE_ALPHABET[usize::from(b & CODE_MASK)]))
            .collect();
        let ttl_ms = i64::try_from(self.ttl.as_millis()).unwrap_or(i64::MAX);
        let expires_at_ms = now_ms().saturating_add(ttl_ms);
        self.lock()?.pending = Some(Pending {
            code: code.clone(),
            expires_at_ms,
            failures: 0,
        });
        Ok(PairingCode {
            code,
            expires_at_ms,
        })
    }

    /// 符号を確かめて端末を登録し、端末と資格（16 進）を返す。
    pub(crate) fn pair(
        &self,
        code: &str,
        name: &str,
        login: &str,
    ) -> Result<Result<(DeviceInfo, String), PairError>, ServeError> {
        let mut st = self.lock()?;
        // 名前の誤りで符号を使い切らないよう、名前を先に確かめる。
        let name = name.trim();
        if name.is_empty() || name.chars().count() > NAME_MAX_CHARS {
            return Ok(Err(PairError::BadName));
        }
        if st.file.devices.iter().any(|d| d.name == name) {
            return Ok(Err(PairError::NameTaken));
        }
        if let Err(e) = take_code(&mut st.pending, code) {
            return Ok(Err(e));
        }
        let credential = random_hex(CREDENTIAL_BYTES)?;
        let record = DeviceRecord {
            id: random_hex(DEVICE_ID_BYTES)?,
            name: name.to_owned(),
            login: login.to_owned(),
            paired_at_ms: now_ms(),
            hash: hash_credential(&credential),
        };
        st.file.devices.push(record.clone());
        if let Err(e) = self.save(&st.file) {
            st.file.devices.pop();
            return Err(e);
        }
        Ok(Ok((info(&record), credential)))
    }

    /// 資格（クッキーの値）の端末。無いか取り消した端末なら `None`。
    pub(crate) fn find(&self, credential: &str) -> Result<Option<DeviceInfo>, ServeError> {
        let hash = hash_credential(credential);
        let st = self.lock()?;
        Ok(st
            .file
            .devices
            .iter()
            .find(|d| constant_time_eq(d.hash.as_bytes(), hash.as_bytes()))
            .map(info))
    }

    /// 登録した端末の一覧（登録した順）。
    pub fn list(&self) -> Result<Vec<DeviceInfo>, ServeError> {
        Ok(self.lock()?.file.devices.iter().map(info).collect())
    }

    /// 端末を取り消す。以後その資格の要求は 401。無い ID なら `false`。
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
        Ok(true)
    }

    fn save(&self, file: &DevicesFile) -> Result<(), ServeError> {
        let bytes =
            serde_json::to_vec_pretty(file).map_err(|e| ServeError::Internal(e.to_string()))?;
        mm_store::write_private_file(&self.path, &bytes)
            .map_err(|e| ServeError::Internal(format!("端末の一覧を書けない: {e}")))
    }
}

/// 符号を確かめる。合えば消し、違えば数え、期限か回数を超えたら消す。
fn take_code(pending: &mut Option<Pending>, given: &str) -> Result<(), PairError> {
    let Some(p) = pending.as_mut() else {
        return Err(PairError::NoCode);
    };
    if now_ms() > p.expires_at_ms {
        *pending = None;
        return Err(PairError::NoCode);
    }
    let given = normalize_code(given);
    if constant_time_eq(given.as_bytes(), p.code.as_bytes()) {
        *pending = None;
        return Ok(());
    }
    p.failures += 1;
    if p.failures >= PAIRING_MAX_FAILURES {
        *pending = None;
    }
    Err(PairError::WrongCode)
}

/// 手で入れた符号の区切り（空白とハイフン）を除き、大文字にそろえる。
fn normalize_code(raw: &str) -> String {
    raw.chars()
        .filter(|c| !c.is_whitespace() && *c != '-')
        .map(|c| c.to_ascii_uppercase())
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
    fn 符号は決まった文字から決まった長さで出る() {
        let (_tmp, d) = devices(PAIRING_TTL);
        let code = d.issue_code().unwrap().code;
        assert_eq!(code.len(), PAIRING_CODE_LEN);
        assert!(code.bytes().all(|c| CODE_ALPHABET.contains(&c)));
    }

    #[test]
    fn 符号は小文字と区切りを許し_1_回で消える() {
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
}
