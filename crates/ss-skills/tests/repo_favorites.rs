//! Local repository favorites persist under the data root.

use std::collections::BTreeSet;
use std::ffi::OsString;
use std::path::{Path, PathBuf};

use ss_skills::repo_favorites::{FavoriteWrite, LoadedFavorites, load, save};

struct DataDir {
    previous: Option<OsString>,
    root: PathBuf,
    _dir: tempfile::TempDir,
}

impl DataDir {
    fn new() -> Self {
        let dir = tempfile::tempdir().expect("temp dir");
        let root = dir.path().to_path_buf();
        let previous = std::env::var_os("SKILLSTAR_DATA_DIR");
        unsafe { std::env::set_var("SKILLSTAR_DATA_DIR", &root) };
        Self {
            previous,
            root,
            _dir: dir,
        }
    }

    fn favorites_path(&self) -> PathBuf {
        ss_core::infra::paths::repo_favorites_path()
    }
}

impl Drop for DataDir {
    fn drop(&mut self) {
        unsafe {
            match self.previous.take() {
                Some(value) => std::env::set_var("SKILLSTAR_DATA_DIR", value),
                None => std::env::remove_var("SKILLSTAR_DATA_DIR"),
            }
        }
    }
}

fn write_raw(path: &Path, body: &str) {
    if let Some(parent) = path.parent() {
        std::fs::create_dir_all(parent).unwrap();
    }
    std::fs::write(path, body).unwrap();
}

#[test]
fn favorites_roundtrip_ignore_stale_writes_and_refuse_a_newer_file() {
    let dir = DataDir::new();

    assert_eq!(load(), LoadedFavorites::Ready(BTreeSet::new()));

    save(
        1,
        BTreeSet::from(["ok/repo".into(), String::new(), "bad\nrepo".into()]),
    )
    .unwrap();
    assert_eq!(
        load(),
        LoadedFavorites::Ready(BTreeSet::from(["ok/repo".into()]))
    );

    assert_eq!(
        save(2, BTreeSet::from(["b/repo".into(), "a/repo".into()])).unwrap(),
        FavoriteWrite::Stored
    );
    assert_eq!(
        load(),
        LoadedFavorites::Ready(BTreeSet::from(["a/repo".into(), "b/repo".into()]))
    );

    save(3, BTreeSet::new()).unwrap();
    assert_eq!(load(), LoadedFavorites::Ready(BTreeSet::new()));

    save(5, BTreeSet::from(["kept/repo".into()])).unwrap();
    assert_eq!(
        save(4, BTreeSet::from(["lost/repo".into()])).unwrap(),
        FavoriteWrite::Stale
    );
    assert_eq!(
        load(),
        LoadedFavorites::Ready(BTreeSet::from(["kept/repo".into()]))
    );

    let path = dir.favorites_path();
    let original = r#"{"version":2,"sources":["future/repo"]}"#;
    write_raw(&path, original);
    assert_eq!(load(), LoadedFavorites::NewerVersion);
    assert_eq!(
        save(6, BTreeSet::from(["this/repo".into()])).unwrap(),
        FavoriteWrite::NewerVersion
    );
    assert_eq!(std::fs::read_to_string(path).unwrap(), original);
}
