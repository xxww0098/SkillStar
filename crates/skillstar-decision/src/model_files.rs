//! Where the AgentJev checkpoint lives, and how it gets there.
//!
//! Weights are deliberately **not** vendored into the repository: the
//! checkpoint is 1.2 GB. They are fetched once into the user's data directory,
//! verified against pinned digests, and reused afterwards. A partial download
//! resumes with an HTTP range request instead of starting over, because a
//! flaky link at 1.1 GB must not cost the whole transfer.

use std::path::{Path, PathBuf};
use std::sync::atomic::{AtomicBool, Ordering};

use serde::Serialize;
use sha2::{Digest, Sha256};
use ts_rs::TS;

use crate::error::{DecisionError, Result};

/// Human-readable model id echoed in responses.
pub const MODEL_ID: &str = "AgentJev-0.6B";

/// Hugging Face repository that publishes the weights.
pub const MODEL_REPO: &str = "aimeigaoshou/agent-jev";

/// Pinned revision. A moving `main` would silently change what the pinned
/// digests below are checked against.
pub const MODEL_REVISION: &str = "b3bf6b6dd443d6e724943b9194da31a4f055428e";

/// One file the runtime needs, with its integrity data.
pub struct ModelFile {
    /// File name inside the checkpoint directory.
    pub name: &'static str,
    /// Exact byte size.
    pub bytes: u64,
    /// SHA-256 of the content.
    pub sha256: &'static str,
}

/// The four files a run needs: trunk + head weights, tokenizer, geometry,
/// calibration temperatures.
pub const MODEL_FILES: &[ModelFile] = &[
    ModelFile {
        name: "model.safetensors",
        bytes: 1_196_881_242,
        sha256: "8166e46dc6019ae13f0d8fc97d603cdbcc2d20eb1882c7350c8deb9fc6bae215",
    },
    ModelFile {
        name: "tokenizer.json",
        bytes: 7_031_645,
        sha256: "c0382117ea329cdf097041132f6d735924b697924d6f6fc3945713e96ce87539",
    },
    ModelFile {
        name: "config.json",
        bytes: 754,
        sha256: "f5fbcc53c06e833cffa4af05b8ab34988be9429df265a17e562c920114852384",
    },
    ModelFile {
        name: "temperatures.json",
        bytes: 389,
        sha256: "5e5032896d77c72feb86b4725abe418488b8a1c7476014c706bd8215eb43c86c",
    },
];

/// Total bytes of a complete checkpoint directory.
pub fn total_bytes() -> u64 {
    MODEL_FILES.iter().map(|file| file.bytes).sum()
}

/// Resolve the Hugging Face endpoint, honoring a mirror.
///
/// `SKILLSTAR_HF_ENDPOINT` wins over the conventional `HF_ENDPOINT` so a user
/// can point only SkillStar at a mirror; both accept `https://hf-mirror.com`.
pub fn endpoint() -> String {
    for key in ["SKILLSTAR_HF_ENDPOINT", "HF_ENDPOINT"] {
        if let Ok(value) = std::env::var(key)
            && !value.trim().is_empty()
        {
            return value.trim().trim_end_matches('/').to_string();
        }
    }
    "https://huggingface.co".to_string()
}

/// URL of one checkpoint file.
pub fn file_url(name: &str) -> String {
    format!(
        "{}/{}/resolve/{}/{name}",
        endpoint(),
        MODEL_REPO,
        MODEL_REVISION
    )
}

/// Checkpoint directory plus derived paths.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct ModelPaths {
    dir: PathBuf,
}

impl ModelPaths {
    /// `SKILLSTAR_DECISION_MODEL_DIR` when set, else
    /// `<data_root>/models/agentjev-0.6b`. `data_root` already honors
    /// `SKILLSTAR_DATA_DIR`.
    pub fn resolve() -> Self {
        if let Ok(dir) = std::env::var("SKILLSTAR_DECISION_MODEL_DIR")
            && !dir.trim().is_empty()
        {
            return Self::at(dir.trim());
        }
        Self::at(
            skillstar_core::infra::paths::data_root()
                .join("models")
                .join("agentjev-0.6b"),
        )
    }

    /// Explicit directory (tests, CLI overrides).
    pub fn at(dir: impl Into<PathBuf>) -> Self {
        Self { dir: dir.into() }
    }

    /// Directory holding the checkpoint files.
    pub fn dir(&self) -> &Path {
        &self.dir
    }

    /// Path of one checkpoint file.
    pub fn file(&self, name: &str) -> PathBuf {
        self.dir.join(name)
    }

    /// `model.safetensors` path.
    pub fn weights(&self) -> PathBuf {
        self.file("model.safetensors")
    }

    /// `tokenizer.json` path.
    pub fn tokenizer(&self) -> PathBuf {
        self.file("tokenizer.json")
    }

    /// `config.json` path.
    pub fn config(&self) -> PathBuf {
        self.file("config.json")
    }

    /// `temperatures.json` path.
    pub fn temperatures(&self) -> PathBuf {
        self.file("temperatures.json")
    }

