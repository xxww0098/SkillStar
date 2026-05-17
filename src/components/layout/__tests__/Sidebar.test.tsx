import { render, screen } from "@testing-library/react";
import { describe, expect, it, vi } from "vitest";

// Mock framer-motion
vi.mock("framer-motion", () => ({
  motion: {
    aside: ({ children, className, ...props }: React.HTMLAttributes<HTMLElement>) => (
      <aside className={className} {...props}>
        {children}
      </aside>
    ),
    div: ({ children, className, ...props }: React.HTMLAttributes<HTMLDivElement>) => (
      <div className={className} {...props}>
        {children}
      </div>
    ),
    img: (props: React.ImgHTMLAttributes<HTMLImageElement>) => <img {...props} />,
    span: ({ children, className, ...props }: React.HTMLAttributes<HTMLSpanElement>) => (
      <span className={className} {...props}>
        {children}
      </span>
    ),
  },
  AnimatePresence: ({ children }: { children: React.ReactNode }) => <>{children}</>,
  useReducedMotion: () => false,
}));

// Mock react-i18next
vi.mock("react-i18next", () => ({
  useTranslation: () => ({
    t: (key: string) => key,
    i18n: { language: "zh-CN" },
  }),
}));

// Mock useNavigation hook — appMode controlled per test
let mockAppMode: "skills" | "usage" | "models" = "skills";
const mockSetAppMode = vi.fn();

vi.mock("../../../hooks/useNavigation", () => ({
  useNavigation: () => ({
    appMode: mockAppMode,
    setAppMode: mockSetAppMode,
    modelsActivePage: "providers",
    navigate: vi.fn(),
    navigateModels: vi.fn(),
    selectedProviderId: null,
    setSelectedProviderId: vi.fn(),
    setShowPresetSelector: vi.fn(),
    usageCatalogFilter: "__all__",
    setUsageCatalogFilter: vi.fn(),
    openUsageCreate: vi.fn(),
  }),
}));

// Mock backgroundStyle
vi.mock("../../../lib/backgroundStyle", () => ({
  readBackgroundStyle: () => "current",
  applyBackgroundStyle: vi.fn(),
}));

// Mock SkillsNav and ModelsNav to verify conditional rendering
vi.mock("../SkillsNav", () => ({
  SkillsNav: () => <div data-testid="skills-nav">SkillsNav</div>,
}));

vi.mock("../ModelsNav", () => ({
  ModelsNav: () => <div data-testid="models-nav">ModelsNav</div>,
}));

vi.mock("../UsageNav", () => ({
  UsageNav: () => <div data-testid="usage-nav">UsageNav</div>,
}));

// Mock ModeSwitcher
vi.mock("../ModeSwitcher", () => ({
  ModeSwitcher: ({ currentMode }: { currentMode: string }) => (
    <div data-testid="mode-switcher" data-mode={currentMode}>
      ModeSwitcher
    </div>
  ),
}));

import { Sidebar } from "../Sidebar";

describe("Sidebar", () => {
  const defaultProps = {
    activePage: "my-skills" as const,
    onNavigate: vi.fn(),
  };

  it("renders SkillsNav when appMode is 'skills'", () => {
    mockAppMode = "skills";
    render(<Sidebar {...defaultProps} />);

    expect(screen.getByTestId("skills-nav")).toBeInTheDocument();
    expect(screen.queryByTestId("models-nav")).not.toBeInTheDocument();
  });

  it("renders ModelsNav when appMode is 'models'", () => {
    mockAppMode = "models";
    render(<Sidebar {...defaultProps} />);

    expect(screen.getByTestId("models-nav")).toBeInTheDocument();
    expect(screen.queryByTestId("skills-nav")).not.toBeInTheDocument();
    expect(screen.queryByTestId("usage-nav")).not.toBeInTheDocument();
  });

  it("renders UsageNav when appMode is 'usage'", () => {
    mockAppMode = "usage";
    render(<Sidebar {...defaultProps} />);

    expect(screen.getByTestId("usage-nav")).toBeInTheDocument();
    expect(screen.queryByTestId("skills-nav")).not.toBeInTheDocument();
    expect(screen.queryByTestId("models-nav")).not.toBeInTheDocument();
  });

  it("always renders the logo regardless of mode", () => {
    mockAppMode = "skills";
    const { rerender } = render(<Sidebar {...defaultProps} />);
    expect(screen.getByAltText("SkillStar")).toBeInTheDocument();

    mockAppMode = "models";
    rerender(<Sidebar {...defaultProps} />);
    expect(screen.getByAltText("SkillStar")).toBeInTheDocument();
  });

  it("always renders the ModeSwitcher regardless of mode", () => {
    mockAppMode = "skills";
    const { rerender } = render(<Sidebar {...defaultProps} />);
    expect(screen.getByTestId("mode-switcher")).toBeInTheDocument();

    mockAppMode = "models";
    rerender(<Sidebar {...defaultProps} />);
    expect(screen.getByTestId("mode-switcher")).toBeInTheDocument();
  });

  it("passes current appMode to ModeSwitcher", () => {
    mockAppMode = "models";
    render(<Sidebar {...defaultProps} />);

    expect(screen.getByTestId("mode-switcher")).toHaveAttribute("data-mode", "models");
  });
});
