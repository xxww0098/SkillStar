import type { ProtoSkill } from "./types";

/** Fixed demo set so variants stay comparable without depending on hub state. */
export const MOCK_SKILLS: ProtoSkill[] = [
  {
    name: "code-review",
    description: "Review PRs with structured findings",
    source: "github.com/acme/skills",
    update_available: true,
  },
  {
    name: "release-notes",
    description: "Draft release notes from commits",
    source: "github.com/acme/skills",
    update_available: true,
  },
  {
    name: "test-plan",
    description: "Generate a test plan from a PRD",
    source: "github.com/acme/skills",
    update_available: true,
  },
  {
    name: "docs-sync",
    description: "Keep README in sync with code",
    source: "github.com/other/kit",
    update_available: true,
  },
  {
    name: "local-notes",
    description: "Personal scratch skill",
    source: "local",
    update_available: false,
  },
  {
    name: "changelog",
    description: "Summarize recent changes",
    source: "github.com/other/kit",
    update_available: false,
  },
];
