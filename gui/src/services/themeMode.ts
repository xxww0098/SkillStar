import { setColorScheme, signal } from "nsh";

// Product semantics: the app switches between two in-app looks — `current`
// (dark canvas) and `paper` (light) — driven by user preference only, never
// by the system appearance. App surfaces never read engine tokens: every
// color goes through paletteToken() below, which reads this signal.
//
// The engine still has an appearance of its own (`setColorScheme`), and the
// parts nsh draws by itself color themselves from it: TextView's Markdown
// code blocks (their syntax palette has a light and a dark set), input
// placeholders, native menus, selection. setThemeMode() therefore moves the
// engine appearance with the look, so both palettes name the same thing —
// theme.json's `light` block IS the paper palette and its `dark` block the
// current one (kept in sync by themeMode.test.ts).
// Persistence lands with the bridge prefs module (gui_prefs.json,
// `background_style` field: "paper" is light, "current" is dark —
// theme.rs:354-377 parity).
export type ThemeMode = "current" | "paper";

export const themeMode = signal<ThemeMode>("current");

export function setThemeMode(mode: ThemeMode) {
  themeMode.set(mode);
  // Engine-native surfaces follow the look (`nsh.json` "appearance" seeds the
  // same value at launch): a paper app must not paint dark syntax on a light
  // code block. App surfaces are unaffected — they carry paletteToken colors.
  setColorScheme(mode === "paper" ? "light" : "dark");
}

export function toggleThemeMode() {
  setThemeMode(themeMode() === "current" ? "paper" : "current");
}

export function isLight(): boolean {
  return themeMode() === "paper";
}

// Obsidian Event Horizon ("current", default dark) & Stardust Alabaster
// ("paper", light) palettes harmonized with the Gargantua Black Hole app icon.
// theme.json carries the matching values for the nsh token system. Keep both
// in sync (checked by themeMode.test.ts).
const DARK = {
  bg: "#06070a",
  panel: "#0c0e14",
  panel_hover: "#161a24",
  panel_active: "#241b10",
  card: "#141720",
  card_hover: "#1b1f2b",
  card_active: "#211910",
  card_active_hover: "#2a2014",
  well: "#10131b",
  input: "#0e1118",
  scrim: "#040508",
  tip_bg: "#000000",
  tip_fg: "#ffffff",

  fg: "#f2efe9",
  fg_muted: "#9ca0ab",
  fg_faint: "#6e7381",
  tag: "#c4c2bc",
  on_accent: "#ffffff",
  on_warn: "#451a03",

  border: "#242834",
  border_soft: "#1b1e28",
  edge: "#464c5c",

  accent: "#ba5a08",
  accent_hover: "#d46b0c",
  accent_fg: "#f6b84c",
  accent_soft: "#251c10",
  accent_soft_edge: "#844f16",

  ok: "#2dd4bf",
  ok_bg: "#0b2f26",
  ok_border: "#146c50",
  ok_hover: "#113f33",
  warn: "#eab308",
  warn_bg: "#31260c",
  warn_border: "#a16207",
  warn_hover: "#3f310f",
  danger: "#f85149",
  danger_bg: "#381317",
  danger_border: "#8e252a",
  danger_fg: "#ff7b72",
  danger_hover: "#49191e",
  info: "#60a5fa",
  info_bg: "#12223d",
  info_border: "#2c528c",
  violet: "#a78bfa",
  violet_fg: "#d8b4fe",

  cyan: "#22d3ee",
  cyan_fg: "#67e8f9",
  cyan_bg: "#0b2c35",
  cyan_border: "#155e75",
  amber: "#f97316",
  amber_fg: "#fdba74",
  amber_bg: "#361b09",
  amber_border: "#9a3412",

  os_surface: "#0c0e14",
  os_ink: "#f2efe9",
  os_line: "#2d313f",
  os_edge: "#4c5265",
  os_hair: "#1f232e",
  os_fill: "#141822",
  os_fill_2: "#1d2230",
  os_muted: "#a3a6b1",
  os_faint: "#989ba7",
  os_tag: "#c4c2bc",
  os_active_edge: "#d46b0c",
  os_ok: "#2dd4bf",
  os_warn: "#eab308",
  os_bad: "#f85149",
} as const;

