import { Scroll, Text, View, type JSX } from "nsh";
import { Alert } from "../../chrome/alerts";
import { openConfirm } from "../../chrome/dialog";
import { Icon } from "../../chrome/icons";
import { paletteToken } from "../../services/themeMode";
import { t } from "../../services/translation";
import { ms } from "./mcpStrings";
import type { McpState } from "./state";
import type { BackupEntry, McpServerRecord } from "./types";

function summarizeServer(s: McpServerRecord | undefined): string {
  if (!s) return "";
  if (s.transport === "stdio") {
    return `stdio · ${[s.command, ...s.args].filter(Boolean).join(" ")}`;
  }
  return `${s.transport} · ${s.url}`;
}

export function ImportConflictModalBody(props: { state: McpState }): JSX.Element {
  const modalVal = () => {
    const m = props.state.modal();
    return m?.kind === "importConflict" ? m : null;
  };

  return (
    <View testID="mcp-import-conflict-modal" style={{ display: "flex", flexDirection: "column" }}>
      <SubModalHeader
        title={() => `${ms().importConflictModalTitle}: ${modalVal()?.name ?? ""}`}
        closeTestID="mcp-conflict-close"
        onClose={() => props.state.modal.set(null)}
      />

      <View style={{ display: "flex", flexDirection: "column", padding: 18, gap: 12 }}>
        <Text style={{ fontSize: 12, lineHeight: 18, color: paletteToken("fg_muted") }}>
          {ms().importConflictDesc}
        </Text>

        <View
          style={{
            display: "flex",
            flexDirection: "column",
            gap: 8,
            padding: 12,
            borderRadius: 10,
            borderWidth: 1,
            borderColor: paletteToken("border"),
            backgroundColor: paletteToken("bg"),
          }}
        >
          <View style={{ display: "flex", flexDirection: "column", gap: 3 }}>
            <Text style={{ fontSize: 11, fontWeight: "600", color: paletteToken("fg") }}>
              {ms().importConflictLibraryHeading}
            </Text>
            <Text
              style={{
                fontSize: 11,
                fontFamily: "monospace",
                color: paletteToken("fg_muted"),
                numberOfLines: 2,
                textOverflow: "ellipsis",
              }}
            >
              {summarizeServer(modalVal()?.existing)}
            </Text>
          </View>
          <View style={{ height: 1, backgroundColor: paletteToken("border_soft") }} />
          <View style={{ display: "flex", flexDirection: "column", gap: 3 }}>
            <Text style={{ fontSize: 11, fontWeight: "600", color: paletteToken("fg") }}>
              {`${ms().importConflictFoundHeading} (${modalVal()?.agentId ?? ""})`}
            </Text>
            <Text
              style={{
                fontSize: 11,
                fontFamily: "monospace",
                color: paletteToken("fg_muted"),
                numberOfLines: 2,
                textOverflow: "ellipsis",
              }}
            >
              {summarizeServer(modalVal()?.discovered)}
            </Text>
          </View>
        </View>

        <View style={{ display: "flex", flexDirection: "column", gap: 8 }}>
          <ConflictOptionButton
            testID="mcp-conflict-keep"
            label={() => ms().importConflictKeepBtn}
            onPress={() => {
              const m = modalVal();
              if (m) void props.state.importFound(m.agentId, m.name, "keep");
            }}
          />
          <ConflictOptionButton
            testID="mcp-conflict-replace"
            label={() => ms().importConflictReplaceBtn}
            primary
            onPress={() => {
              const m = modalVal();
              if (m) void props.state.importFound(m.agentId, m.name, "replace");
            }}
          />
          <ConflictOptionButton
            testID="mcp-conflict-rename"
            label={() => ms().importConflictRenameBtn}
            onPress={() => {
              const m = modalVal();
              if (m) void props.state.importFound(m.agentId, m.name, "rename");
            }}
          />
        </View>
      </View>
    </View>
  );
}

