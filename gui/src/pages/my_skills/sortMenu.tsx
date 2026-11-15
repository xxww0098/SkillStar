import { Overlay, Text, View, signal, type JSX, type ViewRef } from "nsh";
import { paletteToken } from "../../services/themeMode";
import { t } from "../../services/translation";
import { Icon } from "../../chrome/icons";
import { Tip } from "../../chrome/tooltip";
import { SORT_OPTIONS, type SortOption } from "./filters";
import { sortBy } from "./state";

// 排序菜单 — my_skills/toolbar/sort_menu.rs 对位。紧凑触发器
// （ArrowUpDown + 当前排序名 + 下箭头；非 smart 时 accent）打开三行弹层，
// 行前 Check 标当前项，选中即写 sort_by 并收弹层。排序语义在 filters.ts
// （filters.rs 对位），这里只有 UI。

const menuOpen = signal(false);
const triggerRef = signal<ViewRef | null>(null);

/** 触发器 + 弹层宿主（originMenu 同款 Overlay 模式：锚定触发器，透明
 * fixed 幕点外收起——PageBar 命令区 overflow:hidden 裁不掉 root 层）。 */
export function SortMenuButton(): JSX.Element {
  return (
    <View style={{ position: "relative", flexShrink: 0, display: "flex" }}>
      <View
        ref={(node) => {
          triggerRef.set(node);
        }}
      >
        <View
          testID="my-skills-sort-trigger"
          tooltip={<Tip text={`${t("toolbar.sort")}: ${sortLabel(sortBy())}`} />}
          style={{
            display: "flex",
            flexDirection: "row",
            alignItems: "center",
            gap: 6,
            height: 32,
            paddingHorizontal: 12,
            borderRadius: 10,
            borderWidth: 1,
            cursor: "pointer",
            flexShrink: 0,
            borderColor:
              sortBy() !== "smart" ? paletteToken("accent_soft_edge") : paletteToken("border"),
            backgroundColor: sortBy() !== "smart" ? paletteToken("accent_soft") : paletteToken("bg"),
          }}
          hoverStyle={{ backgroundColor: paletteToken("card_hover") }}
          onPress={() => menuOpen.set(!menuOpen())}
        >
          <Icon
            name="arrowUpDown"
            size={14}
            color={sortBy() !== "smart" ? paletteToken("accent") : paletteToken("fg_muted")}
          />
          <Text
            style={{
              fontSize: 12,
              fontWeight: "500",
              color: sortBy() !== "smart" ? paletteToken("accent") : paletteToken("fg"),
            }}
          >
            {sortLabel(sortBy())}
          </Text>
          <Icon
            name="chevronDown"
            size={11}
            color={sortBy() !== "smart" ? paletteToken("accent") : paletteToken("fg_muted")}
          />
        </View>
      </View>
      <If when={menuOpen()}>
        <Overlay style={{ top: 0, left: 0, right: 0, bottom: 0 }}>
          <View
            testID="selection-menu-dismiss-sort"
            style={{ position: "absolute", top: 0, left: 0, right: 0, bottom: 0 }}
            onPress={() => menuOpen.set(false)}
          />
        </Overlay>
        <Overlay anchor={triggerRef()} placement="bottom" align="start" offset={6}>
          <SortPanel onClose={() => menuOpen.set(false)} />
        </Overlay>
      </If>
    </View>
  );
}

function sortLabel(option: SortOption): string {
  return option === "smart"
    ? t("toolbar.sortSmart")
    : option === "updated"
      ? t("toolbar.updated")
      : t("toolbar.sortName");
}

function SortPanel(props: { onClose: () => void }): JSX.Element {
  return (
    <View
      testID="my-skills-sort-panel"
      style={{
        width: 190,
        borderRadius: 12,
        borderWidth: 1,
        borderColor: paletteToken("border"),
        backgroundColor: paletteToken("card"),
        boxShadow: "0 8px 30px rgba(0,0,0,0.35)",
        overflow: "hidden",
        display: "flex",
        flexDirection: "column",
        padding: 6,
        gap: 2,
      }}
    >
      {SORT_OPTIONS.map((option) => (
        <SortRow option={option} active={sortBy() === option} onClose={props.onClose} />
      ))}
    </View>
  );
}

function SortRow(props: { option: SortOption; active: boolean; onClose: () => void }): JSX.Element {
  return (
    <View
      testID={`sort-row-${props.option}`}
      style={{
        display: "flex",
        flexDirection: "row",
        alignItems: "center",
        height: 32,
        paddingHorizontal: 8,
        gap: 4,
        borderRadius: 6,
        cursor: "pointer",
        backgroundColor: props.active ? paletteToken("accent_soft") : "transparent",
      }}
      hoverStyle={{ backgroundColor: paletteToken("panel_hover") }}
      onPress={() => {
        sortBy.set(props.option);
        props.onClose();
      }}
    >
      <View style={{ width: 16, display: "flex", alignItems: "center", justifyContent: "center" }}>
        {props.active ? (
          <Icon name="check" size={13} color={paletteToken("accent")} />
        ) : null}
      </View>
      <Text
        style={{
          flex: 1,
          minWidth: 0,
          fontSize: 12,
          fontWeight: "500",
          color: props.active ? paletteToken("accent") : paletteToken("fg"),
          numberOfLines: 1,
        }}
      >
        {sortLabel(props.option)}
      </Text>
    </View>
  );
}
