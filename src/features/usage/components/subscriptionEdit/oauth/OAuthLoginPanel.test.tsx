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

const translation = { t: (key: string) => key };
vi.mock("react-i18next", () => ({
  useTranslation: () => translation,
}));

describe("OAuthLoginPanel flows", () => {
  it("keeps the paste replay control for a local callback", () => {
    render(<Panel start={mockOAuthStart()} />);

    expect(screen.getByText("usage.oauthCallbackLabel")).toBeInTheDocument();
    expect(screen.getByPlaceholderText("usage.oauthCallbackPlaceholder")).toBeInTheDocument();
    expect(screen.getByRole("button", { name: "usage.oauthSubmitCallback" })).toBeInTheDocument();
  });

  it("shows the user code and no paste box for a remote poll", () => {
    render(<Panel start={mockOAuthStart({ flow: "remote-poll" })} />);

    expect(screen.getByText("ABCD-EFGH")).toBeInTheDocument();
    expect(screen.getByRole("button", { name: "usage.oauthCopyUserCode" })).toBeInTheDocument();
    expect(screen.getByRole("button", { name: "usage.oauthOpenVerification" })).toBeInTheDocument();
    expect(screen.getByText("usage.oauthPollCountdown")).toBeInTheDocument();
    expect(screen.queryByText("usage.oauthCallbackLabel")).not.toBeInTheDocument();
    expect(screen.queryByRole("button", { name: "usage.oauthSubmitCallback" })).not.toBeInTheDocument();
    expect(screen.queryByRole("textbox")).not.toBeInTheDocument();
  });

  it("rejects a scheme paste that does not start with the prefix", () => {
    const onSubmit = vi.fn();
    render(<Panel start={mockOAuthStart({ flow: "scheme-paste", scheme_prefix: "zcode://" })} onSubmit={onSubmit} />);

    fireEvent.change(screen.getByRole("textbox"), { target: { value: "https://evil.example/callback" } });

    expect(screen.getByRole("alert")).toHaveTextContent("usage.oauthSchemePrefixMismatch");
    expect(screen.getByRole("button", { name: "usage.oauthSubmitScheme" })).toBeDisabled();
    fireEvent.click(screen.getByRole("button", { name: "usage.oauthSubmitScheme" }));
    expect(onSubmit).not.toHaveBeenCalled();
  });

  it("renders no paste ui when the login already completed", () => {
    const { container } = render(<Panel start={mockOAuthStart({ flow: "immediate" })} />);

    expect(container).toBeEmptyDOMElement();
    expect(screen.queryByRole("textbox")).not.toBeInTheDocument();
    expect(screen.queryByText("usage.oauthCallbackLabel")).not.toBeInTheDocument();
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
