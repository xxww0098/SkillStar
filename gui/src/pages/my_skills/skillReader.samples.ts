import { splitMarkdown } from "./readerBlocks";

// SKILL.md 阅读器测试的共享真实样本 —— 逐字节内嵌（波 3 spike 同款四类
// 形态 + 中文-only 形态），host 与 test 共用（prose 块数、spanStyle 选择
// 器都从同一份文本推导）。来源见各常量注释；测试不触文件系统、不写
// $HOME。

// ---- 真实样本（spike 用过的四类形态） ---------------------------------------

/** 仓库内技能 .agents/skills/ask-matt/SKILL.md 全文（frontmatter + 9 标题
 * + 20 列表项 + 链接，无代码栅栏）。 */
const ASK_MATT = `---
name: ask-matt
description: Ask which skill or flow fits your situation. A router over the skills in this repo.
disable-model-invocation: true
---

# Ask Matt

You don't remember every skill, so ask.

Before stating what a skill does or recommending a step be skipped, read that skill's SKILL.md: the summaries here are for orientation only.

A **flow** is a path through the skills. Most paths run along one **main flow**, and two **on-ramps** merge onto it. Everything else is standalone, or a vocabulary layer that runs underneath.

## The main flow: idea → ship

The route most work travels. You have an idea and want it built.

1. **\`/grill-with-docs\`** sharpens the idea by interview. Start here whenever you are **working in a working directory**: it's stateful, retaining what it learns in \`GLOSSARY.md\` and ADRs.
2. **Branch: can you settle every question in conversation?** If a question needs a runnable answer (state, business logic, a UI you have to see), detour through a prototype, bridged by **\`/handoff\`** in both directions:
   - **\`/handoff\`** out, then open a fresh session against that file,
   - **\`/prototype\`** to answer the question with throwaway code,
   - **\`/handoff\`** back what you learned, and reference it from the original idea thread.
3. **\`/retro\`** closes the loop. After a build it looks back over the session and suggests changes to the agent's **environment**, not the code.

### Context hygiene

Keep steps 1–3 in **one unbroken context window** so the grilling, spec, and tickets all build on the same thinking.

The limit on this is the **[smart zone](https://www.aihero.dev/ai-coding-dictionary/smart-zone)**: the window (~150k tokens on state-of-the-art models) within which the model still reasons sharply.

## On-ramps

A starting situation that generates work, then merges onto the main flow.

- **Bugs and requests piling up** → **\`/triage\`**. It moves issues through triage roles and produces agent-ready issues, which **\`/implement\`** later picks up.
- **Something's broken** → **\`/diagnosing-bugs\`**. For the hard ones: the bug that resists a first glance, the intermittent flake, the regression that creeps in between two known-good states.

## Standalone

Off the main flow entirely.

- **\`/teach\`**: learn a concept over multiple sessions, using the current directory as a stateful workspace.
- **\`/writing-for-agents\`** is the reference for writing documents agents consume: skills, AGENTS.md, pointed-at docs.

## Precondition

**\`/setup-matt-pocock-skills\`**: run before your first engineering flow to configure the issue tracker, triage labels, and doc layout the other skills assume.
`;

/** ~/.zcode/skills/agent-desktop/SKILL.md 的前 46 行（frontmatter + 标题 +
 * 代码栅栏 + 列表；栅栏内是真实命令行）。 */
const AGENT_DESKTOP = `---
name: agent-desktop
tags: computer-use, desktop-automation, accessibility, ai-agent, gui-automation, cli
requirements:
  - agent-desktop
description: >
  Operate desktop applications on macOS, Windows and Linux through native accessibility trees,
  using the agent-desktop CLI. Use when an AI agent must see or act on a desktop GUI.
---

# agent-desktop

agent-desktop is a CLI that reads and operates desktop apps through the OS accessibility tree. Every command prints one JSON envelope (\`ok\`, \`data\` or \`error\`). It is a tool, not an agent: the observe, act and verify loop runs in you.

## Shell syntax

Examples use a POSIX shell. In PowerShell, quote every ref (\`'@s8f3k2p9:e1'\`), because a bare \`@token\` is the splatting operator.

## First run

1. Run \`agent-desktop status\`. It reports the platform, permissions and \`supported_surfaces\`.
2. Run \`agent-desktop skills get platform\`. It loads the skill for the OS the binary runs on (macOS, Windows or Linux).
3. For flags and syntax, run \`agent-desktop <command> --help\`. This skill does not repeat flag lists.

## The loop

Use \`find\` when you know the role or exact name. Otherwise map the app first and drill into one region.

\`\`\`
agent-desktop snapshot --skeleton --app "Notes" -i --compact   # shallow map, children_count on cut branches
agent-desktop snapshot --root @s8f3k2p9:e3 -i --compact        # drill into one region
agent-desktop click @s8f3k2p9:e12                              # act on a qualified ref
agent-desktop snapshot --root @s8f3k2p9:e3 -i --compact        # verify: re-read the same region
\`\`\`

- A small app needs no skeleton. Take a full \`snapshot -i\`.
- \`-i\` keeps interactive nodes only, so a row's static-text label is dropped. To tell which row a repeated button belongs to, drill that row's container with \`--root\` and leave out \`-i\`.
- Never skeleton a surface (\`--surface menu\`, \`sheet\`, \`alert\`). A surface is already focused.
`;

