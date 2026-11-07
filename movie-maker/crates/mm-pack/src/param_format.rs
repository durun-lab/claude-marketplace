//! 引数の値の検査（真偽、列挙、区切りのある書式）。展開の前に誤りを捕まえ、ページに壊れた値を渡さない。

use mm_types::{MmError, Result};

use crate::macros::{ParamDef, ParamFormat, ParamType};

/// 真偽の引数が取る 2 つの値。
pub(crate) const BOOL_VALUES: [&str; 2] = ["true", "false"];

/// 引数の値 `raw` が `param` の型、choices、書式に合うか確かめる。空の値は書式の検査を通す（任意の引数と空の既定）。
pub(crate) fn validate(param: &ParamDef, raw: &str) -> Result<()> {
    if param.ty == ParamType::Bool && !BOOL_VALUES.contains(&raw) {
        return Err(bad(raw, "true か false"));
    }
    if param.format == Some(ParamFormat::List) {
        for word in raw.split_whitespace() {
            check_choice(param, word)?;
        }
    } else if !param.choices.is_empty() {
        check_choice(param, raw)?;
    }
    match param.format {
        Some(format) => check_format(format, raw),
        None => Ok(()),
    }
}

fn bad(raw: &str, expect: &str) -> MmError {
    MmError::InvalidArgument(format!("{expect} にする: {raw}"))
}

fn check_choice(param: &ParamDef, value: &str) -> Result<()> {
    if param.choices.iter().any(|c| c == value) {
        return Ok(());
    }
    Err(MmError::InvalidArgument(format!(
        "{value} は選べない（選べるのは {}）",
        param.choices.join(", ")
    )))
}

fn number(s: &str) -> Option<f64> {
    s.trim().parse::<f64>().ok().filter(|n| n.is_finite())
}

/// 空白区切りの項目を 1 つずつ `item` で確かめる。
fn each(raw: &str, expect: &str, item: impl Fn(&str) -> bool) -> Result<()> {
    match raw.split_whitespace().find(|w| !item(w)) {
        Some(w) => Err(bad(w, expect)),
        None => Ok(()),
    }
}

/// 「値@始まり+長さ」。値は `value` で確かめ、始まりは数（負でもよい）、長さは 0 以上の数。
fn span(s: &str, value: impl Fn(&str) -> bool) -> bool {
    let Some((v, when)) = s.rsplit_once('@') else {
        return false;
    };
    let Some((start, dur)) = when.split_once('+') else {
        return false;
    };
    value(v) && number(start).is_some() && number(dur).is_some_and(|d| d >= 0.0)
}

/// カンマ区切りで、ちょうど `n` 個の数。
fn numbers_of(s: &str, n: usize) -> bool {
    let parts: Vec<&str> = s.split(',').collect();
    parts.len() == n && parts.iter().all(|p| number(p).is_some())
}

fn row_number(s: &str) -> bool {
    s.parse::<u32>().is_ok_and(|n| n >= 1)
}

fn check_format(format: ParamFormat, raw: &str) -> Result<()> {
    match format {
        ParamFormat::List => Ok(()),
        ParamFormat::Numbers => each(raw, "空白で区切った数（1 2.5）", |w| {
            number(w).is_some()
        }),
        ParamFormat::Points => each(raw, "「x,y」の並び（空白区切り）", |w| {
            numbers_of(w, 2)
        }),
        ParamFormat::Rect if raw.trim().is_empty() || numbers_of(raw, 4) => Ok(()),
        ParamFormat::Rect => Err(bad(raw, "「x,y,w,h」")),
        ParamFormat::Cues => each(raw, "「行の番号@始まり+長さ」", |w| {
            span(w, row_number)
        }),
        ParamFormat::Notes => each(raw, "「音の高さ@始まり+長さ」", |w| {
            span(w, |v| number(v).is_some())
        }),
        ParamFormat::Words => raw.split('/').try_for_each(|line| {
            each(
                line,
                "「言葉@始まり+長さ」（行は / で分ける）",
                |w| span(w, |v| !v.is_empty()),
            )
        }),
        ParamFormat::Segs if raw.trim().is_empty() => Ok(()),
        ParamFormat::Segs => raw.split('|').try_for_each(|seg| {
            if span(seg.trim(), |v| !v.is_empty()) {
                Ok(())
            } else {
                Err(bad(seg, "「言葉@始まり+長さ」（| 区切り）"))
            }
        }),
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn param(ty: ParamType, format: Option<ParamFormat>, choices: &[&str]) -> ParamDef {
        ParamDef {
            ty,
            default: None,
            optional: false,
            description: String::new(),
            choices: choices.iter().map(|c| c.to_string()).collect(),
            format,
        }
    }

    fn ok(format: ParamFormat, raw: &str) -> bool {
        validate(&param(ParamType::Text, Some(format), &[]), raw).is_ok()
    }

    #[test]
    fn 真偽と列挙は値を限る() {
        let b = param(ParamType::Bool, None, &[]);
        assert!(validate(&b, "true").is_ok() && validate(&b, "false").is_ok());
        assert!(validate(&b, "yes").is_err() && validate(&b, "").is_err());
        let e = param(ParamType::Text, None, &["left", "right"]);
        assert!(validate(&e, "left").is_ok());
        assert!(validate(&e, "up").is_err());
        let l = param(ParamType::Text, Some(ParamFormat::List), &["dry", "both"]);
        assert!(validate(&l, "dry both").is_ok() && validate(&l, "").is_ok());
        assert!(validate(&l, "dry wet").is_err());
    }

    #[test]
    fn 点と矩形と数の並びを読む() {
        assert!(ok(ParamFormat::Points, "1,2 3.5,-4") && ok(ParamFormat::Points, ""));
        assert!(!ok(ParamFormat::Points, "1,2,3") && !ok(ParamFormat::Points, "1;2"));
        assert!(!ok(ParamFormat::Points, "1,NaN"));
        assert!(ok(ParamFormat::Rect, "1,2,3,4") && ok(ParamFormat::Rect, ""));
        assert!(!ok(ParamFormat::Rect, "1,2,3") && !ok(ParamFormat::Rect, "a,b,c,d"));
        assert!(ok(ParamFormat::Numbers, "0 1.2 -3") && !ok(ParamFormat::Numbers, "0 x"));
    }

    #[test]
    fn 始まりと長さのある項目を読む() {
        assert!(ok(ParamFormat::Cues, "4@4.5+2 1@0+1") && !ok(ParamFormat::Cues, "0@1+1"));
        assert!(!ok(ParamFormat::Cues, "4@4.5") && !ok(ParamFormat::Cues, "4@x+2"));
        assert!(ok(ParamFormat::Notes, "69@0.6+0.3 67.5@-1+0.3"));
        assert!(!ok(ParamFormat::Notes, "do@0.6+0.3") && !ok(ParamFormat::Notes, "69@0.6+-1"));
        assert!(ok(ParamFormat::Words, "帰り道@0.2+0.6 から@1+0.4/ねえ@2+1"));
        assert!(!ok(ParamFormat::Words, "帰り道@0.2") && !ok(ParamFormat::Words, "@1+1"));
        assert!(ok(ParamFormat::Segs, "a b@0+1 | c@1.5+0.5") && ok(ParamFormat::Segs, ""));
        assert!(!ok(ParamFormat::Segs, "a@0+1|b"));
    }
}
