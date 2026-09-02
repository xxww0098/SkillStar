import { render, screen } from "@testing-library/react";
import { describe, expect, it } from "vitest";
import type { McpServerEntry } from "../../../types";
import { McpFleetStrip } from "./McpFleetStrip";

function server(id: string): McpServerEntry {
  return {
    id,
    name: id,
    transport: "stdio",
    args: [],
    env: {},
    headers: {},
    tags: [],
    enabled: {},
    autoApproveAll: false,
    sortIndex: 0,
  } as McpServerEntry;
}

describe("McpFleetStrip", () => {
  it("treats authorization-required as a sign-in nudge, not a red failure", () => {
    render(
      <McpFleetStrip
        servers={[server("a"), server("b")]}
        entryFor={(id) =>
          id === "a"
            ? {
                report: {
                  serverId: "a",
                  serverName: "a",
                  status: "authorization-required",
                  cachePrivate: false,
                  checkedAt: 1,
                },
                error: null,
                pending: false,
              }
            : {
                report: {
                  serverId: "b",
                  serverName: "b",
                  status: "healthy",
                  cachePrivate: false,
                  schemaTokens: 1200,
                  schemaBytes: 4800,
                  checkedAt: 1,
                },
                error: null,
                pending: false,
              }
        }
      />,
    );
    expect(screen.getByText(/需要登录/)).toBeInTheDocument();
    expect(screen.getByText(/~1.2k schema token/)).toBeInTheDocument();
    expect(screen.queryByText(/失败/)).toBeNull();
  });
});
