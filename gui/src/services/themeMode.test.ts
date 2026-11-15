import { expect, test } from "nsh/test";
import { colorScheme } from "nsh";
import themeJson from "../../theme.json";
import { isLight, paletteToken, setThemeMode, themeMode, type PaletteToken } from "./themeMode";

const ALL_PALETTE_TOKENS: Record<PaletteToken, true> = {
  bg: true,
  panel: true,
  panel_hover: true,
  panel_active: true,
  card: true,
  card_hover: true,
  card_active: true,
  card_active_hover: true,
  well: true,
  input: true,
  scrim: true,
  tip_bg: true,
  tip_fg: true,
  fg: true,
  fg_muted: true,
  fg_faint: true,
  tag: true,
  on_accent: true,
  on_warn: true,
  border: true,
  border_soft: true,
  edge: true,
  accent: true,
  accent_hover: true,
  accent_fg: true,
  accent_soft: true,
  accent_soft_edge: true,
  ok: true,
  ok_bg: true,
  ok_border: true,
  ok_hover: true,
  warn: true,
  warn_bg: true,
  warn_border: true,
  warn_hover: true,
  danger: true,
  danger_bg: true,
  danger_border: true,
  danger_fg: true,
  danger_hover: true,
  info: true,
  info_bg: true,
  info_border: true,
  violet: true,
  violet_fg: true,
  cyan: true,
  cyan_fg: true,
  cyan_bg: true,
  cyan_border: true,
  amber: true,
  amber_fg: true,
  amber_bg: true,
  amber_border: true,
  os_surface: true,
  os_ink: true,
  os_line: true,
  os_edge: true,
  os_hair: true,
  os_fill: true,
  os_fill_2: true,
  os_muted: true,
  os_faint: true,
  os_tag: true,
  os_active_edge: true,
  os_ok: true,
  os_warn: true,
  os_bad: true,
};

function relativeLuminance(hex: string): number {
  const n = Number.parseInt(hex.slice(1), 16);
  const channels = [(n >> 16) & 0xff, (n >> 8) & 0xff, n & 0xff].map((value) => {
    const srgb = value / 255;
    return srgb <= 0.03928 ? srgb / 12.92 : Math.pow((srgb + 0.055) / 1.055, 2.4);
  });
  return 0.2126 * channels[0] + 0.7152 * channels[1] + 0.0722 * channels[2];
}

function contrastRatio(a: string, b: string): number {
  const l1 = relativeLuminance(a);
  const l2 = relativeLuminance(b);
  return (Math.max(l1, l2) + 0.05) / (Math.min(l1, l2) + 0.05);
}

test("default themeMode is dark obsidian (current)", () => {
  expect(themeMode()).toBe("current");
  expect(isLight()).toBe(false);
});

// 引擎外观跟着应用主题走：nsh 自己画的部件（TextView 代码块的语法调色板、
// 输入框占位、原生菜单）按 colorScheme 取 theme.json 的 light/dark 块。
test("setThemeMode moves the engine appearance with the look", () => {
  try {
    setThemeMode("paper");
    expect(colorScheme()).toBe("light");
    setThemeMode("current");
    expect(colorScheme()).toBe("dark");
  } finally {
    setThemeMode("current");
  }
});

test("themeMode tokens match theme.json in both current and paper modes", () => {
  const tokens = Object.keys(ALL_PALETTE_TOKENS) as PaletteToken[];
  const modes: Array<{ mode: "current" | "paper"; vars: Record<string, string> }> = [
    { mode: "current", vars: themeJson.cssVars.dark },
    { mode: "paper", vars: themeJson.cssVars.light },
  ];
  try {
    for (const entry of modes) {
      setThemeMode(entry.mode);
      for (const token of tokens) {
        const jsonKey = token === "accent" ? "primary" : token.replaceAll("_", "-");
        expect(paletteToken(token)).toBe(entry.vars[jsonKey]);
      }
      expect(entry.vars.accent).toBe(paletteToken("accent_soft"));
    }
  } finally {
    setThemeMode("current");
  }
});

test("both themes preserve monotonic elevation and WCAG AA contrast", () => {
  const modes: Array<"current" | "paper"> = ["current", "paper"];
  try {
    for (const mode of modes) {
      setThemeMode(mode);
      expect(relativeLuminance(paletteToken("bg"))).toBeLessThan(
        relativeLuminance(paletteToken("panel"))
      );
      expect(relativeLuminance(paletteToken("panel"))).toBeLessThan(
        relativeLuminance(paletteToken("card"))
      );

      for (const surface of ["panel", "card"] as const) {
        expect(contrastRatio(paletteToken("fg"), paletteToken(surface))).toBeGreaterThanOrEqual(4.5);
        expect(contrastRatio(paletteToken("fg_muted"), paletteToken(surface))).toBeGreaterThanOrEqual(4.5);
        expect(contrastRatio(paletteToken("accent_fg"), paletteToken(surface))).toBeGreaterThanOrEqual(4.5);
        expect(contrastRatio(paletteToken("accent"), paletteToken(surface))).toBeGreaterThanOrEqual(3.0);
      }

      expect(contrastRatio(paletteToken("on_accent"), paletteToken("accent"))).toBeGreaterThanOrEqual(4.5);
      expect(contrastRatio(paletteToken("ok"), paletteToken("ok_bg"))).toBeGreaterThanOrEqual(4.5);
      expect(contrastRatio(paletteToken("warn"), paletteToken("warn_bg"))).toBeGreaterThanOrEqual(4.5);
      expect(contrastRatio(paletteToken("amber_fg"), paletteToken("amber_bg"))).toBeGreaterThanOrEqual(4.5);
      expect(contrastRatio(paletteToken("danger_fg"), paletteToken("danger_bg"))).toBeGreaterThanOrEqual(4.5);
      expect(contrastRatio(paletteToken("info"), paletteToken("info_bg"))).toBeGreaterThanOrEqual(4.5);
    }
  } finally {
    setThemeMode("current");
  }
});
