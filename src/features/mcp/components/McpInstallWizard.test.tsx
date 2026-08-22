import { QueryClient, QueryClientProvider } from "@tanstack/react-query";
import { fireEvent, render, screen, waitFor } from "@testing-library/react";
import { invoke } from "@tauri-apps/api/core";
import { beforeEach, describe, expect, it, vi } from "vitest";
import type { McpInstallInput, McpInstallOutcome, McpInstallPlan, McpInstallPreview } from "../../../types";
import { McpInstallWizard } from "./McpInstallWizard";

/**
 * The two properties the wizard owes the confirmation step, both of which it
 * used to break silently.
 *
 * 1. A refused submit has to re-read the plan, not just the preview. Answers
 *    are addressed by `(scope, ordinal)`, so a form still seeded from the old
 *    declaration re-binds the user's values onto whatever the new row put at
 *    those ordinals.
 * 2. A form with a blank required field has to say *which* field. A disabled
 *    button is not that.
 */

const SERVER_ID = "acme-x";

function inputFor(patch: Partial<McpInstallInput> = {}): McpInstallInput {
  return {
    key: "--port",
    scope: "packageArgument",
    index: 0,
    input: { isRequired: false, isSecret: false, format: "string", default: "3000" },
    prefilled: "3000",
    mustAsk: false,
    ...patch,
  };
}

function planFor(inputs: McpInstallInput[], args: string[]): McpInstallPlan {
  return {
    serverId: SERVER_ID,
    serverName: "acme-x",
    namespace: "io.github.acme/x",
    selection: { serverId: SERVER_ID, candidates: [], recommendedId: "package:0" },
    selectedRuntimeId: "package:0",
    transport: "stdio",
    command: "npx",
    args,
    resolvedCommandPath: "/usr/local/bin/npx",
    commandPreview: `npx ${args.join(" ")}`,
    usesShell: false,
    inputs,
    secretPolicy: {
      storage: "userLevelConfig",
      secretKeys: [],
      writesProjectScopedConfig: false,
      note: "This server declares no secret inputs.",
    },
    warnings: [],
    draft: {
      id: "",
      name: "acme-x",
      transport: "stdio",
      command: "npx",
      args,
      enabled: {},
      autoApproveAll: false,
      sortIndex: 0,
    },
  };
}

function previewFor(plan: McpInstallPlan): McpInstallPreview {
  return {
    entry: plan.draft,
    commandPreview: plan.commandPreview,
    approvalTarget: `target:${plan.commandPreview}`,
    missing: [],
  };
}

/** The row the user opened: one named `--port`, defaulted to 3000. */
const V1 = planFor([inputFor()], ["-y", "@acme/x@1.2.0", "--port", "3000"]);

/**
 * The row after a registry sync: a required positional was inserted *ahead* of
 * `--port`, so ordinal 0 no longer means the same thing.
 */
const V2 = planFor(
  [
    inputFor({
      key: "argument",
      index: 0,
      input: { isRequired: true, isSecret: false, format: "string" },
      prefilled: "",
      mustAsk: true,
    }),
    inputFor({ index: 1 }),
  ],
  ["-y", "@acme/x@1.2.0", "--port", "3000"],
);

function renderWizard(onSubmit: (submission: unknown) => Promise<McpInstallOutcome>) {
  const client = new QueryClient({ defaultOptions: { queries: { retry: false } } });
  render(
    <QueryClientProvider client={client}>
      <McpInstallWizard
        serverId={SERVER_ID}
        submitting={false}
        onSubmit={onSubmit as Parameters<typeof McpInstallWizard>[0]["onSubmit"]}
      />
    </QueryClientProvider>,
  );
}

/** Route each command to the plan currently on the shelf. */
function serveFrom(plan: () => McpInstallPlan, outcome: () => McpInstallOutcome) {
  vi.mocked(invoke).mockImplementation((command: string) => {
    switch (command) {
      case "mcp_market_install_plan":
        return Promise.resolve(plan());
      case "mcp_market_install_preview":
        return Promise.resolve(previewFor(plan()));
      case "mcp_market_install":
        return Promise.resolve(outcome());
      default:
        return Promise.resolve(undefined);
    }
  });
}

async function approveAndSubmit() {
  fireEvent.click(screen.getByRole("checkbox"));
  // The command preview must have landed too, or submit would approve a stale
  // command; the wizard blocks the button until it does.
  await waitFor(() => expect(screen.getByRole("button", { name: "确认并安装" })).toBeEnabled());
  fireEvent.click(screen.getByRole("button", { name: "确认并安装" }));
}

describe("McpInstallWizard", () => {
  beforeEach(() => {
    vi.mocked(invoke).mockReset();
  });

  it("re-reads the plan after a commandChanged refusal, so answers stop binding to the old ordinals", async () => {
    let current = V1;
    const onSubmit = vi.fn(async (): Promise<McpInstallOutcome> => {
      // The row is rewritten under the user, exactly as a registry sync would.
      current = V2;
      return { status: "rejected", rejection: { reason: "commandChanged" } };
    });
    serveFrom(
      () => current,
      () => ({ status: "rejected", rejection: { reason: "commandChanged" } }),
    );

    renderWizard(onSubmit);
    await waitFor(() => expect(screen.getByLabelText(/--port/)).toHaveValue("3000"));
    await approveAndSubmit();

    await waitFor(() => expect(screen.getByText(/目录条目发生了变化/)).toBeInTheDocument());
    // The new row's positional must appear as its own control. Without the
    // refetch the form still shows one box, and the value the user typed for
    // `--port` is submitted as `(packageArgument, 0)` — the positional.
    await waitFor(() => expect(screen.getByLabelText(/argument/)).toBeInTheDocument());
    expect(screen.getByLabelText(/--port/)).toHaveValue("3000");
  });

  it("names the blank required field instead of only disabling the button", async () => {
    serveFrom(
      () => V2,
      () => ({ status: "rejected", rejection: { reason: "commandChanged" } }),
    );
    const onSubmit = vi.fn();

    renderWizard(onSubmit as never);
    await waitFor(() => expect(screen.getByLabelText(/argument/)).toBeInTheDocument());
    fireEvent.click(screen.getByRole("checkbox"));

    const submit = await waitFor(() => {
      const button = screen.getByRole("button", { name: "确认并安装" });
      expect(button).toBeEnabled();
      return button;
    });
    expect(screen.queryByText("必填")).toBeNull();

    fireEvent.click(submit);

    expect(screen.getByText("必填")).toBeInTheDocument();
    expect(onSubmit).not.toHaveBeenCalled();
  });
});
