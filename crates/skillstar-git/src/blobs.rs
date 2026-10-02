//! Batched blob reads for treeless partial clones.
//!
//! Reading N promisor blobs one by one costs N smart-protocol round-trips
//! (measured: 22 `SKILL.md` blobs of `pbakaus/impeccable` took 39 s lazily,
//! 1.8 s as one fetch). Callers prefetch the whole set once, then read each
//! blob locally with lazy fetching disabled so a miss fails fast instead of
//! silently falling back to the slow path.

use anyhow::{Context, Result, anyhow};
use skillstar_core::infra::path_env::command_with_path;
use std::path::Path;

use crate::transport::{self, GitOperationSession};

/// Fetch `oids` from `origin` in a single request. Empty input is a no-op.
pub fn prefetch_blobs_in_session(
    repo_path: &Path,
    oids: &[String],
    session: &GitOperationSession,
) -> Result<()> {
    if oids.is_empty() {
        return Ok(());
    }
    let remote = crate::ops::remote_origin_url(repo_path)?;
    let mut args = vec![
        "-c",
        "fetch.negotiationAlgorithm=noop",
        "fetch",
        "--no-tags",
        "--no-write-fetch-head",
        "--recurse-submodules=no",
        "--filter=blob:none",
        "origin",
    ];
    args.extend(oids.iter().map(String::as_str));
    transport::execute_remote_git(Some(repo_path), &args, &remote, session, true)
        .map(|_| ())
        .map_err(anyhow::Error::from)
        .context("Failed to prefetch blobs")
}

/// Read a blob that is already in the local object store. Never contacts the
/// promisor remote: a missing object is an error.
pub fn read_local_blob(repo_path: &Path, oid: &str, max_bytes: u64) -> Result<String> {
    let run = |args: &[&str]| -> Result<Vec<u8>> {
        let output = command_with_path("git")
            .current_dir(repo_path)
            .env("GIT_NO_LAZY_FETCH", "1")
            .args(args)
            .output()
            .context("Failed to execute git cat-file")?;
        if !output.status.success() {
            return Err(anyhow!(
                "git {} failed: {}",
                args.join(" "),
                String::from_utf8_lossy(&output.stderr).trim()
            ));
        }
        Ok(output.stdout)
    };
    let size: u64 = String::from_utf8_lossy(&run(&["cat-file", "-s", oid])?)
        .trim()
        .parse()
        .with_context(|| format!("git cat-file -s returned no size for {oid}"))?;
    if size > max_bytes {
        return Err(anyhow!("blob {oid} is {size} bytes, above {max_bytes}"));
    }
    String::from_utf8(run(&["cat-file", "blob", oid])?)
        .with_context(|| format!("blob {oid} is not UTF-8"))
}

#[cfg(test)]
mod tests {
    use super::*;

    fn git(repo: &Path, args: &[&str]) -> String {
        let output = command_with_path("git")
            .current_dir(repo)
            .args(args)
            .output()
            .unwrap();
        assert!(
            output.status.success(),
            "git {args:?}: {}",
            String::from_utf8_lossy(&output.stderr)
        );
        String::from_utf8_lossy(&output.stdout).trim().to_string()
    }

    fn promisor_packs(repo: &Path) -> usize {
        std::fs::read_dir(repo.join(".git/objects/pack"))
            .unwrap()
            .flatten()
            .filter(|entry| {
                entry
                    .path()
                    .extension()
                    .is_some_and(|ext| ext == "promisor")
            })
            .count()
    }

    /// One fetch brings every requested blob; reads never go back out.
    #[test]
    fn prefetch_blobs_batches_into_one_fetch() {
        let remote = tempfile::tempdir().unwrap();
        git(remote.path(), &["init", "-q", "--initial-branch=main"]);
        for (key, value) in [
            ("user.email", "t@example.com"),
            ("user.name", "T"),
            ("uploadpack.allowFilter", "true"),
            ("uploadpack.allowAnySHA1InWant", "true"),
        ] {
            git(remote.path(), &["config", key, value]);
        }
        for dir in ["a", "b", "c"] {
            std::fs::create_dir_all(remote.path().join(dir)).unwrap();
            std::fs::write(
                remote.path().join(dir).join("SKILL.md"),
                format!("# {dir}\n"),
            )
            .unwrap();
        }
        git(remote.path(), &["add", "-A"]);
        git(remote.path(), &["commit", "-q", "-m", "init"]);

        let clone = tempfile::tempdir().unwrap();
        let dest = clone.path().join("clone");
        git(
            clone.path(),
            &[
                "clone",
                "-q",
                "--filter=blob:none",
                "--no-checkout",
                &crate::ops::local_file_url(remote.path()),
                dest.to_str().unwrap(),
            ],
        );
        let oids: Vec<String> = ["a", "b", "c"]
            .iter()
            .map(|dir| git(&dest, &["rev-parse", &format!("HEAD:{dir}/SKILL.md")]))
            .collect();
        assert!(
            read_local_blob(&dest, &oids[0], 1024).is_err(),
            "not local yet"
        );

        let before = promisor_packs(&dest);
        prefetch_blobs_in_session(&dest, &oids, &GitOperationSession::public()).unwrap();
        assert_eq!(promisor_packs(&dest), before + 1, "one fetch, one pack");
        for (oid, dir) in oids.iter().zip(["a", "b", "c"]) {
            assert_eq!(
                read_local_blob(&dest, oid, 1024).unwrap(),
                format!("# {dir}\n")
            );
        }
        assert!(read_local_blob(&dest, &oids[0], 2).is_err(), "size cap");
    }
}
