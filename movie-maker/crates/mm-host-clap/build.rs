//! テストに使う CLAP のプラグイン（`tests/plugins/mm_test.c`）を、公式のヘッダーで `.clap` の共有ライブラリに作る。
//! 置き場所は環境変数 `MM_CLAP_TEST_PLUGINS` でテストに渡す。

use std::path::{Path, PathBuf};
use std::process::Command;

const INCLUDE: &str = "vendor/clap/include";
const SOURCE: &str = "tests/plugins/mm_test.c";
const NAME: &str = "MmTest";

fn main() {
    println!("cargo:rerun-if-changed={SOURCE}");
    println!("cargo:rerun-if-changed={INCLUDE}");
    let out = PathBuf::from(std::env::var("OUT_DIR").expect("OUT_DIR")).join("plugins");
    build_plugin(Path::new(SOURCE), &out);
    println!("cargo:rustc-env=MM_CLAP_TEST_PLUGINS={}", out.display());
}

/// `<out>/<NAME>.clap` に共有ライブラリを作る。
fn build_plugin(src: &Path, out: &Path) {
    let obj = out.join("obj");
    for d in [out, &obj] {
        std::fs::create_dir_all(d).expect("出力のフォルダー");
    }
    let binary = out.join(format!("{NAME}.clap"));
    let tool = cc::Build::new().cargo_metadata(false).get_compiler();
    let mut cmd: Command = tool.to_command();
    if tool.is_like_msvc() {
        cmd.args(["/nologo", "/LD", "/O2"])
            .arg(format!("/I{INCLUDE}"))
            .arg(src)
            .arg(format!("/Fe{}", binary.display()))
            .arg(format!("/Fo{}\\", obj.display()))
            .arg("/link")
            .arg(format!(
                "/IMPLIB:{}",
                obj.join(format!("{NAME}.lib")).display()
            ));
    } else {
        cmd.args(["-shared", "-fPIC", "-O2"])
            .arg(format!("-I{INCLUDE}"))
            .arg(src)
            .arg("-o")
            .arg(&binary);
    }
    let status = cmd.status().expect("C コンパイラーを起動できない");
    assert!(status.success(), "{} を作れない: {status}", src.display());
}
