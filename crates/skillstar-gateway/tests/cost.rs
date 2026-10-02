//! Tests for the three-tier effective price (spec slice 08).
//!
//! The tiers, first hit wins: a `prices` row in `model_gateway.json` (exact
//! `catalog/model` key, then the catalog wildcard), then the models.dev
//! cache cost, then unpriced. Explicit zero is a price. Price rows and the
//! keys around them must survive a writer's save untouched, and a `prices`
//! shape the typed view cannot carry must refuse the write the way any
//! malformed known field does — whole-file strict open, bytes untouched —
//! while the lenient read path falls through to the catalog tier.

use std::ffi::OsString;
use std::fs;
use std::path::{Path, PathBuf};
use std::sync::{Mutex, MutexGuard};
use std::time::SystemTime;

use serde_json::{Value, json};
use skillstar_gateway::{
    ModelCost, SaveListenError, TokenCounts, effective_price, models_dev_cache_path, save_listen,
};

/// Catalog cache for the sandbox. `probe/m1` carries a full cost, `probe/m3`
/// a partial one; `other/m7` names one unit, `other/m9` names only an
/// explicit zero `output`, `other/m8` names nothing.
const CATALOG: &[u8] = br#"{
    "probe": {
        "models": {
            "m1": {"id": "m1", "cost": {"input": 3, "output": 6, "cache_read": 0.3, "cache_write": 3.75}},
            "m3": {"id": "m3", "cost": {"input": 9}}
        }
    },
    "other": {
        "models": {
            "m7": {"id": "m7", "cost": {"input": 5}},
            "m8": {"id": "m8"},
            "m9": {"id": "m9", "cost": {"output": 0}}
        }
    }
}"#;

/// The three states plus the two override granularities: an exact row beats
/// the wildcard and the cache, the wildcard beats the cache, and only a
/// model with no row and no catalog cost is unpriced.
#[test]
fn override_beats_catalog_and_missing_is_unpriced() {
    let _lock = lock_gateway_env();
    let root = scratch("tiers");
    let _env = EnvRestore::sandbox(&root);
    install(
        &root,
        br#"{
        "prices": {
            "probe/m1": {"input": 10, "output": 30},
            "probe": {"input": 1, "output": 2}
        }
    }"#,
    );

    // Exact override, not the wildcard and not the cached 3/6/0.3/3.75.
    assert_eq!(
        effective_price("probe", "m1"),
        Some(ModelCost {
            input: 10.0,
            output: 30.0,
            ..ModelCost::default()
        })
    );
    // Wildcard override, not the cached input-only 9.
    assert_eq!(
        effective_price("probe", "m3"),
        Some(ModelCost {
            input: 1.0,
            output: 2.0,
            ..ModelCost::default()
        })
    );
    // No override rows: the cache decides, and its absent units bill zero.
    assert_eq!(
        effective_price("other", "m7"),
        Some(ModelCost {
            input: 5.0,
            ..ModelCost::default()
        })
    );
    // A catalog cost naming only an explicit zero is priced, not missing.
    assert_eq!(effective_price("other", "m9"), Some(ModelCost::default()));
    // A cache row with no cost, and a model no tier knows: unpriced.
    assert_eq!(effective_price("other", "m8"), None);
    assert_eq!(effective_price("ghost", "m1"), None);
}

/// A writer's save keeps every price row, every unknown key inside a row,
/// and every bystander top-level key. Zero unit prices normalize to
/// key-absent inside the row while the row — and its price — survive.
#[test]
fn price_rows_and_bystander_keys_survive_a_writer_save() {
    let _lock = lock_gateway_env();
    let root = scratch("roundtrip");
    let _env = EnvRestore::sandbox(&root);
    install(
        &root,
        br#"{
        "prices": {
            "probe/m1": {"input": 10, "output": 30, "note": "handwritten price note"},
            "probe": {"input": 1, "output": 2, "currency": "EUR"},
            "probe/zero": {"input": 0, "output": 0, "note": "explicit zero"}
        },
        "x-custom": {"whatever": ["also", "kept"]}
    }"#,
    );

    save_listen("lan").unwrap();

    let after = read_doc(&root);
    assert_eq!(after["listen"], json!("lan"));
    // Every row is still there with its values, its handwritten note, and
    // its reserved currency, exactly as typed or carried (D5 flatten). Unit
    // prices are typed f64, so integer literals normalize to `10.0` —
    // value-equal, like any typed number field.
    assert_eq!(
        after["prices"]["probe/m1"],
        json!({"input": 10.0, "output": 30.0, "note": "handwritten price note"})
    );
    assert_eq!(
        after["prices"]["probe"],
        json!({"input": 1.0, "output": 2.0, "currency": "EUR"})
    );
    // Zero units serialize as key-absent; the row and its note stay.
    assert_eq!(after["prices"]["probe/zero"], json!({"note": "explicit zero"}));
    assert_eq!(after["x-custom"], json!({"whatever": ["also", "kept"]}));

    // The saved file still resolves: the exact override, and an explicit
    // all-zero row that stays priced.
    assert_eq!(
        effective_price("probe", "m1"),
        Some(ModelCost {
            input: 10.0,
            output: 30.0,
            ..ModelCost::default()
        })
    );
    assert_eq!(effective_price("probe", "zero"), Some(ModelCost::default()));
}

