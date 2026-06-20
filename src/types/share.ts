//! share domain types. Split out of the old monolithic index for
//! navigability; all re-exported by `index.ts`.

export type GhStatus =
  | { status: "NotInstalled" }
  | { status: "NotAuthenticated" }
  | { status: "Ready"; username: string };

export interface PublishResult {
  url: string;
  git_url: string;
  source_folder: string;
}

export interface GitInstallInstruction {
  label: string;
  command: string;
}

export type GitStatus =
  | { status: "Installed"; version: string }
  | {
      status: "NotInstalled";
      os: string;
      install_instructions: GitInstallInstruction[];
      download_url: string;
    };

export interface UserRepo {
  full_name: string;
  url: string;
  description: string;
  is_public: boolean;
  folders: string[];
}

// ── Skill Bundle (.ags) ──────────────────────────────────────

export interface BundleManifest {
  format_version: number;
  name: string;
  description: string;
  version: string;
  author: string;
  created_at: string;
  files: string[];
  checksum: string;
}

export interface ImportBundleResult {
  name: string;
  description: string;
  file_count: number;
  replaced: boolean;
}

export interface MultiManifestEntry {
  name: string;
  description: string;
  file_count: number;
}

export interface MultiManifest {
  format_version: number;
  created_at: string;
  skills: MultiManifestEntry[];
  checksum: string;
}

export interface ImportMultiBundleResult {
  skill_names: string[];
  total_file_count: number;
  replaced_count: number;
}

// ── Share Code Install ───────────────────────────────────────

/** One entry of a share-code payload sent to the Rust installer. */

export interface ShareCodeSkillInput {
  /** Skill name. */
  n: string;
  /** Git URL (empty when `c` is provided). */
  u: string;
  /** Base64-encoded SKILL.md body (optional). */
  c?: string;
  /** `true` when the source repo requires auth. */
  p?: boolean;
}

export type ShareSkillOutcome =
  | { status: "existing"; name: string }
  | { status: "installed"; name: string }
  | { status: "embedded"; name: string }
  | { status: "skipped"; name: string; reason: string };

export interface ShareCodeInstallSummary {
  requested_count: number;
  installed_names: string[];
  existing_names: string[];
  embedded_names: string[];
  skipped: { name: string; reason: string }[];
  outcomes: ShareSkillOutcome[];
}

// === Models Mode Types ===
