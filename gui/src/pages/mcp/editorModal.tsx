import { Overlay, Scroll, Text, View, effect, signal, type JSX, type TextStyle } from "nsh";
import { Input } from "nsh/base";
import { AgentIcon } from "../../chrome/agentRail";
import { Alert } from "../../chrome/alerts";
import { SHEET_SHADOW, promptVeil } from "../../chrome/dialog";
import { Icon } from "../../chrome/icons";
import { fadeTransition } from "../../chrome/motion";
import { paletteToken } from "../../services/themeMode";
import { t } from "../../services/translation";
import { BackupsModalBody, ImportConflictModalBody } from "./backupsModal";
import {
  formatShellArgs,
  parseEnvLines,
  parseHeaderLines,
  parseShellArgs,
  parseTimeoutSec,
  validateServerDraft,
} from "./editorValidation";
import { ms } from "./mcpStrings";
import type { McpState } from "./state";
import type { McpAgentTarget, McpServerRecord, McpTransport, StoreInputSpec } from "./types";

const MODAL_SCROLL_MAX_H = 420;

export function McpModalHost(props: { state: McpState }): JSX.Element {
  return (
    <If when={props.state.modal() !== null}>
      <Overlay
        style={{
          position: "fixed",
          top: 0,
          left: 0,
          right: 0,
          bottom: 0,
          zIndex: 55,
        }}
        onCancel={() => props.state.modal.set(null)}
        onPress={() => props.state.modal.set(null)}
      >
        <View
          testID="mcp-modal-backdrop"
          style={{
            position: "absolute",
            top: 0,
            left: 0,
            right: 0,
            bottom: 0,
            backgroundColor: promptVeil(),
          }}
        />
        <View
          style={{
            position: "absolute",
            top: 0,
            left: 0,
            right: 0,
            bottom: 0,
            display: "flex",
            alignItems: "center",
            justifyContent: "center",
            padding: 24,
          }}
        >
          <View
            style={{
              width: props.state.modal()?.kind === "backups" ? 640 : 560,
              maxWidth: "100%",
              maxHeight: "90%",
              borderRadius: 16,
              borderWidth: 1,
              borderColor: paletteToken("border"),
              backgroundColor: paletteToken("panel"),
              boxShadow: SHEET_SHADOW,
              display: "flex",
              flexDirection: "column",
              overflow: "hidden",
              transform: "translateY(0)",
              ...fadeTransition(true),
            }}
            onPress={() => {
              // Swallow press inside card so backdrop click does not dismiss.
            }}
          >
            <If when={props.state.modal()?.kind === "paste"}>
              <PasteModalBody state={props.state} />
            </If>
            <If when={props.state.modal()?.kind === "editor"}>
              <EditorModalBody state={props.state} />
            </If>
            <If when={props.state.modal()?.kind === "storeInstall"}>
              <StoreInstallModalBody state={props.state} />
            </If>
            <If when={props.state.modal()?.kind === "importConflict"}>
              <ImportConflictModalBody state={props.state} />
            </If>
            <If when={props.state.modal()?.kind === "backups"}>
              <BackupsModalBody state={props.state} />
            </If>
          </View>
        </View>
      </Overlay>
    </If>
  );
}

function ModalHeader(props: {
  title: () => string;
  closeTestID?: string;
  onClose: () => void;
}): JSX.Element {
  return (
    <View
      style={{
        display: "flex",
        flexDirection: "row",
        alignItems: "center",
        justifyContent: "space-between",
        gap: 12,
        paddingHorizontal: 18,
        paddingTop: 16,
        paddingBottom: 12,
        borderBottomWidth: 1,
        borderBottomColor: paletteToken("border_soft"),
        flexShrink: 0,
      }}
    >
      <Text
        style={{
          fontSize: 15,
          fontWeight: "600",
          color: paletteToken("fg"),
          flex: 1,
          minWidth: 0,
          numberOfLines: 1,
          textOverflow: "ellipsis",
        }}
      >
        {props.title()}
      </Text>
      <View
        testID={props.closeTestID}
        role="button"
        accessibilityLabel={t("common.close")}
        style={{
          display: "flex",
          alignItems: "center",
          justifyContent: "center",
          width: 26,
          height: 26,
          borderRadius: 7,
          cursor: "pointer",
          flexShrink: 0,
        }}
        hoverStyle={{ backgroundColor: paletteToken("panel_hover") }}
        onPress={props.onClose}
      >
        <Icon name="x" size={15} color={paletteToken("fg_muted")} />
      </View>
    </View>
  );
}

