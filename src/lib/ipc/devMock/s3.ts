/**
 * Dev-mock fragment: S3 cloud sync — sync targets, connection tests, and the
 * cloud manifest push/pull/install flow. Owns the in-memory target store and
 * the S3 sample data (single-domain, so both live here).
 */

import type { DevMockHandlers } from "./shared";

// S3 cloud sync targets (browser dev only). Cloudflare R2 + a MinIO-style
// path-style target exercise both endpoint flavours the form supports.
export const S3_TARGETS = [
  {
    id: "s3_demo_r2",
    display_name: "Cloudflare R2",
    endpoint_url: "https://abc123.r2.cloudflarestorage.com",
    region: "auto",
    bucket: "skillstar",
    prefix: "skillstar/",
    access_key_id: "r2-access-key-id",
    force_path_style: false,
  },
  {
    id: "s3_demo_minio",
    display_name: "Home MinIO",
    endpoint_url: "http://192.168.1.10:9000",
    region: "us-east-1",
    bucket: "skills",
    prefix: "",
    access_key_id: "minioadmin",
    force_path_style: true,
  },
];

// A sample cloud manifest — git-backed skills + a local-authored skill — so the
// Cloud scope shows realistic cards before any real push happens.
export const CLOUD_MANIFEST_SAMPLE = [
  {
    kind: "hub" as const,
    name: "react-best-practices",
    description: "React 19 patterns, hooks, and server components.",
    git_url: "https://github.com/owner/react-best-practices.git",
    source_folder: "skills/react",
    tree_hash: "abc123",
    installed_locally: true,
  },
  {
    kind: "hub" as const,
    name: "pdf-tools",
    description: "Read, merge, split, and OCR PDF files.",
    git_url: "https://github.com/owner/pdf-tools.git",
    installed_locally: false,
  },
  {
    kind: "local" as const,
    name: "my-team-workflow",
    description: "Internal on-call runbook and triage scripts.",
    tarball_key: "tarballs/my-team-workflow/deadbeef.tar.gz",
    sha256: "deadbeef",
    size_bytes: 8192,
    uploaded_at: "2026-06-20T09:00:00Z",
    installed_locally: false,
  },
];

// S3 sync targets are held in memory so browser-dev add/edit/delete persists
// across queries within a session (mirrors the SSH host store in ./ssh.ts).
// Seeded from S3_TARGETS once on first use.
let s3TargetsStore: Record<string, unknown>[] | null = null;
function s3Targets(): Record<string, unknown>[] {
  if (s3TargetsStore === null) {
    s3TargetsStore = S3_TARGETS.map((t) => ({ ...t }));
  }
  return s3TargetsStore;
}

export const S3_HANDLERS: DevMockHandlers = {
  list_s3_targets: () => s3Targets().map((t) => ({ ...t })),
  add_s3_target: (args) => {
    const def = (args?.def ?? {}) as Record<string, unknown>;
    const created = {
      ...def,
      id: def.id ? String(def.id) : `s3_${Date.now()}`,
    };
    s3Targets().push(created);
    return created;
  },
  update_s3_target: (args) => {
    const { id, def } = (args ?? {}) as {
      id?: string;
      def?: Record<string, unknown>;
    };
    const idx = s3Targets().findIndex((t) => t.id === id);
    if (idx >= 0 && def) s3Targets()[idx] = { ...def, id };
    return undefined;
  },
  delete_s3_target: (args) => {
    const { id } = (args ?? {}) as { id?: string };
    const store = s3Targets();
    const idx = store.findIndex((t) => t.id === id);
    if (idx >= 0) store.splice(idx, 1);
    return undefined;
  },
  test_s3_connection: () => ({ latency_ms: 38 }),
  push_skills_to_cloud: () => ({
    hubCount: 2,
    localCount: 1,
    tarballsUploaded: 1,
    tarballsSkipped: 0,
    manifestUploaded: true,
  }),
  pull_cloud_manifest: () => CLOUD_MANIFEST_SAMPLE.map((e) => ({ ...e })),
  install_from_cloud_manifest: (args) => {
    const entries = (args?.entries ?? []) as { name?: string }[];
    const names = entries.map((e) => String(e.name ?? "")).filter(Boolean);
    return {
      requested_count: names.length,
      installed_names: names,
      existing_names: [] as string[],
      restored_names: [] as string[],
      skipped_names: [] as string[],
      outcomes: names.map((name) => ({ status: "installed" as const, name })),
    };
  },
};
