import "@testing-library/jest-dom/vitest";
import { vi } from "vitest";
// Initialize i18next (zh-CN, module side-effect) so components render real
// strings in tests instead of raw translation keys.
import "../i18n";

// jsdom here does not expose Web Storage; many components persist UI state to
// localStorage (view mode, My Skills scope, remote host selection). Provide an
// in-memory polyfill so those code paths behave like the real app.
if (typeof globalThis.localStorage === "undefined") {
  const store = new Map<string, string>();
  const storage: Storage = {
    getItem: (key) => (store.has(key) ? (store.get(key) as string) : null),
    setItem: (key, value) => {
      store.set(key, String(value));
    },
    removeItem: (key) => {
      store.delete(key);
    },
    clear: () => store.clear(),
    key: (index) => Array.from(store.keys())[index] ?? null,
    get length() {
      return store.size;
    },
  };
  Object.defineProperty(globalThis, "localStorage", { value: storage, configurable: true });
}

// jsdom lacks ResizeObserver, which PageToolbar / SkillGrid use for layout.
if (typeof globalThis.ResizeObserver === "undefined") {
  globalThis.ResizeObserver = class {
    observe() {}
    unobserve() {}
    disconnect() {}
  } as unknown as typeof ResizeObserver;
}

// Mock Tauri IPC — all invoke calls return undefined by default.
// Override per-test with vi.mocked(invoke).mockResolvedValue(...)
vi.mock("@tauri-apps/api/core", () => ({
  invoke: vi.fn(),
  isTauri: vi.fn(() => true),
}));

vi.mock("@tauri-apps/api/event", () => ({
  listen: vi.fn(() => Promise.resolve(() => {})),
  emit: vi.fn(),
}));
