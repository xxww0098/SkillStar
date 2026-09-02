import { fireEvent, render, screen } from "@testing-library/react";
import { describe, expect, it, vi } from "vitest";
import { McpTransportPicker } from "./McpTransportPicker";

describe("McpTransportPicker", () => {
  it("labels http as Streamable HTTP and marks it recommended, not the raw token", () => {
    render(<McpTransportPicker value="stdio" onChange={vi.fn()} />);
    expect(screen.queryByRole("radio", { name: /^http$/i })).toBeNull();
    expect(screen.getByText("Streamable HTTP")).toBeInTheDocument();
    expect(screen.getByText("无状态 · 2026-07-28")).toBeInTheDocument();
    expect(screen.getByText("本地进程")).toBeInTheDocument();
    expect(screen.getByText("SSE（已弃用）")).toBeInTheDocument();
  });

  it("keeps the store value http when Streamable HTTP is chosen", () => {
    const onChange = vi.fn();
    render(<McpTransportPicker value="stdio" onChange={onChange} />);
    fireEvent.click(screen.getByRole("radio", { name: /Streamable HTTP/i }));
    expect(onChange).toHaveBeenCalledWith("http");
  });

  it("explains that SSE is a deprecated session transport", () => {
    render(<McpTransportPicker value="sse" onChange={vi.fn()} />);
    expect(screen.getByRole("radio", { name: /SSE/ })).toHaveAttribute("aria-checked", "true");
    expect(screen.getByText(/SSE 传输已弃用/)).toBeInTheDocument();
  });
});
