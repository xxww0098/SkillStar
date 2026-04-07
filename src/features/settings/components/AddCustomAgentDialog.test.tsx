import { fireEvent, render, screen } from "@testing-library/react";
import { describe, expect, it, vi } from "vitest";
import { AddCustomAgentDialog } from "./AddCustomAgentDialog";

vi.mock("react-i18next", () => ({
  useTranslation: () => ({
    t: (_key: string, options?: { defaultValue?: string }) => options?.defaultValue ?? _key,
    i18n: { language: "zh-CN" },
  }),
}));

vi.mock("../../../lib/toast", () => ({
  toast: {
    error: vi.fn(),
  },
}));

vi.mock("../../../lib/utils", async (importOriginal) => {
  const actual = await importOriginal<typeof import("../../../lib/utils")>();
  return {
    ...actual,
    formatPlatformPath: (path: string) => path.replace(/\//g, "\\"),
  };
});

describe("AddCustomAgentDialog", () => {
  it("normalizes Windows project skill paths before confirm", () => {
    const onConfirm = vi.fn();

    render(<AddCustomAgentDialog open onClose={vi.fn()} onConfirm={onConfirm} />);

    fireEvent.change(screen.getByPlaceholderText("e.g. My AI Assistant"), {
      target: { value: "Ollama" },
    });
    fireEvent.change(screen.getByPlaceholderText("e.g. ~/.myagent/skills"), {
      target: { value: "D:\\ollama\\skills" },
    });
    fireEvent.change(screen.getByPlaceholderText("e.g. .myagent\\skills"), {
      target: { value: ".ollama\\skills" },
    });

    fireEvent.click(screen.getByRole("button", { name: "common.add" }));

    expect(onConfirm).toHaveBeenCalledWith(
      expect.objectContaining({
        display_name: "Ollama",
        global_skills_dir: "D:\\ollama\\skills",
        project_skills_rel: ".ollama/skills",
      }),
    );
  });

  it("formats existing project skill paths with Windows separators when editing", () => {
    render(
      <AddCustomAgentDialog
        open
        onClose={vi.fn()}
        onConfirm={vi.fn()}
        initialData={{
          id: "custom_ollama",
          display_name: "Ollama",
          global_skills_dir: "D:\\ollama\\skills",
          project_skills_rel: ".ollma/skills",
          icon_data_uri: null,
        }}
      />,
    );

    expect(screen.getByDisplayValue(".ollma\\skills")).toBeInTheDocument();
  });
});
