import { fireEvent, render, screen } from "@testing-library/react";
import { describe, expect, it, vi } from "vitest";
import { AgentTargetCarousel, type AgentTargetCarouselItem } from "./AgentTargetCarousel";

const ICON = "data:image/svg+xml;base64,PHN2Zy8+";

function item(
  id: string,
  selected: AgentTargetCarouselItem["selected"],
  overrides: Partial<AgentTargetCarouselItem> = {},
): AgentTargetCarouselItem {
  return {
    id,
    profile: {
      id,
      display_name: id,
      icon: ICON,
      enabled: true,
    },
    selected,
    title: `Toggle ${id}`,
    ...overrides,
  };
}

describe("AgentTargetCarousel", () => {
  it("exposes active, mixed, and inactive states and reports the selected target", () => {
    const onToggle = vi.fn();
    const items = [item("active", true), item("partial", "mixed"), item("inactive", false)];
    render(<AgentTargetCarousel items={items} onToggle={onToggle} />);

    expect(screen.getByRole("button", { name: "Toggle active" })).toHaveAttribute("aria-pressed", "true");
    expect(screen.getByRole("button", { name: "Toggle partial" })).toHaveAttribute("aria-pressed", "mixed");
    expect(screen.getByRole("button", { name: "Toggle inactive" })).toHaveAttribute("aria-pressed", "false");

    fireEvent.click(screen.getByRole("button", { name: "Toggle inactive" }));
    expect(onToggle).toHaveBeenCalledWith(items[2]);
  });

  it("blocks pending and explicitly disabled targets", () => {
    const onToggle = vi.fn();
    render(
      <AgentTargetCarousel
        items={[item("pending", false, { pending: true }), item("disabled", false, { disabled: true })]}
        onToggle={onToggle}
      />,
    );

    expect(screen.getByRole("button", { name: "Toggle pending" })).toBeDisabled();
    expect(screen.getByRole("button", { name: "Toggle pending" })).toHaveAttribute("aria-busy", "true");
    expect(screen.getByRole("button", { name: "Toggle disabled" })).toBeDisabled();
    fireEvent.click(screen.getByRole("button", { name: "Toggle pending" }));
    fireEvent.click(screen.getByRole("button", { name: "Toggle disabled" }));
    expect(onToggle).not.toHaveBeenCalled();
  });

  it("renders nothing when no Agent is targetable", () => {
    const { container } = render(<AgentTargetCarousel items={[]} onToggle={vi.fn()} />);
    expect(container).toBeEmptyDOMElement();
  });

  it("fills the footer track with fixed-spacing icons: no visible-icon cap", () => {
    const { container } = render(
      <AgentTargetCarousel
        items={["one", "two", "three", "four", "five"].map((id) => item(id, false))}
        onToggle={vi.fn()}
      />,
    );

    const rail = container.querySelector(".hscroll-row-wrapper > div");
    expect(rail).not.toHaveStyle({ maxWidth: "130px" });
    // The track stretches to the wrapper (the card footer's free width, so
    // scroll arrows pin to its far edges) while the icons keep their fixed
    // spacing — start-anchored, never spread apart.
    expect(rail).toHaveClass("flex-1");
    expect(rail).toHaveClass("gap-1.5");
    expect(rail).not.toHaveClass("justify-between");
  });

  it("keeps a Settings-disabled Agent as a stopped SVG instead of dropping it", () => {
    const onToggle = vi.fn();
    render(
      <AgentTargetCarousel
        items={[
          item("ready", false),
          item("disabled", true, { profile: { ...item("disabled", true).profile, enabled: false } }),
        ]}
        onToggle={onToggle}
      />,
    );

    expect(screen.getByRole("button", { name: "Toggle ready" })).toBeInTheDocument();
    const stopped = screen.getByRole("button", { name: /disabled/i });
    expect(stopped).toBeDisabled();
    fireEvent.click(stopped);
    expect(onToggle).not.toHaveBeenCalled();
  });
});
