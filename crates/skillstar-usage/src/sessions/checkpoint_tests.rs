//! Unit tests for the sessions checkpoint store and file fingerprints.
//!
//! All go through the `SKILLSTAR_DATA_DIR` sandbox and never write the real
//! `$HOME`.

use std::path::PathBuf;

use super::FileCheckpoint;
use super::checkpoint::{
    CheckpointStore, can_resume, head_hash, head_matches, index_path, prefix_hash,
};
use crate::test_support::EnvGuard;

fn sandbox() -> (tempfile::TempDir, EnvGuard) {
    let data = tempfile::tempdir().unwrap();
    let guard = EnvGuard::set(&[("SKILLSTAR_DATA_DIR", data.path())]);
    (data, guard)
}

fn sample_checkpoint() -> FileCheckpoint {
    FileCheckpoint {
        version: 7,
        head_hash: "v1:0:e3b0c44298fc1c149afbf4c8996fb92427ae41e4649b934ca495991b7852b855"
            .to_string(),
        prefix_hash: "sample-v1:abc".to_string(),
        size: 10,
        offset: 8,
        agent_state: serde_json::json!({"msgs": {}, "requested": "claude-sonnet-5"}),
        calls_seen: 3,
    }
}

#[test]
fn store_save_load_roundtrip_is_equal() {
    let (_data, _guard) = sandbox();
    let mut store = CheckpointStore::empty();
    store.upsert(PathBuf::from("/tmp/session.jsonl"), sample_checkpoint());
    store.upsert(
        PathBuf::from("/tmp/other.jsonl"),
        FileCheckpoint {
            version: 7,
            ..sample_checkpoint()
        },
    );
    store.save().unwrap();

    let loaded = CheckpointStore::load();
    assert_eq!(
        loaded.get(&PathBuf::from("/tmp/session.jsonl")),
        Some(&sample_checkpoint())
    );
    assert!(loaded.get(&PathBuf::from("/tmp/other.jsonl")).is_some());
}

#[test]
fn store_load_without_file_or_with_bad_json_is_empty() {
    let (_data, _guard) = sandbox();
    assert!(CheckpointStore::load().get(&PathBuf::from("/x")).is_none());

    // Corrupted index: counts as empty (full reread next run), no panic.
    std::fs::create_dir_all(index_path().parent().unwrap()).unwrap();
    std::fs::write(index_path(), b"not json").unwrap();
    assert!(CheckpointStore::load().get(&PathBuf::from("/x")).is_none());
}

#[test]
fn store_version_mismatch_drops_everything() {
    let (_data, _guard) = sandbox();
    let mut store = CheckpointStore::empty();
    store.upsert(PathBuf::from("/tmp/a.jsonl"), sample_checkpoint());
    store.save().unwrap();

    // Manually rewrite the top-level version to an unrecognized one: the
    // whole index is invalidated (hard cut, D6: no migration).
    let bytes = std::fs::read(index_path()).unwrap();
    let mut value: serde_json::Value = serde_json::from_slice(&bytes).unwrap();
    value["version"] = serde_json::json!(9999);
    std::fs::write(index_path(), serde_json::to_vec(&value).unwrap()).unwrap();

    assert!(
        CheckpointStore::load()
            .get(&PathBuf::from("/tmp/a.jsonl"))
            .is_none()
    );
}

#[test]
fn store_keeps_only_upserted_files() {
    let (_data, _guard) = sandbox();
    let mut store = CheckpointStore::empty();
    store.upsert(PathBuf::from("/tmp/kept.jsonl"), sample_checkpoint());
    store.save().unwrap();
    // The new round upserts only files that still exist: checkpoints of
    // vanished files are pruned.
    let mut next = CheckpointStore::empty();
    next.upsert(PathBuf::from("/tmp/live.jsonl"), sample_checkpoint());
    next.save().unwrap();

    let loaded = CheckpointStore::load();
    assert!(loaded.get(&PathBuf::from("/tmp/kept.jsonl")).is_none());
    assert!(loaded.get(&PathBuf::from("/tmp/live.jsonl")).is_some());
}

