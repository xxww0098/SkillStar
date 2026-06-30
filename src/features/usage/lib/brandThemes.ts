/**
 * Per-brand visual themes for usage cards.
 *
 * The fixed catalog (`skillstar-usage::catalog`) only carries a single
 * `brand_color` hex. That gives every card the same shape differentiated by
 * one accent — which reads as "no brand character". This registry layers a
 * richer, brand-authentic identity on top: a signature two-stop header
 * gradient, a vivid duotone for progress bars/accents, the legible foreground
 * color on the band, and a glow tint.
 *
 * Keyed by `catalog_id`. Unknown ids fall back to a gradient derived from the
 * catalog's single `brand_color`, so new catalog entries still look intentional
 * before they get a hand-tuned theme here.
 */

export interface BrandTheme {
  /** Signature band gradient `[from, to]` (rendered at 135deg). May include a
   *  deep/near-black stop for dark-forward brands; legible with `fg`. */
  header: [string, string];
  /** Vivid duotone for progress bars / accents — never a near-black stop. */
  bar: [string, string];
  /** Text + icon color drawn on the header band. */
  fg: string;
  /** Glow / radial tint color. */
  glow: string;
}

const THEMES: Record<string, BrandTheme> = {
  // ── OAuth IDEs / agents ──────────────────────────────────────────────
  cursor: { header: ["#0F1417", "#00C4A3"], bar: ["#00E5BC", "#00B89A"], fg: "#ffffff", glow: "#00E5BC" },
  codex: { header: ["#0E8E6D", "#19C37D"], bar: ["#10A37F", "#19C37D"], fg: "#ffffff", glow: "#10A37F" },
  antigravity: { header: ["#4285F4", "#34A853"], bar: ["#4285F4", "#1A73E8"], fg: "#ffffff", glow: "#4285F4" },
  trae: { header: ["#FF7A45", "#F9376E"], bar: ["#FF7A45", "#FF5630"], fg: "#ffffff", glow: "#FF7A45" },
  qoder: { header: ["#7C3AED", "#5B21B6"], bar: ["#8B5CF6", "#7C3AED"], fg: "#ffffff", glow: "#7C3AED" },
  xai: { header: ["#1A1A1A", "#000000"], bar: ["#3F3F46", "#18181B"], fg: "#ffffff", glow: "#52525B" },

  // ── API-key plans ────────────────────────────────────────────────────
  deepseek: { header: ["#4D6BFE", "#1A56DB"], bar: ["#4D6BFE", "#3B5BDB"], fg: "#ffffff", glow: "#4D6BFE" },
  glm: { header: ["#4A90E2", "#2D6BD0"], bar: ["#4A90E2", "#2D6BD0"], fg: "#ffffff", glow: "#4A90E2" },
  kimi: { header: ["#23201A", "#F5B400"], bar: ["#F5B400", "#FF8A00"], fg: "#ffffff", glow: "#F5B400" },
  minimax: { header: ["#9333EA", "#C026D3"], bar: ["#9333EA", "#A855F7"], fg: "#ffffff", glow: "#9333EA" },

  // ── Cookie / manual ──────────────────────────────────────────────────
  stepfun: { header: ["#008A80", "#00B5A9"], bar: ["#00B5A9", "#00D9C0"], fg: "#ffffff", glow: "#00B5A9" },
  opencode: { header: ["#1E293B", "#334155"], bar: ["#64748B", "#334155"], fg: "#ffffff", glow: "#475569" },
};

function normalizeHex(hex: string): string {
  const h = hex.replace("#", "").trim();
  return /^[0-9a-fA-F]{6}$/.test(h) ? `#${h}` : "#6B7280";
}

/** Relative luminance (0..1) of an `#rrggbb` color. */
function luminance(hex: string): number {
  const h = hex.replace("#", "");
  const r = parseInt(h.substring(0, 2), 16) / 255;
  const g = parseInt(h.substring(2, 4), 16) / 255;
  const b = parseInt(h.substring(4, 6), 16) / 255;
  const lin = (c: number) => (c <= 0.03928 ? c / 12.92 : ((c + 0.055) / 1.055) ** 2.4);
  return 0.2126 * lin(r) + 0.7152 * lin(g) + 0.0722 * lin(b);
}

/** Scale each channel toward black by `amount` (0..1). */
function darken(hex: string, amount: number): string {
  const h = hex.replace("#", "");
  const scale = (start: number) => {
    const v = parseInt(h.substring(start, start + 2), 16);
    return Math.max(0, Math.round(v * (1 - amount)))
      .toString(16)
      .padStart(2, "0");
  };
  return `#${scale(0)}${scale(2)}${scale(4)}`;
}

/**
 * Resolve a brand theme for a catalog id, falling back to a gradient derived
 * from the provided `brand_color` hex (with or without leading `#`).
 */
export function getBrandTheme(catalogId: string, brandColorHex: string): BrandTheme {
  const explicit = THEMES[catalogId];
  if (explicit) return explicit;

  const base = normalizeHex(brandColorHex);
  const deep = darken(base, 0.28);
  const fg = luminance(base) > 0.62 ? "#0b0b0c" : "#ffffff";
  return { header: [base, deep], bar: [base, deep], fg, glow: base };
}
