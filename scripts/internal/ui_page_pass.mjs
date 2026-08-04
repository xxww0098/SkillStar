/**
 * Capture desktop + mobile screenshots of every SkillStar nav surface for UI polish passes.
 * Usage: node scripts/internal/ui_page_pass.mjs [outDir]
 */
import { chromium } from "playwright";
import { mkdirSync } from "node:fs";
import { join } from "node:path";

const BASE = process.env.UI_PASS_BASE ?? "http://127.0.0.1:5173";
const outDir = process.argv[2] ?? "/tmp/skillstar-ui-pass";
mkdirSync(outDir, { recursive: true });

const pages = [
  { id: "skills", hash: "#skills" },
  { id: "marketplace", hash: "#marketplace" },
  { id: "cards", hash: "#cards" },
  { id: "projects", hash: "#projects" },
  { id: "mcp", hash: "#mcp" },
  { id: "settings", hash: "#settings" },
  { id: "models", hash: "#models" },
  { id: "usage", hash: "#usage" },
];

const viewports = [
  { name: "desktop", width: 1440, height: 900 },
  { name: "mobile", width: 390, height: 844 },
];

const browser = await chromium.launch({ headless: true });
const context = await browser.newContext({ deviceScaleFactor: 1 });
const page = await context.newPage();

for (const vp of viewports) {
  await page.setViewportSize({ width: vp.width, height: vp.height });
  for (const p of pages) {
    const url = `${BASE}/${p.hash}`;
    await page.goto(url, { waitUntil: "networkidle", timeout: 30000 }).catch(() => {});
    await page.waitForTimeout(800);
    const file = join(outDir, `${p.id}-${vp.name}.png`);
    await page.screenshot({ path: file, fullPage: false });
    console.log(`ok ${file}`);
  }
}

await browser.close();
console.log(`done → ${outDir}`);
