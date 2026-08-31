import { render } from "@testing-library/react";
import { describe, expect, it } from "vitest";
import { PROVIDER_AGENTS } from "../../lib/agentRegistry";
import { AgentToolIcon } from "./AgentToolIcon";

describe("AgentToolIcon", () => {
  it("renders a brand glyph for every Models hub agent (no letter fallback)", () => {
    for (const agent of PROVIDER_AGENTS) {
      const { container, unmount } = render(<AgentToolIcon toolId={agent.iconId} />);
      const root = container.firstElementChild;
      expect(root).toHaveAttribute("aria-hidden");
      const svg = container.querySelector("svg");
      expect(svg).toBeTruthy();
      // The brand glyph carries a `<title>`, which is metadata rather than
      // painted text. Drop the SVG before checking, so this keeps asserting
      // "no letter fallback next to the glyph" instead of the brand name.
      svg?.remove();
      expect(container.textContent?.trim()).toBe("");
      unmount();
    }
  });

  it("applies the requested size box", () => {
    const { container: sm } = render(<AgentToolIcon toolId="pi" size="sm" />);
    expect(sm.firstElementChild).toHaveClass("h-6", "w-6");

    const { container: md } = render(<AgentToolIcon toolId="pi" size="md" />);
    expect(md.firstElementChild).toHaveClass("h-7", "w-7");
  });
});