function ModalFooter(props: {
  submitTestID: string;
  submitLabel: () => string;
  onCancel: () => void;
  onSubmit: () => void;
}): JSX.Element {
  return (
    <View
      style={{
        display: "flex",
        flexDirection: "row",
        alignItems: "center",
        justifyContent: "flex-end",
        gap: 8,
        paddingHorizontal: 18,
        paddingVertical: 12,
        borderTopWidth: 1,
        borderTopColor: paletteToken("border_soft"),
        backgroundColor: paletteToken("well"),
        flexShrink: 0,
      }}
    >
      <View
        role="button"
        style={{
          display: "flex",
          alignItems: "center",
          justifyContent: "center",
          height: 30,
          paddingHorizontal: 14,
          borderRadius: 8,
          borderWidth: 1,
          borderColor: paletteToken("border"),
          backgroundColor: paletteToken("card"),
          cursor: "pointer",
        }}
        hoverStyle={{ backgroundColor: paletteToken("panel_hover"), borderColor: paletteToken("edge") }}
        onPress={props.onCancel}
      >
        <Text style={{ fontSize: 12, fontWeight: "500", color: paletteToken("fg") }}>
          {t("common.cancel")}
        </Text>
      </View>
      <View
        testID={props.submitTestID}
        role="button"
        style={{
          display: "flex",
          alignItems: "center",
          justifyContent: "center",
          height: 30,
          paddingHorizontal: 14,
          borderRadius: 8,
          backgroundColor: paletteToken("accent"),
          cursor: "pointer",
        }}
        hoverStyle={{ backgroundColor: paletteToken("accent_hover") }}
        onPress={props.onSubmit}
      >
        <Text style={{ fontSize: 12, fontWeight: "600", color: paletteToken("on_accent") }}>
          {props.submitLabel()}
        </Text>
      </View>
    </View>
  );
}

function PasteModalBody(props: { state: McpState }): JSX.Element {
  const rawVal = () => {
    const m = props.state.modal();
    return m?.kind === "paste" ? m.raw : "";
  };
  const errVal = () => {
    const m = props.state.modal();
    return m?.kind === "paste" ? m.error : null;
  };
  return (
    <View style={{ display: "flex", flexDirection: "column" }}>
      <ModalHeader
        title={() => ms().modalPasteTitle}
        closeTestID="mcp-modal-close"
        onClose={() => props.state.modal.set(null)}
      />
      <View style={{ display: "flex", flexDirection: "column", padding: 18, gap: 10 }}>
        <Input
          testID="mcp-paste-input"
          multiline
          value={rawVal()}
          placeholder={ms().modalPastePlaceholder}
          onChange={(e: { value: string }) =>
            props.state.modal.set({ kind: "paste", raw: e.value, error: null })
          }
          style={{
            minHeight: 120,
            padding: 10,
            fontSize: 12,
            fontFamily: "monospace",
            borderRadius: 10,
            borderWidth: 1,
            borderColor: paletteToken("border"),
            backgroundColor: paletteToken("bg"),
            color: paletteToken("fg"),
          }}
        />
        <If when={errVal() !== null}>
          <Text style={{ fontSize: 11, color: paletteToken("danger") }}>{errVal() ?? ""}</Text>
        </If>
      </View>
      <ModalFooter
        submitTestID="mcp-paste-submit"
        submitLabel={() => ms().modalPasteParseBtn}
        onCancel={() => props.state.modal.set(null)}
        onSubmit={() => void props.state.applyPaste(rawVal())}
      />
    </View>
  );
}

