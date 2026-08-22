import { fireEvent, render, screen } from "@testing-library/react";
import { describe, expect, it, vi } from "vitest";
import type { McpSecretPolicy } from "../../../types";
import { buildCommandConfirmation, buildEnvPreview, SECRET_MASK } from "../lib/commandPreview";
import { McpCommandConfirm } from "./McpCommandConfirm";

const policy: McpSecretPolicy = {
  storage: "userLevelConfig",
  secretKeys: ["API_KEY"],
  writesProjectScopedConfig: false,
  note: "Secret values are stored in SkillStar's user-level MCP store.",
};

function renderConfirm(overrides: Partial<Parameters<typeof McpCommandConfirm>[0]> = {}) {
  const onAcknowledge = vi.fn();
  const confirmation = buildCommandConfirmation({
    preview: "npx -y @acme/server --root '/Users/dev/My Files'",
    resolvedCommandPath: "/usr/local/bin/npx",
    planPreview: "npx -y @acme/server --root '/Users/dev/My Files'",
  });

  render(
    <McpCommandConfirm
      confirmation={confirmation}
      env={buildEnvPreview({ API_KEY: "sk-secret", PORT: "8080" }, ["API_KEY"])}
      headers={[]}
      url={null}
      warnings={[]}
      secretPolicy={policy}
      acknowledged={false}
      onAcknowledge={onAcknowledge}
      requiresAcknowledgement
      {...overrides}
    />,
  );
  return { onAcknowledge };
}

describe("McpCommandConfirm", () => {
  it("shows the full command line and the binary it resolves to", () => {
    renderConfirm();

    expect(screen.getByText("npx -y @acme/server --root '/Users/dev/My Files'")).toBeInTheDocument();
    expect(screen.getByText("/usr/local/bin/npx")).toBeInTheDocument();
  });

  it("states that no shell is involved", () => {
    renderConfirm();
    expect(screen.getByText("不经 shell —— 直接执行启动器")).toBeInTheDocument();
  });

  it("gates the install behind an explicit acknowledgement for local servers", () => {
    const { onAcknowledge } = renderConfirm();

    const checkbox = screen.getByRole("checkbox");
    expect(checkbox).not.toBeChecked();
    fireEvent.click(checkbox);
    expect(onAcknowledge).toHaveBeenCalledWith(true);
  });

  it("asks for no acknowledgement when nothing local will execute", () => {
    renderConfirm({
      requiresAcknowledgement: false,
      confirmation: buildCommandConfirmation({
        preview: null,
        resolvedCommandPath: null,
        planPreview: null,
      }),
      url: "https://api.example.com/mcp",
    });

    expect(screen.queryByRole("checkbox")).toBeNull();
    expect(screen.getByText("https://api.example.com/mcp")).toBeInTheDocument();
  });

  it("masks a secret env value but keeps its key visible", () => {
    renderConfirm();

    expect(screen.getByText("API_KEY")).toBeInTheDocument();
    expect(screen.getByText(SECRET_MASK)).toBeInTheDocument();
    expect(screen.queryByText("sk-secret")).toBeNull();
  });

  it("renders every warning verbatim", () => {
    renderConfirm({ warnings: ["The registry marks this server 'deprecated'.", "SSE transport is deprecated."] });

    expect(screen.getByText("The registry marks this server 'deprecated'.")).toBeInTheDocument();
    expect(screen.getByText("SSE transport is deprecated.")).toBeInTheDocument();
  });

  it("says the command was rebuilt when the user's answers changed it", () => {
    renderConfirm({
      confirmation: buildCommandConfirmation({
        preview: "npx -y @acme/server --port 8080",
        resolvedCommandPath: "/usr/local/bin/npx",
        planPreview: "npx -y @acme/server",
      }),
    });

    expect(screen.getByText("你填写的值改变了这条命令行，上面是它最终的样子。")).toBeInTheDocument();
  });

  it("escalates the secret-policy note when a target writes project-scoped config", () => {
    renderConfirm({ secretPolicy: { ...policy, writesProjectScopedConfig: true } });

    expect(
      screen.getByText("至少有一个启用的目标把 MCP 配置放在项目内。写到那里的密钥可能进入版本控制。"),
    ).toBeInTheDocument();
  });
});