/** ~/.zcode/skills/cua-driver/SKILL.md 的表格节（29 表格行的 Goal/Tool/
 * Read-when 表 + 该文件自己的 frontmatter 首两键）。 */
const CUA_TABLE = `---
name: cua-driver
description: Drive a native GUI app (macOS, Windows, Linux) via the cua-driver CLI (default) or MCP server; snapshot its accessibility tree, act through snapshot-bound element tokens.
---

# cua-driver

## Task routing

| Goal | Tool or command | Read when needed |
| ---- | -------------- | ---------------- |
| Check installation and capabilities | \`cua-driver --version\`, \`status\`, \`doctor\`, \`describe <tool>\`; MCP \`tools/list\` | [Runtime](RUNTIME.md) |
| Install, update, or grant macOS permissions | \`update --apply\`, \`permissions status\`; MDM, authorized VNC approval, or ask the user | [macOS](MACOS.md#install-and-approve-permissions) |
| Find or open the requested app | \`list_apps\`, \`list_windows\`, \`launch_app\` | Current platform guide below |
| View a local file in the GUI | \`launch_app({bundle_id, urls:["file:///…"]})\` opens it in that app | Current platform guide below |
| Observe one window | \`get_window_state({pid, window_id, query})\` | [Workflow](WORKFLOW.md) |
| Act on a control | \`click\` / \`type_text\` with a fresh \`element_token\` and exact window target | [Workflow](WORKFLOW.md) |

Press **Escape** on a headed connection to abort the current act.
`;

/** SkillStar 自己安装的技能 ~/.skillstar/data/skills/installed/
 * codebase-memory/SKILL.md 的前 40 行（frontmatter + 决策表 + 代码式命
 * 令清单 + 列表）。 */
const CBM = `---
name: codebase-memory
description: "Use the codebase knowledge graph for structural code queries. Triggers on: explore the codebase, understand the architecture, what functions exist."
---

# Codebase Memory — Knowledge Graph Tools

Graph tools return precise structural results in ~500 tokens vs ~80K for grep.

## Quick Decision Matrix

| Question | Tool call |
|----------|----------|
| Who calls X? | \`trace_path(direction="inbound")\` |
| What does X call? | \`trace_path(direction="outbound")\` |
| Full call context | \`trace_path(direction="both")\` |
| Find by name pattern | \`search_graph(name_pattern="...")\` |
| Dead code | \`search_graph(max_degree=0, exclude_entry_points=true)\` |
| Cross-service edges | \`query_graph\` with Cypher |

## Exploration Workflow
1. \`list_projects\` — pick the project whose \`root_path\` contains your working directory
2. \`get_graph_schema\` — understand node/edge types
3. \`search_graph(label="Function", name_pattern=".*Pattern.*")\` — find code

## Evidence Tiers
- **Scout (Tier 1):** fast positive lookup with few graph calls and targeted source checks. Treat results as provisional.
- **Verify (Tier 2, default):** task-directed searches, relevant trace directions, exact snippets for material claims.
`;

/** 中文文案样本（GPUI chinese_prose 测试的形态）：prose 全中文 → 不提供
 * 翻译按钮与样式按钮。 */
const ZH_ONLY = `---
name: zh-demo
description: 中文示例技能。
---

# 中文技能

这一段全是中文，目标语言不需要翻译。

- 第一条要点。
- 第二条要点。

## 第二节

另一段中文说明。
`;


export interface ReaderSample {
  id: string;
  name: string;
  markdown: string;
}

export const READER_SAMPLES: Record<string, ReaderSample> = {
  askMatt: { id: "askMatt", name: "ask-matt", markdown: ASK_MATT },
  agentDesktop: { id: "agentDesktop", name: "agent-desktop", markdown: AGENT_DESKTOP },
  cuaTable: { id: "cuaTable", name: "cua-driver", markdown: CUA_TABLE },
  cbm: { id: "cbm", name: "codebase-memory", markdown: CBM },
  zhOnly: { id: "zhOnly", name: "zh-demo", markdown: ZH_ONLY },
};

export type ReaderSampleId = keyof typeof READER_SAMPLES;

/** splitMarkdown 的 prose 块数（译文行计数期望）。 */
export function proseCount(markdown: string): number {
  return splitMarkdown(markdown).filter((block) => block.kind === "prose").length;
}

/** 第 index 块若是 prose，其预填译文行文本（装置与断言同一公式）。 */
export function prefillLine(markdown: string, index: number): string {
  const block = splitMarkdown(markdown)[index];
  return block === undefined || block.kind !== "prose"
    ? ""
    : `〔译${index}〕${block.markdown.slice(0, 12)}`;
}