function EditorModalBody(props: { state: McpState }): JSX.Element {
  const modalVal = () => {
    const m = props.state.modal();
    return m?.kind === "editor" ? m : null;
  };
  const validationError = signal<string | null>(null);
  const updateDraft = (patch: Partial<McpServerRecord>) => {
    const m = modalVal();
    if (m) {
      validationError.set(null);
      props.state.modal.set({
        kind: "editor",
        originalName: m.originalName,
        draft: { ...m.draft, ...patch },
      });
    }
  };
  const argsText = signal("");
  const envText = signal("");
  const headersText = signal("");
  const timeoutText = signal("");

  // Track the opened server identity + initial collection references so typing
  // into name/command/url/transport never overwrites in-progress text helpers.
  let lastSeed: {
    originalName: string | null;
    argsRef: string[];
    envRef: Record<string, string>;
    headersRef: Record<string, string>;
    timeoutVal: number | undefined;
  } | null = null;

  effect(() => {
    const m = modalVal();
    if (!m) {
      lastSeed = null;
      validationError.set(null);
      return;
    }
    const changed =
      lastSeed === null ||
      lastSeed.originalName !== m.originalName ||
      lastSeed.argsRef !== m.draft.args ||
      lastSeed.envRef !== m.draft.env ||
      lastSeed.headersRef !== m.draft.headers ||
      lastSeed.timeoutVal !== m.draft.startupTimeoutSec;
    if (!changed) return;

    lastSeed = {
      originalName: m.originalName,
      argsRef: m.draft.args,
      envRef: m.draft.env,
      headersRef: m.draft.headers,
      timeoutVal: m.draft.startupTimeoutSec,
    };
    argsText.set(formatShellArgs(m.draft.args));
    envText.set(
      Object.entries(m.draft.env)
        .map(([k, v]) => `${k}=${v}`)
        .join("\n"),
    );
    headersText.set(
      Object.entries(m.draft.headers)
        .map(([k, v]) => `${k}: ${v}`)
        .join("\n"),
    );
    timeoutText.set(
      m.draft.startupTimeoutSec !== undefined ? String(m.draft.startupTimeoutSec) : "",
    );
    validationError.set(null);
  });

  const submitSave = () => {
    const m = modalVal();
    if (!m) return;
    const draftErr = validateServerDraft(m.draft);
    if (draftErr) {
      validationError.set(draftErr);
      return;
    }

    const argsRes = parseShellArgs(argsText());
    if (argsRes.error) {
      validationError.set(argsRes.error);
      return;
    }
    const envRes = parseEnvLines(envText());
    if (envRes.error) {
      validationError.set(envRes.error);
      return;
    }
    const hdrRes = parseHeaderLines(headersText());
    if (hdrRes.error) {
      validationError.set(hdrRes.error);
      return;
    }
    const timeoutRes = parseTimeoutSec(timeoutText());
    if (timeoutRes.error) {
      validationError.set(timeoutRes.error);
      return;
    }

    validationError.set(null);
    void props.state.saveEditedServer(
      {
        ...m.draft,
        name: m.draft.name.trim(),
        command: m.draft.command.trim(),
        url: m.draft.url.trim(),
        args: argsRes.args,
        env: envRes.env,
        headers: hdrRes.headers,
        startupTimeoutSec: timeoutRes.timeout,
      },
      m.originalName,
    );
  };

  return (
    <View style={{ display: "flex", flexDirection: "column" }}>
      <ModalHeader
        title={() => (modalVal()?.originalName !== null ? ms().modalEditTitle : ms().modalNewTitle)}
        closeTestID="mcp-editor-close"
        onClose={() => props.state.modal.set(null)}
      />

      <Scroll
        style={{ maxHeight: MODAL_SCROLL_MAX_H }}
        contentStyle={{ display: "flex", flexDirection: "column", padding: 18, gap: 12 }}
      >
        <View
          style={{
            display: "flex",
            flexDirection: "row",
            alignItems: "flex-end",
            gap: 12,
          }}
        >
          <View style={{ display: "flex", flexDirection: "column", gap: 5, flex: 1, minWidth: 0 }}>
            <FieldLabel text={() => ms().modalNameLabel} />
            <Input
              testID="mcp-editor-name"
              value={modalVal()?.draft.name ?? ""}
              onChange={(e: { value: string }) => updateDraft({ name: e.value })}
              style={fieldStyle(true)}
            />
          </View>

          <View style={{ display: "flex", flexDirection: "column", gap: 5, flexShrink: 0 }}>
            <FieldLabel text={() => ms().modalTransportLabel} />
            <View
              style={{
                display: "flex",
                flexDirection: "row",
                alignItems: "center",
                gap: 3,
                padding: 2,
                height: 32,
                borderRadius: 9,
                borderWidth: 1,
                borderColor: paletteToken("border"),
                backgroundColor: paletteToken("bg"),
              }}
            >
              {(["stdio", "http", "sse"] as McpTransport[]).map((tr) => (
                <View
                  testID={`mcp-editor-transport-${tr}`}
                  role="button"
                  style={{
                    display: "flex",
                    alignItems: "center",
                    justifyContent: "center",
                    height: 26,
                    paddingHorizontal: 10,
                    borderRadius: 7,
                    borderWidth: 1,
                    borderColor:
                      modalVal()?.draft.transport === tr
                        ? paletteToken("accent_soft_edge")
                        : "transparent",
                    backgroundColor:
                      modalVal()?.draft.transport === tr
                        ? paletteToken("accent_soft")
                        : "transparent",
                    cursor: "pointer",
                  }}
                  hoverStyle={
                    modalVal()?.draft.transport === tr
                      ? undefined
                      : { backgroundColor: paletteToken("panel_hover") }
                  }
                  onPress={() => updateDraft({ transport: tr })}
                >
                  <Text
                    style={{
                      fontSize: 11,
                      fontWeight: "600",
                      fontFamily: "monospace",
                      color:
                        modalVal()?.draft.transport === tr
                          ? paletteToken("accent_fg")
                          : paletteToken("fg_muted"),
                    }}
                  >
                    {tr.toUpperCase()}
                  </Text>
                </View>
              ))}
            </View>
          </View>
        </View>

        <If when={modalVal()?.draft.transport === "stdio"}>
          <View style={{ display: "flex", flexDirection: "column", gap: 10 }}>
            <View style={{ display: "flex", flexDirection: "column", gap: 5 }}>
              <FieldLabel text={() => ms().modalCommandLabel} />
              <Input
                testID="mcp-editor-command"
                value={modalVal()?.draft.command ?? ""}
                onChange={(e: { value: string }) => updateDraft({ command: e.value })}
                style={fieldStyle(true)}
              />
            </View>
            <View style={{ display: "flex", flexDirection: "column", gap: 5 }}>
              <FieldLabel text={() => ms().modalArgsLabel} />
              <Input
                testID="mcp-editor-args"
                multiline
                value={argsText()}
                onChange={(e: { value: string }) => {
                  validationError.set(null);
                  argsText.set(e.value);
                }}
                style={fieldStyle(true, true)}
              />
            </View>
            <View style={{ display: "flex", flexDirection: "column", gap: 5 }}>
              <FieldLabel text={() => ms().modalEnvLabel} />
              <Input
                testID="mcp-editor-env"
                multiline
                value={envText()}
                onChange={(e: { value: string }) => {
                  validationError.set(null);
                  envText.set(e.value);
                }}
                style={fieldStyle(true, true)}
              />
            </View>
          </View>
        </If>

        <If when={modalVal()?.draft.transport !== "stdio"}>
          <View style={{ display: "flex", flexDirection: "column", gap: 10 }}>
            <View style={{ display: "flex", flexDirection: "column", gap: 5 }}>
              <FieldLabel text={() => ms().modalUrlLabel} />
              <Input
                testID="mcp-editor-url"
                value={modalVal()?.draft.url ?? ""}
                onChange={(e: { value: string }) => {
                  validationError.set(null);
                  updateDraft({ url: e.value });
                }}
                style={fieldStyle(true)}
              />
            </View>
            <View style={{ display: "flex", flexDirection: "column", gap: 5 }}>
              <FieldLabel text={() => ms().modalHeadersLabel} />
              <Input
                testID="mcp-editor-headers"
                multiline
                value={headersText()}
                onChange={(e: { value: string }) => {
                  validationError.set(null);
                  headersText.set(e.value);
                }}
                style={fieldStyle(true, true)}
              />
            </View>
          </View>
        </If>

        <View style={{ display: "flex", flexDirection: "column", gap: 5 }}>
          <FieldLabel text={() => ms().modalTimeoutLabel} />
          <Input
            testID="mcp-editor-timeout"
            value={timeoutText()}
            placeholder="30"
            onChange={(e: { value: string }) => {
              validationError.set(null);
              timeoutText.set(e.value);
            }}
            style={fieldStyle(true)}
          />
        </View>
      </Scroll>

      <If when={validationError() !== null}>
        <View testID="mcp-editor-error" style={{ paddingHorizontal: 18, paddingTop: 10 }}>
          <Alert tone="error" message={validationError() ?? ""} />
        </View>
      </If>

      <ModalFooter
        submitTestID="mcp-editor-save"
        submitLabel={() => ms().modalSaveBtn}
        onCancel={() => props.state.modal.set(null)}
        onSubmit={submitSave}
      />
    </View>
  );
}

