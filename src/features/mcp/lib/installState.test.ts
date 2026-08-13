import { describe, expect, it } from "vitest";
import type { McpMarketEntry, McpServerEntry } from "../../../types";
import {
  buildInstalledIndex,
  compareMcpVersions,
  installedHasUpdate,
  latestVersionForInstalled,
  resolveMcpEntryStatus,
} from "./installState";

function entry(patch: Partial<McpMarketEntry> = {}): McpMarketEntry {
  return {
    id: "row-1",
    name: "server-filesystem",
    namespace: "io.github.modelcontextprotocol/server-filesystem",
    description: "",
    repoUrl: "",
    stars: 0,
    license: null,
    version: "1.2.0",
    kind: "stdio",
    runtimes: ["npx"],
    updatedAt: null,
    recommended: false,
    source: null,
    title: null,
    websiteUrl: null,
    iconUrl: null,
    status: "active",
    isLatest: true,
    registrySource: "official",
    ...patch,
  };
}

function installed(patch: Partial<McpServerEntry> = {}): McpServerEntry {
  return {
    id: "srv-1",
    name: "server-filesystem",
    transport: "stdio",
    enabled: {},
    autoApproveAll: false,
    sortIndex: 0,
    ...patch,
  };
}

describe("compareMcpVersions", () => {
  it("orders release versions numerically, not lexically", () => {
    expect(compareMcpVersions("1.10.0", "1.9.0")).toBeGreaterThan(0);
    expect(compareMcpVersions("1.2.0", "1.2.0")).toBe(0);
    expect(compareMcpVersions("1.2", "1.2.0")).toBe(0);
  });

  it("tolerates a leading v", () => {
    expect(compareMcpVersions("v2.0.0", "1.9.9")).toBeGreaterThan(0);
  });

  it("sorts a prerelease before its own release", () => {
    expect(compareMcpVersions("1.0.0-rc1", "1.0.0")).toBeLessThan(0);
    expect(compareMcpVersions("1.0.0", "1.0.0-rc1")).toBeGreaterThan(0);
  });

  it("returns null rather than guessing at non-numeric versions", () => {
    expect(compareMcpVersions("0.0.1a4", "0.0.1a3")).toBeNull();
    expect(compareMcpVersions("latest", "1.0.0")).toBeNull();
    expect(compareMcpVersions("", "1.0.0")).toBeNull();
  });
});

describe("resolveMcpEntryStatus", () => {
  it("reports not-installed against an empty store", () => {
    const status = resolveMcpEntryStatus(entry(), buildInstalledIndex([]));
    expect(status).toMatchObject({ state: "notInstalled", installed: null, matchedByName: false });
  });

  it("matches by registry fingerprint, not by config key", () => {
    const index = buildInstalledIndex([
      installed({ name: "renamed-by-user", registryName: "io.github.modelcontextprotocol/server-filesystem" }),
    ]);
    const status = resolveMcpEntryStatus(entry(), index);

    expect(status.state).toBe("installed");
    expect(status.matchedByName).toBe(false);
    expect(status.installed?.name).toBe("renamed-by-user");
  });

  it("does not confuse two servers that sanitize to the same config key", () => {
    const index = buildInstalledIndex([installed({ name: "github", registryName: "io.github.other/github-mcp" })]);
    const status = resolveMcpEntryStatus(entry({ name: "github", namespace: "io.github.github/github-mcp" }), index);

    // The names collide, the fingerprints do not — so the name fallback answers,
    // and it is flagged as a name match rather than presented as identity.
    expect(status.state).toBe("installed");
    expect(status.matchedByName).toBe(true);
  });

  it("reports an update when the catalog version is strictly newer", () => {
    const index = buildInstalledIndex([
      installed({ registryName: "io.github.modelcontextprotocol/server-filesystem", installedVersion: "1.1.0" }),
    ]);
    const status = resolveMcpEntryStatus(entry({ version: "1.2.0" }), index);

    expect(status.state).toBe("updateAvailable");
    expect([status.installedVersion, status.latestVersion]).toEqual(["1.1.0", "1.2.0"]);
  });

  it("never claims an update for a downgrade", () => {
    const index = buildInstalledIndex([
      installed({ registryName: "io.github.modelcontextprotocol/server-filesystem", installedVersion: "2.0.0" }),
    ]);
    expect(resolveMcpEntryStatus(entry({ version: "1.2.0" }), index).state).toBe("installed");
  });

  it("never claims an update from a name-only match", () => {
    // A name match has no provenance: the recorded version was not necessarily
    // taken from this catalog row.
    const index = buildInstalledIndex([installed({ installedVersion: "1.0.0" })]);
    expect(resolveMcpEntryStatus(entry({ version: "9.9.9" }), index).state).toBe("installed");
  });

  it("falls back to the registry's own isLatest for incomparable versions", () => {
    const index = buildInstalledIndex([
      installed({ registryName: "io.github.modelcontextprotocol/server-filesystem", installedVersion: "0.0.1a3" }),
    ]);

    expect(resolveMcpEntryStatus(entry({ version: "0.0.1a4", isLatest: true }), index).state).toBe("updateAvailable");
    expect(resolveMcpEntryStatus(entry({ version: "0.0.1a4", isLatest: false }), index).state).toBe("installed");
  });

  it("surfaces deprecated and superseded independently of installation", () => {
    const status = resolveMcpEntryStatus(entry({ status: "deprecated", isLatest: false }), buildInstalledIndex([]));
    expect(status).toMatchObject({ state: "notInstalled", deprecated: true, superseded: true });
  });

  it("matches fingerprints case-insensitively", () => {
    const index = buildInstalledIndex([
      installed({ registryName: "IO.GitHub.ModelContextProtocol/Server-Filesystem" }),
    ]);
    expect(resolveMcpEntryStatus(entry(), index).matchedByName).toBe(false);
  });
});

describe("installedHasUpdate", () => {
  it("needs a version on both sides", () => {
    expect(installedHasUpdate(installed({ installedVersion: "1.0.0" }), entry({ version: null }))).toBe(false);
    expect(installedHasUpdate(installed({}), entry({ version: "1.0.0" }))).toBe(false);
    expect(installedHasUpdate(installed({ installedVersion: "1.0.0" }), null)).toBe(false);
  });

  it("is true only for a newer catalog version", () => {
    expect(installedHasUpdate(installed({ installedVersion: "1.0.0" }), entry({ version: "1.1.0" }))).toBe(true);
    expect(installedHasUpdate(installed({ installedVersion: "1.1.0" }), entry({ version: "1.1.0" }))).toBe(false);
  });
});

describe("latestVersionForInstalled", () => {
  it("only matches a row with the same reverse-DNS name", () => {
    const server = installed({ registryName: "io.github.modelcontextprotocol/server-filesystem" });
    expect(latestVersionForInstalled(server, [entry()])?.id).toBe("row-1");
    expect(latestVersionForInstalled(server, [entry({ namespace: "io.github.other/thing" })])).toBeNull();
  });

  it("returns null for an entry with no fingerprint", () => {
    expect(latestVersionForInstalled(installed({}), [entry()])).toBeNull();
  });
});