export function BackupsModalBody(props: { state: McpState }): JSX.Element {
  return (
    <View testID="mcp-backups-modal" style={{ display: "flex", flexDirection: "column" }}>
      <SubModalHeader
        title={() => ms().backupsModalTitle}
        closeTestID="mcp-backups-close"
        onClose={() => props.state.modal.set(null)}
      />

      <View style={{ paddingHorizontal: 18, paddingTop: 12, paddingBottom: 8 }}>
        <Alert tone="info" message={ms().backupsSubtitle} />
      </View>

      <Scroll
        style={{ maxHeight: 380 }}
        contentStyle={{
          display: "flex",
          flexDirection: "column",
          gap: 8,
          paddingHorizontal: 18,
          paddingBottom: 16,
        }}
      >
        <If when={!props.state.backupsLoading() && props.state.backups().length === 0}>
          <Text
            style={{
              fontSize: 12,
              color: paletteToken("fg_muted"),
              paddingVertical: 18,
              textAlign: "center",
            }}
          >
            {ms().backupsEmpty}
          </Text>
        </If>

        <Each of={props.state.backups()} key={(b: BackupEntry) => b.id}>
            {(entry: () => BackupEntry) => (
              <View
                testID={`mcp-backup-row-${entry().id}`}
                style={{
                  display: "flex",
                  flexDirection: "row",
                  alignItems: "center",
                  gap: 10,
                  padding: 10,
                  borderRadius: 10,
                  borderWidth: 1,
                  borderColor: paletteToken("border"),
                  backgroundColor: paletteToken("bg"),
                }}
              >
                <View style={{ flex: 1, minWidth: 0, gap: 3 }}>
                  <Text
                    style={{
                      fontSize: 12,
                      fontWeight: "600",
                      fontFamily: "monospace",
                      color: paletteToken("fg"),
                      numberOfLines: 1,
                      textOverflow: "ellipsis",
                    }}
                  >
                    {entry().targetPath ?? entry().fileName}
                  </Text>
                  <View style={{ display: "flex", flexDirection: "row", alignItems: "center", gap: 6 }}>
                    <View
                      style={{
                        paddingHorizontal: 6,
                        paddingVertical: 1,
                        borderRadius: 5,
                        backgroundColor: entry().existed
                          ? paletteToken("well")
                          : paletteToken("warn_bg"),
                        borderWidth: 1,
                        borderColor: entry().existed
                          ? paletteToken("border_soft")
                          : paletteToken("warn_border"),
                      }}
                    >
                      <Text
                        style={{
                          fontSize: 10,
                          fontWeight: "600",
                          color: entry().existed ? paletteToken("fg_muted") : paletteToken("warn"),
                        }}
                      >
                        {entry().existed ? ms().backupsSnapshotBadge : ms().backupsAbsentBadge}
                      </Text>
                    </View>
                    <Text
                      style={{
                        fontSize: 10,
                        fontFamily: "monospace",
                        color: paletteToken("fg_muted"),
                        numberOfLines: 1,
                      }}
                    >
                      {entry().existed
                        ? `${entry().sizeBytes} B · ${entry().createdAt}`
                        : entry().createdAt}
                    </Text>
                  </View>
                </View>

                <View
                  testID={`mcp-backup-restore-${entry().id}`}
                  role="button"
                  style={{
                    height: 28,
                    paddingHorizontal: 10,
                    borderRadius: 7,
                    borderWidth: 1,
                    borderColor: paletteToken("border"),
                    backgroundColor: paletteToken("card"),
                    display: "flex",
                    alignItems: "center",
                    justifyContent: "center",
                    cursor: "pointer",
                    flexShrink: 0,
                  }}
                  hoverStyle={{
                    backgroundColor: paletteToken("panel_hover"),
                    borderColor: paletteToken("edge"),
                  }}
                  onPress={() => {
                    const b = entry();
                    openConfirm({
                      title: ms().backupsRestoreConfirmTitle,
                      body: `${ms().backupsRestoreConfirmBody}\n${b.targetPath ?? b.fileName}`,
                      okText: ms().backupsRestoreBtn,
                      danger: !b.existed,
                      onOk: () => {
                        void props.state.restoreBackup(b.id);
                      },
                    });
                  }}
                >
                  <Text style={{ fontSize: 11, fontWeight: "600", color: paletteToken("fg") }}>
                    {ms().backupsRestoreBtn}
                  </Text>
                </View>
              </View>
            )}
          </Each>
      </Scroll>
    </View>
  );
}

function SubModalHeader(props: {
  title: () => string;
  closeTestID: string;
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

function ConflictOptionButton(props: {
  testID: string;
  label: () => string;
  primary?: boolean;
  onPress: () => void;
}): JSX.Element {
  return (
    <View
      testID={props.testID}
      role="button"
      style={{
        height: 34,
        paddingHorizontal: 12,
        borderRadius: 9,
        borderWidth: props.primary ? 0 : 1,
        borderColor: paletteToken("border"),
        backgroundColor: props.primary ? paletteToken("accent") : paletteToken("bg"),
        display: "flex",
        alignItems: "center",
        justifyContent: "center",
        cursor: "pointer",
      }}
      hoverStyle={{
        backgroundColor: props.primary ? paletteToken("accent_hover") : paletteToken("panel_hover"),
      }}
      onPress={props.onPress}
    >
      <Text
        style={{
          fontSize: 12,
          fontWeight: "600",
          color: props.primary ? paletteToken("on_accent") : paletteToken("fg"),
        }}
      >
        {props.label()}
      </Text>
    </View>
  );
}