    /// Current state of every file, without hashing (size only, so this stays
    /// cheap enough to call from a status command).
    pub fn status(&self) -> ModelStatus {
        let mut files = Vec::with_capacity(MODEL_FILES.len());
        let mut present_bytes = 0u64;
        for spec in MODEL_FILES {
            let path = self.file(spec.name);
            let actual = std::fs::metadata(&path).map(|meta| meta.len()).ok();
            let present = actual == Some(spec.bytes);
            if present {
                present_bytes += spec.bytes;
            }
            files.push(ModelFileStatus {
                name: spec.name.to_string(),
                bytes: spec.bytes,
                present,
            });
        }
        let state = if present_bytes == 0 {
            ModelState::Missing
        } else if present_bytes == total_bytes() {
            ModelState::Ready
        } else {
            ModelState::Partial
        };
        ModelStatus {
            dir: self.dir.display().to_string(),
            endpoint: endpoint(),
            revision: MODEL_REVISION.to_string(),
            state,
            present_bytes,
            total_bytes: total_bytes(),
            files,
        }
    }

    /// Full verification (size + digest) before loading the checkpoint.
    pub fn verify(&self) -> Result<()> {
        for spec in MODEL_FILES {
            let path = self.file(spec.name);
            let actual = std::fs::metadata(&path)
                .map_err(|error| DecisionError::io(&path, error))?
                .len();
            if actual != spec.bytes {
                return Err(DecisionError::ModelFiles(format!(
                    "{} is {actual} bytes, expected {}",
                    path.display(),
                    spec.bytes
                )));
            }
            let digest = sha256_file(&path)?;
            if !digest.eq_ignore_ascii_case(spec.sha256) {
                return Err(DecisionError::ModelFiles(format!(
                    "{} failed its SHA-256 check; delete it and download again",
                    path.display()
                )));
            }
        }
        Ok(())
    }
}

/// Coarse state of the checkpoint directory.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, TS)]
#[serde(rename_all = "lowercase")]
#[ts(export, export_to = "DecisionModelState.ts", rename = "DecisionModelState")]
pub enum ModelState {
    /// Nothing downloaded yet.
    Missing,
    /// Some files are present, none partial-complete.
    Partial,
    /// Every file is present at its exact size (digests are checked on load).
    Ready,
}

/// Per-file presence.
#[derive(Debug, Clone, Serialize, TS)]
#[serde(rename_all = "camelCase")]
#[ts(export, export_to = "DecisionModelFileStatus.ts", rename = "DecisionModelFileStatus")]
pub struct ModelFileStatus {
    /// File name.
    pub name: String,
    /// Expected size in bytes.
    // ts-rs maps u64 to bigint; every consumer reads these through JSON.parse,
    // which yields a plain number, and a checkpoint file is far under 2^53.
    #[ts(type = "number")]
    pub bytes: u64,
    /// Present at the expected size.
    pub present: bool,
}

/// Snapshot handed to the UI.
#[derive(Debug, Clone, Serialize, TS)]
#[serde(rename_all = "camelCase")]
#[ts(export, export_to = "DecisionModelStatus.ts", rename = "DecisionModelStatus")]
pub struct ModelStatus {
    /// Checkpoint directory on disk.
    pub dir: String,
    /// Endpoint that downloads would use.
    pub endpoint: String,
    /// Pinned revision.
    pub revision: String,
    /// Coarse state.
    pub state: ModelState,
    /// Bytes present at the expected size.
    #[ts(type = "number")]
    pub present_bytes: u64,
    /// Bytes of a complete checkpoint.
    #[ts(type = "number")]
    pub total_bytes: u64,
    /// Per-file detail.
    pub files: Vec<ModelFileStatus>,
}

/// One progress tick of [`download`].
#[derive(Debug, Clone, Serialize, TS)]
#[serde(rename_all = "camelCase")]
#[ts(export, export_to = "DecisionDownloadProgress.ts", rename = "DecisionDownloadProgress")]
pub struct DownloadProgress {
    /// Bytes finished across all files (including resumed bytes).
    #[ts(type = "number")]
    pub downloaded: u64,
    /// Bytes of a complete checkpoint.
    #[ts(type = "number")]
    pub total: u64,
    /// File currently being written.
    pub file: String,
}

/// Download using SkillStar's shared HTTP client.
///
/// The command layer is not allowed to build an HTTP client (architecture.md's
/// `probe_http_client` rule), so the crate exposes this one entry point that
/// carries the user's proxy configuration into the transfer.
pub async fn download_with_shared_client(
    paths: &ModelPaths,
    cancel: &AtomicBool,
    on_progress: impl FnMut(DownloadProgress),
) -> Result<()> {
    let client = skillstar_core::infra::http_client::probe_http_client(std::time::Duration::from_secs(
        600,
    ))
    .map_err(|error| DecisionError::Download(error.to_string()))?;
    download(&client, paths, cancel, on_progress).await
}

