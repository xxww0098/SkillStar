import { fireEvent, render, screen } from "@testing-library/react";
import { describe, expect, it, vi } from "vitest";
import { ModeSwitcher } from "./ModeSwitcher";

vi.mock("framer-motion", () => ({
  motion: {
    div: ({ children, className, ...props }: React.HTMLAttributes<HTMLDivElement>) => (
      <div className={className} {...props}>
        {children}
      </div>
    ),
  },
  useReducedMotion: () => false,
}));

describe("ModeSwitcher", () => {
  it("renders Skills, Usage, and Models buttons", () => {
    render(<ModeSwitcher currentMode="skills" onModeChange={vi.fn()} collapsed={false} />);

    expect(screen.getByRole("button", { name: "Skills" })).toBeInTheDocument();
    expect(screen.getByRole("button", { name: "Usage" })).toBeInTheDocument();
    expect(screen.getByRole("button", { name: "Models" })).toBeInTheDocument();
  });

  it("highlights the active mode button with aria-pressed", () => {
    render(<ModeSwitcher currentMode="skills" onModeChange={vi.fn()} collapsed={false} />);

    expect(screen.getByRole("button", { name: "Skills" })).toHaveAttribute("aria-pressed", "true");
    expect(screen.getByRole("button", { name: "Models" })).toHaveAttribute("aria-pressed", "false");
  });

  it("highlights Models button when models mode is active", () => {
    render(<ModeSwitcher currentMode="models" onModeChange={vi.fn()} collapsed={false} />);

    expect(screen.getByRole("button", { name: "Skills" })).toHaveAttribute("aria-pressed", "false");
    expect(screen.getByRole("button", { name: "Models" })).toHaveAttribute("aria-pressed", "true");
  });

  it("calls onModeChange with the correct mode when clicking inactive button", () => {
    const onModeChange = vi.fn();
    render(<ModeSwitcher currentMode="skills" onModeChange={onModeChange} collapsed={false} />);

    fireEvent.click(screen.getByRole("button", { name: "Models" }));

    expect(onModeChange).toHaveBeenCalledWith("models");
    expect(onModeChange).toHaveBeenCalledTimes(1);
  });

  it("calls onModeChange when clicking the already active button", () => {
    const onModeChange = vi.fn();
    render(<ModeSwitcher currentMode="skills" onModeChange={onModeChange} collapsed={false} />);

    fireEvent.click(screen.getByRole("button", { name: "Skills" }));

    expect(onModeChange).toHaveBeenCalledWith("skills");
  });

  it("does not show text labels in collapsed state", () => {
    render(<ModeSwitcher currentMode="skills" onModeChange={vi.fn()} collapsed={true} />);

    expect(screen.queryByText("Skills")).not.toBeInTheDocument();
    expect(screen.queryByText("Models")).not.toBeInTheDocument();
  });

  it("does not show text labels in expanded state", () => {
    const { rerender } = render(<ModeSwitcher currentMode="skills" onModeChange={vi.fn()} collapsed={false} />);

    expect(screen.queryByText("Skills")).not.toBeInTheDocument();
    expect(screen.queryByText("Models")).not.toBeInTheDocument();

    rerender(<ModeSwitcher currentMode="models" onModeChange={vi.fn()} collapsed={false} />);

    expect(screen.queryByText("Skills")).not.toBeInTheDocument();
    expect(screen.queryByText("Models")).not.toBeInTheDocument();
  });
});
