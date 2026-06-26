/** S3 cloud sync — target management + manifest-based skill sync.
 *
 * DTO field names are snake_case to match the Rust serde default (same
 * convention as `SshHost`). Tauri *command parameter* names stay camelCase
 * because Tauri auto-converts `target_id` ↔ `targetId` at the IPC edge.
 *
 * Works against any S3-compatible service (Cloudflare R2, Backblaze B2, 七牛云,
 * 阿里云 OSS, 腾讯云 COS, AWS S3, MinIO) by configuring `endpoint_url` +
 * `region` + `bucket`. */

/** A user-defined S3-compatible sync target (non-sensitive fields only).
 *  `secret_access_key` is held in the OS keyring, never persisted in the TOML. */
export interface S3Target {
  id: string;
  display_name: string;
  /** S3-compatible endpoint URL. Empty for AWS S3 (region-based). Required for
   *  R2 / MinIO / 七牛 / OSS. */
  endpoint_url: string;
  region: string;
  bucket: string;
  /** Key prefix inside the bucket (normalised to end with `/`). */
  prefix: string;
  /** Public-ish access key id (safe to persist in TOML). */
  access_key_id: string;
  /** `true` → path-style addressing (`endpoint/bucket/key`). Required for MinIO. */
  force_path_style: boolean;
}

/** Result of a HeadBucket probe. */
export interface S3ConnectionTestResult {
  latency_ms: number;
}

/** A single skill entry in the cloud manifest. Tagged by `kind`. */
export type ManifestEntry =
  | {
      kind: "hub";
      name: string;
      git_url: string;
      source_folder?: string;
      tree_hash?: string;
      description: string;
    }
  | {
      kind: "local";
      name: string;
      tarball_key: string;
      sha256: string;
      size_bytes: number;
      description: string;
      uploaded_at: string;
    };

/** A manifest entry annotated with local install state — returned by
 *  `pull_cloud_manifest` so the UI can badge already-installed skills. */
export interface ManifestEntryView {
  /** The discriminated manifest entry (mirrors `ManifestEntry`). */
  kind: "hub" | "local";
  name: string;
  description: string;
  // hub-only
  git_url?: string;
  source_folder?: string;
  tree_hash?: string;
  // local-only
  tarball_key?: string;
  sha256?: string;
  size_bytes?: number;
  uploaded_at?: string;
  /** Whether this skill is already installed on the current device. */
  installed_locally: boolean;
}

/** Per-skill restore outcome. */
export type InstallOutcome =
  | { status: "existing"; name: string }
  | { status: "installed"; name: string }
  | { status: "restored"; name: string }
  | { status: "skipped"; name: string; reason: string };

/** Aggregate restore result, mirrors `ShareCodeInstallSummary`. */
export interface S3InstallSummary {
  requested_count: number;
  installed_names: string[];
  existing_names: string[];
  restored_names: string[];
  skipped_names: string[];
  outcomes: InstallOutcome[];
}

/** Push summary returned by `push_skills_to_cloud`. */
export interface S3PushSummary {
  hubCount: number;
  localCount: number;
  tarballsUploaded: number;
  tarballsSkipped: number;
  manifestUploaded: boolean;
}

/** S3 cloud sync commands. */
export interface S3Commands {
  list_s3_targets: { args: Record<string, never>; result: S3Target[] };
  add_s3_target: { args: { def: S3Target; secretAccessKey?: string }; result: S3Target };
  update_s3_target: {
    args: { id: string; def: S3Target; secretAccessKey?: string };
    result: void;
  };
  delete_s3_target: { args: { id: string }; result: void };
  test_s3_connection: { args: { def: S3Target }; result: S3ConnectionTestResult };

  push_skills_to_cloud: { args: { targetId: string }; result: S3PushSummary };
  pull_cloud_manifest: { args: { targetId: string }; result: ManifestEntryView[] };
  install_from_cloud_manifest: {
    args: { targetId: string; entries: ManifestEntry[] };
    result: S3InstallSummary;
  };
}
