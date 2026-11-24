// 详情抽屉扩展块：未受管同名目录冲突恢复、Copy 模式本地修改恢复与同源仓库相邻技能发现。

import { Text, View, computed, effect, signal, untracked, type Accessor, type JSX } from "nsh";
import { Icon } from "../../chrome/icons";
import { paletteToken } from "../../services/themeMode";
import { publishToast } from "../../shell/events";
import { et, etf } from "./extrasStrings";
import { openImportModalWithInput } from "./importModalState";
import {
  extrasVersion,
  type DrawerExtrasDto,
  type DrawerOps,
  type DrawerSiblingSkill,
  type ModifiedAgentCopyDto,
  type UnmanagedAgentEntryDto,
} from "./detailDrawerActions";

export function DetailDrawerExtras(props: {
  skillName: string;
  ops: DrawerOps;
}): JSX.Element | null {
  const extras = signal<DrawerExtrasDto | null>(null);
  const busyAction = signal<string | null>(null);
  let seq = 0;
  let loadedKey: string | null = null;

  const load = (skill: string): void => {
    untracked(() => {
      const inspect = props.ops.inspectDrawerExtras;
      if (inspect === undefined || skill === "") {
        extras.set(null);
        return;
      }
      const req = (seq += 1);
      void inspect(skill)
        .then((res) => {
          if (req !== seq) return;
          extras.set(res);
        })
        .catch(() => {
          if (req !== seq) return;
          extras.set(null);
        });
    });
  };

  effect(() => {
    const skill = props.skillName;
    const version = extrasVersion();
    const key = `${skill}#${version}`;
    if (key === loadedKey) return;
    loadedKey = key;
    untracked(() => load(skill));
  });

  const adopt = (item: ModifiedAgentCopyDto): void => {
    const fn = props.ops.takeAgentCopyEdits;
    if (fn === undefined || busyAction() !== null) return;
    busyAction.set(`adopt:${item.agentId}`);
    void fn(props.skillName, item.agentId)
      .then(() => {
        busyAction.set(null);
        publishToast({
          tone: "success",
          message: etf("mySkills.adoptCopyDone", {
            agent: item.agentDisplayName || item.agentId,
          }),
        });
        load(props.skillName);
        void props.ops.refresh();
      })
      .catch(() => {
        busyAction.set(null);
        publishToast({ tone: "error", message: et("mySkills.adoptCopyFailed") });
      });
  };

  const overwrite = (item: ModifiedAgentCopyDto): void => {
    const fn = props.ops.overwriteAgentCopyWithBackup;
    if (fn === undefined || busyAction() !== null) return;
    busyAction.set(`overwrite:${item.agentId}`);
    void fn(props.skillName, item.agentId)
      .then((res) => {
        busyAction.set(null);
        publishToast({
          tone: "success",
          message: etf("mySkills.overwriteCopyDone", {
            id: res.backupId,
            agent: item.agentDisplayName || item.agentId,
          }),
        });
        load(props.skillName);
        void props.ops.refresh();
      })
      .catch(() => {
        busyAction.set(null);
        publishToast({ tone: "error", message: et("mySkills.overwriteCopyFailed") });
      });
  };

  const takeOverUnmanaged = (item: UnmanagedAgentEntryDto): void => {
    const fn = props.ops.takeOverUnmanagedAgentEntry;
    if (fn === undefined || busyAction() !== null) return;
    busyAction.set(`takeover:${item.agentId}`);
    void fn(props.skillName, item.agentId)
      .then((res) => {
        busyAction.set(null);
        const agent = item.agentDisplayName || item.agentId;
        const message =
          typeof res.backupId === "string" && res.backupId.length > 0
            ? etf("mySkills.takeOverDoneWithBackup", { id: res.backupId, agent })
            : etf("mySkills.takeOverDone", { agent });
        publishToast({
          tone: "success",
          message,
          replaceKey: "skill-toggle-skip",
        });
        load(props.skillName);
        void props.ops.refresh();
      })
      .catch((err: unknown) => {
        busyAction.set(null);
        publishToast({
          tone: "error",
          message: `${et("mySkills.takeOverFailed")}: ${String(err)}`,
        });
      });
  };

  const adoptUnmanaged = (item: UnmanagedAgentEntryDto): void => {
    const fn = props.ops.adoptUnmanagedAgentEntry;
    if (fn === undefined || busyAction() !== null) return;
    busyAction.set(`adopt-unmanaged:${item.agentId}`);
    void fn(props.skillName, item.agentId)
      .then(() => {
        busyAction.set(null);
        publishToast({
          tone: "success",
          message: etf("mySkills.adoptUnmanagedDone", {
            agent: item.agentDisplayName || item.agentId,
          }),
          replaceKey: "skill-toggle-skip",
        });
        load(props.skillName);
        void props.ops.refresh();
      })
      .catch((err: unknown) => {
        busyAction.set(null);
        publishToast({
          tone: "error",
          message: `${et("mySkills.adoptUnmanagedFailed")}: ${String(err)}`,
        });
      });
  };

  const installBeside = (sib: DrawerSiblingSkill): void => {
    if (!sib.source) return;
    openImportModalWithInput(`${sib.source} --skill ${sib.name}`, true);
  };

  const unmanaged = computed(() => extras()?.unmanagedEntries ?? []);
  const modified = computed(() => extras()?.modifiedCopies ?? []);
  const beside = computed(() => extras()?.besideSkills ?? []);
  const hasAny = computed(
    () => unmanaged().length > 0 || modified().length > 0 || beside().length > 0,
  );

  return (
    <If when={hasAny()}>
      <View style={{ display: "flex", flexDirection: "column", gap: 12 }}>
        <If when={unmanaged().length > 0}>
          <View
            testID="drawer-unmanaged-entries-card"
            style={{
              display: "flex",
              flexDirection: "column",
              gap: 8,
              padding: 12,
              borderRadius: 10,
              borderWidth: 1,
              borderColor: paletteToken("warn_border"),
              backgroundColor: paletteToken("warn_bg"),
            }}
          >
            <View
              style={{ display: "flex", flexDirection: "row", alignItems: "center", gap: 6 }}
            >
              <Icon name="alertTriangle" size={14} color={paletteToken("warn")} />
              <Text style={{ fontSize: 12, fontWeight: "600", color: paletteToken("fg") }}>
                {et("mySkills.unmanagedEntriesTitle")}
              </Text>
            </View>
            <Text style={{ fontSize: 11, color: paletteToken("fg_muted") }}>
              {et("mySkills.unmanagedEntriesHint")}
            </Text>
            <Each of={unmanaged()} key={(item) => item.agentId}>
              {(item: Accessor<UnmanagedAgentEntryDto>) => (
                <View
                  testID={`drawer-unmanaged-entry-${item().agentId}`}
                  style={{
                    display: "flex",
                    flexDirection: "column",
                    gap: 6,
                    padding: 8,
                    borderRadius: 8,
                    borderWidth: 1,
                    borderColor: paletteToken("border"),
                    backgroundColor: paletteToken("card"),
                  }}
                >
                  <Text style={{ fontSize: 12, fontWeight: "600", color: paletteToken("fg") }}>
                    {item().agentDisplayName || item().agentId}
                  </Text>
                  <Text
                    style={{
                      fontSize: 11,
                      color: paletteToken("fg_muted"),
                      numberOfLines: 1,
                    }}
                  >
                    {item().targetDir}
                  </Text>
                  <View
                    style={{
                      display: "flex",
                      flexDirection: "row",
                      flexWrap: "wrap",
                      gap: 6,
                    }}
                  >
                    <ActionChip
                      testID={`drawer-takeover-unmanaged-${item().agentId}`}
                      label={et(
                        item().isSymlink
                          ? "mySkills.takeOverForeignLink"
                          : "mySkills.takeOverUnmanaged",
                      )}
                      disabled={busyAction() !== null}
                      tone="accent"
                      onPress={() => takeOverUnmanaged(item())}
                    />
                    <If when={item().canAdoptToHub}>
                      <ActionChip
                        testID={`drawer-adopt-unmanaged-${item().agentId}`}
                        label={et("mySkills.adoptUnmanagedToHub")}
                        disabled={busyAction() !== null}
                        tone="warn"
                        onPress={() => adoptUnmanaged(item())}
                      />
                    </If>
                    <If when={props.ops.revealPath !== undefined}>
                      <ActionChip
                        testID={`drawer-open-conflict-dir-${item().agentId}`}
                        label={et("mySkills.openConflictFolder")}
                        disabled={false}
                        tone="neutral"
                        onPress={() => props.ops.revealPath?.(item().targetDir)}
                      />
                    </If>
                  </View>
                </View>
              )}
            </Each>
          </View>
        </If>

        <If when={modified().length > 0}>
          <View
            testID="drawer-modified-copies-card"
            style={{
              display: "flex",
              flexDirection: "column",
              gap: 8,
              padding: 12,
              borderRadius: 10,
              borderWidth: 1,
              borderColor: paletteToken("warn_border"),
              backgroundColor: paletteToken("warn_bg"),
            }}
          >
            <View
              style={{ display: "flex", flexDirection: "row", alignItems: "center", gap: 6 }}
            >
              <Icon name="alertTriangle" size={14} color={paletteToken("warn")} />
              <Text style={{ fontSize: 12, fontWeight: "600", color: paletteToken("fg") }}>
                {et("mySkills.modifiedCopiesTitle")}
              </Text>
            </View>
            <Text style={{ fontSize: 11, color: paletteToken("fg_muted") }}>
              {et("mySkills.modifiedCopiesHint")}
            </Text>
            <Each of={modified()} key={(item) => item.agentId}>
              {(item: Accessor<ModifiedAgentCopyDto>) => (
                <View
                  testID={`drawer-modified-copy-${item().agentId}`}
                  style={{
                    display: "flex",
                    flexDirection: "column",
                    gap: 6,
                    padding: 8,
                    borderRadius: 8,
                    borderWidth: 1,
                    borderColor: paletteToken("border"),
                    backgroundColor: paletteToken("card"),
                  }}
                >
                  <Text style={{ fontSize: 12, fontWeight: "600", color: paletteToken("fg") }}>
                    {item().agentDisplayName || item().agentId}
                  </Text>
                  <Text
                    style={{
                      fontSize: 11,
                      color: paletteToken("fg_muted"),
                      numberOfLines: 1,
                    }}
                  >
                    {item().targetDir}
                  </Text>
                  <View
                    style={{
                      display: "flex",
                      flexDirection: "row",
                      flexWrap: "wrap",
                      gap: 6,
                    }}
                  >
                    <ActionChip
                      testID={`drawer-adopt-copy-${item().agentId}`}
                      label={et("mySkills.adoptCopyEdits")}
                      disabled={busyAction() !== null}
                      tone="accent"
                      onPress={() => adopt(item())}
                    />
                    <ActionChip
                      testID={`drawer-overwrite-copy-${item().agentId}`}
                      label={et("mySkills.overwriteCopyWithBackup")}
                      disabled={busyAction() !== null}
                      tone="warn"
                      onPress={() => overwrite(item())}
                    />
                  </View>
                </View>
              )}
            </Each>
          </View>
        </If>

        <If when={beside().length > 0}>
          <View
            testID="drawer-beside-skills-card"
            style={{
              display: "flex",
              flexDirection: "column",
              gap: 8,
              padding: 12,
              borderRadius: 10,
              borderWidth: 1,
              borderColor: paletteToken("border"),
              backgroundColor: paletteToken("well"),
            }}
          >
            <View
              style={{ display: "flex", flexDirection: "row", alignItems: "center", gap: 6 }}
            >
              <Icon name="packageOpen" size={14} color={paletteToken("accent_fg")} />
              <Text style={{ fontSize: 12, fontWeight: "600", color: paletteToken("fg") }}>
                {et("mySkills.besideSkillsTitle")}
              </Text>
            </View>
            <Text style={{ fontSize: 11, color: paletteToken("fg_muted") }}>
              {et("mySkills.besideSkillsHint")}
            </Text>
            <Each of={beside()} key={(sib) => sib.name}>
              {(sib: Accessor<DrawerSiblingSkill>) => (
                <View
                  testID={`drawer-beside-item-${sib().name}`}
                  style={{
                    display: "flex",
                    flexDirection: "row",
                    alignItems: "center",
                    justifyContent: "space-between",
                    gap: 8,
                    paddingHorizontal: 10,
                    paddingVertical: 7,
                    borderRadius: 8,
                    borderWidth: 1,
                    borderColor: paletteToken("border"),
                    backgroundColor: paletteToken("card"),
                  }}
                >
                  <View style={{ flex: 1, minWidth: 0, display: "flex", flexDirection: "column" }}>
                    <Text
                      style={{
                        fontSize: 12,
                        fontWeight: "600",
                        color: paletteToken("fg"),
                        numberOfLines: 1,
                      }}
                    >
                      {sib().name}
                    </Text>
                    <Text
                      style={{
                        fontSize: 11,
                        color: paletteToken("fg_muted"),
                        numberOfLines: 1,
                      }}
                    >
                      {sib().source}
                    </Text>
                  </View>
                  <ActionChip
                    testID={`drawer-beside-install-${sib().name}`}
                    label={et("mySkills.besideInstall")}
                    disabled={false}
                    tone="accent"
                    onPress={() => installBeside(sib())}
                  />
                </View>
              )}
            </Each>
          </View>
        </If>
      </View>
    </If>
  );
}

