import {
  Overlay,
  Scroll,
  Text,
  View,
  signal,
  type JSX,
  type TextStyle,
  type ViewRef,
  type ViewStyle,
} from "nsh";
import {
  MarketplaceTranslation,
  MySkills,
  MySkillsSelection,
} from "skillstar-gui/native";
import { paletteToken } from "../../services/themeMode";
import { t, tf } from "../../services/translation";
import { AgentBadge } from "../../chrome/agentRail";
import { Icon } from "../../chrome/icons";
import { Tip } from "../../chrome/tooltip";
import { uninstallTitleCount, openUninstallConfirm } from "../../chrome/dialog";
import { toast } from "../../shell/events";
import type { AgentProfileRecord, SkillRecord } from "./filters";
import {
  batchUpdateSelected,
  busy,
  clearBatch,
  currentFiltered,
  descriptionChoices,
  linkMenuOpen,
  profiles,
  refresh,
  selectedBatch,
  selectedSkill,
  skills,
} from "./state";

// 批量选择胶囊 — my_skills/selection_bar.rs 对位。勾选多张技能卡后浮在
// 卡槽底部中央（SELECTION_BAR_INSET=12，定位层无背景、不占布局行——卡
// 片在胶囊旁边和下面继续绘制；view.rs
// selection_bar_floats_without_a_background_band 的契约）。胶囊内五类批量
// 操作：链接（agent 菜单向上开）/ 翻译 / 更新 / 卸载 / 清除。
//
// 状态模型归 state.ts（波 3 基座持有）：selectedBatch（ReadonlySet，头像
// 点击进出）、linkMenuOpen、busy（一次只跑一个变更；批量卸载 token
// "batch_uninstall"，更新走 state.batchUpdateSelected 的 "batch_update"）。
// 组件体只执行一次：一切派生读取（三态全选、可译集、待更新计数）以函数
// 调用形式挂在 JSX 表达式上，signal 在 JSX 上下文里现读。

/** 胶囊距卡槽底边的空气层。不是填充带（selection_bar.rs 同名常量）。 */
export const SELECTION_BAR_INSET = 12;

/** Agent rows that fit before the link menu scrolls
 * (LINK_AGENTS_VISIBLE / LINK_AGENTS_VIEW_H 先例：高度写在视口框上，
 * 滚轮留在列表里——滚动盒契约，不给滚动包装写 max_h)。 */
export const LINK_AGENTS_VISIBLE = 6;
export const LINK_AGENTS_VIEW_H = 198;

// ---- 纯派生（JSX 表达式里调用；selectionBar.test.tsx 直测） ---------------

/** description_source（mod.rs:449）：非空本地化描述优先，回退描述；空 →
 * null（无可译文本）。 */
export function descriptionSource(skill: SkillRecord): string | null {
  const localized = (skill.localizedDescription ?? "").trim();
  const text = (localized.length > 0 ? localized : skill.description).trim();
  return text.length > 0 ? text : null;
}

/** 翻译门（translate_batch 的 nsh 视角）：目标语言是否要译 + 缓存译文是否
 * 已在。两个都是 facade 同步纯读；测试注入替身。 */
export interface TranslationGate {
  needsTranslation(source: string): boolean;
  cachedTranslation(source: string): string | null;
}

const facadeGate: TranslationGate = {
  needsTranslation: (source) => MarketplaceTranslation.needsTranslation(source),
  cachedTranslation: (source) => MarketplaceTranslation.lookup(source),
};

/** 一次点击还要打开译文的名字集（translate_batch 的 idle 半）：选中的、
 * 该目标语言要译的、且「已请求且缓存命中（已在显示译文）」排除后的剩余。
 * 中文文案（zh-CN 目标下）与未选中卡天然不在内；GPUI 的 pending（在飞扫
 * 掠）半无 nsh 在飞通道，见 gaps。 */
