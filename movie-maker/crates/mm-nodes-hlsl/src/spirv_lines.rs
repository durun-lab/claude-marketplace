//! naga が読めなかった SPIR-V の命令を、利用者の HLSL の行に戻す。
//!
//! naga の誤りは SPIR-V の命令の種類しか持たないので、DXC に付けさせた `OpLine`（`-fspv-debug=line`）を自前でたどり、
//! その種類の最初の命令の直前の行を利用者の源の名前の分だけ拾う。

use naga::front::spv::Error;

/// SPIR-V のヘッダーの語数。
const HEADER_WORDS: usize = 5;
const OP_STRING: u32 = 7;
const OP_LINE: u32 = 8;
const OP_EXT_INST_IMPORT: u32 = 11;
const OP_EXT_INST: u32 = 12;
const OP_NO_LINE: u32 = 317;

/// 利用者の源の中の場所。
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub(crate) struct Location {
    pub line: u32,
    pub column: u32,
}

/// 1 つの命令。`words[0]` は命令の種類と語数。
struct Inst<'a> {
    op: u32,
    operands: &'a [u32],
}

fn words(spirv: &[u8]) -> Vec<u32> {
    spirv
        .chunks_exact(4)
        .map(|c| u32::from_le_bytes([c[0], c[1], c[2], c[3]]))
        .collect()
}

fn insts(words: &[u32]) -> Vec<Inst<'_>> {
    let mut out = Vec::new();
    let mut at = HEADER_WORDS;
    while at < words.len() {
        let count = (words[at] >> 16) as usize;
        if count == 0 || at + count > words.len() {
            break;
        }
        out.push(Inst {
            op: words[at] & 0xffff,
            operands: &words[at + 1..at + count],
        });
        at += count;
    }
    out
}

/// `OpLine` と `OpNoLine` を除いた SPIR-V。naga は型の宣言の間の `OpLine` を読めないので、渡す前に落とす。
pub(crate) fn strip_lines(spirv: &[u8]) -> Vec<u8> {
    let words = words(spirv);
    let mut out: Vec<u32> = words.iter().take(HEADER_WORDS).copied().collect();
    let mut at = HEADER_WORDS;
    while at < words.len() {
        let count = ((words[at] >> 16) as usize).max(1);
        let end = (at + count).min(words.len());
        if !matches!(words[at] & 0xffff, OP_LINE | OP_NO_LINE) {
            out.extend_from_slice(&words[at..end]);
        }
        at = end;
    }
    out.iter().flat_map(|w| w.to_le_bytes()).collect()
}

/// リテラルの文字列（NUL 終端、4 バイトずつ詰める）。
fn literal(operands: &[u32]) -> String {
    let bytes: Vec<u8> = operands.iter().flat_map(|w| w.to_le_bytes()).collect();
    let end = bytes.iter().position(|b| *b == 0).unwrap_or(bytes.len());
    String::from_utf8_lossy(&bytes[..end]).into_owned()
}

/// `name` を持つ `OpString` か `OpExtInstImport` の結果の ID。
fn id_of(insts: &[Inst<'_>], op: u32, name: &str) -> Option<u32> {
    insts
        .iter()
        .filter(|i| i.op == op && !i.operands.is_empty())
        .find(|i| literal(&i.operands[1..]) == name)
        .map(|i| i.operands[0])
}

/// `hit` に当たる最初の命令の、利用者の源 `label` の中の場所。
fn first_line(
    insts: &[Inst<'_>],
    label: &str,
    hit: impl Fn(&Inst<'_>) -> bool,
) -> Option<Location> {
    let file = id_of(insts, OP_STRING, label)?;
    let mut current = None;
    for inst in insts {
        match inst.op {
            OP_LINE if inst.operands.len() >= 3 => {
                current = (inst.operands[0] == file).then(|| Location {
                    line: inst.operands[1],
                    column: inst.operands[2],
                });
            }
            OP_NO_LINE => current = None,
            _ if hit(inst) && current.is_some() => return current,
            _ => {}
        }
    }
    None
}

/// naga の誤りの命令を利用者の行に戻す。命令に結び付かない誤りと、利用者の源の外の命令は `None`。
pub(crate) fn locate(spirv: &[u8], label: &str, error: &Error) -> Option<Location> {
    let words = words(spirv);
    let insts = insts(&words);
    let set_of = |name: &str| id_of(&insts, OP_EXT_INST_IMPORT, name);
    let ext_with_set =
        |set: u32| move |i: &Inst<'_>| i.op == OP_EXT_INST && i.operands.get(2) == Some(&set);
    match error {
        Error::UnsupportedInstruction(_, op) => {
            let code = *op as u32;
            first_line(&insts, label, |i| i.op == code)
        }
        Error::UnknownInstruction(code) => first_line(&insts, label, |i| i.op == u32::from(*code)),
        Error::UnsupportedExtSet(name) => first_line(&insts, label, ext_with_set(set_of(name)?)),
        Error::UnsupportedExtInstSet(set) => first_line(&insts, label, ext_with_set(*set)),
        Error::UnsupportedExtInst(code) => first_line(&insts, label, |i| {
            i.op == OP_EXT_INST && i.operands.get(3) == Some(code)
        }),
        _ => None,
    }
}