/// A `prices` value the typed view cannot carry — not a map, a row that is
/// not an object, a unit that is not a number — fails the strict open, so
/// writers refuse and the bytes stay as they were; the lenient read path
/// then falls through to the catalog tier.
#[test]
fn a_prices_shape_the_typed_view_cannot_carry_refuses_the_write() {
    let _lock = lock_gateway_env();
    let root = scratch("malformed");
    let _env = EnvRestore::sandbox(&root);
    for broken in [
        br#"{"prices": 3}"#.as_slice(),
        br#"{"prices": {"probe/m1": 3}}"#,
        br#"{"prices": {"probe/m1": ["input"]}}"#,
        br#"{"prices": {"probe/m1": {"input": "free"}}}"#,
        br#"{"prices": null}"#,
    ] {
        install(&root, broken);
        assert!(
            matches!(save_listen("lan"), Err(SaveListenError::Store)),
            "writer must refuse bytes {broken:?}"
        );
        assert_eq!(fs::read(gateway_file(&root)).unwrap(), broken);
    }

    // The same file on the read path: lenient open, no overrides, so the
    // cached cost of `probe/m1` decides.
    assert_eq!(
        effective_price("probe", "m1"),
        Some(ModelCost {
            input: 3.0,
            output: 6.0,
            cache_read: 0.3,
            cache_write: 3.75,
        })
    );
}

/// `cost()` sums all four units over 1e6 and bills reasoning exactly once,
/// inside `output`.
#[test]
fn cost_sums_all_four_units_with_reasoning_inside_output() {
    let price = ModelCost {
        input: 3.0,
        output: 6.0,
        cache_read: 0.3,
        cache_write: 3.75,
    };
    let tokens = TokenCounts {
        input: 1_000_000,
        output: 2_000_000, // already folds the 500_000 reasoning tokens
        cache_read: 3_000_000,
        cache_write: 4_000_000,
        reasoning: 500_000,
    };

    // 3 + 12 + 0.9 + 15 USD; billing reasoning again would read 33.9.
    let total = price.cost(&tokens);
    assert!((total - 30.9).abs() < 1e-6, "got {total}");
    // Zero prices or zero tokens bill exactly zero.
    assert_eq!(ModelCost::default().cost(&tokens), 0.0);
    assert_eq!(price.cost(&TokenCounts::default()), 0.0);
}

fn install(root: &Path, gateway_body: &[u8]) {
    fs::write(gateway_file(root), gateway_body).unwrap();
    let cache = models_dev_cache_path();
    fs::create_dir_all(cache.parent().unwrap()).unwrap();
    fs::write(cache, CATALOG).unwrap();
}

fn read_doc(root: &Path) -> Value {
    serde_json::from_slice(&fs::read(gateway_file(root)).unwrap()).unwrap()
}

fn gateway_file(root: &Path) -> PathBuf {
    root.join("data").join("config").join("model_gateway.json")
}

fn lock_gateway_env() -> MutexGuard<'static, ()> {
    static LOCK: Mutex<()> = Mutex::new(());
    LOCK.lock().unwrap_or_else(|poison| poison.into_inner())
}

fn scratch(label: &str) -> PathBuf {
    let nanos = SystemTime::now()
        .duration_since(SystemTime::UNIX_EPOCH)
        .unwrap_or_default()
        .as_nanos();
    let path =
        std::env::temp_dir().join(format!("skillstar-{label}-{}-{nanos}", std::process::id()));
    fs::create_dir_all(&path).unwrap();
    path
}

struct EnvRestore {
    saved: Vec<(&'static str, Option<OsString>)>,
    root: PathBuf,
}

impl EnvRestore {
    fn sandbox(root: &Path) -> Self {
        let home = root.join("home");
        let data = root.join("data");
        fs::create_dir_all(&home).unwrap();
        fs::create_dir_all(data.join("config")).unwrap();
        let pairs = [
            ("HOME", Some(home.to_string_lossy().into_owned())),
            ("USERPROFILE", Some(home.to_string_lossy().into_owned())),
            (
                "SKILLSTAR_TOOL_SYNC_HOME",
                Some(home.to_string_lossy().into_owned()),
            ),
            (
                "SKILLSTAR_DATA_DIR",
                Some(data.to_string_lossy().into_owned()),
            ),
        ];
        let saved = pairs
            .into_iter()
            .map(|(key, value)| {
                let previous = std::env::var_os(key);
                unsafe {
                    match value {
                        Some(value) => std::env::set_var(key, value),
                        None => std::env::remove_var(key),
                    }
                }
                (key, previous)
            })
            .collect();
        Self {
            saved,
            root: root.to_path_buf(),
        }
    }
}

impl Drop for EnvRestore {
    fn drop(&mut self) {
        for (key, previous) in self.saved.drain(..) {
            unsafe {
                match previous {
                    Some(value) => std::env::set_var(key, value),
                    None => std::env::remove_var(key),
                }
            }
        }
        let _ = fs::remove_dir_all(&self.root);
    }
}
