/**
 * Dev-mock fragment: global app shell — agent profiles, patrol, developer
 * mode, and app-update checks. Owns the AGENTS sample data (also consumed by
 * the skills fragment for project agent detection).
 */

import { type DevMockHandlers, iso } from "./shared";

export const AGENTS = [
  ["claude", "Claude Code", "lobe:claude", ".claude/skills", false, false, 4],
  ["codex", "Codex", "lobe:codex", ".agents/skills", false, false, 2],
  ["cursor", "Cursor", "lobe:cursor", ".agents/skills", false, false, 1],
  ["gemini", "Gemini CLI", "lobe:gemini", ".agents/skills", false, false, 0],
  ["antigravity", "Antigravity", "lobe:antigravity", ".agents/skills", false, false, 0],
  ["opencode", "OpenCode", "lobe:opencode", ".agents/skills", false, false, 3],
  ["qoder", "Qoder", "lobe:qoder", ".qoder/skills", false, false, 0],
  ["trae", "Trae", "lobe:trae", ".trae/skills", false, false, 0],
  ["openclaw", "OpenClaw", "lobe:openclaw", "skills", false, false, 0],
  ["hermes", "Hermes Agent", "lobe:hermes", ".hermes/skills", false, false, 0],
  ["zcode", "ZCode", "lobe:zcode", ".zcode/skills", false, false, 0],
].map(([id, display_name, icon, rel, installed, enabled, synced]) => ({
  id,
  display_name,
  icon,
  global_skills_dir: `/Users/dev/${id}/skills`,
  project_skills_rel: rel,
  installed,
  enabled,
  synced_count: synced,
}));

export const APP_SHELL_HANDLERS: DevMockHandlers = {
  list_agent_profiles: () => AGENTS,
  toggle_agent_profile: (args) => {
    const agent = AGENTS.find((profile) => profile.id === String(args.id));
    if (!agent) return false;
    const nextEnabled = !agent.enabled;
    agent.enabled = nextEnabled;
    // `installed` is a frozen compatibility field; manual activation is now
    // the only source of truth in the browser mock as well.
    agent.installed = nextEnabled;
    return nextEnabled;
  },
  get_patrol_status: () => ({
    enabled: true,
    running: true,
    interval_secs: 3600,
    last_check: iso(0),
  }),
  set_patrol_enabled: () => undefined,
  check_developer_mode: () => true,
  check_app_update: () => ({
    available: false,
    version: null,
    date: null,
    body: null,
    release_url: null,
  }),
};
