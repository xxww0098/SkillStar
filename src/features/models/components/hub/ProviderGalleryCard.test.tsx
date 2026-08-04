import { render, screen } from "@testing-library/react";
import { describe, expect, it, vi } from "vitest";
import type { ProviderEntryFlat } from "../../../../types";
import { ProviderGalleryCard } from "./ProviderGalleryCard";

vi.mock("react-i18next", () => ({
  useTranslation: () => ({
    t: (key: string) => key,
  }),
}));

vi.mock("../../../../components/shared/ProviderBrandIcon", () => ({
  ProviderBrandIcon: () => <span data-testid="provider-icon" />,
}));

const provider: ProviderEntryFlat = {
  id: "prov-1",
  name: "Acme",
  base_url_openai: "https://api.example.com/v1",
  base_url_anthropic: "",
  models_url: "https://api.example.com/v1/models",
  api_key: "sk-test",
  models: ["m1"],
  default_model: "m1",
  sort_index: 0,
};

describe("ProviderGalleryCard", () => {
  it("renders a Pi brand glyph when the provider is bound to pi", () => {
    const { container } = render(
      <ProviderGalleryCard
        provider={provider}
        toolActivations={{
          pi: { entries: [{ provider_id: provider.id, model: "m1" }], active_index: 0 },
        }}
        onOpen={() => {}}
        onDuplicate={() => {}}
        onDelete={() => {}}
      />,
    );

    expect(screen.getByTitle("Pi")).toBeInTheDocument();
    expect(container.querySelector("svg")).toBeTruthy();
    expect(screen.queryByText("P")).not.toBeInTheDocument();
  });
});
