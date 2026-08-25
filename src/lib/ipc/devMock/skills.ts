/**
 * Dev-mock fragment: skills mode — installed skills, skill content/tutorials,
 * deploy status, skill groups, and project skill management. Sample data lives
 * in ./skillsData.ts; AGENTS comes from the app-shell fragment.
 */

import type { LocalDivergenceResolution } from "../../../types";
import { AGENTS } from "./appShell";
import { type DevMockHandlers, getAcpConfigState, iso } from "./shared";
import { DECKS, DEMO_TUTORIAL_HTML, PROJECTS } from "./skillsData";
import {
  devGhostSkills,
  devListSkills,
  devMigrateRenamedSkill,
  devResolveSkillUpdate,
  devSkillUpdateStates,
  devUpdateSkills,
} from "./skillsUpdateStore";

function demoSkillTutorial(args: Record<string, unknown>) {
  const skillName = String(args.name ?? "pdf-tools");
  return {
    state: "fresh",
    currentHash: "demo-skill-content-hash",
    html: DEMO_TUTORIAL_HTML,
    metadata: {
      skillName,
      contentHash: "demo-skill-content-hash",
      promptVersion: "1",
      schemaVersion: "1",
      tutorialStyle: getAcpConfigState().tutorial_style,
      agentLabel: "Claude Code",
      generatedAt: iso(0),
      fileCount: 3,
      totalBytes: 8_192,
    },
    staleReason: null,
  };
}

export const SKILLS_HANDLERS: DevMockHandlers = {
  list_skills: () => devListSkills(),
  update_skills: (args) => devUpdateSkills((args?.names as string[]) ?? []),
  resolve_skill_update: (args) =>
    devResolveSkillUpdate(String(args?.name ?? ""), args?.resolution as LocalDivergenceResolution),
  refresh_skill_updates: () => devSkillUpdateStates(),
  migrate_renamed_skill: (args) => devMigrateRenamedSkill(String(args?.name ?? "")),
  open_skill_folder: () => undefined,
  check_new_repo_skills: () => devGhostSkills(),
  get_dismissed_new_skills: () => [],
  read_skill_content: (args) => ({
    name: String((args?.name as string) ?? "pdf-tools"),
    description: "Read, merge, split, and OCR PDF files with a single command.",
    triggers: ["pdf", "ocr", "merge pdf"],
    scopes: ["files"],
    "allowed-tools": ["Bash", "Read"],
    content:
      "# PDF Tools\n\nA skill for working with **PDF** files — read, merge, split and OCR.\n\n## Features\n\n- Merge & split documents\n- OCR scanned pages\n- Fill interactive forms\n\n```bash\nskillstar run pdf-tools merge a.pdf b.pdf -o out.pdf\n```\n\n## Commands\n\n| Command | Description | Input formats | Output | Notes |\n| --- | --- | --- | --- | --- |\n| `merge` | Combine multiple PDFs into one document | PDF, PDF/A | single PDF | preserves bookmarks and metadata |\n| `split` | Split a PDF into separate pages or ranges | PDF | many PDFs | supports `1-3,5,8-` page expressions |\n| `ocr` | Run OCR over scanned pages and embed a text layer | PDF, PNG, JPEG, TIFF | searchable PDF | language auto-detected, falls back to English |\n| `form-fill` | Fill interactive AcroForm fields from a JSON map | PDF | filled PDF | flattening optional via `--flatten` |\n\n> Tip: pair with the `xlsx` skill for spreadsheet exports. See the [docs](https://example.com).",
  }),
  read_skill_file_raw: (args) =>
    `---\nname: ${String((args?.name as string) ?? "pdf-tools")}\ndescription: Read, merge, split, and OCR PDF files.\n---\n\n# PDF Tools\n\nA skill for working with **PDF** files.\n\n## Commands\n\n| Command | Description | Input formats | Output | Notes |\n| --- | --- | --- | --- | --- |\n| \`merge\` | Combine multiple PDFs into one document | PDF, PDF/A | single PDF | preserves bookmarks and metadata |\n| \`split\` | Split a PDF into separate pages or ranges | PDF | many PDFs | supports \`1-3,5,8-\` page expressions |\n| \`ocr\` | Run OCR over scanned pages and embed a text layer | PDF, PNG, JPEG, TIFF | searchable PDF | language auto-detected, falls back to English |`,
  list_skill_files: () => ["SKILL.md", "scripts/merge.py", "README.md"],
  get_skill_tutorial: (args) => demoSkillTutorial(args),
  generate_skill_tutorial: (args) => demoSkillTutorial(args),
  get_skill_deploy_status: (args) => {
    const name = String((args?.skillName as string) ?? "pdf-tools");
    // Mixed kinds so the degraded-deploy badges are visible in browser dev:
    // healthy link (no badge), copy fallback, and a dangling link.
    return [
      {
        agent_id: "claude",
        agent_name: "Claude Code",
        target_path: `/Users/dev/claude/skills/${name}`,
        kind: "link",
        link_alive: true,
      },
      {
        agent_id: "codex",
        agent_name: "Codex CLI",
        target_path: `/Users/dev/codex/skills/${name}`,
        kind: "copy",
        link_alive: true,
      },
      {
        agent_id: "opencode",
        agent_name: "OpenCode",
        target_path: `/Users/dev/opencode/skills/${name}`,
        kind: "link",
        link_alive: false,
      },
    ];
  },
  batch_toggle_skills_for_agent: (args) => ({
    succeeded: ((args?.skillNames as string[]) ?? []).slice(),
    failed: [],
  }),
  list_skill_groups: () => DECKS,
  list_projects: () => PROJECTS,
  get_project_skills: () => ({
    agents: { claude: ["pdf-tools", "xlsx"] },
    updated_at: iso(1),
  }),
  // Disk scan of a project's agent skill folders. Returns an empty-but-well-
  // typed result so the Projects selection flow (buildSymlinkSkillIndex etc.)
  // runs end-to-end in browser dev instead of crashing on `undefined.skills`.
  scan_project_skills: () => ({ skills: [], agents_found: [] }),
  rebuild_project_skills_from_disk: () => ({
    agents: { claude: ["pdf-tools", "xlsx"] },
    updated_at: iso(1),
  }),
  detect_project_agents: () => ({
    detected: AGENTS.filter((a) => a.project_skills_rel).map((a) => ({
      agent_id: a.id,
      display_name: a.display_name,
      icon: a.icon,
      project_skills_rel: a.project_skills_rel,
      exists: a.enabled,
    })),
    ambiguous_groups: [],
    auto_enable: ["claude"],
  }),
};
