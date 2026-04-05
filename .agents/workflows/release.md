---
description: How to release a new version of SkillStar (e.g. "发布版本 x.y.z")
---

# Release Workflow

// turbo-all

When the user says "发布版本 X.Y.Z" or "release version X.Y.Z", follow these steps **in order**.

## 0. Prerequisites: GitHub Secrets Configuration

Before initiating a release, ensure the following GitHub Secrets are configured in your repository (`Settings > Secrets and variables > Actions`). If these are missing, the `.github/workflows/release.yml` CI pipeline will fail or skip signing, breaking the auto-update mechanism and triggering macOS Gatekeeper warnings.

### Tauri Updater Signatures (Required for all platforms)
To ensure secure auto-updates via the `tauri-plugin-updater`:
- `TAURI_SIGNING_PRIVATE_KEY`: Private key string for the updater (generated via `tauri signer generate -w ~/.tauri/skillstar.key`).
- `TAURI_SIGNING_PRIVATE_KEY_PASSWORD`: Password used to encrypt the private key.

### Apple Developer Keys (Required for macOS signing & notarization)
To pass Apple's Gatekeeper and allow users to run the app normally:
- `APPLE_CERTIFICATE`: Base64 encoded string of the `.p12` backup (your "Developer ID Application" certificate).
- `APPLE_CERTIFICATE_PASSWORD`: Password used when exporting the `.p12` file.
- `APPLE_SIGNING_IDENTITY`: Exact certificate identity name (e.g., `Developer ID Application: Your Name (TEAMID1234)`).
- `APPLE_TEAM_ID`: Your 10-character Apple Developer Team ID.
- `APPLE_ID`: The Apple ID email account used for developer enrollment.
- `APPLE_PASSWORD`: An "App-Specific Password" generated in appleid.apple.com for `notarytool`.

### GitHub Permissions
Ensure the default `GITHUB_TOKEN` has the necessary writing permissions: 
Go to **Settings > Actions > General > Workflow permissions** and select **Read and write permissions**.

---

## 1. Bump version in all three files

Update the version string from the **current** value to the **target** value in:

| File | Field |
|------|-------|
| `package.json` | `"version": "X.Y.Z"` |
| `src-tauri/Cargo.toml` | `version = "X.Y.Z"` |
| `src-tauri/tauri.conf.json` | `"version": "X.Y.Z"` |

All three **must** match exactly.

## 2. Update CHANGELOG.md

- Rename `## [Unreleased]` → `## [X.Y.Z] - YYYY-MM-DD` (today's date).
- Insert a new blank `## [Unreleased]` section **above** the newly-versioned section.

The result should look like:

```markdown
## [Unreleased]

## [X.Y.Z] - 2026-04-01

### Changed
- ...
```

## 3. Stage and commit

```bash
git add -A
git commit -m "chore: bump version to X.Y.Z"
```

## 4. Push to remote

```bash
git push origin main
```

> [!NOTE]
> **Network Issues:** If the push fails with an error like `RPC failed; curl 56 Recv failure: Connection reset by peer` or `fatal: the remote end hung up unexpectedly`, it is typically due to a proxy or firewall issue (e.g. GFW). Ensure your VPN or proxy is active before retrying. 
> If you have a local proxy running but Git isn't using it, set the proxy explicitly:
> `git config --global http.proxy http://127.0.0.1:<port>`
> *(Don't forget to unset it later with `git config --global --unset http.proxy` if needed)*

## 5. Tag and push the tag

```bash
git tag vX.Y.Z
git push origin vX.Y.Z
```

This triggers the CI pipeline (`.github/workflows/release.yml`) which will:
1. Matrix-build for macOS arm64/x64, Linux x64, Windows x64
2. Generate `latest.json` with per-platform signatures
3. Publish the draft release

## 6. Verify

Print a summary table of what was done and provide the GitHub Actions link:

```
https://github.com/xxww0098/SkillStar/actions
```

---

## Fixing a missed file (amend flow)

If a file was missed after the commit + tag have already been pushed:

1. Make the fix.
2. `git add <file> && git commit --amend --no-edit`
3. Delete the remote tag: `git push origin :refs/tags/vX.Y.Z`
4. Recreate the tag: `git tag -f vX.Y.Z`
5. Force-push both: `git push --force origin main && git push origin vX.Y.Z`