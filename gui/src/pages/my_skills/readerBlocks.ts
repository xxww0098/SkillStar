// SKILL.md 分块器 — crates/ss-gpui/src/my_skills/skill_reader.rs 的
// peel_frontmatter（:527）与 split_markdown（:542）逐行对位（波 3 spike
// 的 spike 实现（2026-11 已删除）验证过与旧壳分块形状一致，本文件
// 是它的正式落位）。
//
// 翻译模式按块挂 TextView：代码栅栏（含 frontmatter 的 yaml 栅栏）是
// Code 块，空行分隔的连续行合并为 Prose 块（列表因此保持一块）。

export type MdBlock =
  | { kind: "code"; markdown: string }
  | { kind: "prose"; markdown: string };

/** skill_reader.rs peel_frontmatter：`---\n…\n---` 变 ```yaml 栅栏，
 * 其余原样（\r\n 归一）。 */
export function peelFrontmatter(text: string): [string | null, string] {
  const normalized = text.replace(/\r\n/g, "\n");
  if (normalized.startsWith("---\n")) {
    const after = normalized.slice("---\n".length);
    const end = after.indexOf("\n---");
    if (end !== -1) {
      // Rust trim_end()：去掉全部尾随空白（含 U+3000）。
      const yaml = after.slice(0, end).replace(/[\s\u3000]+$/, "");
      let body = after.slice(end + "\n---".length);
      if (body.startsWith("\n")) body = body.slice(1);
      return ["```yaml\n" + yaml + "\n```", body];
    }
  }
  return [null, normalized];
}

/** skill_reader.rs split_markdown：代码块（保留栅栏行）与 prose 段
 * （空行分隔；连续行以 \n 连接）。frontmatter 是第一个 Code 块。 */
export function splitMarkdown(text: string): MdBlock[] {
  const [front, body] = peelFrontmatter(text);
  const blocks: MdBlock[] = [];
  if (front !== null) blocks.push({ kind: "code", markdown: front });
  let prose = "";
  let code = "";
  let inCode = false;
  const flushProse = () => {
    const trimmed = prose.trim();
    if (trimmed.length > 0) blocks.push({ kind: "prose", markdown: trimmed });
    prose = "";
  };
  for (const line of body.split("\n")) {
    if (inCode) {
      code += line;
      code += "\n";
      if (line.trimStart().startsWith("```")) {
        inCode = false;
        blocks.push({ kind: "code", markdown: code.replace(/[\s\u3000]+$/, "") });
        code = "";
      }
      continue;
    }
    if (line.trimStart().startsWith("```")) {
      flushProse();
      inCode = true;
      code += line;
      code += "\n";
      continue;
    }
    if (line.trim().length === 0) {
      flushProse();
      continue;
    }
    if (prose.length > 0) prose += "\n";
    prose += line;
  }
  if (inCode) {
    blocks.push({ kind: "code", markdown: code.replace(/[\s\u3000]+$/, "") });
  } else {
    flushProse();
  }
  return blocks;
}
