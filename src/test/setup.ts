import "@testing-library/jest-dom/vitest";
import { vi } from "vitest";
// Initialize i18next (zh-CN, module side-effect) so components render real
// strings in tests instead of raw translation keys.
import "../i18n";

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