export function translateIdleNames(
  skillsToScan: SkillRecord[],
  batch: ReadonlySet<string>,
  choices: Record<string, boolean>,
  globalOn: boolean,
  gate: TranslationGate
): string[] {
  const idle: string[] = [];
  for (const skill of skillsToScan) {
    if (!batch.has(skill.name)) continue;
    const source = descriptionSource(skill);
    if (source === null) continue;
    if (!gate.needsTranslation(source)) continue;
    const requested = choices[skill.name] ?? globalOn;
    if (requested && gate.cachedTranslation(source) !== null) continue;
    idle.push(skill.name);
  }
  return idle;
}

/** 全选三态（render_select_all）：all = 过滤集非空且全在批次；some = 批次
 * 非空且非 all。字形 square/squareMinus/squareCheck（chrome/icons）。 */
export function selectAllFace(
  filtered: SkillRecord[],
  batch: ReadonlySet<string>
): { all: boolean; some: boolean; icon: "square" | "squareMinus" | "squareCheck" } {
  const all = filtered.length > 0 && filtered.every((skill) => batch.has(skill.name));
  const some = batch.size > 0 && !all;
  return {
    all,
    some,
    icon: all ? "squareCheck" : some ? "squareMinus" : "square",
  };
}

/** 选中的卡里待更新的数量（render_update_button 的徽章口径）。 */
export function updatesInSelection(
  skillsToScan: SkillRecord[],
  batch: ReadonlySet<string>
): number {
  return skillsToScan.filter((skill) => batch.has(skill.name) && skill.updateAvailable).length;
}

/** 目标 agent 集（skill_card::targetable_agent_profiles 对位：facade 的
 * targetable 位 = enabled 且有全局技能目录）。 */
export function targetableAgents(records: AgentProfileRecord[]): AgentProfileRecord[] {
  return records.filter((profile) => profile.targetable);
}

// JSX 上下文里的现读封装（组件体不持有快照）。
function translateIdleNow(): string[] {
  return translateIdleNames(
    skills(),
    selectedBatch(),
    descriptionChoices(),
    MarketplaceTranslation.descriptionsEnabled(),
    facadeGate
  );
}

function updatesNow(): number {
  return updatesInSelection(skills(), selectedBatch());
}

// ---- 批次动作 ----------------------------------------------------------------

/** 全选/取消全选（render_select_all on_click）：全选态清整个批次（含过滤
 * 集外的名字），否则把过滤集全名并入。 */
export function toggleSelectAll(): void {
  const filtered = currentFiltered();
  const batch = selectedBatch();
  const { all } = selectAllFace(filtered, batch);
  if (all) {
    selectedBatch.set(new Set());
    return;
  }
  const next = new Set(batch);
  for (const skill of filtered) next.add(skill.name);
  selectedBatch.set(next);
}

/** 批量链接到一个 agent（commands.rs batch_link_to_agent）：点击时快照名
 * 单；完成清选择并刷新；失败走通知带（nsh 页面的错误通道；GPUI 侧是页
 * error 横幅）。 */
export async function runLinkToAgent(agentId: string): Promise<void> {
  const names = [...selectedBatch()];
  linkMenuOpen.set(false);
  try {
    await MySkillsSelection.linkToAgent(names, agentId);
  } catch (err) {
    toast(String(err), "error");
  } finally {
    selectedBatch.set(new Set());
    await refresh();
  }
}

/** 批量从全部 agent 取消链接（commands.rs batch_unlink_selected）：逐名
 * remove_skill_from_all_agents，失败行 `name: err` 由 facade 收集。 */
export async function runUnlinkAll(): Promise<void> {
  const names = [...selectedBatch()];
  linkMenuOpen.set(false);
  try {
    await MySkillsSelection.unlinkFromAllAgents(names);
  } catch (err) {
    toast(String(err), "error");
  } finally {
    selectedBatch.set(new Set());
    await refresh();
  }
}

/** 批量卸载（commands.rs batch_uninstall_selected）：确认后逐名
 * uninstall_skill（失败行收集），busy token "batch_uninstall"，先清选择与
 * 详情（这些名字即将消失）。 */
