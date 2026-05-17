import { fireEvent, render, screen } from "@testing-library/react";
import { describe, expect, it, vi } from "vitest";

// Mock react-i18next — return the key so we can assert on known keys
vi.mock("react-i18next", () => ({
  useTranslation: () => ({
    t: (key: string) => {
      const translations: Record<string, string> = {
        "sidebar.skills": "My Skills",
        "sidebar.market": "Marketplace",
        "sidebar.groups": "Groups",
        "sidebar.projects": "Projects",
        "sidebar.settings": "Settings",
      };
      return translations[key] ?? key;
    },
    i18n: { language: "en" },
  }),
}));

import { SkillsNav } from "../SkillsNav";

describe("SkillsNav", () => {
  const defaultProps = {
    activePage: "my-skills" as const,
    onNavigate: vi.fn(),
    collapsed: false,
  };

  it("renders all navigation items", () => {
    render(<SkillsNav {...defaultProps} />);

    expect(screen.getByText("My Skills")).toBeInTheDocument();
    expect(screen.getByText("Marketplace")).toBeInTheDocument();
    expect(screen.getByText("Groups")).toBeInTheDocument();
    expect(screen.getByText("Projects")).toBeInTheDocument();
    expect(screen.queryByText("Settings")).not.toBeInTheDocument();
  });

  it("calls onNavigate with correct page when a nav item is clicked", () => {
    const onNavigate = vi.fn();
    render(<SkillsNav {...defaultProps} onNavigate={onNavigate} />);

    fireEvent.click(screen.getByText("Marketplace"));
    expect(onNavigate).toHaveBeenCalledWith("marketplace");

    fireEvent.click(screen.getByText("Projects"));
    expect(onNavigate).toHaveBeenCalledWith("projects");
  });

  it("highlights the active page with aria-current", () => {
    render(<SkillsNav {...defaultProps} activePage="projects" />);

    const projectsBtn = screen.getByText("Projects").closest("button");
    expect(projectsBtn).toHaveAttribute("aria-current", "page");
  });

  it("calls onPrefetch on mouse enter", () => {
    const onPrefetch = vi.fn();
    render(<SkillsNav {...defaultProps} onPrefetch={onPrefetch} />);

    fireEvent.mouseEnter(screen.getByText("Marketplace").closest("button")!);
    expect(onPrefetch).toHaveBeenCalledWith("marketplace");
  });

  it("in collapsed mode, does not show text labels", () => {
    render(<SkillsNav {...defaultProps} collapsed={true} />);

    expect(screen.queryByText("My Skills")).not.toBeInTheDocument();
    expect(screen.queryByText("Marketplace")).not.toBeInTheDocument();
    expect(screen.queryByText("Navigation")).not.toBeInTheDocument();
  });

  it("in collapsed mode, nav items have title attribute for tooltip", () => {
    render(<SkillsNav {...defaultProps} collapsed={true} />);

    const buttons = screen.getAllByRole("button");
    // Each nav item button should have a title in collapsed mode
    const titledButtons = buttons.filter((btn) => btn.hasAttribute("title"));
    expect(titledButtons.length).toBeGreaterThanOrEqual(4);
  });
});