/// SHA-256 of a file, streamed so a 1.2 GB read does not fit in memory.
pub fn sha256_file(path: &Path) -> Result<String> {
    use std::io::Read;

    let mut file =
        std::fs::File::open(path).map_err(|error| DecisionError::io(path, error))?;
    let mut hasher = Sha256::new();
    let mut buffer = vec![0u8; 4 * 1024 * 1024];
    loop {
        let read = file
            .read(&mut buffer)
            .map_err(|error| DecisionError::io(path, error))?;
        if read == 0 {
            break;
        }
        hasher.update(&buffer[..read]);
    }
    let digest = hasher.finalize();
    let mut hex = String::with_capacity(digest.len() * 2);
    for byte in digest {
        use std::fmt::Write;
        let _ = write!(hex, "{byte:02x}");
    }
    Ok(hex)
}

/// Download every missing or invalid checkpoint file.
///
/// Blocks the calling task; callers run it off the UI thread and forward
/// [`DownloadProgress`] to their own event channel.
pub async fn download(
    client: &reqwest::Client,
    paths: &ModelPaths,
    cancel: &AtomicBool,
    mut on_progress: impl FnMut(DownloadProgress),
) -> Result<()> {
    use futures::StreamExt;

    std::fs::create_dir_all(paths.dir())
        .map_err(|error| DecisionError::io(paths.dir(), error))?;
    let total = total_bytes();
    let mut downloaded: u64 = 0;

    for spec in MODEL_FILES {
        let destination = paths.file(spec.name);
        if is_valid(&destination, spec)? {
            downloaded += spec.bytes;
            on_progress(DownloadProgress {
                downloaded,
                total,
                file: spec.name.to_string(),
            });
            continue;
        }

        let part = paths.file(&format!("{}.part", spec.name));
        // A `.part` longer than the target means a stale/corrupt attempt.
        let mut have = std::fs::metadata(&part).map(|meta| meta.len()).unwrap_or(0);
        if have > spec.bytes {
            std::fs::remove_file(&part).map_err(|error| DecisionError::io(&part, error))?;
            have = 0;
        }

        let mut url = file_url(spec.name);
        if have > 0 {
            url.push_str(&format!("?download=true&range={have}-"));
        }
        let mut request = client.get(&url);
        if have > 0 {
            request = request.header(reqwest::header::RANGE, format!("bytes={have}-"));
        }
        let response = request
            .send()
            .await
            .map_err(|error| DecisionError::Download(format!("{}: {error}", spec.name)))?;

        let status = response.status();
        // 206: the server honored the range, append. 200: it ignored it (or
        // this is a fresh start), so write the file from the beginning.
        let append = status == reqwest::StatusCode::PARTIAL_CONTENT;
        if !(status.is_success() || append) {
            return Err(DecisionError::Download(format!(
                "{}: HTTP {status}",
                spec.name
            )));
        }
        if !append && have > 0 {
            have = 0;
        }

        let mut file = if append {
            std::fs::OpenOptions::new()
                .append(true)
                .open(&part)
                .map_err(|error| DecisionError::io(&part, error))?
        } else {
            std::fs::File::create(&part).map_err(|error| DecisionError::io(&part, error))?
        };

        let mut written = have;
        let mut stream = response.bytes_stream();
        while let Some(chunk) = stream.next().await {
            if cancel.load(Ordering::Relaxed) {
                return Err(DecisionError::Download("cancelled".to_string()));
            }
            let chunk = chunk
                .map_err(|error| DecisionError::Download(format!("{}: {error}", spec.name)))?;
            use std::io::Write;
            file.write_all(&chunk)
                .map_err(|error| DecisionError::io(&part, error))?;
            written += chunk.len() as u64;
            on_progress(DownloadProgress {
                downloaded: downloaded + written,
                total,
                file: spec.name.to_string(),
            });
        }
        file.sync_all()
            .map_err(|error| DecisionError::io(&part, error))?;
        drop(file);

        if written != spec.bytes {
            return Err(DecisionError::Download(format!(
                "{}: got {written} bytes, expected {}",
                spec.name, spec.bytes
            )));
        }
        let digest = sha256_file(&part)?;
        if !digest.eq_ignore_ascii_case(spec.sha256) {
            let _ = std::fs::remove_file(&part);
            return Err(DecisionError::ModelFiles(format!(
                "{} failed its SHA-256 check after download",
                spec.name
            )));
        }
        std::fs::rename(&part, &destination)
            .map_err(|error| DecisionError::io(&destination, error))?;
        downloaded += spec.bytes;
    }

    paths.verify()
}

/// A file counts as present only at its exact size; digests are checked when
/// the engine loads, so a status command stays cheap.
fn is_valid(path: &Path, spec: &ModelFile) -> Result<bool> {
    match std::fs::metadata(path) {
        Ok(meta) => Ok(meta.len() == spec.bytes),
        Err(error) if error.kind() == std::io::ErrorKind::NotFound => Ok(false),
        Err(error) => Err(DecisionError::io(path, error)),
    }
}