export function batchUninstallSelected(): void {
  const names = [...selectedBatch()];
  if (names.length === 0) return;
  busy.set("batch_uninstall");
  selectedBatch.set(new Set());
  linkMenuOpen.set(false);
  selectedSkill.set(null);
  void (async () => {
    const failed: string[] = [];
    for (const name of names) {
      try {
        await MySkills.uninstallSkill(name);
      } catch (err) {
        failed.push(`${name}: ${String(err)}`);
      }
    }
    if (failed.length > 0) toast(failed.join("\n"), "error");
    busy.set(null);
    await refresh();
  })();
}

/** 批量翻译（selection_bar.rs batch_translate_selected）：为 idle 集里还
 * 未显式 true 的名字写入选择（与抽屉语言按钮同一存储，Settings 开关从不
 * 写）；随后刷新（note_translation_prefs 的重同步对位）。写失败只保持内存
 * 态（GPUI tracing::warn 对位）。 */
export function batchTranslateSelected(): void {
  const idle = translateIdleNow();
  if (idle.length === 0) return;
  const choices = descriptionChoices();
  const write: Record<string, boolean> = {};
  for (const name of idle) {
    if (choices[name] === true) continue;
    write[name] = true;
  }
  if (Object.keys(write).length === 0) return;
  descriptionChoices.set({ ...choices, ...write });
  void MySkills.setDescriptionChoices(write)
    .catch(() => {
      // 持久化失败：内存选择保留（卡面即时生效），下次装载重读。
    })
    .finally(() => {
      void refresh();
    });
}

// ---- 胶囊 --------------------------------------------------------------------

export function SelectionBarSlot(): JSX.Element | null {
  // 空批次不挂浮层：返回 null，不占位（GPUI 返回空 div，nsh 侧直接不挂）。
  return (
    <If when={selectedBatch().size > 0}>
      <SelectionBarFloat />
    </If>
  );
}

/** 定位层：absolute 底 12 全宽居中，无背景、无事件——胶囊两侧的点击落到
 * 卡上（render_selection_bar 的居中层契约）。 */
function SelectionBarFloat(): JSX.Element {
  return (
    <View
      testID="selection-bar"
      style={{
        position: "absolute",
        bottom: SELECTION_BAR_INSET,
        left: 0,
        right: 0,
        display: "flex",
        justifyContent: "center",
        zIndex: 20,
      }}
    >
      <View
        testID="selection-pill"
        style={{
          display: "flex",
          alignItems: "center",
          gap: 4,
          paddingLeft: 6,
          paddingRight: 8,
          paddingVertical: 6,
          borderRadius: 999,
          borderWidth: 1,
          borderColor: paletteToken("border"),
          backgroundColor: paletteToken("card"),
          boxShadow: "0 8px 30px rgba(0,0,0,0.35)",
        }}
      >
        <CountCapsule />
        <SelectAllToggle />
        <PillDivider />
        <LinkMenuMount />
        <TranslateButton />
        <UpdateButton />
        <UninstallButton />
        <PillDivider />
        <ClearButton />
      </View>
    </View>
  );
}

/** 计数胶囊：accent 底上唯一强调位（render_selection_bar 的 count 子）。 */
function CountCapsule(): JSX.Element {
  return (
    <View
      testID="selection-count"
      style={{
        paddingHorizontal: 10,
        paddingVertical: 2,
        borderRadius: 999,
        backgroundColor: paletteToken("accent"),
        flexShrink: 0,
      }}
    >
      <Text style={{ fontSize: 12, fontWeight: "600", color: paletteToken("on_accent") }}>
        {tf("selectionBar.selected", { count: selectedBatch().size })}
      </Text>
    </View>
  );
}