function StoreInstallModalBody(props: { state: McpState }): JSX.Element {
  const modalVal = () => {
    const m = props.state.modal();
    return m?.kind === "storeInstall" ? m : null;
  };
  return (
    <View style={{ display: "flex", flexDirection: "column" }}>
      <ModalHeader
        title={() => `${ms().storeInstallModalTitle}: ${modalVal()?.item.title ?? ""}`}
        closeTestID="mcp-store-install-close"
        onClose={() => props.state.modal.set(null)}
      />

      <Scroll
        style={{ maxHeight: MODAL_SCROLL_MAX_H }}
        contentStyle={{ display: "flex", flexDirection: "column", padding: 18, gap: 12 }}
      >
        <View style={{ display: "flex", flexDirection: "column", gap: 5 }}>
          <FieldLabel text={() => ms().storeNameLabel} />
          <Input
            testID="mcp-store-install-name"
            value={modalVal()?.customName ?? ""}
            onChange={(e: { value: string }) => {
              const m = modalVal();
              if (m) props.state.modal.set({ ...m, customName: e.value });
            }}
            style={fieldStyle(true)}
          />
        </View>

        <Each of={modalVal()?.item.inputs ?? []} key={(inp: StoreInputSpec) => inp.key}>
          {(inp: () => StoreInputSpec) => (
            <View style={{ display: "flex", flexDirection: "column", gap: 4 }}>
              <Text style={{ fontSize: 11, fontWeight: "600", color: paletteToken("fg") }}>
                {`${inp().label}${inp().required ? " *" : ""}`}
              </Text>
              <If when={inp().description.length > 0}>
                <Text style={{ fontSize: 10, color: paletteToken("fg_muted") }}>
                  {inp().description}
                </Text>
              </If>
              <Input
                testID={`mcp-store-input-${inp().key}`}
                masked={inp().secret}
                value={modalVal()?.values[inp().key] ?? ""}
                placeholder={inp().placeholder}
                onChange={(e: { value: string }) => {
                  const m = modalVal();
                  if (m) {
                    props.state.modal.set({
                      ...m,
                      error: null,
                      values: { ...m.values, [inp().key]: e.value },
                    });
                  }
                }}
                style={fieldStyle()}
              />
            </View>
          )}
        </Each>

        <View style={{ display: "flex", flexDirection: "column", gap: 6 }}>
          <FieldLabel text={() => ms().storeAgentsLabel} />
          <View style={{ display: "flex", flexDirection: "row", flexWrap: "wrap", gap: 6 }}>
            <Each of={props.state.visibleAgents()} key={(a: McpAgentTarget) => a.id}>
              {(agent: () => McpAgentTarget) => (
                <View
                  testID={`mcp-store-agent-${agent().id}`}
                  role="button"
                  style={{
                    display: "flex",
                    flexDirection: "row",
                    alignItems: "center",
                    gap: 5,
                    paddingHorizontal: 8,
                    paddingVertical: 5,
                    borderRadius: 8,
                    borderWidth: 1,
                    borderColor: modalVal()?.agents.includes(agent().id)
                      ? paletteToken("accent_soft_edge")
                      : paletteToken("border"),
                    backgroundColor: modalVal()?.agents.includes(agent().id)
                      ? paletteToken("accent_soft")
                      : paletteToken("bg"),
                    cursor: "pointer",
                  }}
                  hoverStyle={
                    modalVal()?.agents.includes(agent().id)
                      ? undefined
                      : { backgroundColor: paletteToken("panel_hover"), borderColor: paletteToken("edge") }
                  }
                  onPress={() => {
                    const m = modalVal();
                    if (!m) return;
                    const has = m.agents.includes(agent().id);
                    const next = has
                      ? m.agents.filter((a) => a !== agent().id)
                      : [...m.agents, agent().id];
                    props.state.modal.set({ ...m, error: null, agents: next });
                  }}
                >
                  <AgentIcon id={agent().id} size={13} />
                  <Text
                    style={{
                      fontSize: 11,
                      fontWeight: modalVal()?.agents.includes(agent().id) ? "600" : "500",
                      color: modalVal()?.agents.includes(agent().id)
                        ? paletteToken("accent_fg")
                        : paletteToken("fg"),
                    }}
                  >
                    {agent().name}
                  </Text>
                </View>
              )}
            </Each>
          </View>
        </View>
      </Scroll>

      <If when={Boolean(modalVal()?.error)}>
        <View testID="mcp-store-install-error" style={{ paddingHorizontal: 18, paddingTop: 10 }}>
          <Alert tone="error" message={modalVal()?.error ?? ""} />
        </View>
      </If>

      <ModalFooter
        submitTestID="mcp-store-install-confirm"
        submitLabel={() => ms().storeInstall}
        onCancel={() => props.state.modal.set(null)}
        onSubmit={() => {
          const m = modalVal();
          if (m) {
            void props.state.installStoreItem(m.item, m.customName, m.values, m.agents);
          }
        }}
      />
    </View>
  );
}

function FieldLabel(props: { text: () => string }): JSX.Element {
  return (
    <Text style={{ fontSize: 11, fontWeight: "600", color: paletteToken("fg_muted") }}>
      {props.text()}
    </Text>
  );
}

function fieldStyle(mono = false, multi = false): TextStyle {
  return {
    paddingHorizontal: 10,
    paddingVertical: 7,
    minHeight: multi ? 64 : undefined,
    fontSize: 12,
    fontFamily: mono ? "monospace" : undefined,
    borderRadius: 8,
    borderWidth: 1,
    borderColor: paletteToken("border"),
    backgroundColor: paletteToken("bg"),
    color: paletteToken("fg"),
  };
}