#[test]
fn head_hash_encodes_length_and_matches_prefix_of_longer_head() {
    let head = b"abcdef".to_vec();
    let fingerprint = head_hash(&head);
    assert!(fingerprint.starts_with("v1:6:"));

    // After the file grows the head window gets longer: comparing truncated
    // to the recorded length still matches (growth is not replacement).
    let grown = [head.as_slice(), b"ghijklmnop".as_slice()].concat();
    assert!(head_matches(&grown, &fingerprint));
    // The head prefix was rewritten: no match (replacement, must reread from
    // zero).
    let replaced = [b"xxxdef".as_slice(), b"ghijklmnop".as_slice()].concat();
    assert!(!head_matches(&replaced, &fingerprint));
    // Current head shorter than recorded: truncation, no match.
    assert!(!head_matches(b"abc".as_ref(), &fingerprint));
    // An unrecognized fingerprint shape (old format/dirty data): no match,
    // take the full-reread path.
    assert!(!head_matches(&head, "sha256:deadbeef"));
    assert!(!head_matches(&head, "v1:x:deadbeef"));
}

#[test]
fn prefix_hash_is_stable_and_bounded_by_sampling() {
    let dir = tempfile::tempdir().unwrap();
    let small = dir.path().join("small.jsonl");
    std::fs::write(&small, vec![b'x'; 1000]).unwrap();
    assert_eq!(prefix_hash(&small, 1000), prefix_hash(&small, 1000));
    // Only the prefix is hashed: appending does not change the already-read
    // prefix's fingerprint (the key to growth not triggering a reread).
    let before = prefix_hash(&small, 1000);
    std::fs::write(&small, vec![b'x'; 2000]).unwrap();
    assert_eq!(prefix_hash(&small, 1000), before);
    // Prefix content changed → fingerprint changed (full-window path).
    std::fs::write(&small, vec![b'y'; 1000]).unwrap();
    assert_ne!(prefix_hash(&small, 1000), before);

    // Beyond 64 KiB, sampling windows kick in: two computations agree; a
    // rewrite inside a window must be caught.
    let big = dir.path().join("big.jsonl");
    std::fs::write(&big, vec![b'a'; 200_000]).unwrap();
    let big_before = prefix_hash(&big, 200_000);
    assert_eq!(prefix_hash(&big, 200_000), big_before);

    let mut sampled = vec![b'a'; 200_000];
    // The first sampling window starts at 0: changing bytes inside a window
    // always hits.
    sampled[..64].fill(b'b');
    std::fs::write(&big, &sampled).unwrap();
    assert_ne!(prefix_hash(&big, 200_000), big_before);
}

#[test]
fn prefix_hash_missing_file_is_empty() {
    assert_eq!(
        prefix_hash(PathBuf::from("/nonexistent/sessions/x.jsonl").as_path(), 10),
        ""
    );
}

#[test]
fn can_resume_checks_all_five_guards() {
    let dir = tempfile::tempdir().unwrap();
    let path = dir.path().join("s.jsonl");
    std::fs::write(&path, vec![b'0'; 4096]).unwrap();
    let head = super::checkpoint::read_head(&path);
    let prior = FileCheckpoint {
        version: 7,
        head_hash: head_hash(&head),
        prefix_hash: prefix_hash(&path, 4096),
        size: 4096,
        offset: 4096,
        agent_state: serde_json::json!({}),
        calls_seen: 1,
    };
    // Everything matches (file unchanged): resumable.
    assert!(can_resume(&prior, 7, 4096, &head, &prior.prefix_hash));
    // 1) Parser version mismatch: from zero.
    assert!(!can_resume(&prior, 8, 4096, &head, &prior.prefix_hash));
    // 2) File shrank (truncation): from zero.
    assert!(!can_resume(&prior, 7, 2048, &head, &prior.prefix_hash));
    // 3) offset past end of file: from zero.
    let beyond = FileCheckpoint {
        offset: 5000,
        ..prior.clone()
    };
    assert!(!can_resume(&beyond, 7, 4096, &head, &prior.prefix_hash));
    // 4) Head fingerprint mismatch (replacement): from zero.
    let bad_head = FileCheckpoint {
        head_hash: "v1:256:0000000000000000000000000000000000000000000000000000000000000000"
            .to_string(),
        ..prior.clone()
    };
    assert!(!can_resume(&bad_head, 7, 4096, &head, &prior.prefix_hash));
    // 5) Already-read prefix fingerprint mismatch: from zero.
    let bad_prefix = FileCheckpoint {
        prefix_hash: "sample-v1:different".to_string(),
        ..prior.clone()
    };
    assert!(!can_resume(&bad_prefix, 7, 4096, &head, &prior.prefix_hash));
    // An empty prior (size 0 / no prefix fingerprint): meaningless, from
    // zero.
    let empty = FileCheckpoint {
        size: 0,
        offset: 0,
        prefix_hash: String::new(),
        ..prior
    };
    assert!(!can_resume(&empty, 7, 4096, &head, ""));
}
