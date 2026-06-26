/** SSH remote host management + remote skill operations.
 *
 * DTO field names are snake_case to match the Rust serde default (same
 * convention as `AgentProfile`). Tauri *command parameter* names stay
 * camelCase because Tauri auto-converts `host_id` ↔ `hostId` at the IPC edge. */
export type AuthMethod = { kind: "password" } | { kind: "key"; key_path: string };

/** A user-defined SSH remote host (non-sensitive fields only). */
export interface SshHost {
  id: string;
  display_name: string;
  host: string;
  port: number;
  username: string;
  auth_method: AuthMethod;
  /** Default remote dir the UI opens, e.g. `~/.claude/skills`. */
  default_remote_dir: string;
}

/** A host discovered from `~/.ssh/config` (read-only, not persisted). */
export interface SystemHost {
  alias: string;
  host: string;
  port: number;
  username: string;
  identity_file?: string;
}

/** A list entry: either a managed host (editable) or a system host (read-only,
 * importable). Tagged via `source` so the UI can render the two classes. */
export type SshHostListItem = ({ source: "managed" } & SshHost) | ({ source: "system" } & SystemHost);

export type RemoteSkillLayout = "hub_managed" | "standalone";

/** A skill detected on the remote host. */
export interface RemoteSkill {
  name: string;
  path: string;
  /** Agent id the skill belongs to (e.g. `grok`), derived from the parent dir. */
  agent: string;
  size: number;
  modified?: string;
  /** Whether files live under `~/.skillstar/hub/content` with an agent symlink. */
  layout?: RemoteSkillLayout;
}

/** An agent discovered by scanning the remote $HOME, with its skill count. */
export interface RemoteAgentSkills {
  agent: string;
  path: string;
  count: number;
}

/** Result of a remote skill discovery scan. */
export interface DiscoveryResult {
  agents: RemoteAgentSkills[];
  skills: RemoteSkill[];
  needs_migration_count?: number;
}

/** Result of a connection probe (connect + whoami + uname). */
export interface ConnectionTestResult {
  latency_ms: number;
  remote_user: string;
  system?: string;
}

/** `verified` | `unverified` | `mismatch` — drives the host-key TOFU prompt. */
export type HostKeyState = "verified" | "unverified" | "mismatch";

/** Output of `test_ssh_connection`: probe result + host-key trust state. */
export interface TestConnectionOutput {
  result: ConnectionTestResult;
  host_key_state: HostKeyState;
  /** Present only when `host_key_state` is `unverified` or `mismatch`. */
  fingerprint?: string;
}

/** Aggregate result of pushing one skill to a remote host. */
export interface PushResult {
  files_uploaded: number;
  bytes: number;
  remote_path: string;
}

export interface MigrateResult {
  remote_path: string;
  hub_content_path: string;
}

/** Aggregate result of a batch push: per-skill outcome + tally. */
export interface BatchPushResult {
  pushed: PushResult[];
  /** Skills that failed to push, with the error message. */
  failed: BatchPushFailure[];
  total: number;
  succeeded: number;
}

export interface BatchPushFailure {
  skill_name: string;
  error: string;
}

/** Content of a remote skill's SKILL.md read from the remote hub layout. */
export interface RemoteSkillContent {
  name: string;
  content: string;
  modified?: string;
}

/** Update availability state for a remote skill (hub-managed git repo). */
export interface RemoteSkillUpdateState {
  name: string;
  /** Whether `git rev-list HEAD..@{u}` reports > 0 commits. */
  update_available: boolean;
}

/** SSH remote host + skill commands. */
export interface SshCommands {
  list_ssh_hosts: { args: Record<string, never>; result: SshHostListItem[] };
  add_ssh_host: { args: { def: SshHost; credential?: string }; result: SshHost };
  update_ssh_host: {
    args: { id: string; def: SshHost; credential?: string };
    result: void;
  };
  delete_ssh_host: { args: { id: string }; result: void };
  import_system_host: { args: { alias: string }; result: SshHost };

  test_ssh_connection: { args: { def: SshHost }; result: TestConnectionOutput };
  accept_ssh_host_key: {
    args: { id: string; host: string; fingerprint: string };
    result: void;
  };

  discover_remote_skills: { args: { hostId: string }; result: DiscoveryResult };
  list_remote_skills: { args: { hostId: string; remoteDir: string }; result: RemoteSkill[] };
  push_skill_to_remote: {
    args: { hostId: string; skillName: string; remoteDir: string };
    result: PushResult;
  };
  migrate_remote_skill_to_hub: {
    args: {
      hostId: string;
      skillName: string;
      agentSkillsDir: string;
      standalonePath: string;
    };
    result: MigrateResult;
  };
  delete_remote_skill: { args: { hostId: string; remotePath: string }; result: void };

  /** Push many skills to the same host in one SSH session (non-atomic; per-skill failures collected). */
  push_skills_to_remote: {
    args: { hostId: string; skillNames: string[]; remoteDir: string };
    result: BatchPushResult;
  };
  /** Read the SKILL.md content of a hub-managed remote skill. */
  read_remote_skill_content: {
    args: { hostId: string; skillName: string };
    result: RemoteSkillContent;
  };
  /** Write raw text to a hub-managed remote skill's SKILL.md (atomic write). */
  write_remote_skill_content: {
    args: { hostId: string; skillName: string; content: string };
    result: void;
  };
  /** `git pull --ff-only` a hub-managed remote skill (git clones only). */
  pull_remote_skill: { args: { hostId: string; skillName: string }; result: void };
  /** Toggle (create/remove) the agent symlink for a hub-managed skill. */
  toggle_remote_agent_link: {
    args: { hostId: string; skillName: string; agentSkillsDir: string; enable: boolean };
    result: void;
  };
  /** Install a skill from a git URL directly onto the remote host (clone + link). */
  install_remote_skill: {
    args: { hostId: string; url: string; skillName: string; agentSkillsDir: string };
    result: void;
  };
  /** Check update availability for all hub-managed skills on a host. */
  check_remote_skill_updates: { args: { hostId: string }; result: RemoteSkillUpdateState[] };
}