/** 安静的三态全选开关（render_select_all）：不是动作，是切换。 */
function SelectAllToggle(): JSX.Element {
  return (
    <View
      testID="select-all-toggle-btn"
      style={pillButton()}
      hoverStyle={{ backgroundColor: paletteToken("card_hover") }}
      onPress={toggleSelectAll}
    >
      {(() => {
        const face = selectAllFace(currentFiltered(), selectedBatch());
        return (
          <>
            <Icon
              name={face.icon}
              size={12}
              color={face.all ? paletteToken("accent") : paletteToken("fg_muted")}
            />
            <Text style={pillLabel()}>
              {face.all ? t("common.deselectAll") : t("common.selectAll")}
            </Text>
          </>
        );
      })()}
    </View>
  );
}

/** 翻译描述（render_translate_button）：idle 集空 → 半透明不可点。 */
function TranslateButton(): JSX.Element {
  return (
    <View
      testID="batch-translate-btn"
      style={(() => {
        const enabled = translateIdleNow().length > 0;
        return { ...pillButton(), opacity: enabled ? 1 : 0.45, cursor: enabled ? "pointer" : "default" };
      })()}
      hoverStyle={(() =>
        translateIdleNow().length > 0
          ? { backgroundColor: paletteToken("card_hover") }
          : undefined)()}
      onPress={batchTranslateSelected}
    >
      <Icon name="languages" size={12} color={paletteToken("fg_muted")} />
      <Text style={pillLabel()}>{t("selectionBar.translateDescriptions")}</Text>
    </View>
  );
}

/** 批量更新（render_update_button）：busy 或选中无待更新 → 半透明；有待
 * 更新时带计数徽章；点击走 state.batchUpdateSelected（busy token
 * "batch_update"，卡面省略号跟点击时快照）。 */
function UpdateButton(): JSX.Element {
  return (
    <View
      testID="batch-update-btn"
      style={(() => {
        const enabled = busy() === null && updatesNow() > 0;
        return { ...pillButton(), opacity: enabled ? 1 : 0.45 };
      })()}
      hoverStyle={(() =>
        busy() === null && updatesNow() > 0
          ? { backgroundColor: paletteToken("card_hover") }
          : undefined)()}
      onPress={() => {
        if (busy() === null && updatesNow() > 0) batchUpdateSelected();
      }}
    >
      <Icon name="refresh" size={12} color={paletteToken("fg_muted")} />
      <Text style={pillLabel()}>{t("common.update")}</Text>
      {(() => {
        const count = updatesNow();
        if (count === 0) return null;
        return (
          <View
            testID="batch-update-count"
            style={{
              paddingHorizontal: 6,
              borderRadius: 999,
              backgroundColor: paletteToken("accent_soft"),
            }}
          >
            <Text style={{ fontSize: 12, color: paletteToken("accent_fg") }}>{count}</Text>
          </View>
        );
      })()}
    </View>
  );
}

/** 批量卸载（render_uninstall_button）：安静 danger 色；确认弹窗拦在前
 * （openUninstallConfirm + uninstallTitleCount），确认才起批次。 */
function UninstallButton(): JSX.Element {
  return (
    <View
      testID="batch-uninstall-btn"
      style={(() => {
        const enabled = busy() === null;
        return { ...pillButton(), opacity: enabled ? 1 : 0.45 };
      })()}
      hoverStyle={(() =>
        busy() === null ? { backgroundColor: paletteToken("card_hover") } : undefined)()}
      onPress={() => {
        if (busy() !== null) return;
        openUninstallConfirm(uninstallTitleCount(selectedBatch().size), () => {
          batchUninstallSelected();
          return true;
        });
      }}
    >
      <Icon name="trash" size={12} color={paletteToken("danger")} />
      <Text style={{ ...pillLabel(), color: paletteToken("danger") }}>
        {t("common.uninstall")}
      </Text>
    </View>
  );
}

/** 清除选择（render_selection_bar 的 X 子）：图标钮，tooltip 报名。 */
function ClearButton(): JSX.Element {
  return (
    <View
      testID="selection-clear"
      tooltip={<Tip text={t("common.clear")} />}
      style={pillButton()}
      hoverStyle={{ backgroundColor: paletteToken("card_hover") }}
      onPress={clearBatch}
    >
      <Icon name="x" size={13} color={paletteToken("fg_muted")} />
    </View>
  );
}

