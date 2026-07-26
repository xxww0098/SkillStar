/**
 * Dev-mock fragment: SSH remote hosts — managed/system host lists, connection
 * tests, and remote skill discovery/sync. Owns the in-memory host store and
 * the SSH sample data (single-domain, so both live here).
 */

import type { DevMockHandlers } from "./shared";

export const SSH_HOSTS = [
  {
    id: "ssh_demo_prod",
    display_name: "Prod GPU Box",
    host: "10.0.0.42",
    port: 22,
    username: "ubuntu",
    auth_method: { kind: "key", key_path: "~/.ssh/id_ed25519" },
    default_remote_dir: "~/.claude/skills",
  },
  {
    id: "ssh_demo_dev",
    display_name: "Dev Server",
    host: "dev.internal",
    port: 2222,
    username: "root",
    auth_method: { kind: "password" },
    default_remote_dir: "~/.codex/skills",
  },
];

export const REMOTE_SKILLS_SAMPLE = [
  {
    name: "pdf-tools",
    path: "~/.claude/skills/pdf-tools",
    size: 12480,
    modified: "2026-06-10",
  },
  {
    name: "git-flow",
    path: "~/.claude/skills/git-flow",
    size: 5120,
    modified: "2026-05-28",
  },
];

// Hosts discovered from ~/.ssh/config (browser dev only — real backend parses
// the actual file at runtime).
export const SYSTEM_SSH_HOSTS = [
  {
    alias: "vps-yy",
    host: "64.83.38.21",
    port: 22,
    username: "root",
    identity_file: "~/.ssh/id_ed25519_dstools",
  },
];

// SSH hosts are held in memory so browser-dev add/edit/delete persists across
// queries within a session (mirrors the real Tauri TOML store). Seeded from
// SSH_HOSTS once on first use.
let sshHostsStore: Record<string, unknown>[] | null = null;
function sshHosts(): Record<string, unknown>[] {
  if (sshHostsStore === null) {
    sshHostsStore = SSH_HOSTS.map((h) => ({ ...h }));
  }
  return sshHostsStore;
}

export const SSH_HANDLERS: DevMockHandlers = {
  list_ssh_hosts: () => {
    const managed = sshHosts().map((h) => ({ ...h, source: "managed" }));
    // De-dup system hosts already present in the managed store (by host).
    const managedHosts = new Set(sshHosts().map((h) => String(h.host)));
    const system = SYSTEM_SSH_HOSTS.filter((s) => !managedHosts.has(s.host)).map((s) => ({
      ...s,
      source: "system",
    }));
    return [...managed, ...system];
  },
  add_ssh_host: (args) => {
    const def = (args?.def ?? {}) as Record<string, unknown>;
    const created = {
      ...def,
      id: def.id ? String(def.id) : `ssh_${Date.now()}`,
    };
    sshHosts().push(created);
    return created;
  },
  update_ssh_host: (args) => {
    const { id, def } = (args ?? {}) as {
      id?: string;
      def?: Record<string, unknown>;
    };
    const idx = sshHosts().findIndex((h) => h.id === id);
    if (idx >= 0 && def) sshHosts()[idx] = { ...def, id };
    return undefined;
  },
  delete_ssh_host: (args) => {
    const { id } = (args ?? {}) as { id?: string };
    const store = sshHosts();
    const idx = store.findIndex((h) => h.id === id);
    if (idx >= 0) store.splice(idx, 1);
    return undefined;
  },
  import_system_host: (args) => {
    const { alias } = (args ?? {}) as { alias?: string };
    const sys = SYSTEM_SSH_HOSTS.find((s) => s.alias === alias);
    if (!sys) throw new Error(`system host '${alias}' not found`);
    const created = {
      id: `ssh_${Date.now()}`,
      display_name: sys.alias,
      host: sys.host,
      port: sys.port,
      username: sys.username,
      auth_method: sys.identity_file ? { kind: "key", key_path: sys.identity_file } : { kind: "password" },
      default_remote_dir: "",
    };
    sshHosts().push(created);
    return created;
  },
  test_ssh_connection: () => ({
    result: {
      latency_ms: 42,
      remote_user: "ubuntu",
      system: "Linux 6.5 x86_64",
    },
    host_key_state: "verified",
  }),
  accept_ssh_host_key: () => undefined,
  discover_remote_skills: () => ({
    agents: [
      { agent: "claude", path: "/root/.claude/skills", count: 2 },
      { agent: "codex", path: "/root/.codex/skills", count: 1 },
      { agent: "grok", path: "/root/.grok/skills", count: 1 },
    ],
    skills: [
      {
        name: "code-review",
        path: "/root/.claude/skills/code-review",
        agent: "claude",
        size: 8192,
        layout: "hub_managed",
      },
      {
        name: "brandkit",
        path: "/root/.claude/skills/brandkit",
        agent: "claude",
        size: 6144,
        layout: "standalone",
      },
      {
        name: "imagine",
        path: "/root/.codex/skills/imagine",
        agent: "codex",
        size: 4096,
        layout: "standalone",
      },
      {
        name: "find-skills",
        path: "/root/.grok/skills/find-skills",
        agent: "grok",
        size: 3072,
        layout: "standalone",
      },
    ],
    needs_migration_count: 3,
  }),
  migrate_remote_skill_to_hub: () => ({
    remote_path: "/root/.grok/skills/imagine",
    hub_content_path: "~/.skillstar/hub/content/imagine",
  }),
  list_remote_skills: () => REMOTE_SKILLS_SAMPLE,
  push_skill_to_remote: (args) => ({
    files_uploaded: 3,
    bytes: 8192,
    remote_path: `~/.claude/skills/${args?.skillName ?? "skill"}`,
  }),
  delete_remote_skill: () => undefined,
  push_skills_to_remote: (args) => {
    const names = (args?.skillNames ?? []) as string[];
    const pushed = names.map((skillName) => ({
      files_uploaded: 3,
      bytes: 8192,
      remote_path: `~/.claude/skills/${skillName}`,
    }));
    return {
      pushed,
      failed: [],
      total: names.length,
      succeeded: names.length,
    };
  },
  read_remote_skill_content: (args) => ({
    name: args?.skillName ?? "skill",
    content: "---\nname: skill\ndescription: Mocked remote SKILL.md\n---\n\n# Mocked remote skill body.\n",
    modified: "2025-01-01",
  }),
  write_remote_skill_content: () => undefined,
  pull_remote_skill: () => undefined,
  toggle_remote_agent_link: () => undefined,
  install_remote_skill: () => undefined,
  check_remote_skill_updates: () => [
    { name: "code-review", update_available: false },
    { name: "brandkit", update_available: true },
  ],
};
