import { expect, open, test } from "nsh/test";

const HOST = "./channels.test.host.tsx";

function ops(app: { texts: () => string[] }): string {
  return app.texts().find((t) => t.startsWith("ops:")) ?? "";
}

test("channels workspace works offline for local snapshot, auto-update toggle and marketplace export", async () => {
  const app = await open(HOST, { width: 1120, height: 800 });
  await app.waitUntil(() => app.shows("id:channels-item-101"));
  // Signed out state still displays local channel + skills + auto-update + marketplace export
  expect(app.shows("id:channels-github-sign-in")).toBe(true);
  expect(app.shows("id:channels-skill-row-alpha")).toBe(true);
  expect(app.shows("id:channels-auto-update-toggle")).toBe(true);
  expect(app.shows("id:channels-export-btn")).toBe(true);

  await app.press("id:channels-auto-update-toggle");
  await app.waitUntil(() => ops(app).includes("autoUpdate:101:false"));

  await app.press("id:channels-export-btn");
  await app.waitUntil(() => app.shows("id:channels-modal-card"));
  await app.press("id:channels-export-browse");
  await app.waitUntil(() => ops(app).includes("dir:/tmp/exported-mkt"));
  await app.press("id:channels-export-confirm");
  await app.waitUntil(() => !app.shows("id:channels-modal-card"));
  expect(ops(app)).toContain("export:101:/tmp/exported-mkt");
  expect(ops(app)).not.toContain("localRefresh");
});

test("signed-in channels workspace handles divergence resolution, install, rollback, uninstall, convert-to-local, inbox accept, create, register and publish", async () => {
  const app = await open(HOST, { width: 1120, height: 800 });
  await app.waitUntil(() => app.shows("id:channels-item-101"));

  await app.press("id:host-sign-in");
  await app.waitUntil(() => app.shows("id:channels-inbox"));

  // Apply blocked update after selecting Preserve resolution
  await app.waitUntil(() => app.shows("id:channels-conflict-preserve-alpha"));
  await app.press("id:channels-conflict-preserve-alpha");
  await app.press("id:channels-apply-update-btn");
  await app.waitUntil(() => app.texts().some((t) => t.includes("apply:101:alpha=preserve")));

  // Install on-demand added channel skill
  await app.press("id:channels-install-gamma");
  await app.waitUntil(() => ops(app).includes("install:101:gamma"));

  // Rollback channel skill to an earlier release
  await app.press("id:channels-rollback-alpha");
  await app.waitUntil(() => app.shows("id:channels-rollback-confirm"));
  await app.press("id:channels-rollback-confirm");
  await app.waitUntil(() => !app.shows("id:channels-modal-card"));
  expect(ops(app)).toContain("rollback:101:alpha:channel-v000001");

  // Uninstall removed upstream skill
  await app.press("id:channels-removed-uninstall-delta");
  await app.waitUntil(() => ops(app).includes("uninstallRemoved:101:delta"));

  // Convert removed upstream skill to independent local copy
  await app.press("id:channels-removed-to-local-beta");
  await app.waitUntil(() => app.shows("id:channels-convert-confirm"));
  await app.press("id:channels-convert-confirm");
  await app.waitUntil(() => !app.shows("id:channels-modal-card"));
  expect(ops(app)).toContain("convertLocal:101:beta:beta.local");

  // Publish release flow
  await app.press("id:channels-publish-btn");
  await app.waitUntil(() => app.shows("id:channels-publish-confirm"));
  await app.press("id:channels-publish-confirm");
  await app.waitUntil(() => !app.shows("id:channels-modal-card"));
  expect(app.texts().some((t) => t.includes("publish:"))).toBe(true);

  // Accept inbox invitation -> review & subscribe
  await app.press("id:channels-inbox-accept-901");
  await app.waitUntil(() => app.shows("id:channels-review-subscribe-btn"));
  await app.press("id:channels-review-subscribe-btn");
  await app.waitUntil(() => app.shows("id:channels-review-confirm"));
  await app.press("id:channels-review-confirm");
  await app.waitUntil(() => !app.shows("id:channels-modal-card"));
  expect(app.texts().some((t) => t.includes("subscribe:202:sec-check"))).toBe(true);

  // Register existing org repo flow
  await app.press("id:channels-register-btn");
  await app.waitUntil(() => app.shows("id:channels-register-scan-404"));
  await app.press("id:channels-register-scan-404");
  await app.waitUntil(() => app.shows("id:channels-register-confirm"));
  await app.press("id:channels-register-confirm");
  await app.waitUntil(() => !app.shows("id:channels-modal-card"));
  expect(app.shows("id:channels-item-404")).toBe(true);

  // Exactly the 6 local-skill-mutating actions notified local refresh
  const refreshCount = ops(app)
    .replace(/^ops:/, "")
    .replace(/ dir:.*$/, "")
    .split(";")
    .filter((entry) => entry === "localRefresh").length;
  expect(refreshCount).toBe(6);
});
