import { render, screen } from "@testing-library/react";
import { describe, expect, it, vi } from "vitest";

vi.mock("./AppInstancesPanel", () => ({
  AppInstancesPanel: ({ appId }: { appId: string }) => <div data-testid={`panel-${appId}`} />,
}));

vi.mock("../ProviderLogo", () => ({
  ProviderLogo: () => <span data-testid="logo" />,
}));

vi.mock("react-i18next", () => ({
  useTranslation: () => ({
    t: (key: string) => key,
  }),
}));

import { DesktopAppsSection } from "./DesktopAppsSection";

describe("DesktopAppsSection", () => {
  it("lists Grok Bot as its own desktop app, not xai", () => {
    render(<DesktopAppsSection appIds={["grok-bot"]} />);
    expect(screen.getByText("usage.grokBotDesktop")).toBeInTheDocument();
    expect(screen.queryByText("xai")).not.toBeInTheDocument();
    expect(screen.getByTestId("panel-grok-bot")).toBeInTheDocument();
  });
});
