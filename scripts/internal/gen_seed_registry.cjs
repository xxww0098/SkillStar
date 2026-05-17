#!/usr/bin/env node
/**
 * Generate seed skill registry from skills.sh API.
 *
 * Usage:
 *   node scripts/internal/gen_seed_registry.cjs [--output path]
 *
 * Fetches popular skills from each known publisher via the skills.sh search API
 * and writes a JSON seed file for bundling with the app.
 *
 * Rate limit: skills.sh allows 30 req/min. The script respects this by sleeping
 * 2.5s between requests and retrying on 429 responses.
 *
 * If the existing seed file is less than 24h old, the script skips regeneration
 * unless --force is passed.
 */

const https = require("node:https");
const fs = require("node:fs");
const path = require("node:path");

const SKILLS_SH_API = "https://skills.sh/api/search";
const DEFAULT_OUTPUT = path.join(__dirname, "../../src-tauri/resources/seed_registry.json");
const USER_AGENT = "SkillStar-RegistryGen/1.0";

// Rate limit: 30 req/min → 2s between requests (with margin)
const REQUEST_DELAY_MS = 2500;
const MAX_RETRIES = 2;
const RETRY_DELAY_MS = 10000;
// Skip regeneration if file is less than this many hours old
const FRESHNESS_HOURS = 24;

// Known publishers — same list as known_official_publishers() in remote.rs
const PUBLISHERS = [
  "vercel-labs",
  "microsoft",
  "anthropics",
  "google-labs-code",
  "github",
  "cloudflare",
  "expo",
  "firebase",
  "openai",
  "supabase",
  "langchain-ai",
  "hashicorp",
  "stripe",
  "posthog",
  "prisma",
  "figma",
  "firecrawl",
  "flutter",
  "vercel",
  "shadcn",
  "google-gemini",
  "huggingface",
  "remotion-dev",
  "tavily-ai",
  "browser-use",
  "facebook",
  "better-auth",
  "resend",
  "sentry",
  "neondatabase",
  "dagster-io",
  "datadog-labs",
  "bitwarden",
  "upstash",
  "sanity-io",
  "pulumi",
  "mapbox",
  "semgrep",
  "clerk",
];

// Broad categories to catch popular community skills
const BROAD_QUERIES = [
  "react",
  "nextjs",
  "python",
  "typescript",
  "rust",
  "go",
  "docker",
  "kubernetes",
  "aws",
  "testing",
  "security",
  "database",
  "api",
  "frontend",
  "backend",
  "devops",
  "ai",
  "coding",
  "git",
  "debug",
];

function fetch(url) {
  return new Promise((resolve, reject) => {
    const req = https.get(url, { headers: { "User-Agent": USER_AGENT, Accept: "application/json" } }, (res) => {
      let data = "";
      res.on("data", (chunk) => (data += chunk));
      res.on("end", () => {
        if (res.statusCode === 429) {
          reject(Object.assign(new Error("rate_limited"), { statusCode: 429 }));
        } else if (res.statusCode >= 400) {
          reject(new Error(`HTTP ${res.statusCode}: ${data.slice(0, 200)}`));
        } else {
          resolve(data);
        }
      });
    });
    req.on("error", reject);
    req.setTimeout(15000, () => {
      req.destroy();
      reject(new Error("Timeout"));
    });
  });
}

function sleep(ms) {
  return new Promise((r) => setTimeout(r, ms));
}

async function searchSkills(query, limit = 100) {
  const url = `${SKILLS_SH_API}?q=${encodeURIComponent(query)}&limit=${limit}`;

  for (let attempt = 0; attempt <= MAX_RETRIES; attempt++) {
    try {
      const data = await fetch(url);
      const parsed = JSON.parse(data);
      return (parsed.skills || []).map((s) => ({
        name: s.name,
        source: s.source,
        git_url: s.repoUrl || `https://github.com/${s.source}`,
        description: s.description || "",
        installs: s.installs || 0,
      }));
    } catch (err) {
      if (err.statusCode === 429 && attempt < MAX_RETRIES) {
        const wait = RETRY_DELAY_MS * (attempt + 1);
        process.stdout.write(` [429, retry in ${wait / 1000}s]`);
        await sleep(wait);
        continue;
      }
      console.error(`  ⚠ Search failed for "${query}": ${err.message}`);
      return [];
    }
  }
  return [];
}

function isFileFreshEnough(filePath) {
  try {
    const stat = fs.statSync(filePath);
    const ageMs = Date.now() - stat.mtimeMs;
    const ageHours = ageMs / (1000 * 60 * 60);
    return ageHours < FRESHNESS_HOURS;
  } catch {
    return false; // File doesn't exist
  }
}

async function main() {
  const args = process.argv.slice(2);
  const outputIdx = args.indexOf("--output");
  const output = outputIdx >= 0 ? args[outputIdx + 1] : DEFAULT_OUTPUT;
  const force = args.includes("--force");

  // Skip if the file is fresh enough (unless --force)
  if (!force && isFileFreshEnough(output)) {
    const stat = fs.statSync(output);
    const ageH = ((Date.now() - stat.mtimeMs) / (1000 * 60 * 60)).toFixed(1);
    console.log(`✓ Seed registry is fresh (${ageH}h old, threshold ${FRESHNESS_HOURS}h). Skipping.`);
    console.log(`  Use --force to regenerate anyway.`);
    return;
  }

  console.log("Generating seed registry...");

  const allSkills = new Map(); // key: `${source}/${name}` → skill

  // 1. Search by publisher name
  for (const pub of PUBLISHERS) {
    process.stdout.write(`  Fetching ${pub}...`);
    const skills = await searchSkills(pub, 100);
    let added = 0;
    for (const s of skills) {
      const key = `${s.source}/${s.name}`;
      if (!allSkills.has(key) || allSkills.get(key).installs < s.installs) {
        allSkills.set(key, s);
        added++;
      }
    }
    console.log(` ${skills.length} results, ${added} new`);
    await sleep(REQUEST_DELAY_MS);
  }

  // 2. Search broad categories
  for (const query of BROAD_QUERIES) {
    process.stdout.write(`  Searching "${query}"...`);
    const skills = await searchSkills(query, 100);
    let added = 0;
    for (const s of skills) {
      const key = `${s.source}/${s.name}`;
      if (!allSkills.has(key) || allSkills.get(key).installs < s.installs) {
        allSkills.set(key, s);
        added++;
      }
    }
    console.log(` ${skills.length} results, ${added} new`);
    await sleep(REQUEST_DELAY_MS);
  }

  // 3. Sort by installs descending
  const skills = [...allSkills.values()].sort((a, b) => b.installs - a.installs);

  // 4. Sanity check — don't overwrite a good file with empty results
  if (skills.length < 100) {
    console.error(`\n⚠ Only fetched ${skills.length} skills (expected 500+). Keeping existing file.`);
    if (fs.existsSync(output)) {
      console.error(`  Existing file preserved: ${output}`);
      return;
    }
    // No existing file at all — write what we have
    console.error(`  No existing file found. Writing partial results.`);
  }

  const registry = {
    version: 1,
    generated_at: new Date().toISOString(),
    source: "skills.sh",
    skill_count: skills.length,
    skills,
  };

  // Ensure output directory exists
  fs.mkdirSync(path.dirname(output), { recursive: true });
  fs.writeFileSync(output, JSON.stringify(registry, null, 2));

  console.log(`\n✓ Generated ${skills.length} skills → ${output}`);
  console.log(`  File size: ${(fs.statSync(output).size / 1024).toFixed(1)} KB`);
}

main().catch(console.error);
