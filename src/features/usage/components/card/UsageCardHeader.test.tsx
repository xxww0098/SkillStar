import { render, screen } from "@testing-library/react";
import { describe, expect, it, vi } from "vitest";
import { getBrandTheme } from "../../lib/brandThemes";
import type { CliAccountBadge } from "../../lib/cliCustody";
import { UsageCardHeader } from "./UsageCardHeader";

vi.mock("react-i18next", () => ({
  useTranslation: () => ({ t: (key: string) => key }),
}));

function renderHeader(cliBadge: CliAccountBadge, catalogId = "xai", displayName = "Grok · alice") {
  return render(
    <UsageCardHeader
      catalogId={catalogId}
      displayName={displayName}
      brandColorHex="2563EB"
      theme={getBrandTheme("xai", "2563EB")}
      planName={null}
      cliBadge={cliBadge}
    />,
  );
}

/** The chip is the whole claim the card makes about the CLI, so each state has
 *  to reach the screen as its own words — never as the absence of the green. */
describe("UsageCardHeader CLI badge", () => {
  it("says the CLI is on this account", () => {
    renderHeader("current");
    expect(screen.getByText("usage.cardActive")).toBeInTheDocument();
    expect(screen.queryByText("usage.cardCliDiverged")).not.toBeInTheDocument();
  });

  it("says the CLI is on something else instead of quietly showing nothing", () => {
    renderHeader("diverged");
    const chip = screen.getByText("usage.cardCliDiverged");
    expect(chip).toBeInTheDocument();
    expect(chip.closest("[data-cli-badge]")).toHaveAttribute("data-cli-badge", "diverged");
    expect(screen.queryByText("usage.cardActive")).not.toBeInTheDocument();
  });

  it("says the CLI has nobody signed in", () => {
    renderHeader("missing");
    expect(screen.getByText("usage.cardCliMissing")).toBeInTheDocument();
    expect(screen.queryByText("usage.cardActive")).not.toBeInTheDocument();
  });

  it("shows no chip at all for a card with nothing to claim", () => {
    const { container } = renderHeader("none");
    expect(container.querySelector("[data-cli-badge]")).toBeNull();
  });

  it("gives the three states visibly different chips", () => {
    const classOf = (badge: CliAccountBadge) => {
      const { container, unmount } = renderHeader(badge);
      const className = container.querySelector("[data-cli-badge]")?.className ?? "";
      unmount();
      return className;
    };
    const [current, diverged, missing] = (["current", "diverged", "missing"] as const).map(classOf);
    expect(new Set([current, diverged, missing]).size).toBe(3);
    expect(current).toContain("emerald");
    expect(diverged).toContain("amber");
  });

  it("uses IDE wording for Antigravity's diverged and missing states", () => {
    const { unmount } = renderHeader("diverged", "antigravity");
    expect(screen.getByText("usage.cardIdeDiverged")).toBeInTheDocument();
    unmount();

    renderHeader("missing", "antigravity");
    expect(screen.getByText("usage.cardIdeMissing")).toBeInTheDocument();
    expect(screen.queryByText("usage.cardCliMissing")).not.toBeInTheDocument();
  });

  it("uses IDE wording for Cursor's diverged and missing states", () => {
    const { unmount } = renderHeader("diverged", "cursor");
    expect(screen.getByText("usage.cardIdeDiverged")).toBeInTheDocument();
    unmount();

    renderHeader("missing", "cursor");
    expect(screen.getByText("usage.cardIdeMissing")).toBeInTheDocument();
    expect(screen.queryByText("usage.cardCliMissing")).not.toBeInTheDocument();
  });

  it("keeps the complete email identity in the heading", () => {
    const email = "account.with.a.long.local.part@example.com";
    renderHeader("none", "cursor", email);

    expect(screen.getByRole("heading", { name: email })).toBeInTheDocument();
  });

  it("reserves the same title and tool-status rhythm with or without a current badge", () => {
    renderHeader("current", "cursor", "account@example.com");

    const heading = screen.getByRole("heading", { name: "account@example.com" });
    expect(heading).toHaveClass("min-h-[2.25rem]");
    expect(heading.nextElementSibling).toHaveClass("h-[18px]");
  });

  it("gives the drag handle a 24px target that stays visible without hover", () => {
    renderHeader("none", "cursor", "account@example.com");
    const handle = screen.getByRole("button", { name: "usage.dragHandle" });
    expect(handle).toHaveClass("size-6");
    expect(handle.className).not.toContain("opacity-0");
  });
});