const LIGHT = {
  bg: "#ebe8e2",
  panel: "#f7f6f2",
  panel_hover: "#ece9e1",
  panel_active: "#f5e8cf",
  card: "#ffffff",
  card_hover: "#fbf7f0",
  card_active: "#fcf4e4",
  card_active_hover: "#f8ebd2",
  well: "#efece6",
  input: "#ffffff",
  scrim: "#181614",
  tip_bg: "#000000",
  tip_fg: "#ffffff",

  fg: "#18181b",
  fg_muted: "#57534e",
  fg_faint: "#857f77",
  tag: "#4c4843",
  on_accent: "#ffffff",
  on_warn: "#ffffff",

  border: "#ded8cd",
  border_soft: "#eae5dc",
  edge: "#afa799",

  accent: "#b45309",
  accent_hover: "#964206",
  accent_fg: "#9a4406",
  accent_soft: "#faefd9",
  accent_soft_edge: "#dfb46b",

  ok: "#0f766e",
  ok_bg: "#dcf5e4",
  ok_border: "#8ed4a2",
  ok_hover: "#c7eed4",
  warn: "#a16207",
  warn_bg: "#fef5d4",
  warn_border: "#eab308",
  warn_hover: "#fdeba8",
  danger: "#dc2626",
  danger_bg: "#fbe7e9",
  danger_border: "#efaab0",
  danger_fg: "#b91c1c",
  danger_hover: "#f6d3d7",
  info: "#1d4ed8",
  info_bg: "#e8effd",
  info_border: "#b4cbf9",
  violet: "#7c3aed",
  violet_fg: "#6d28d9",

  cyan: "#0891b2",
  cyan_fg: "#0e7490",
  cyan_bg: "#e0f7fa",
  cyan_border: "#67e8f9",
  amber: "#c2410c",
  amber_fg: "#9a3412",
  amber_bg: "#ffedd5",
  amber_border: "#fdba74",

  os_surface: "#ffffff",
  os_ink: "#18181b",
  os_line: "#dedad3",
  os_edge: "#b5afa4",
  os_hair: "#eae6df",
  os_fill: "#f3f1ec",
  os_fill_2: "#eae7e0",
  os_muted: "#635f59",
  os_faint: "#635f59",
  os_tag: "#494641",
  os_active_edge: "#b45309",
  os_ok: "#0f766e",
  os_warn: "#a16207",
  os_bad: "#dc2626",
} as const;

export type PaletteToken = keyof typeof DARK;

const PALETTES: Record<ThemeMode, Record<PaletteToken, string>> = {
  current: DARK,
  paper: LIGHT,
};

// Mode-aware token helper: returns the concrete color for the current mode.
// Reactive — reads the themeMode signal, so bindings re-render on switch.
export function paletteToken(token: PaletteToken): string {
  return PALETTES[themeMode()][token];
}

/** `#rrggbb` + alpha → CSS `rgba()` (GPUI's `color.alpha(n)` parity). */
export function hexAlpha(hex: string, alpha: number): string {
  const [r, g, b] = hexToRgb(hex);
  return `rgba(${r}, ${g}, ${b}, ${alpha})`;
}

// HSL lerp between two hex colors — theme.rs mix_hsl (lines 578-589)
// parity, used by the quota bars and the nav emphasis ink so midpoints
// stay hue-clean instead of going muddy through sRGB.
export function mixHsl(from: string, to: string, tRaw: number): string {
  const t = Math.min(1, Math.max(0, tRaw));
  if (t === 0) return from;
  if (t === 1) return to;
  const [h1, s1, l1] = rgbToHsl(from);
  const [h2, s2, l2] = rgbToHsl(to);
  let d = h2 - h1;
  if (d > 0.5) d -= 1;
  if (d < -0.5) d += 1;
  const h = ((h1 + d * t) % 1 + 1) % 1;
  return hslToRgb(h, s1 + (s2 - s1) * t, l1 + (l2 - l1) * t);
}

function hexToRgb(hex: string): [number, number, number] {
  const n = parseInt(hex.slice(1), 16);
  return [(n >> 16) & 0xff, (n >> 8) & 0xff, n & 0xff];
}

function rgbToHsl(hex: string): [number, number, number] {
  const [r0, g0, b0] = hexToRgb(hex);
  const r = r0 / 255;
  const g = g0 / 255;
  const b = b0 / 255;
  const max = Math.max(r, g, b);
  const min = Math.min(r, g, b);
  const l = (max + min) / 2;
  const d = max - min;
  if (d < Number.EPSILON) return [0, 0, l];
  const s = l > 0.5 ? d / (2 - max - min) : d / (max + min);
  let h: number;
  if (max === r) {
    h = (g - b) / d + (g < b ? 6 : 0);
  } else if (max === g) {
    h = (b - r) / d + 2;
  } else {
    h = (r - g) / d + 4;
  }
  return [h / 6, s, l];
}

function hueToRgb(p: number, q: number, tRaw: number): number {
  let t = tRaw;
  if (t < 0) t += 1;
  if (t > 1) t -= 1;
  if (t < 1 / 6) return p + (q - p) * 6 * t;
  if (t < 1 / 2) return q;
  if (t < 2 / 3) return p + (q - p) * (2 / 3 - t) * 6;
  return p;
}

function hslToRgb(h: number, s: number, l: number): string {
  let r: number;
  let g: number;
  let b: number;
  if (s <= Number.EPSILON) {
    r = g = b = l;
  } else {
    const q = l < 0.5 ? l * (1 + s) : l + s - l * s;
    const p = 2 * l - q;
    r = hueToRgb(p, q, h + 1 / 3);
    g = hueToRgb(p, q, h);
    b = hueToRgb(p, q, h - 1 / 3);
  }
  const ch = (c: number) =>
    Math.round(Math.min(1, Math.max(0, c)) * 255)
      .toString(16)
      .padStart(2, "0");
  return `#${ch(r)}${ch(g)}${ch(b)}`;
}
