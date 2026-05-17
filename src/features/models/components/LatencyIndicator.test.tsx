import { cleanup, fireEvent, render, screen } from "@testing-library/react";
import { afterEach, describe, expect, it, vi } from "vitest";
import { LatencyIndicator } from "./LatencyIndicator";

afterEach(cleanup);

describe("LatencyIndicator - dot variant", () => {
  it("renders green dot for latency < 500ms", () => {
    const { container } = render(<LatencyIndicator latencyMs={200} variant="dot" />);
    const dot = container.querySelector("span");
    expect(dot).toHaveClass("bg-emerald-400");
    expect(dot).toHaveClass("w-1.5", "h-1.5", "rounded-full");
  });

  it("renders yellow dot for latency 500–2000ms", () => {
    const { container } = render(<LatencyIndicator latencyMs={1000} variant="dot" />);
    const dot = container.querySelector("span");
    expect(dot).toHaveClass("bg-amber-400");
  });

  it("renders red dot for latency > 2000ms", () => {
    const { container } = render(<LatencyIndicator latencyMs={3000} variant="dot" />);
    const dot = container.querySelector("span");
    expect(dot).toHaveClass("bg-red-400");
  });

  it("renders gray dot for untested (null)", () => {
    const { container } = render(<LatencyIndicator latencyMs={null} variant="dot" />);
    const dot = container.querySelector("span");
    expect(dot).toHaveClass("bg-muted-foreground/40");
  });

  it("renders gray dot for untested (undefined)", () => {
    const { container } = render(<LatencyIndicator latencyMs={undefined} variant="dot" />);
    const dot = container.querySelector("span");
    expect(dot).toHaveClass("bg-muted-foreground/40");
  });

  it("defaults to dot variant when variant prop is omitted", () => {
    const { container } = render(<LatencyIndicator latencyMs={100} />);
    const dot = container.querySelector("span");
    expect(dot).toHaveClass("w-1.5", "h-1.5", "rounded-full", "bg-emerald-400");
  });
});

describe("LatencyIndicator - full variant", () => {
  it("shows '正常' status with latency for healthy connection", () => {
    render(<LatencyIndicator latencyMs={234} variant="full" />);
    expect(screen.getByText(/正常 · 234ms/)).toBeInTheDocument();
  });

  it("shows '未测试' for null latency", () => {
    render(<LatencyIndicator latencyMs={null} variant="full" />);
    expect(screen.getByText(/未测试/)).toBeInTheDocument();
  });

  it("shows '超时' for latency > 2000ms", () => {
    render(<LatencyIndicator latencyMs={5000} variant="full" />);
    expect(screen.getByText(/超时/)).toBeInTheDocument();
  });

  it("shows '网络错误' for negative latency (error indicator)", () => {
    render(<LatencyIndicator latencyMs={-1} variant="full" />);
    expect(screen.getByText(/网络错误/)).toBeInTheDocument();
  });

  it("displays last tested timestamp when provided", () => {
    const recentTime = new Date(Date.now() - 5 * 60000).toISOString(); // 5 minutes ago
    render(<LatencyIndicator latencyMs={200} variant="full" lastTestedAt={recentTime} />);
    expect(screen.getByText(/5 分钟前/)).toBeInTheDocument();
  });

  it("does not display timestamp when lastTestedAt is null", () => {
    const { container } = render(<LatencyIndicator latencyMs={200} variant="full" lastTestedAt={null} />);
    expect(container.querySelectorAll("span")).toHaveLength(2); // color dot + status text only
  });

  it("renders '再次测试' button when onRetest is provided", () => {
    const onRetest = vi.fn();
    render(<LatencyIndicator latencyMs={200} variant="full" onRetest={onRetest} />);
    const button = screen.getByText("再次测试");
    expect(button).toBeInTheDocument();
    fireEvent.click(button);
    expect(onRetest).toHaveBeenCalledOnce();
  });

  it("does not render '再次测试' button when onRetest is not provided", () => {
    render(<LatencyIndicator latencyMs={200} variant="full" />);
    expect(screen.queryByText("再次测试")).not.toBeInTheDocument();
  });

  it("shows yellow status for boundary value 500ms", () => {
    render(<LatencyIndicator latencyMs={500} variant="full" />);
    expect(screen.getByText(/正常 · 500ms/)).toBeInTheDocument();
    const { container } = render(<LatencyIndicator latencyMs={500} variant="full" />);
    const dot = container.querySelector("span");
    expect(dot).toHaveClass("bg-amber-400");
  });
});