// ---- 上开 agent 菜单（render_link_menu） --------------------------------------

/** 链接触发器 + 它的菜单宿主（originMenu 同款 Overlay 模式：fixed 透明幕
 * 点外收起 + 触发器 ref 锚定的弹层；placement="top" —— 菜单向上开，胶囊
 * 在面板底边）。 */
function LinkMenuMount(): JSX.Element {
  // 锚 ref 是本挂载的局部 signal：组件体只执行一次，signal 随挂载创建——
  // 模块级 ref 会把上一个已关 app 的死视图带给下一个 app 的 Overlay。
  const triggerRef = signal<ViewRef | null>(null);
  return (
    <View style={{ position: "relative", display: "flex", flexShrink: 0 }}>
      <View
        ref={(node) => {
          triggerRef.set(node);
        }}
      >
        <LinkTrigger />
      </View>
      <If when={linkMenuOpen() && busy() === null}>
        <Overlay style={{ top: 0, left: 0, right: 0, bottom: 0 }}>
          <View
            testID="selection-link-dismiss"
            style={{ position: "absolute", top: 0, left: 0, right: 0, bottom: 0 }}
            onPress={() => linkMenuOpen.set(false)}
          />
        </Overlay>
        <Overlay anchor={triggerRef()} placement="top" align="start" offset={6}>
          <LinkMenuPanel />
        </Overlay>
      </If>
    </View>
  );
}

/** 触发器（batch-link-trigger）：开合翻 accent_soft 底；busy 半透明。 */
function LinkTrigger(): JSX.Element {
  return (
    <View
      testID="batch-link-trigger"
      style={{
        ...pillButton(),
        backgroundColor: linkMenuOpen() ? paletteToken("accent_soft") : "transparent",
        opacity: busy() === null ? 1 : 0.45,
      }}
      hoverStyle={(() =>
        linkMenuOpen() ? undefined : { backgroundColor: paletteToken("card_hover") })()}
      onPress={() => {
        if (busy() === null) linkMenuOpen.set(!linkMenuOpen());
      }}
    >
      <Icon
        name="link"
        size={12}
        color={linkMenuOpen() ? paletteToken("accent_fg") : paletteToken("fg_muted")}
      />
      <Text style={pillLabel()}>{t("selectionBar.linkToAgent")}</Text>
      <Icon name="chevronUp" size={11} color={paletteToken("fg_muted")} />
    </View>
  );
}

/** 菜单面板（selection-link-menu）：标题恒在；目标 agent 集与卡面轮播同源
 * （targetable，facade snapshot 已算好）；>6 行固定 198px 视口内滚（标题
 * 与取消链接行不滚）。 */
function LinkMenuPanel(): JSX.Element {
  return (
    <View
      testID="selection-link-menu"
      style={{
        width: 224,
        padding: 6,
        borderRadius: 12,
        borderWidth: 1,
        borderColor: paletteToken("border"),
        backgroundColor: paletteToken("card"),
        boxShadow: "0 8px 30px rgba(0,0,0,0.35)",
        display: "flex",
        flexDirection: "column",
        gap: 2,
      }}
    >
      <View style={{ paddingHorizontal: 8, paddingVertical: 4 }}>
        <Text style={{ fontSize: 12, fontWeight: "600", color: paletteToken("fg_muted") }}>
          {t("selectionBar.linkToAgent")}
        </Text>
      </View>
      {(() => {
        const agents = targetableAgents(profiles());
        if (agents.length === 0) {
          return (
            <View
              testID="selection-link-empty"
              style={{ paddingHorizontal: 8, paddingVertical: 6 }}
            >
              <Text style={{ fontSize: 12, color: paletteToken("fg_muted") }}>
                {t("selectionBar.noAgents")}
              </Text>
            </View>
          );
        }
        const list = (
          <View
            style={{
              display: "flex",
              flexDirection: "column",
              gap: 2,
              width: "100%",
              minWidth: 0,
            }}
          >
            {agents.map((profile) => (
              <AgentRow id={profile.id} name={profile.displayName} icon={profile.icon} />
            ))}
          </View>
        );
        if (agents.length <= LINK_AGENTS_VISIBLE) return list;
        // 高度写在视口框上，滚轮留在列表里（GPUI LINK_AGENTS_VIEW_H 先例）。
        return (
          <View
            testID="selection-link-agents"
            style={{ height: LINK_AGENTS_VIEW_H, width: "100%", minWidth: 0, flexShrink: 0 }}
          >
            <Scroll testID="selection-link-agents-scroll" style={{ width: "100%", height: "100%" }}>
              {list}
            </Scroll>
          </View>
        );
      })()}
      <UnlinkRow />
    </View>
  );
}

