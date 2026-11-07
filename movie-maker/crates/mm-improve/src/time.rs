//! 時刻と ID。chrono を入れず、UTC の暦の計算だけを持つ。

use std::time::{SystemTime, UNIX_EPOCH};

const MS_PER_SEC: u64 = 1000;
const SECS_PER_DAY: u64 = 86_400;

/// いまの時刻（UNIX 時刻のミリ秒）。
pub fn now_ms() -> u64 {
    SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .map(|d| d.as_millis() as u64)
        .unwrap_or(0)
}

/// UNIX 時刻の日数から UTC の年月日を出す（Howard Hinnant の civil_from_days）。
fn civil(days: i64) -> (i64, u32, u32) {
    let z = days + 719_468;
    let era = z.div_euclid(146_097);
    let doe = z.rem_euclid(146_097);
    let yoe = (doe - doe / 1460 + doe / 36_524 - doe / 146_096) / 365;
    let doy = doe - (365 * yoe + yoe / 4 - yoe / 100);
    let mp = (5 * doy + 2) / 153;
    let d = (doy - (153 * mp + 2) / 5 + 1) as u32;
    let m = if mp < 10 { mp + 3 } else { mp - 9 } as u32;
    let y = yoe + era * 400 + i64::from(m <= 2);
    (y, m, d)
}

fn parts(ms: u64) -> (i64, u32, u32, u64, u64, u64) {
    let secs = ms / MS_PER_SEC;
    let (y, mo, d) = civil((secs / SECS_PER_DAY) as i64);
    let rem = secs % SECS_PER_DAY;
    (y, mo, d, rem / 3600, rem % 3600 / 60, rem % 60)
}

/// 名前順が時刻順になる `20261107-225801` の形（UTC）。要望と回の ID の先頭に置く。
pub fn stamp(ms: u64) -> String {
    let (y, mo, d, h, mi, s) = parts(ms);
    format!("{y:04}{mo:02}{d:02}-{h:02}{mi:02}{s:02}")
}

/// 人が読む `2026-11-07 22:58:01 UTC` の形。
pub fn human(ms: u64) -> String {
    let (y, mo, d, h, mi, s) = parts(ms);
    format!("{y:04}-{mo:02}-{d:02} {h:02}:{mi:02}:{s:02} UTC")
}

fn random_bytes<const N: usize>() -> [u8; N] {
    let mut buf = [0u8; N];
    // 乱数が取れない環境でも ID は作る。時刻の下位で埋め、同じミリ秒の衝突だけを許す。
    if getrandom::fill(&mut buf).is_err() {
        let t = now_ms().to_le_bytes();
        for (i, b) in buf.iter_mut().enumerate() {
            *b = t[i % t.len()];
        }
    }
    buf
}

/// 4 桁の 16 進。同じ秒の ID を分ける。
pub fn short_hex() -> String {
    let b = random_bytes::<2>();
    format!("{:02x}{:02x}", b[0], b[1])
}

/// `claude -p --session-id` に渡す UUID（バージョン 4）。
pub fn uuid_v4() -> String {
    let mut b = random_bytes::<16>();
    b[6] = (b[6] & 0x0f) | 0x40;
    b[8] = (b[8] & 0x3f) | 0x80;
    let h: Vec<String> = b.iter().map(|x| format!("{x:02x}")).collect();
    format!(
        "{}-{}-{}-{}-{}",
        h[0..4].concat(),
        h[4..6].concat(),
        h[6..8].concat(),
        h[8..10].concat(),
        h[10..16].concat()
    )
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn stamp_is_utc_calendar() {
        // 2026-11-07T22:58:01Z
        assert_eq!(stamp(1_794_092_281_000), "20261107-225801");
        assert_eq!(human(0), "1970-01-01 00:00:00 UTC");
    }

    #[test]
    fn uuid_has_version_4_shape() {
        let u = uuid_v4();
        assert_eq!(u.len(), 36);
        assert_eq!(&u[14..15], "4");
    }
}
