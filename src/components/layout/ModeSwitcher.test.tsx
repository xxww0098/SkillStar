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
  it("renders Skills and Usage buttons", () => {
    render(<ModeSwitcher currentMode="skills" onModeChange={vi.fn()} collapsed={false} />);

    expect(screen.getByRole("button", { name: "Skills" })).toBeInTheDocument();
    expect(screen.getByRole("button", { name: "Usage" })).toBeInTheDocument();
  });

  it("highlights the active mode button with aria-pressed", () => {
    render(<ModeSwitcher currentMode="skills" onModeChange={vi.fn()} collapsed={false} />);

    expect(screen.getByRole("button", { name: "Skills" })).toHaveAttribute("aria-pressed", "true");
    expect(screen.getByRole("button", { name: "Usage" })).toHaveAttribute("aria-pressed", "false");
  });

  it("highlights Usage button when usage mode is active", () => {
    render(<ModeSwitcher currentMode="usage" onModeChange={vi.fn()} collapsed={false} />);

    expect(screen.getByRole("button", { name: "Skills" })).toHaveAttribute("aria-pressed", "false");
    expect(screen.getByRole("button", { name: "Usage" })).toHaveAttribute("aria-pressed", "true");
  });

  it("calls onModeChange with the correct mode when clicking inactive button", () => {
    const onModeChange = vi.fn();
    render(<ModeSwitcher currentMode="skills" onModeChange={onModeChange} collapsed={false} />);

    fireEvent.click(screen.getByRole("button", { name: "Usage" }));

    expect(onModeChange).toHaveBeenCalledWith("usage");
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
    expect(screen.queryByText("Usage")).not.toBeInTheDocument();
  });

  it("does not show text labels in expanded state", () => {
    const { rerender } = render(<ModeSwitcher currentMode="skills" onModeChange={vi.fn()} collapsed={false} />);

    expect(screen.queryByText("Skills")).not.toBeInTheDocument();
    expect(screen.queryByText("Usage")).not.toBeInTheDocument();

    rerender(<ModeSwitcher currentMode="usage" onModeChange={vi.fn()} collapsed={false} />);

    expect(screen.queryByText("Skills")).not.toBeInTheDocument();
    expect(screen.queryByText("Usage")).not.toBeInTheDocument();
  });
});
