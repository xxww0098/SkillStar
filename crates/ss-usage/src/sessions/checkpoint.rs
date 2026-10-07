//! The incremental checkpoint for session parsing: on-disk index + file
//! fingerprints.
//!
//! The index lives at `cache/sessions/index.json`, atomically replaced
//! via `atomic_write` — this is SkillStar's own derived data; deleting it
//! only causes a full reread next time and loses nothing on the agent side.
//! The file fingerprints follow magpie's empirical design:
//!
//! - `head_hash`: sha256 of the file head (at most 256 bytes). Head
//!   unchanged + file longer ⇒ most likely "growth" (append), resume may be
//!   attempted; head changed ⇒ the file was replaced and must be reread from
//!   zero. After a short file grows, the head window itself gets longer and
//!   the hash naturally changes, so the fingerprint encodes "the head length
//!   at record time" and validation truncates to that length before
//!   comparing.
//! - `prefix_hash`: a **sampled** hash of the already-read prefix (the
//!   `size` bytes finished at the last parse): hashed in full when at most
//!   64 KiB, otherwise 16 evenly spaced 4 KiB windows. Rewrites outside the
//!   windows can slip through (accepted: session files only append), but no
//!   matter how long the session is, validation costs at most 64 KiB of
//!   reads.
//! - The sampled hash carries a `sample-v1:` version prefix; switching the
//!   sampling algorithm mismatches all old fingerprints, which naturally
//!   triggers a full reread, so no migration is needed.

use std::collections::BTreeMap;
use std::io::Read;
use std::path::{Path, PathBuf};

use serde::{Deserialize, Serialize};
use sha2::{Digest, Sha256};

use ss_core::infra::fs_ops::atomic_write;
use ss_core::infra::paths::sessions_index_path;

use super::FileCheckpoint;

/// File head fingerprint window (magpie headLen precedent).
const HEAD_LEN: u64 = 256;

/// Index schema version. Bump when the structure changes; old indexes are
/// invalidated wholesale (hard cut, D6: no migration).
const STORE_VERSION: u32 = 1;

/// Sampling hash window count and window size (magpie prefixHash precedent).
const SAMPLE_WINDOW: u64 = 4096;
const SAMPLE_COUNT: u64 = 16;

/// `cache/sessions/index.json` (v3; rebuildable checkpoint cache).
pub(super) fn index_path() -> PathBuf {
    sessions_index_path()
}

/// The checkpoint index for all session files (path → checkpoint).
#[derive(Debug, Default)]
pub(super) struct CheckpointStore {
    files: BTreeMap<PathBuf, FileCheckpoint>,
}

#[derive(Serialize, Deserialize)]
struct StoreFile {
    version: u32,
    files: BTreeMap<PathBuf, FileCheckpoint>,
}

impl CheckpointStore {
    pub(super) fn empty() -> Self {
        Self::default()
    }

    /// Load the index; a missing file, corruption or a version mismatch all
    /// count as empty (full reread next run).
    pub(super) fn load() -> Self {
        let Ok(bytes) = std::fs::read(index_path()) else {
            return Self::empty();
        };
        match serde_json::from_slice::<StoreFile>(&bytes) {
            Ok(store) if store.version == STORE_VERSION => Self { files: store.files },
            _ => Self::empty(),
        }
    }

    pub(super) fn get(&self, path: &Path) -> Option<&FileCheckpoint> {
        self.files.get(path)
    }

    pub(super) fn paths(&self) -> impl Iterator<Item = &Path> {
        self.files.keys().map(PathBuf::as_path)
    }

    pub(super) fn upsert(&mut self, path: PathBuf, checkpoint: FileCheckpoint) {
        self.files.insert(path, checkpoint);
    }

    pub(super) fn path(&self) -> PathBuf {
        index_path()
    }

    /// Atomic write to disk. Failures are degraded by the caller (warn +
    /// full reread next run); this is not an error path.
    pub(super) fn save(&self) -> std::io::Result<()> {
        let store = StoreFile {
            version: STORE_VERSION,
            files: self.files.clone(),
        };
        let bytes = serde_json::to_vec(&store).map_err(|error| {
            std::io::Error::other(format!("failed to serialize sessions index: {error}"))
        })?;
        atomic_write(&index_path(), &bytes)
    }
}

/// Read the file head (at most [`HEAD_LEN`] bytes). An unopenable file
/// counts as empty (treated as a full reread).
pub(super) fn read_head(path: &Path) -> Vec<u8> {
    let mut head = vec![0u8; HEAD_LEN as usize];
    let Ok(mut file) = std::fs::File::open(path) else {
        return Vec::new();
    };
    let n = read_exact_or_short(&mut file, &mut head) as usize;
    head.truncate(n);
    head
}

/// File head fingerprint: `v1:<head length>:<sha256 hex>`.
pub(super) fn head_hash(head: &[u8]) -> String {
    format!("v1:{}:{}", head.len(), hex(&Sha256::digest(head)))
}

