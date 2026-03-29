export type BackgroundStyle = "current" | "paper";

const STORAGE_KEY = "skillstar:background-style";
const DEFAULT_BACKGROUND_STYLE: BackgroundStyle = "paper";

function normalizeBackgroundStyle(value: string | null): BackgroundStyle {
  if (value === "current" || value === "paper") return value;
  return DEFAULT_BACKGROUND_STYLE;
}

export function readBackgroundStyle(): BackgroundStyle {
  try {
    return normalizeBackgroundStyle(localStorage.getItem(STORAGE_KEY));
  } catch {
    return DEFAULT_BACKGROUND_STYLE;
  }
}

export function applyBackgroundStyle(style: BackgroundStyle) {
  const normalized = normalizeBackgroundStyle(style);

  document.documentElement.setAttribute("data-bg-style", normalized);

  try {
    localStorage.setItem(STORAGE_KEY, normalized);
  } catch {
    // ignore localStorage write errors
  }
}

export function initializeBackgroundStyle() {
  applyBackgroundStyle(readBackgroundStyle());
}
