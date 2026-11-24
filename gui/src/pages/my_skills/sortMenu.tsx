import { Overlay, View, signal, type JSX, type ViewRef } from "nsh";
import { t } from "../../services/translation";
import { BarMenuPanel, BarMenuRow, BarMenuTrigger } from "../../chrome/barMenu";
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
        <BarMenuTrigger
          testID="my-skills-sort-trigger"
          glyph="arrowUpDown"
          label={() => sortLabel(sortBy())}
          tip={() => `${t("toolbar.sort")}: ${sortLabel(sortBy())}`}
          active={() => sortBy() !== "smart"}
          onPress={() => menuOpen.update((open) => !open)}
        />
      </View>
      <If when={menuOpen()}>
        <Overlay
          anchor={triggerRef()}
          placement="bottom"
          align="start"
          offset={6}
          onPressOutside={() => menuOpen.set(false)}
          onCancel={() => menuOpen.set(false)}
        >
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
    <BarMenuPanel testID="my-skills-sort-panel">
      <Each of={SORT_OPTIONS} key={(option) => option}>
        {(option) => (
          <BarMenuRow
            testID={`sort-row-${option()}`}
            label={sortLabel(option())}
            active={sortBy() === option()}
            onPress={() => {
              sortBy.set(option());
              props.onClose();
            }}
          />
        )}
      </Each>
    </BarMenuPanel>
  );
}