/** 一行 agent（link-to-{id}）：18px 品牌 SVG 徽标 + 名称；点击关菜单并起批次。 */
function AgentRow(props: { id: string; name: string; icon?: string }): JSX.Element {
  return (
    <View
      testID={`link-to-${props.id}`}
      style={{
        display: "flex",
        flexDirection: "row",
        alignItems: "center",
        gap: 6,
        paddingHorizontal: 8,
        paddingVertical: 6,
        borderRadius: 6,
        flexShrink: 0,
        cursor: "pointer",
      }}
      hoverStyle={{ backgroundColor: paletteToken("card_hover") }}
      onPress={() => void runLinkToAgent(props.id)}
    >
      <AgentBadge id={props.id} name={props.name} icon={props.icon} size={18} />
      <Text
        style={{
          flex: 1,
          minWidth: 0,
          fontSize: 12,
          color: paletteToken("fg"),
          numberOfLines: 1,
        }}
      >
        {props.name}
      </Text>
    </View>
  );
}

/** 取消链接行（batch-unlink-all）：warn 色，分隔线之上垫底不滚；点击关菜
 * 单并起批次（GPUI 踩过的坑：命中必须同时做这两件事）。 */
function UnlinkRow(): JSX.Element {
  return (
    <View
      testID="batch-unlink-all"
      style={{
        marginTop: 4,
        paddingTop: 4,
        borderTopWidth: 1,
        borderTopColor: paletteToken("border"),
      }}
    >
      <View
        testID="batch-unlink-all-btn"
        style={{
          display: "flex",
          flexDirection: "row",
          alignItems: "center",
          gap: 6,
          paddingHorizontal: 8,
          paddingVertical: 6,
          borderRadius: 6,
          cursor: "pointer",
        }}
        hoverStyle={{ backgroundColor: paletteToken("warn_hover") }}
        onPress={() => void runUnlinkAll()}
      >
        <Icon name="link2Off" size={12} color={paletteToken("warn")} />
        <Text style={{ fontSize: 12, fontWeight: "500", color: paletteToken("warn") }}>
          {t("selectionBar.unlinkAll")}
        </Text>
      </View>
    </View>
  );
}

// ---- 胶囊件 -------------------------------------------------------------------

/** 胶囊内一颗安静按钮（pill_button）：同 24px 框、图标+文案 gap、悬停处
 * 理（悬停颜色零帧——nsh 动效纪律；GPUI 的 InteractionSpring 只余色变，
 * 见视觉硬契约「颜色严禁走弹簧」）。 */
function pillButton(): ViewStyle {
  return {
    display: "flex",
    flexDirection: "row",
    alignItems: "center",
    justifyContent: "center",
    gap: 6,
    height: 24,
    paddingHorizontal: 8,
    borderRadius: 999,
    flexShrink: 0,
    cursor: "pointer",
  };
}

function pillLabel(): TextStyle {
  return { fontSize: 12, fontWeight: "500", color: paletteToken("fg") };
}

/** 选择控件与批量命令之间的发丝线（pill_divider）。 */
function PillDivider(): JSX.Element {
  return (
    <View
      style={{
        width: 1,
        height: 16,
        marginHorizontal: 2,
        flexShrink: 0,
        backgroundColor: paletteToken("border"),
      }}
    />
  );
}
