// 详情抽屉扩展块：Copy 模式本地修改恢复（吸收回 Hub / 备份后覆盖）与同源仓库相邻技能发现。

import { Text, View, effect, signal, untracked, type JSX } from "nsh";
import { Icon } from "../../chrome/icons";
import { paletteToken } from "../../services/themeMode";
import { publishToast } from "../../shell/events";
import { et, etf } from "./extrasStrings";
import { openImportModalWithInput } from "./importModalState";
import type {
  DrawerExtrasDto,
  DrawerOps,
  DrawerSiblingSkill,
  ModifiedAgentCopyDto,
} from "./detailDrawerActions";

export function DetailDrawerExtras(props: {
  skillName: string;
  ops: DrawerOps;
}): JSX.Element | null {
  const extras = signal<DrawerExtrasDto | null>(null);
  const busyAction = signal<string | null>(null);
  let seq = 0;
  let loadedSkill: string | null = null;

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
    if (skill === loadedSkill) return;
    loadedSkill = skill;
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

  const installBeside = (sib: DrawerSiblingSkill): void => {
    if (!sib.source) return;
    openImportModalWithInput(`${sib.source} --skill ${sib.name}`, true);
  };

  return (
    <View style={{ display: "flex", flexDirection: "column", gap: 12 }}>
      {(() => {
        const data = extras();
        if (data === null) return null;
        const modified = data.modifiedCopies ?? [];
        const beside = data.besideSkills ?? [];
        if (modified.length === 0 && beside.length === 0) return null;
        return (
          <>
            {modified.length > 0 ? (
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
                {modified.map((item) => (
                  <View
                    testID={`drawer-modified-copy-${item.agentId}`}
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
                      {item.agentDisplayName || item.agentId}
                    </Text>
                    <Text
                      style={{
                        fontSize: 11,
                        color: paletteToken("fg_muted"),
                        numberOfLines: 1,
                      }}
                    >
                      {item.targetDir}
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
                        testID={`drawer-adopt-copy-${item.agentId}`}
                        label={et("mySkills.adoptCopyEdits")}
                        disabled={busyAction() !== null}
                        tone="accent"
                        onPress={() => adopt(item)}
                      />
                      <ActionChip
                        testID={`drawer-overwrite-copy-${item.agentId}`}
                        label={et("mySkills.overwriteCopyWithBackup")}
                        disabled={busyAction() !== null}
                        tone="warn"
                        onPress={() => overwrite(item)}
                      />
                    </View>
                  </View>
                ))}
              </View>
            ) : null}

            {beside.length > 0 ? (
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
                {beside.map((sib) => (
                  <View
                    testID={`drawer-beside-item-${sib.name}`}
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
                        {sib.name}
                      </Text>
                      <Text
                        style={{
                          fontSize: 11,
                          color: paletteToken("fg_muted"),
                          numberOfLines: 1,
                        }}
                      >
                        {sib.source}
                      </Text>
                    </View>
                    <ActionChip
                      testID={`drawer-beside-install-${sib.name}`}
                      label={et("mySkills.besideInstall")}
                      disabled={false}
                      tone="accent"
                      onPress={() => installBeside(sib)}
                    />
                  </View>
                ))}
              </View>
            ) : null}
          </>
        );
      })()}
    </View>
  );
}

function ActionChip(props: {
  testID: string;
  label: string;
  disabled: boolean;
  tone: "accent" | "warn";
  onPress: () => void;
}): JSX.Element {
  const isAccent = props.tone === "accent";
  return (
    <View
      testID={props.testID}
      role="button"
      style={{
        paddingHorizontal: 10,
        paddingVertical: 5,
        borderRadius: 7,
        borderWidth: 1,
        borderColor: paletteToken(isAccent ? "accent_soft_edge" : "warn_border"),
        backgroundColor: paletteToken(isAccent ? "accent_soft" : "warn_bg"),
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
          color: paletteToken(isAccent ? "accent_fg" : "warn"),
        }}
      >
        {props.label}
      </Text>
    </View>
  );
}
