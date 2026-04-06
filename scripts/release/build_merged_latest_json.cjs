#!/usr/bin/env node

const fs = require("fs");
const path = require("path");

function parseArgs(argv) {
  const args = {};
  for (let i = 0; i < argv.length; i += 1) {
    const token = argv[i];
    if (!token.startsWith("--")) continue;
    const key = token.slice(2);
    const value = argv[i + 1];
    if (!value || value.startsWith("--")) {
      args[key] = "true";
      continue;
    }
    args[key] = value;
    i += 1;
  }
  return args;
}

function requiredArg(args, key) {
  const value = args[key];
  if (!value) throw new Error(`Missing required argument --${key}`);
  return value;
}

function normalizePubDate(raw) {
  const value = (raw || "").trim();
  if (!value || value === "true" || value === "null") {
    throw new Error(`Invalid --published-at value: "${raw}"`);
  }
  const ts = Date.parse(value);
  if (Number.isNaN(ts)) throw new Error(`Invalid --published-at: "${raw}"`);
  return new Date(ts).toISOString();
}

function readText(filePath) {
  return fs.readFileSync(filePath, "utf8").trim();
}

function buildUrl(repo, version, fileName) {
  return `https://github.com/${repo}/releases/download/v${version}/${encodeURIComponent(fileName)}`;
}

function findAsset(assets, pattern, label) {
  const hit = assets.find((name) => pattern.test(name));
  if (!hit) throw new Error(`Missing required asset for ${label}. Pattern: ${pattern}`);
  return hit;
}

function buildEntry(assetName, signatures, repo, version) {
  const signature = signatures.get(assetName);
  if (!signature) throw new Error(`Missing .sig for ${assetName}`);
  return { signature, url: buildUrl(repo, version, assetName) };
}

function clone(entry) {
  return { signature: entry.signature, url: entry.url };
}

function main() {
  const args = parseArgs(process.argv.slice(2));
  const version = requiredArg(args, "version");
  const repo = requiredArg(args, "repo");
  const assetsDir = requiredArg(args, "assets-dir");
  const notesFile = requiredArg(args, "notes-file");
  const publishedAt = normalizePubDate(requiredArg(args, "published-at"));
  const output = args.output || "latest.json";

  const files = fs.readdirSync(assetsDir).filter((f) => fs.statSync(path.join(assetsDir, f)).isFile());

  const signatures = new Map();
  for (const name of files) {
    if (!name.endsWith(".sig")) continue;
    signatures.set(name.slice(0, -4), readText(path.join(assetsDir, name)));
  }

  const assets = files.filter((n) => !n.endsWith(".sig") && n !== "latest.json" && n !== "SHA256SUMS.txt");

  // Detect asset patterns — Tauri v2 generates these by default:
  // macOS:   {Product}_{version}_{arch}.app.tar.gz
  // Windows: {Product}_{version}_{arch}-setup.exe  (NSIS)
  //          {Product}_{version}_{arch}.msi         (MSI)
  // Linux:   {product}_{version}_{arch}.AppImage
  //          {product}_{version}_{arch}.deb
  //          {product}-{version}-1.{arch}.rpm
  const darwinAarch64 = findAsset(assets, /_aarch64\.app\.tar\.gz$/, "darwin-aarch64");
  const darwinX64 = findAsset(assets, /_x64\.app\.tar\.gz$/, "darwin-x86_64");
  const linuxAppImage = findAsset(assets, /_amd64\.AppImage$/, "linux-x86_64-appimage");
  const linuxDeb = findAsset(assets, /_amd64\.deb$/, "linux-x86_64-deb");
  const linuxRpm = findAsset(assets, /-1\.x86_64\.rpm$/, "linux-x86_64-rpm");

  // Windows — try NSIS first, then MSI
  let windowsAsset;
  try {
    windowsAsset = findAsset(assets, /_x64-setup\.exe$/, "windows-x86_64-nsis");
  } catch {
    windowsAsset = findAsset(assets, /_x64\.msi$/, "windows-x86_64-msi");
  }

  const darwinAarch64Entry = buildEntry(darwinAarch64, signatures, repo, version);
  const darwinX64Entry = buildEntry(darwinX64, signatures, repo, version);
  const windowsEntry = buildEntry(windowsAsset, signatures, repo, version);
  const linuxAppImageEntry = buildEntry(linuxAppImage, signatures, repo, version);
  const linuxDebEntry = buildEntry(linuxDeb, signatures, repo, version);
  const linuxRpmEntry = buildEntry(linuxRpm, signatures, repo, version);

  const latest = {
    version,
    notes: readText(notesFile),
    pub_date: publishedAt,
    platforms: {
      "darwin-aarch64": darwinAarch64Entry,
      "darwin-x86_64": darwinX64Entry,
      "windows-x86_64": clone(windowsEntry),
      "windows-x86_64-nsis": clone(windowsEntry),
      "windows-x86_64-msi": clone(windowsEntry),
      "linux-x86_64": clone(linuxAppImageEntry),
      "linux-x86_64-appimage": clone(linuxAppImageEntry),
      "linux-x86_64-deb": linuxDebEntry,
      "linux-x86_64-rpm": linuxRpmEntry,
    },
  };

  fs.writeFileSync(output, `${JSON.stringify(latest, null, 2)}\n`);
  console.log(`latest.json generated at ${output} (${Object.keys(latest.platforms).length} platforms)`);
}

try {
  main();
} catch (error) {
  console.error(`[build_merged_latest_json] ${error.message}`);
  process.exit(1);
}