/// sha2 0.11's digest type no longer implements `LowerHex`, so hex is drawn
/// by hand here.
pub(super) fn hex(bytes: &[u8]) -> String {
    const DIGITS: &[u8; 16] = b"0123456789abcdef";
    let mut out = String::with_capacity(bytes.len() * 2);
    for byte in bytes {
        out.push(DIGITS[(byte >> 4) as usize] as char);
        out.push(DIGITS[(byte & 0xf) as usize] as char);
    }
    out
}

/// The parsers' shared "file did not grow and the head still matches" check.
/// Packed or rewritten-in-place formats must not use this: they have no
/// resume and always reread.
pub(super) fn is_unchanged(
    prior: &FileCheckpoint,
    file: &super::SessionFile,
    version: u32,
) -> bool {
    prior.version == version
        && prior.size == file.size
        && head_matches(&read_head(&file.path), &prior.head_hash)
}

/// Whether the current file head matches the head fingerprint recorded in a
/// checkpoint: re-hash the current head truncated to the head length
/// recorded in the checkpoint and compare against the full fingerprint
/// string (including the length segment). Current head shorter than
/// recorded ⇒ truncation; unrecognized fingerprint shape (old format/dirty
/// data) ⇒ no match, take the full-reread path.
pub(super) fn head_matches(head: &[u8], prior: &str) -> bool {
    match recorded_head_len(prior) {
        Some(len) if head.len() >= len => head_hash(&head[..len]) == prior,
        _ => false,
    }
}

/// The head length inside a `v1:<len>:<hex>` fingerprint.
fn recorded_head_len(prior: &str) -> Option<usize> {
    let rest = prior.strip_prefix("v1:")?;
    let (len, _) = rest.split_once(':')?;
    if len.is_empty() || !len.bytes().all(|b| b.is_ascii_digit()) {
        return None;
    }
    len.parse().ok()
}

/// Sampled hash of the already-read prefix (first `n` bytes):
/// `sample-v1:<sha256 hex>`. Read failure returns an empty string (the
/// caller treats it as a mismatch).
pub(super) fn prefix_hash(path: &Path, n: u64) -> String {
    let Ok(mut file) = std::fs::File::open(path) else {
        return String::new();
    };
    let mut hasher = Sha256::new();
    // A prefix of at most 64 KiB is hashed in full; longer ones take 16
    // evenly spaced 4 KiB windows (magpie prefixHash precedent: bounded
    // validation cost for appends).
    let full = n <= SAMPLE_WINDOW * SAMPLE_COUNT;
    let windows: Vec<(u64, u64)> = if full {
        vec![(0, n)]
    } else {
        (0..SAMPLE_COUNT)
            .map(|i| ((n - SAMPLE_WINDOW) * i / (SAMPLE_COUNT - 1), SAMPLE_WINDOW))
            .collect()
    };
    for (start, len) in windows {
        if let Some(mut section) = section(&mut file, start, len) {
            let mut buffer = vec![0u8; len as usize];
            if read_exact_or_short(&mut section, &mut buffer) == len {
                hasher.update(&buffer);
                continue;
            }
        }
        return String::new();
    }
    format!("sample-v1:{}", hex(&hasher.finalize()))
}

/// A read view of `[start, start+len)`.
fn section(
    file: &mut std::fs::File,
    start: u64,
    len: u64,
) -> Option<std::io::Take<&mut std::fs::File>> {
    use std::io::Seek;
    file.seek(std::io::SeekFrom::Start(start)).ok()?;
    Some(file.take(len))
}

/// Fill `buffer`, returning the number of bytes actually read (a short EOF
/// read is not an error).
fn read_exact_or_short(reader: &mut impl Read, buffer: &mut [u8]) -> u64 {
    let mut filled = 0usize;
    while filled < buffer.len() {
        match reader.read(&mut buffer[filled..]) {
            Ok(0) => break,
            Ok(n) => filled += n,
            Err(_) if filled == 0 => return 0,
            Err(_) => break,
        }
    }
    filled as u64
}

/// Whether a prior checkpoint can resume on the current file.
///
/// Five checks (magpie prepareCalls precedent): parser version, file not
/// shrunk, offset not past end of file, head fingerprint match, and
/// already-read prefix sampled-hash match. Any single failure means no
/// resume and the parser starts over from zero — this is the first gate of
/// truncation/replacement detection.
pub(super) fn can_resume(
    prior: &FileCheckpoint,
    parser_version: u32,
    file_size: u64,
    head: &[u8],
    current_prefix: &str,
) -> bool {
    prior.version == parser_version
        && file_size >= prior.size
        && prior.offset <= file_size
        && prior.size > 0
        && !prior.prefix_hash.is_empty()
        && head_matches(head, &prior.head_hash)
        && prior.prefix_hash == current_prefix
}
