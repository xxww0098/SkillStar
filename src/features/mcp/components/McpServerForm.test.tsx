import { fireEvent, render, screen } from "@testing-library/react";
import { describe, expect, it, vi } from "vitest";
import { McpServerForm } from "./McpServerForm";

describe("McpServerForm", () => {
  it("uses the /mcp placeholder for Streamable HTTP, not a leftover /sse URL", () => {
    render(<McpServerForm onSubmit={vi.fn()} />);
    fireEvent.click(screen.getByRole("radio", { name: /Streamable HTTP/i }));
    expect(screen.getByPlaceholderText("https://mcp.example.com/mcp")).toBeInTheDocument();
    expect(screen.queryByPlaceholderText(/\/sse$/)).toBeNull();
  });

  it("still writes transport: http when the Streamable HTTP card is selected", async () => {
    const onSubmit = vi.fn();
    render(<McpServerForm onSubmit={onSubmit} />);
    fireEvent.change(screen.getByPlaceholderText("例如 context7"), { target: { value: "demo" } });
    fireEvent.click(screen.getByRole("radio", { name: /Streamable HTTP/i }));
    fireEvent.change(screen.getByPlaceholderText("https://mcp.example.com/mcp"), {
      target: { value: "https://mcp.example.com/mcp" },
    });
    fireEvent.click(screen.getByRole("button", { name: "添加" }));
    expect(onSubmit).toHaveBeenCalledWith(expect.objectContaining({ transport: "http" }));
  });
});
