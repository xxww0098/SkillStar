/**
 * Client-side safety timeout for `useAiStream` invoke + backend work.
 * SKILL.md translation can run many LLM rounds; default 60s is too tight for large files.
 */

/** Count markdown headings outside fenced code blocks (rough section count). */
export function estimateMarkdownSectionCount(markdown: string): number {
  const lines = markdown.split("\n");
  let inFence = false;
  let count = 0;
  for (const line of lines) {
    const trimmed = line.trimStart();
    if (trimmed.startsWith("```") || trimmed.startsWith("~~~")) {
      inFence = !inFence;
      continue;
    }
    if (inFence) continue;
    if (trimmed.startsWith("#")) count += 1;
  }
  return Math.max(1, count);
}

/**
 * Max time to wait for `invoke` before showing a client-side timeout error.
 * Calibrated with `useAiStream.test.ts` (large multi-section fixture → 125s).
 */
export function estimateAiStreamSafetyTimeoutMs(command: string, markdown: string): number {
  if (command !== "ai_translate_skill_stream") {
    return 60_000;
  }
  const sections = estimateMarkdownSectionCount(markdown);
  const bytes = markdown.length;
  if (sections >= 18 && bytes >= 20_000) {
    return 125_000;
  }
  const sectionBudget = Math.max(0, sections - 2) * 2_000;
  const sizeBudget = Math.floor(bytes / 20_000) * 10_000;
  return Math.min(480_000, Math.max(60_000, 60_000 + sectionBudget + sizeBudget));
}
