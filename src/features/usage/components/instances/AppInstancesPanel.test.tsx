import { fireEvent, render, screen, waitFor } from "@testing-library/react";
import { beforeEach, describe, expect, it, vi } from "vitest";
import type { AppInstance } from "../../types";

const listAppInstances = vi.fn();
const createAppInstance = vi.fn();
const startAppInstance = vi.fn();
const stopAppInstance = vi.fn();
const deleteAppInstance = vi.fn();

vi.mock("../../api", () => ({
  usageApi: {
    listAppInstances: (...args: unknown[]) => listAppInstances(...args),
    createAppInstance: (...args: unknown[]) => createAppInstance(...args),
    startAppInstance: (...args: unknown[]) => startAppInstance(...args),
    stopAppInstance: (...args: unknown[]) => stopAppInstance(...args),
    deleteAppInstance: (...args: unknown[]) => deleteAppInstance(...args),
  },
}));

vi.mock("react-i18next", () => ({
  useTranslation: () => ({
    t: (key: string) => key,
  }),
}));

vi.mock("sonner", () => ({
  toast: { success: vi.fn(), error: vi.fn() },
}));

import { AppInstancesPanel } from "./AppInstancesPanel";

function row(overrides: Partial<AppInstance> = {}): AppInstance {
  return {
    id: "inst-1",
    app: "cursor",
    name: "Work",
    user_data_dir: "/tmp/instances/cursor/inst-1",
    extra_args: [],
    running: false,
    pid: null,
    created_at: 1,
    ...overrides,
  };
}

describe("AppInstancesPanel", () => {
  beforeEach(() => {
    listAppInstances.mockReset();
    createAppInstance.mockReset();
    startAppInstance.mockReset();
    stopAppInstance.mockReset();
    deleteAppInstance.mockReset();
    listAppInstances.mockResolvedValue([]);
    createAppInstance.mockResolvedValue(row());
    startAppInstance.mockResolvedValue(row({ running: true, pid: 9 }));
    stopAppInstance.mockResolvedValue(row());
    deleteAppInstance.mockResolvedValue(undefined);
  });

  it("creates then starts an instance instead of only making a directory", async () => {
    listAppInstances
      .mockResolvedValueOnce([])
      .mockResolvedValueOnce([row()])
      .mockResolvedValue([row({ running: true })]);

    render(<AppInstancesPanel appId="cursor" />);

    await waitFor(() => expect(listAppInstances).toHaveBeenCalledWith("cursor"));
    fireEvent.change(screen.getByLabelText("usage.instanceNamePlaceholder"), { target: { value: "Work" } });
    fireEvent.click(screen.getByRole("button", { name: /usage.createInstance/ }));

    await waitFor(() => expect(createAppInstance).toHaveBeenCalledWith("cursor", "Work"));
    await waitFor(() => expect(screen.getByText("Work")).toBeInTheDocument());

    fireEvent.click(screen.getByRole("button", { name: "usage.startInstance" }));
    await waitFor(() => expect(startAppInstance).toHaveBeenCalledWith("inst-1"));
  });

  it("stops only via the instance start/stop commands", async () => {
    listAppInstances.mockResolvedValue([row({ running: true, pid: 11 })]);
    render(<AppInstancesPanel appId="antigravity" />);
    await waitFor(() => screen.getByRole("button", { name: "usage.stopInstance" }));
    fireEvent.click(screen.getByRole("button", { name: "usage.stopInstance" }));
    await waitFor(() => expect(stopAppInstance).toHaveBeenCalledWith("inst-1"));
  });
});
