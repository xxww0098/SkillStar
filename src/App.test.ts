import { readFileSync } from "node:fs";
import path from "node:path";
import { describe, expect, it } from "vitest";

/**
 * Ratchet for the sidebar-collapse jank.
 *
 * `#main-content` wraps every page, so a CSS transition on one of its layout
 * properties re-lays out the whole page tree on every animation frame. While
 * `transition-[padding-left] duration-200` lived on it, collapsing the sidebar
 * woke `SkillGrid`'s ResizeObserver ~12 times in 200ms; each wake re-rendered
 * the grid, recomputed `gridColumnCount` into a second reflow, and made
 * framer-motion re-measure every `layout="position"` card. The toggle now FLIPs
 * instead: padding lands at its final value in one reflow and the 200ms
 * displacement is a transform the compositor owns.
 *
 * Nothing else goes red if the transition comes back. It just gets slow again,
 * silently, which is why this is asserted against the source rather than left
 * to a reviewer noticing one Tailwind class.
 */
const APP = readFileSync(path.resolve(__dirname, "App.tsx"), "utf8");

function mainContentClassName(): string {
  const el = APP.slice(APP.indexOf('id="main-content"'));
  return /className="([^"]*)"/.exec(el)?.[1] ?? "";
}

describe("app shell", () => {
  it("does not CSS-transition a layout property on #main-content", () => {
    const className = mainContentClassName();
    // Guards the guard: a slice/regex that stopped matching would make the
    // assertion below pass on an empty string.
    expect(className).toContain("overflow-hidden");
    expect(className).not.toMatch(/transition-\[/);
  });
});
