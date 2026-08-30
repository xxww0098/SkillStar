import { execFileSync } from "node:child_process";
import { existsSync, mkdirSync, rmSync, writeFileSync } from "node:fs";
import path from "node:path";
import { afterEach, describe, expect, it } from "vitest";

/**
 * Fixture test for `scripts/internal/check_ts_orphan_modules.sh`.
 *
 * A ratchet nobody has ever seen fail is indistinguishable from a ratchet that
 * cannot fail. This gate exists precisely because `bun run lint`, `bun run
 * build` and `bun run test` were all green while ~4000 unreachable lines sat in
 * `src/features/models/` — so "the suite is green" is not evidence about it.
 * The only convincing check is to plant a real orphan and watch it go red.
 *
 * The fixture is a genuine file under `src/features/` rather than a synthetic
 * tree: the script resolves its own repo root and roots its walk at
 * `src/main.tsx` + `src/pages/`, so exercising it anywhere else would be
 * exercising a different program.
 *
 * Two of the three cases cover the ways the original 4000 lines stayed
 * invisible — a module reachable only from another dead module, and a module
 * reachable only from its own co-located test.
 */

const ROOT = path.resolve(__dirname, "../..");
const SCRIPT = path.join(ROOT, "scripts/internal/check_ts_orphan_modules.sh");
const FIXTURE_DIR = path.join(ROOT, "src/features/models/__orphan_fixture__");
const PLANTED = path.join(FIXTURE_DIR, "plantedOrphan.ts");
const PLANTED_REL = path.relative(ROOT, PLANTED);
// The gate walks every frontend module and synchronously waits for a child
// process. Bound that child, then give Vitest enough extra time to observe its
// exit and run afterEach cleanup under a busy full-suite worker.
const GATE_PROCESS_TIMEOUT_MS = 15_000;
const GATE_TEST_TIMEOUT_MS = GATE_PROCESS_TIMEOUT_MS + 5_000;

type Run = {
  status: number;
  output: string;
  timedOut: boolean;
  signal: string | null;
};

function runGate(): Run {
  try {
    const output = execFileSync("bash", [SCRIPT], {
      cwd: ROOT,
      encoding: "utf8",
      timeout: GATE_PROCESS_TIMEOUT_MS,
    });
    return { status: 0, output, timedOut: false, signal: null };
  } catch (err) {
    const e = err as {
      status?: number;
      stdout?: string;
      stderr?: string;
      code?: string;
      signal?: string | null;
    };
    return {
      status: e.status ?? 1,
      output: `${e.stdout ?? ""}${e.stderr ?? ""}`,
      timedOut: e.code === "ETIMEDOUT",
      signal: e.signal ?? null,
    };
  }
}

function expectGateFinished(run: Run) {
  expect(run.timedOut).toBe(false);
  expect(run.signal).toBeNull();
}

function plant(relPath: string, contents: string): string {
  const file = path.join(FIXTURE_DIR, relPath);
  mkdirSync(path.dirname(file), { recursive: true });
  writeFileSync(file, contents);
  return path.relative(ROOT, file);
}

afterEach(() => {
  rmSync(FIXTURE_DIR, { recursive: true, force: true });
});

describe("check_ts_orphan_modules.sh", () => {
  it(
    "passes on the repository as it stands",
    () => {
      expect(existsSync(FIXTURE_DIR)).toBe(false);
      const result = runGate();
      expectGateFinished(result);
      expect(result.output).toContain("0 new orphan module(s)");
      expect(result.status).toBe(0);
    },
    GATE_TEST_TIMEOUT_MS,
  );

  it(
    "fails, and names the file, once an unreachable module is planted",
    () => {
      plant("plantedOrphan.ts", "export const plantedOrphan = 'nothing imports this';\n");

      const result = runGate();
      expectGateFinished(result);
      expect(result.status).toBe(1);
      expect(result.output).toContain(`FAIL  ${PLANTED_REL}`);
      expect(result.output).toContain("1 new orphan module(s)");
    },
    GATE_TEST_TIMEOUT_MS,
  );

  it(
    "still fails when the only importer is itself dead, or is a test",
    () => {
      plant("plantedOrphan.ts", "export const plantedOrphan = 'imported, but only by the dead';\n");
      // Transitive death: `CodexSettingsForm` survived review for exactly this
      // reason — it had an importer, and the importer was dead too.
      const deadImporter = plant(
        "deadImporter.ts",
        "import { plantedOrphan } from './plantedOrphan';\nexport const echo = plantedOrphan;\n",
      );
      // Test-only reachability: `ProviderGalleryCard` looked used because its
      // co-located test imported it. A test is not an entry point.
      plant(
        "__tests__/consumer.ts",
        "import { plantedOrphan } from '../plantedOrphan';\nexport default plantedOrphan;\n",
      );

      const result = runGate();
      expectGateFinished(result);
      expect(result.status).toBe(1);
      expect(result.output).toContain(`FAIL  ${PLANTED_REL}`);
      expect(result.output).toContain(`FAIL  ${deadImporter}`);
      // The test-side file is neither a root nor a reportable orphan.
      expect(result.output).not.toContain("__tests__/consumer.ts");
      expect(result.output).toContain("2 new orphan module(s)");
    },
    GATE_TEST_TIMEOUT_MS,
  );
});
