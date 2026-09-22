import { fireEvent, render, screen } from "@testing-library/react";
import { useState } from "react";
import { describe, expect, it, vi } from "vitest";
import { mockOAuthStart } from "@/lib/ipc/devMock/usage";
import type { CatalogEntry, OAuthStart } from "../../../types";
import { OAuthLoginPanel } from "./OAuthLoginPanel";

const entry = {
  id: "codex",
  display_name: "Codex",
  description: "OpenAI Codex CLI",
  tier: "o-auth",
  auth_modes: ["o-auth"],
  brand_color: "10A37F",
  default_currency: "USD",
  subscription_url: "https://chatgpt.com",
  warning: null,
  regions: [],
} as CatalogEntry;

function Panel({ start, onSubmit = vi.fn() }: { start: OAuthStart | null; onSubmit?: () => void }) {
  const [value, setValue] = useState("");
  return (
    <OAuthLoginPanel
      selectedEntry={entry}
      submitting={false}
      oauthIsActiveMode
      oauthStart={start}
      oauthPendingId={start?.pending_id ?? null}
      oauthStatus={start ? "等待认证中…" : null}
      oauthCallbackInput={value}
      setOauthCallbackInput={setValue}
      oauthSubmittingCallback={false}
      reduceMotion
      onStartOAuth={vi.fn()}
      onCopyAuthLink={vi.fn()}
      onOpenOAuthLink={vi.fn()}
      onSubmitCallback={onSubmit}
      onCancelOAuth={vi.fn()}
    />
  );
}

describe("OAuthLoginPanel flows", () => {
  it("keeps the paste replay control for a local callback", () => {
    render(<Panel start={mockOAuthStart()} />);

    expect(screen.getByText("回调 URL 或授权码")).toBeInTheDocument();
    expect(screen.getByPlaceholderText("粘贴页面显示的 code，或完整 callback URL")).toBeInTheDocument();
    expect(screen.getByRole("button", { name: "提交回调 URL" })).toBeInTheDocument();
  });

  it("shows the user code and no paste box for a remote poll", () => {
    render(<Panel start={mockOAuthStart({ flow: "remote-poll" })} />);

    expect(screen.getByText("ABCD-EFGH")).toBeInTheDocument();
    expect(screen.getByRole("button", { name: "复制用户码" })).toBeInTheDocument();
    expect(screen.getByRole("button", { name: "打开验证页" })).toBeInTheDocument();
    expect(screen.getByText("5 秒后再次检查")).toBeInTheDocument();
    expect(screen.queryByText("回调 URL 或授权码")).not.toBeInTheDocument();
    expect(screen.queryByRole("button", { name: "提交回调 URL" })).not.toBeInTheDocument();
    expect(screen.queryByRole("textbox")).not.toBeInTheDocument();
  });

  it("rejects a scheme paste that does not start with the prefix", () => {
    const onSubmit = vi.fn();
    render(<Panel start={mockOAuthStart({ flow: "scheme-paste", scheme_prefix: "zcode://" })} onSubmit={onSubmit} />);

    fireEvent.change(screen.getByRole("textbox"), { target: { value: "https://evil.example/callback" } });

    expect(screen.getByText(/打不开/)).toBeInTheDocument();
    expect(screen.getByRole("alert")).toHaveTextContent("地址必须以 zcode:// 开头");
    expect(screen.getByRole("button", { name: "提交 URL" })).toBeDisabled();
    fireEvent.click(screen.getByRole("button", { name: "提交 URL" }));
    expect(onSubmit).not.toHaveBeenCalled();
  });

  it("renders no paste ui when the login already completed", () => {
    const { container } = render(<Panel start={mockOAuthStart({ flow: "immediate" })} />);

    expect(container).toBeEmptyDOMElement();
    expect(screen.queryByRole("textbox")).not.toBeInTheDocument();
    expect(screen.queryByText("回调 URL 或授权码")).not.toBeInTheDocument();
  });
});

describe("mockOAuthStart", () => {
  it("defaults to local-callback and previews the other flows without a catalog", () => {
    expect(mockOAuthStart().flow).toBe("local-callback");
    expect(mockOAuthStart({ flow: "remote-poll" }).user_code).toBe("ABCD-EFGH");
    expect(mockOAuthStart({ flow: "immediate" }).flow).toBe("immediate");
    expect(mockOAuthStart({ flow: "scheme-paste", scheme_prefix: "zcode://" }).flow).toEqual({
      "scheme-paste": { scheme_prefix: "zcode://" },
    });
  });
});
