import { McpManager } from "../features/mcp/components/McpManager";

export interface McpProps {
  onOpenMarket: () => void;
}

/**
 * MCP mode page (Skills-mode sidebar entry).
 *
 * Hosts the unified MCP server manager for installed servers, with one-click
 * enable into each agent tool's native config.
 */
export function Mcp({ onOpenMarket }: McpProps) {
  return <McpManager onOpenMarket={onOpenMarket} />;
}
