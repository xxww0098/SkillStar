#!/usr/bin/env node
/**
 * verify-readme-versions.cjs
 *
 * Reads package.json and README.md, extracts version information,
 * and verifies they are in sync. Exits 0 if all match, 1 otherwise.
 *
 * Checks:
 *  - App version (package.json "version" vs README version badge)
 *  - React version (package.json dependencies.react vs README React badge)
 *  - Vite version (package.json devDependencies.vite vs README tech table)
 */

"use strict";

const fs = require("node:fs");
const path = require("node:path");

const PKG_JSON_PATH = path.join(__dirname, "..", "package.json");
const README_PATH = path.join(__dirname, "..", "README.md");

// ─── helpers ────────────────────────────────────────────────────────────────

/** Extract major.minor.patch from a semver string like "^19.2.4", "19.2.4", "~1.0.0" */
function cleanSemver(raw) {
  return raw.replace(/^[\^~>=<]+/, "").trim();
}

/** Extract major version from semver string */
function majorVersion(raw) {
  const cleaned = cleanSemver(raw);
  const parts = cleaned.split(".");
  return parts[0] ? parts[0] : "";
}

/** Read file, throw with context on failure */
function readFile(filePath) {
  if (!fs.existsSync(filePath)) {
    throw new Error(`File not found: ${filePath}`);
  }
  return fs.readFileSync(filePath, "utf8");
}

// ─── extractors ─────────────────────────────────────────────────────────────

function extractFromPkg(pkgJsonContent) {
  const pkg = JSON.parse(pkgJsonContent);

  const appVersion = cleanSemver(pkg.version || "");

  // React: check dependencies first, then peerDependencies
  const reactRaw = pkg.dependencies?.react || pkg.peerDependencies?.react || "";
  const reactVersion = cleanSemver(reactRaw);

  // Vite: check devDependencies first (standard), then dependencies
  const viteRaw = pkg.devDependencies?.vite || pkg.dependencies?.vite || "";
  const viteVersion = cleanSemver(viteRaw);

  return { appVersion, reactVersion, viteVersion };
}

function extractFromReadme(readmeContent) {
  // App version badge: [![Version](https://img.shields.io/badge/version-0.2.2-blueviolet)]
  const appVersionMatch = readmeContent.match(
    /!\[[^\]]*\]\(https:\/\/img\.shields\.io\/badge\/version-([0-9]+\.[0-9]+\.[0-9]+)[^)]*\)/i,
  );
  const appVersion = appVersionMatch ? appVersionMatch[1] : "";

  // React badge: [![React 18](https://img.shields.io/badge/React-18-61dafb...)]
  // Badge label format: React-18 or React-19 or React-19.2.4
  const reactBadgeMatch = readmeContent.match(
    /!\[[^\]]*\]\(https:\/\/img\.shields\.io\/badge\/React-(\d+(?:\.\d+)?(?:\.\d+)?)[^)]*\)/i,
  );
  const reactBadge = reactBadgeMatch ? reactBadgeMatch[1] : "";

  // Vite in tech table: | Frontend | React 19 + TypeScript + Vite 8 | SPA UI |
  const viteTableMatch = readmeContent.match(
    /\|\s*Frontend\s*\|\s*[^|]*TypeScript\s*\+\s*Vite\s*(\d+(?:\.\d+)?(?:\.\d+)?)[^|]*\|/i,
  );
  const viteTable = viteTableMatch ? viteTableMatch[1] : "";

  return { appVersion, reactBadge, viteTable };
}

// ─── main ────────────────────────────────────────────────────────────────────

function main() {
  let exitCode = 0;

  console.log("=== README Version Drift Detector ===\n");

  // 1. Read files
  let pkgJsonContent;
  let readmeContent;
  try {
    pkgJsonContent = readFile(PKG_JSON_PATH);
    readmeContent = readFile(README_PATH);
  } catch (err) {
    console.error(`ERROR: ${err.message}`);
    process.exit(1);
  }

  // 2. Extract
  const pkg = extractFromPkg(pkgJsonContent);
  const readme = extractFromReadme(readmeContent);

  console.log("package.json:");
  console.log(`  appVersion : ${pkg.appVersion || "(missing)"}`);
  console.log(`  reactVersion: ${pkg.reactVersion || "(missing)"}`);
  console.log(`  viteVersion : ${pkg.viteVersion || "(missing)"}`);
  console.log("\nREADME.md badges/table:");
  console.log(`  appVersion  : ${readme.appVersion || "(missing)"}`);
  console.log(`  reactBadge  : ${readme.reactBadge || "(missing)"}`);
  console.log(`  viteTable   : ${readme.viteTable || "(missing)"}`);
  console.log("");

  // 3. Compare

  // App version: compare full version (major.minor.patch)
  if (pkg.appVersion !== readme.appVersion) {
    console.log(`MISMATCH [app version]: package.json=${pkg.appVersion}, README=${readme.appVersion}`);
    exitCode = 1;
  } else {
    console.log(`OK [app version]: ${pkg.appVersion}`);
  }

  // React: compare major version only (README badge uses "18" or "19", not full semver)
  if (majorVersion(pkg.reactVersion) !== readme.reactBadge) {
    console.log(
      `MISMATCH [react version]: package.json major=${majorVersion(
        pkg.reactVersion,
      )} (${pkg.reactVersion}), README badge=${readme.reactBadge}`,
    );
    exitCode = 1;
  } else {
    console.log(`OK [react version]: ${readme.reactBadge} (${pkg.reactVersion})`);
  }

  // Vite: compare major version only (README table uses "5" or "8", not full semver)
  if (majorVersion(pkg.viteVersion) !== readme.viteTable) {
    console.log(
      `MISMATCH [vite version]: package.json major=${majorVersion(
        pkg.viteVersion,
      )} (${pkg.viteVersion}), README table=${readme.viteTable}`,
    );
    exitCode = 1;
  } else {
    console.log(`OK [vite version]: ${readme.viteTable} (${pkg.viteVersion})`);
  }

  console.log("");

  if (exitCode === 0) {
    console.log("All versions in sync.");
  } else {
    console.log("VERSION DRIFT DETECTED — fix README.md or package.json");
  }

  process.exit(exitCode);
}

main();
