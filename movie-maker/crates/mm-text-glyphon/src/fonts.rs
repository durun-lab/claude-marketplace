//! プロジェクトの書体を fontdb に読み込む。
//!
//! 同じ family の OS の書体は外しておく。fontdb は同じ名前のフェイスが複数あると読み込みの順で選ぶので、
//! 外さないと PC ごとに入っているバージョンで画素が変わる。組を入れ替えるときは、外した OS の書体を戻してから読み直す。

use std::collections::BTreeSet;
use std::sync::Arc;

use cosmic_text::fontdb::{Database, FaceInfo, Language, Source, ID};
use cosmic_text::{FontSystem, SwashCache};
use mm_text_api::{fonts_fingerprint, FontFile};

/// 読み込んだプロジェクトの書体と、そのために外した OS の書体。
#[derive(Default)]
pub(crate) struct ProjectFonts {
    fingerprint: String,
    loaded: Vec<ID>,
    hidden: Vec<FaceInfo>,
}

impl ProjectFonts {
    /// `fonts` の組に入れ替える。同じ組なら何もしない。読めないファイルは飛ばす（OS の書体で描く）。
    pub(crate) fn replace(
        &mut self,
        fs: &mut FontSystem,
        cache: &mut SwashCache,
        fonts: &[FontFile],
    ) {
        let fingerprint = fonts_fingerprint(fonts);
        if fingerprint == self.fingerprint {
            return;
        }
        let db = fs.db_mut();
        self.unload(db);
        self.loaded = load_files(db, fonts);
        self.hidden = hide_same_family(db, &self.loaded);
        self.fingerprint = fingerprint;
        // グリフの画像はフェイスの ID でキャッシュしている。外した ID は使われなくなるだけだが、まとめて捨てる。
        *cache = SwashCache::new();
    }

    fn unload(&mut self, db: &mut Database) {
        for id in self.loaded.drain(..) {
            db.remove_face(id);
        }
        for info in self.hidden.drain(..) {
            db.push_face_info(info);
        }
    }
}

/// ファイルを読み込んで、できたフェイスの ID を返す。同じ中身は 1 回だけ読む。
fn load_files(db: &mut Database, fonts: &[FontFile]) -> Vec<ID> {
    let mut seen = BTreeSet::new();
    let mut ids = Vec::new();
    for font in fonts {
        if !seen.insert(font.identity()) {
            continue;
        }
        match std::fs::read(&font.path) {
            Ok(bytes) => {
                let loaded = db.load_font_source(Source::Binary(Arc::new(bytes)));
                ids.extend(
                    loaded
                        .into_iter()
                        .map(|id| alias(db, id, font.family.as_deref())),
                );
            }
            Err(e) => {
                tracing::warn!(path = %font.path.display(), %e, "プロジェクトの書体を読めない")
            }
        }
    }
    ids
}

/// フェイスに family の名前を足す。fontdb はフェイスを書き換えられないので、名前を足したコピーと入れ替える。
fn alias(db: &mut Database, id: ID, family: Option<&str>) -> ID {
    let Some(family) = family else {
        return id;
    };
    let Some(face) = db.face(id) else {
        return id;
    };
    if face.families.iter().any(|(n, _)| n == family) {
        return id;
    }
    let mut info = face.clone();
    info.families
        .push((family.to_owned(), Language::English_UnitedStates));
    db.remove_face(id);
    db.push_face_info(info)
}

/// `loaded` と同じ family を持つほかのフェイスを外し、外したフェイスを返す。
fn hide_same_family(db: &mut Database, loaded: &[ID]) -> Vec<FaceInfo> {
    let families: BTreeSet<&str> = loaded
        .iter()
        .filter_map(|id| db.face(*id))
        .flat_map(|f| f.families.iter().map(|(name, _)| name.as_str()))
        .collect();
    let hidden: Vec<FaceInfo> = db
        .faces()
        .filter(|f| !loaded.contains(&f.id))
        .filter(|f| {
            f.families
                .iter()
                .any(|(n, _)| families.contains(n.as_str()))
        })
        .cloned()
        .collect();
    for info in &hidden {
        db.remove_face(info.id);
    }
    hidden
}
