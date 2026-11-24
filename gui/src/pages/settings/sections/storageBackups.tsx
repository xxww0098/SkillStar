// 技能历史备份快照卡（data/skills/backups/，最多 25 份）：列表、恢复与确认后清空。

import { Text, View, computed, type JSX } from "nsh";
import { openConfirm } from "../../../chrome/dialog";
import { paletteToken } from "../../../services/themeMode";
import {
  formatBytes,
  MetaChip,
  MONO_FONT,
  SettingsButton,
  SettingsGroup,
  StatusPill,
  type StatusTone,
} from "../parts";
import { st, stf } from "../settingsStrings";
import type { BackupEntryDto, BackupReason, StorageState } from "./storageState";

function reasonMeta(reason: BackupReason): { tone: StatusTone; labelKey: string } {
  switch (reason) {
    case "UpdateOverwrite":
      return { tone: "info", labelKey: "settings.backupReasonUpdate" };
    case "CopyConflict":
      return { tone: "warn", labelKey: "settings.backupReasonCopyConflict" };
    default:
      return { tone: "neutral", labelKey: "settings.backupReasonUninstall" };
  }
}

export function StorageBackupsCard(props: { state: StorageState }): JSX.Element {
  const state = props.state;
  const totalBytes = computed(() =>
    state.backups().reduce((sum, entry) => sum + entry.sizeBytes, 0),
  );
  const busy = computed(() => state.backupBusyId() !== null);

  const confirmClearAll = (): void => {
    const count = state.backups().length;
    if (count === 0 || busy()) return;
    openConfirm({
      title: st("settings.backupsClearConfirmTitle"),
      body: stf("settings.backupsClearConfirmDesc", {
        count,
        size: formatBytes(totalBytes()),
      }),
      okText: st("settings.backupsClearConfirmBtn"),
      danger: true,
      onOk: () => {
        state.clearAllBackups();
      },
    });
  };

  return (
    <SettingsGroup
      title={st("settings.backupsTitle")}
      trailing={
        <View style={{ display: "flex", flexDirection: "row", alignItems: "center", gap: 8 }}>
          <StatusPill
            testID="storage-backups-count"
            tone={state.backups().length > 0 ? "accent" : "neutral"}
            mono
            text={`${state.backups().length} · ${formatBytes(totalBytes())}`}
          />
          <If when={state.backups().length > 0}>
            <SettingsButton
              testID="storage-backups-clear"
              label={st("settings.backupsClearAll")}
              icon="trash"
              size="sm"
              variant="danger"
              busy={state.backupBusyId() === "__all__"}
              disabled={busy()}
              onPress={confirmClearAll}
            />
          </If>
        </View>
      }
    >
      <View style={{ display: "flex", flexDirection: "column", width: "100%" }}>
        <View
          style={{
            paddingHorizontal: 18,
            paddingVertical: 10,
            borderBottomWidth: 1,
            borderBottomColor: paletteToken("border_soft"),
            backgroundColor: paletteToken("well"),
          }}
        >
          <Text style={{ fontSize: 12, color: paletteToken("fg_muted") }}>
            {st("settings.backupsSubtitle")}
          </Text>
        </View>
        <If
          when={state.backups().length === 0}
          else={
            <Each of={state.backups()} key={(entry) => entry.id}>
              {(entry, index) => (
                <BackupRow
                  entry={entry()}
                  borderTop={index() > 0}
                  busy={busy()}
                  restoring={state.backupBusyId() === entry().id}
                  onRestore={() => state.restoreBackup(entry().id)}
                  onReveal={() => state.reveal(entry().path)}
                />
              )}
            </Each>
          }
        >
          <View style={{ paddingHorizontal: 18, paddingVertical: 16 }}>
            <Text style={{ fontSize: 12, color: paletteToken("fg_muted") }}>
              {st("settings.backupsEmpty")}
            </Text>
          </View>
        </If>
      </View>
    </SettingsGroup>
  );
}

function BackupRow(props: {
  entry: BackupEntryDto;
  borderTop: boolean;
  busy: boolean;
  restoring: boolean;
  onRestore: () => void;
  onReveal: () => void;
}): JSX.Element {
  const reason = computed(() => reasonMeta(props.entry.reason));
  const slug = computed(() => props.entry.id.toLowerCase().replace(/[^a-z0-9]/g, "-"));
  return (
    <View
      testID={`backup-row-${slug()}`}
      style={{
        paddingHorizontal: 18,
        paddingVertical: 12,
        display: "flex",
        flexDirection: "row",
        flexWrap: "wrap",
        alignItems: "center",
        justifyContent: "space-between",
        gap: 10,
        borderTopWidth: props.borderTop ? 1 : 0,
        borderTopColor: props.borderTop ? paletteToken("border_soft") : "transparent",
      }}
    >
      <View style={{ flex: 1, minWidth: 220, display: "flex", flexDirection: "column", gap: 5 }}>
        <View
          style={{
            display: "flex",
            flexDirection: "row",
            flexWrap: "wrap",
            alignItems: "center",
            gap: 6,
          }}
        >
          <Text style={{ fontSize: 13, fontWeight: "600", color: paletteToken("fg") }}>
            {props.entry.skillName}
          </Text>
          <If when={Boolean(props.entry.called && props.entry.called !== props.entry.skillName)}>
            <MetaChip
              tone="info"
              text={stf("settings.backupFromCalled", { called: props.entry.called ?? "" })}
            />
          </If>
          <StatusPill tone={reason().tone} text={st(reason().labelKey)} />
        </View>
        <View
          style={{
            display: "flex",
            flexDirection: "row",
            flexWrap: "wrap",
            alignItems: "center",
            gap: 10,
          }}
        >
          <Text
            style={{
              fontSize: 11,
              fontFamily: MONO_FONT,
              color: paletteToken("fg_muted"),
            }}
          >
            {props.entry.createdAt}
          </Text>
          <Text
            style={{
              fontSize: 11,
              fontFamily: MONO_FONT,
              color: paletteToken("fg_muted"),
            }}
          >
            {formatBytes(props.entry.sizeBytes)}
          </Text>
          <If when={Boolean(props.entry.source)}>
            <Text style={{ fontSize: 11, color: paletteToken("fg_muted"), numberOfLines: 1 }}>
              {props.entry.source ?? ""}
            </Text>
          </If>
        </View>
      </View>
      <View style={{ display: "flex", flexDirection: "row", alignItems: "center", gap: 6 }}>
        <SettingsButton
          testID={`backup-reveal-${slug()}`}
          label={st("settings.backupsOpenFolder")}
          icon="folderOpen"
          size="sm"
          variant="ghost"
          onPress={props.onReveal}
        />
        <SettingsButton
          testID={`backup-restore-${slug()}`}
          label={st("settings.backupsRestore")}
          icon="rotateCcw"
          size="sm"
          variant="secondary"
          busy={props.restoring}
          disabled={props.busy}
          onPress={props.onRestore}
        />
      </View>
    </View>
  );
}