function ActionChip(props: {
  testID: string;
  label: string;
  disabled: boolean;
  tone: "accent" | "warn" | "neutral";
  onPress: () => void;
}): JSX.Element {
  const borderColor =
    props.tone === "accent"
      ? paletteToken("accent_soft_edge")
      : props.tone === "warn"
        ? paletteToken("warn_border")
        : paletteToken("border");
  const backgroundColor =
    props.tone === "accent"
      ? paletteToken("accent_soft")
      : props.tone === "warn"
        ? paletteToken("warn_bg")
        : paletteToken("well");
  const textColor =
    props.tone === "accent"
      ? paletteToken("accent_fg")
      : props.tone === "warn"
        ? paletteToken("warn")
        : paletteToken("fg_muted");
  return (
    <View
      testID={props.testID}
      role="button"
      style={{
        paddingHorizontal: 10,
        paddingVertical: 5,
        borderRadius: 7,
        borderWidth: 1,
        borderColor,
        backgroundColor,
        opacity: props.disabled ? 0.5 : 1,
        cursor: props.disabled ? "default" : "pointer",
      }}
      hoverStyle={props.disabled ? undefined : { backgroundColor: paletteToken("panel_hover") }}
      onPress={() => {
        if (!props.disabled) props.onPress();
      }}
    >
      <Text
        style={{
          fontSize: 11,
          fontWeight: "600",
          color: textColor,
        }}
      >
        {props.label}
      </Text>
    </View>
  );
}
