import "@testing-library/jest-dom/vitest";
import { vi } from "vitest";

// Mock Tauri IPC — all invoke calls return undefined by default.
// Override per-test with vi.mocked(invoke).mockResolvedValue(...)
vi.mock("@tauri-apps/api/core", () => ({
  invoke: vi.fn(),
}));

vi.mock("@tauri-apps/api/event", () => ({
  listen: vi.fn(() => Promise.resolve(() => {})),
  emit: vi.fn(),
}));
