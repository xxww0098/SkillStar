const fs = require("fs");
const https = require("https");
const path = require("path");

const FALLBACK_PUBLISHERS = [
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
  "getsentry",
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
  "apify",
  "apollographql",
  "auth0",
  "automattic",
  "axiomhq",
  "base",
];

const RESERVED_NAMES = new Set(["official", "audits", "docs"]);
const OFFICIAL_URL = "https://skills.sh/official";
const dest = path.join(__dirname, "../public/publishers");

const refreshAll = process.argv.includes("--refresh");
const maxAgeDays = (() => {
  const arg = process.argv.find((v) => v.startsWith("--max-age-days="));
  if (!arg) return 30;
  const value = Number(arg.split("=")[1]);
  if (!Number.isFinite(value) || value <= 0) return 30;
  return value;
})();
const maxAgeMs = maxAgeDays * 24 * 60 * 60 * 1000;

function fetchText(url) {
  return new Promise((resolve, reject) => {
    const req = https.get(
      url,
      {
        headers: {
          "User-Agent": "AgentHub Avatar Sync Script",
          Accept: "text/html,application/xhtml+xml",
        },
      },
      (res) => {
        if (res.statusCode === 301 || res.statusCode === 302) {
          const next = res.headers.location;
          if (!next) {
            reject(new Error(`Redirect without location: ${url}`));
            return;
          }
          fetchText(next).then(resolve).catch(reject);
          return;
        }

        if (res.statusCode !== 200) {
          reject(new Error(`Unexpected status ${res.statusCode} for ${url}`));
          return;
        }

        let body = "";
        res.setEncoding("utf8");
        res.on("data", (chunk) => {
          body += chunk;
        });
        res.on("end", () => resolve(body));
      },
    );

    req.on("error", reject);
  });
}

function discoverPublishersFromOfficial(html) {
  const found = new Set();
  const normalized = html.replace(/\n/g, "");

  // Current skills.sh official table row format.
  const rowRe = /href="\/([a-z0-9_-]+)"[^>]*><div class="min-w-0 flex items-center gap-3">/g;

  for (const match of normalized.matchAll(rowRe)) {
    const name = match[1];
    if (name && !RESERVED_NAMES.has(name)) {
      found.add(name);
    }
  }

  return [...found];
}

function isFileFresh(filePath) {
  if (refreshAll) return false;
  if (!fs.existsSync(filePath)) return false;

  try {
    const stat = fs.statSync(filePath);
    return Date.now() - stat.mtimeMs < maxAgeMs;
  } catch {
    return false;
  }
}

function downloadAvatar(name) {
  return new Promise((resolve) => {
    const url = `https://avatars.githubusercontent.com/${encodeURIComponent(name)}?size=120`;
    const filePath = path.join(dest, `${name}.png`);
    const tempPath = `${filePath}.tmp`;

    const req = https.get(
      url,
      {
        headers: {
          "User-Agent": "AgentHub Avatar Sync Script",
          Accept: "image/avif,image/webp,image/apng,image/*,*/*;q=0.8",
        },
      },
      (res) => {
        if (res.statusCode !== 200) {
          console.error(`Failed ${name}: HTTP ${res.statusCode}`);
          resolve(false);
          return;
        }

        const out = fs.createWriteStream(tempPath);
        res.pipe(out);

        out.on("finish", () => {
          out.close(() => {
            try {
              fs.renameSync(tempPath, filePath);
              console.log(`Downloaded ${name}`);
              resolve(true);
            } catch (e) {
              console.error(`Failed to save ${name}: ${e.message}`);
              resolve(false);
            }
          });
        });

        out.on("error", (e) => {
          console.error(`Stream error ${name}: ${e.message}`);
          try {
            if (fs.existsSync(tempPath)) fs.unlinkSync(tempPath);
          } catch {
            // noop
          }
          resolve(false);
        });
      },
    );

    req.on("error", (e) => {
      console.error(`Request error ${name}: ${e.message}`);
      resolve(false);
    });
  });
}

async function resolvePublishers() {
  try {
    const html = await fetchText(OFFICIAL_URL);
    const discovered = discoverPublishersFromOfficial(html);
    if (discovered.length > 0) {
      return discovered.sort();
    }
    console.warn("No publishers discovered from skills.sh/official, using fallback list.");
  } catch (e) {
    console.warn(`Failed to discover publishers from skills.sh: ${e.message}`);
  }
  return [...FALLBACK_PUBLISHERS].sort();
}

async function main() {
  if (!fs.existsSync(dest)) {
    fs.mkdirSync(dest, { recursive: true });
  }

  const publishers = await resolvePublishers();
  console.log(`Publisher count: ${publishers.length} | max-age: ${maxAgeDays} day(s) | refreshAll: ${refreshAll}`);

  let downloaded = 0;
  let skipped = 0;
  let failed = 0;

  for (const name of publishers) {
    const filePath = path.join(dest, `${name}.png`);
    if (isFileFresh(filePath)) {
      console.log(`Cache hit ${name}`);
      skipped += 1;
      continue;
    }

    const ok = await downloadAvatar(name);
    if (ok) downloaded += 1;
    else failed += 1;
  }

  console.log(`Done. Downloaded: ${downloaded}, Skipped (cache): ${skipped}, Failed: ${failed}`);
}

main().catch((e) => {
  console.error(e);
  process.exit(1);
});
