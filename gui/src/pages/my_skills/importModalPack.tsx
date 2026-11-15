import { Text, View, type JSX } from "nsh";
import { Input } from "nsh/base";
import { IconSpin } from "../../chrome/feedback";
import { paletteToken } from "../../services/themeMode";
import { t } from "../../services/translation";
import { commitPack, packBusy, packError, packName, packSeed } from "./importModalState";
import { PrimaryButton } from "./importModalPhases";

// Quick Pack 相 — import_modal.rs Phase::Pack（GPUI 内嵌卡组编辑器
// CreateGroupDialog）对位。nsh 侧跨页不 import 卡组页模块：卡组写入走
// SkillCardsCreateGroup 域 facade（importModalState.packBridge），本相只
// 留 Quick Pack 的本质交互——命名预选成员的卡组并创建（成员固定预选，
// 不再增删；完整编辑器在卡组页，分歧记录于 gaps）。

export function PackBody(): JSX.Element {
  const seed = () => packSeed();
  return (
    <View testID="import-pack" style={{ display: "flex", flexDirection: "column", paddingHorizontal: 24, paddingTop: 20, paddingBottom: 20, gap: 16 }}>
      <View style={{ display: "flex", flexDirection: "column", gap: 4 }}>
        <Text style={{ fontSize: 12, color: paletteToken("fg_muted") }}>
          {t("createGroupModal.groupName")}
        </Text>
        <Input
          testID="import-pack-name"
          value={packName()}
          autoFocus
          readonly={packBusy()}
          submitBehavior="submit"
          style={{
            width: "100%",
            height: 32,
            paddingHorizontal: 10,
            borderRadius: 6,
            borderWidth: 1,
            borderColor: paletteToken("border"),
            backgroundColor: paletteToken("input"),
            fontSize: 14,
            color: paletteToken("fg"),
          }}
          onChange={(event: { value: string }) => packName.set(event.value)}
          onSubmit={() => commitPack()}
        />
      </View>
      <View style={{ display: "flex", flexDirection: "column", gap: 6 }}>
        <Text style={{ fontSize: 12, color: paletteToken("fg_muted") }}>
          {t("githubImportModal.quickPack")}
        </Text>
        <View style={{ display: "flex", flexDirection: "row", flexWrap: "wrap", gap: 6 }}>
          {(seed()?.members ?? []).map((member) => (
            <View
              style={{
                paddingHorizontal: 8,
                paddingVertical: 3,
                borderRadius: 999,
                backgroundColor: paletteToken("well"),
              }}
            >
              <Text style={{ fontSize: 12, color: paletteToken("fg") }}>{member}</Text>
            </View>
          ))}
        </View>
      </View>
      <If when={packError() !== null}>
        <Text testID="import-pack-error" style={{ fontSize: 12, color: paletteToken("danger") }}>
          {packError() ?? ""}
        </Text>
      </If>
      <View style={{ display: "flex", flexDirection: "row", alignItems: "center", gap: 8 }}>
        <PrimaryButton
          testID="import-pack-create"
          label={packBusy() ? t("createGroupModal.saving") : t("createGroupModal.create")}
          onPress={() => commitPack()}
        />
        <If when={packBusy()}>
          <IconSpin name="loader" size={14} color={paletteToken("fg_muted")} spin />
        </If>
      </View>
    </View>
  );
}
