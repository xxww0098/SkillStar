import { fireEvent, render, screen } from "@testing-library/react";
import { describe, expect, it, vi } from "vitest";
import type { ProviderForm } from "../../../hooks/useProviderForm";
import { ModelsTab } from "./ModelsTab";

const MODELS = ["alpha", "beta", "gamma", "delta", "epsilon", "zeta", "omega"];

function form(overrides: Partial<ProviderForm> = {}): ProviderForm {
  return {
    values: {
      models: MODELS,
      modelCatalog: [],
      defaultModel: "alpha",
      modelsUrl: "https://api.example.com/v1/models",
      apiKey: "secret-key",
    },
    setField: vi.fn(),
    handleFetchModels: vi.fn(),
    codexModelOptions: MODELS,
    isFetchingModels: false,
    fetchError: null,
    modelFetchCount: null,
    ...overrides,
  } as unknown as ProviderForm;
}

describe("ModelsTab", () => {
  it("filters a long model catalog and adds a custom id", () => {
    const providerForm = form();
    render(<ModelsTab form={providerForm} />);

    expect(screen.getAllByRole("listitem")).toHaveLength(MODELS.length);
    fireEvent.change(screen.getByPlaceholderText("搜索模型 id 或名称…"), { target: { value: "omega" } });
    expect(screen.getAllByRole("listitem")).toHaveLength(1);
    expect(screen.getByRole("listitem")).toHaveTextContent("omega");

    fireEvent.change(screen.getByPlaceholderText("手动添加模型 id，回车确认"), {
      target: { value: "custom-model" },
    });
    fireEvent.click(screen.getByRole("button", { name: "添加" }));
    expect(providerForm.setField).toHaveBeenCalledWith("models", [...MODELS, "custom-model"]);

    fireEvent.change(screen.getByRole("combobox", { name: "默认模型（Codex / OpenCode 默认使用）" }), {
      target: { value: "custom-default-model" },
    });
    expect(providerForm.setField).toHaveBeenCalledWith("defaultModel", "custom-default-model");
  });

  it("keeps the fetch requirement visible when credentials are missing", () => {
    render(
      <ModelsTab
        form={form({
          values: {
            models: [],
            modelCatalog: [],
            defaultModel: "",
            modelsUrl: "",
            apiKey: "",
          } as unknown as ProviderForm["values"],
        })}
      />,
    );

    expect(screen.getByRole("button", { name: "拉取模型" })).toBeDisabled();
    expect(screen.getByText(/需要先填写模型列表 URL 和 API Key/)).toBeInTheDocument();
  });

  it("does not keep a hidden search filter after the list shrinks", () => {
    const providerForm = form();
    const { rerender } = render(<ModelsTab form={providerForm} />);

    fireEvent.change(screen.getByPlaceholderText("搜索模型 id 或名称…"), { target: { value: "omega" } });
    expect(screen.getAllByRole("listitem")).toHaveLength(1);

    rerender(
      <ModelsTab
        form={form({
          values: {
            ...providerForm.values,
            models: MODELS.slice(0, 6),
          },
          codexModelOptions: MODELS.slice(0, 6),
        })}
      />,
    );

    expect(screen.queryByPlaceholderText("搜索模型 id 或名称…")).not.toBeInTheDocument();
    expect(screen.getAllByRole("listitem")).toHaveLength(6);
  });
});
