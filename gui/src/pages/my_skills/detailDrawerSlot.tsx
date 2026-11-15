import { Scroll, Text, View, openUrl, type JSX } from "nsh";
import { Switch } from "nsh/base";
import { paletteToken } from "../../services/themeMode";
import { t, tf } from "../../services/translation";
import { AgentBadge } from "../../chrome/agentRail";
import { Icon } from "../../chrome/icons";
import { Sheet } from "../../chrome/sheet";
import { Tip } from "../../chrome/tooltip";
import { openUninstallConfirm, uninstallTitleNamed } from "../../chrome/dialog";
import { IconSweep } from "../../chrome/feedback";
import { mixHex, motionEnabled, motionTransition } from "../../chrome/motion";
import {
  busy,
  descriptionChoices,
  profiles,
  selectedSkill,
  skills,
  updatingBatch,
  openSkillFolder,
  updateSkill,
} from "./state";
import type { AgentProfileRecord, SkillRecord } from "./filters";
import { etf } from "./extrasStrings";
import { DetailDrawerExtras } from "./detailDrawerExtras";
import { skillUpdateInFlight } from "./updates";
import { openSkillReader } from "./skillReaderSlot";
import {
  descriptionSource,
  formatUpdated,
  profileIsLinked,
  shownAuthor,
  sourceValues,
  upstreamNoteOf,
  wrapLongToken,
  type UpstreamNote,
} from "./detailFacts";
import {
  drawerTranslateSession,
  type DrawerTranslateSession,
} from "./drawerTranslate";
import {
  baselineMissing,
  drawerOps,
  pendingAgents,
  runMasterToggle,
  runToggle,
  runUninstall,
  type DrawerOps,
} from "./detailDrawerActions";

// 安装技能详情列 — my_skills/detail_drawer.rs（+ detail_facts /
// detail_parts / detail_agents / detail_uninstall）对位。
//
// 形态（chrome::sheet 约定）：页面行右侧固定 DETAIL_COLUMN_W 轨道内的浮
// 动面板，一次重绘出现；正文是列内唯一 Scroll（头/脚不滚，nsh 原生嵌套
// 滚动承担滚轮隔离）；关闭 = props.onClose（selectedSkill 置 null）。
//
// 事实面（detail_facts）：来源行（http 链接外开、SSH 原文）、作者、最近更
// 新各至多一条，URL 已含的不再单列；上游异常块三态（移除/改名/本地改动
// 的 danger/warn 说明）；本地改动是否记过基线由 MySkillsDrawer.upstreamNote
// 异步补齐（快照只带 kind）。
//
// agent 开关（detail_agents + commands.rs）：列表 = targetable profiles；
// 单行开关乐观翻链、失败回滚（flip/restore_agent_link）；master 一键批量
// （master_switch_* 只动未到位的行），pending 键 `skill:::agent` 挡住重复
// 点击；域返回 → 通知带（partial/skip/canonical 文案）→ refresh。
//
// 动作脚（drawer_actions）：查看 SKILL.md…（唯一入口 openSkillReader）；
// 有更新时的 warn 强调更新钮（label 三态：更新中/覆盖本地/更新）；打开目
// 录；卸载（确认弹窗 → 关抽屉 → busy=名字 → MySkills.uninstallSkill）。
// 卸载钮是全页唯一允许颜色缓动的控件（~80ms），其余 hover 零帧。
//
// nsh 组件体只执行一次：一切动态读取落在 JSX 子表达式的 IIFE 里，信号消
// 费在 JSX（NoticeBand/DetailDrawerTrack 的教训）。

export interface DetailDrawerSlotProps {
  skill: SkillRecord | null;
  agents: AgentProfileRecord[];
  busyToken: string | null;
  updatingBatch: ReadonlySet<string>;
  descriptionChoices: Record<string, boolean>;
  onClose(): void;
  /** 域命令注入位（deck 抽屉 ops 同款约定）：缺省绑定 MySkillsDrawer /
   * MySkills facade；无头测试注入假件。 */
  ops?: DrawerOps;
  /** 翻译会话注入位，同上。 */
  session?: DrawerTranslateSession;
  /** 「查看 SKILL.md…」入口（缺省 skillReaderSlot 的 openSkillReader）。 */
  onOpenReader?(name: string): void;
  /** 打开目录（缺省 state.openSkillFolder）。 */
  onOpenFolder?(name: string): void;
  /** 单卡更新（缺省 state.updateSkill）。 */
  onUpdate?(name: string): void;
}

// ---- 组件 ---------------------------------------------------------------------

export function DetailDrawerSlot(props: DetailDrawerSlotProps): JSX.Element {
  const ops = props.ops ?? drawerOps;
  const session = props.session ?? drawerTranslateSession;

  return (
    <Sheet style={{ width: "100%", height: "100%" }}>
      {(() => {
        const skill = props.skill;
        if (skill === null) {
          // 按名查不到卡（GPUI：render_detail_drawer 的 else 空分支）。
          return (
            <View testID="my-skills-detail-empty" style={{ flex: 1, minHeight: 0 }} />
          );
        }
        return (
          <View
            style={{ display: "flex", flexDirection: "column", flex: 1, minHeight: 0 }}
          >
            {drawerHeader(skill, props)}
            {/* 列内滚轮不外泄：正文是唯一 Scroll（chrome/sheet 约定）。 */}
            <Scroll
              testID="skill-detail-scroll"
              style={{ flex: 1, minHeight: 0 }}
              contentStyle={{
                paddingHorizontal: 16,
                paddingVertical: 12,
                display: "flex",
                flexDirection: "column",
                gap: 16,
              }}
            >
              {descriptionSection(props, session)}
              {factsSection(skill)}
              {upstreamSection(skill)}
              <DetailDrawerExtras skillName={skill.name} ops={ops} />
              {agentSection(props, ops)}
            </Scroll>
            {drawerActions(props, ops)}
          </View>
        );
      })()}
    </Sheet>
  );
}

// ---- 头部（drawer_header） ----------------------------------------------------

function drawerHeader(skill: SkillRecord, props: DetailDrawerSlotProps): JSX.Element {
  return (
    <View
      style={{
        display: "flex",
        flexDirection: "row",
        alignItems: "flex-start",
        justifyContent: "space-between",
        gap: 8,
        flexShrink: 0,
        paddingHorizontal: 16,
        paddingTop: 16,
        paddingBottom: 12,
        borderBottomWidth: 1,
        borderBottomColor: paletteToken("border"),
      }}
    >
      <View style={{ display: "flex", flexDirection: "column", gap: 8, flex: 1, minWidth: 0 }}>
        <View style={{ minWidth: 0 }} tooltip={<Tip text={skill.name} />}>
          <Text style={{ fontSize: 14, color: paletteToken("fg") }}>
            {wrapLongToken(skill.name)}
          </Text>
        </View>
        <View style={{ display: "flex", flexWrap: "wrap", alignItems: "center", gap: 4 }}>
          {skill.skillType === "local" ? (
            <Badge bg={paletteToken("ok_bg")} fg={paletteToken("ok")} label={t("detailPanel.localCreation")} />
          ) : (
            <Badge bg={paletteToken("well")} fg={paletteToken("fg_muted")} label={t("detailPanel.hubSkill")} />
          )}
          {skill.updateAvailable ? (
            <Badge
              bg={paletteToken("warn")}
              fg={paletteToken("on_accent")}
              label={t("detailPanel.updateAvailable")}
              testID={`drawer-update-badge-${skill.name}`}
            />
          ) : null}
          {skill.called && skill.called !== skill.name ? (
            <Badge
              bg={paletteToken("info_bg")}
              fg={paletteToken("info")}
              label={etf("mySkills.fromCalled", { called: skill.called })}
              testID={`drawer-called-badge-${skill.name}`}
            />
          ) : null}
        </View>
      </View>
      <View
        testID="close-detail-drawer"
        accessibilityLabel={t("detailPanel.dismissDrawer")}
        tooltip={<Tip text={t("detailPanel.dismissDrawer")} />}
        style={{
          display: "flex",
          alignItems: "center",
          justifyContent: "center",
          width: 28,
          height: 28,
          flexShrink: 0,
          borderRadius: 6,
          cursor: "pointer",
        }}
        hoverStyle={{ backgroundColor: paletteToken("card_hover") }}
        onPress={props.onClose}
      >
        <Icon name="x" size={18} color={paletteToken("fg_muted")} />
      </View>
    </View>
  );
}

/** chrome::badge：小号 semibold 徽章。 */
function Badge(props: { bg: string; fg: string; label: string; testID?: string }): JSX.Element {
  return (
    <View
      testID={props.testID}
      style={{
        paddingHorizontal: 6,
        paddingVertical: 1,
        borderRadius: 4,
        backgroundColor: props.bg,
      }}
    >
      <Text style={{ fontSize: 12, fontWeight: "600", color: props.fg }}>{props.label}</Text>
    </View>
  );
}

// ---- 描述段（drawer_body 的 description section + translate_button） ---------

function descriptionSection(
  props: DetailDrawerSlotProps,
  session: DrawerTranslateSession
): JSX.Element {
  return (
    <View style={{ display: "flex", flexDirection: "column", gap: 4 }}>
      {(() => {
        // 派生读取落在本表达式：generation/props 变化都重算。
        void session.generation();
        const skill = props.skill;
        if (skill === null) return null;
        const source = descriptionSource(skill);
        if (source === null) {
          return (
            <>
              <FieldLabel label={t("detailPanel.description")} />
              <View>
                <Text style={{ fontSize: 12, color: paletteToken("fg") }}>
                  {t("detailPanel.noDescription")}
                </Text>
              </View>
            </>
          );
        }
        const showing = session.resolveTranslate(skill.name, props.descriptionChoices);
        // schedule_when：开着译文而磁盘未命中时补一次引擎调用。
        session.ensureTranslation(source, showing);
        const shown = session.descriptionLine(source, showing);
        const translated = shown !== source;
        const gate = session.buttonGate(source, showing);
        const failure = showing ? session.failureLine() : null;
        return (
          <>
            <View
              style={{
                display: "flex",
                alignItems: "center",
                justifyContent: "space-between",
                gap: 8,
              }}
            >
              <FieldLabel label={t("detailPanel.description")} />
              {gate.visible ? (
                <TranslateButton
                  pending={gate.pending}
                  tip={showing ? t("detailPanel.showOriginal") : t("detailPanel.translate")}
                  onPress={() => session.applyChoice(skill.name, showing)}
                />
              ) : null}
            </View>
            <View style={{ display: "flex", flexDirection: "column", gap: 4 }}>
              <View testID={translated ? "drawer-translation" : `drawer-original-${skill.name}`}>
                <Text style={{ fontSize: 12, color: paletteToken("fg") }}>{shown}</Text>
              </View>
              {failure !== null ? (
                <View>
                  <Text style={{ fontSize: 12, color: paletteToken("fg_muted") }}>{failure}</Text>
                </View>
              ) : null}
            </View>
          </>
        );
      })()}
    </View>
  );
}

/** 语言按钮（translate_button）：目标语言不需要翻译的文案不画。 */
function TranslateButton(props: { pending: boolean; tip: string; onPress(): void }): JSX.Element {
  return (
    <View
      testID="drawer-translate"
      tooltip={<Tip text={props.tip} />}
      style={{
        display: "flex",
        alignItems: "center",
        justifyContent: "center",
        width: 22,
        height: 22,
        flexShrink: 0,
        borderRadius: 6,
        cursor: "pointer",
      }}
      hoverStyle={{ backgroundColor: paletteToken("card_hover") }}
      onPress={props.onPress}
    >
      <IconSweep
        name="languages"
        size={14}
        color={paletteToken("fg_muted")}
        sweep={props.pending}
        testID="drawer-translate-sweep"
      />
    </View>
  );
}

// ---- 事实卡（facts + detail_parts 的 source_line / fact_row） -----------------

function FieldLabel(props: { label: string }): JSX.Element {
  return <Text style={{ fontSize: 12, color: paletteToken("fg_muted") }}>{props.label}</Text>;
}

function factsSection(skill: SkillRecord): JSX.Element {
  return (
    <View style={{ display: "flex", flexDirection: "column", gap: 12, minWidth: 0 }}>
      {(() => {
        const lines = sourceValues(skill.gitUrl, skill.source, skill.name);
        const author = shownAuthor(skill.author, lines);
        const updated = formatUpdated(skill.lastUpdated);
        if (lines.length === 0 && author === null && updated === null) return null;
        return (
          <>
            {lines.length > 0 ? (
              <View style={{ display: "flex", flexDirection: "column", gap: 4, minWidth: 0 }}>
                <FieldLabel label={t("detailPanel.source")} />
                {lines.map((line, index) => (
                  <SourceLine index={index} line={line} />
                ))}
              </View>
            ) : null}
            {author !== null ? (
              <FactRow label={t("detailPanel.author")} value={atHandle(author)} />
            ) : null}
            {updated !== null ? (
              <FactRow label={t("detailPanel.lastUpdated")} value={updated} />
            ) : null}
          </>
        );
      })()}
    </View>
  );
}

/** facts()：作者展示带 @（已带则不重复）。 */
function atHandle(author: string): string {
  return author.startsWith("@") ? author : `@${author}`;
}

/** source_line：Link 用系统浏览器外开，Text 原文照排。 */
function SourceLine(props: { index: number; line: { kind: "link" | "text"; value: string } }): JSX.Element {
  const shown = wrapLongToken(props.line.value);
  if (props.line.kind === "text") {
    return (
      <View style={{ minWidth: 0 }}>
        <Text style={{ fontSize: 12, color: paletteToken("fg") }}>{shown}</Text>
      </View>
    );
  }
  return (
    <View
      testID={`skill-detail-source-${props.index}`}
      style={{ minWidth: 0, cursor: "pointer" }}
      tooltip={<Tip text={props.line.value} />}
      onPress={() => {
        openUrl(props.line.value);
      }}
    >
      <Text
        style={{
          fontSize: 12,
          color: paletteToken("accent"),
          textDecorationLine: "underline",
        }}
      >
        {shown}
      </Text>
    </View>
  );
}

function FactRow(props: { label: string; value: string }): JSX.Element {
  return (
    <View
      style={{
        display: "flex",
        alignItems: "flex-start",
        justifyContent: "space-between",
        gap: 12,
        minWidth: 0,
      }}
    >
      <FieldLabel label={props.label} />
      <View style={{ minWidth: 0 }}>
        <Text
          style={{
            fontSize: 12,
            color: paletteToken("fg"),
            textAlign: "right",
          }}
        >
          {props.value}
        </Text>
      </View>
    </View>
  );
}

// ---- 上游异常块（upstream_note + upstream_block） -----------------------------

function upstreamSection(skill: SkillRecord): JSX.Element {
  return (
    <View style={{ display: "flex", flexDirection: "column", gap: 4, minWidth: 0 }}>
      {(() => {
        // baselineMissing 信号变化（异步补读落地）时重算。
        const missing = baselineMissing()[skill.name] ?? false;
        const note = upstreamNoteOf(skill, missing);
        if (note === null) return null;
        const { title, body, tone } = upstreamBlockCopy(note);
        return (
          <>
            <Text style={{ fontSize: 12, fontWeight: "600", color: tone }}>{title}</Text>
            <Text style={{ fontSize: 12, color: paletteToken("fg") }}>{body}</Text>
          </>
        );
      })()}
    </View>
  );
}

/** detail_parts.rs upstream_block 的文案/色彩三分支。 */
function upstreamBlockCopy(note: UpstreamNote): { title: string; body: string; tone: string } {
  switch (note.kind) {
    case "removed":
      return {
        title: t("detailPanel.upstreamRemovedTitle"),
        body:
          note.source.length === 0
            ? t("skillCard.upstreamRemovedHint")
            : tf("detailPanel.upstreamRemovedDesc", {
                source: note.source,
                folder: note.folder,
              }),
        tone: paletteToken("danger"),
      };
    case "renamed":
      return {
        title: t("skillCard.upstreamRenamed"),
        body: tf("mySkills.updateIdentityChanged", { name: note.from, upstream: note.to }),
        tone: paletteToken("danger"),
      };
    case "localEdits":
      return {
        title: t("detailPanel.localChangesTitle"),
        body: note.baselineMissing
          ? t("detailPanel.localChangesUnknown")
          : t("detailPanel.localChanges"),
        tone: paletteToken("warn"),
      };
  }
}

// ---- agent 开关（agent_switches + master_agent_switch） -----------------------

function agentSection(props: DetailDrawerSlotProps, ops: DrawerOps): JSX.Element {
  return (
    <View style={{ display: "flex", flexDirection: "column", gap: 8 }}>
      <FieldLabel label={t("detailPanel.agentDeployments")} />
      <View
        style={{
          display: "flex",
          flexDirection: "column",
          gap: 6,
          padding: 8,
          borderRadius: 8,
          backgroundColor: paletteToken("card"),
          borderWidth: 1,
          borderColor: paletteToken("border"),
        }}
      >
        {(() => {
          const skill = props.skill;
          // pendingAgents 信号在 JSX 里消费：链接在途时 master 禁用。
          const pending = pendingAgents();
          if (skill === null) return null;
          const targetable = props.agents.filter((agent) => agent.targetable);
          if (targetable.length === 0) {
            return (
              <View style={{ paddingHorizontal: 8, paddingVertical: 6 }}>
                <Text style={{ fontSize: 12, color: paletteToken("fg_muted") }}>
                  {t("selectionBar.noAgents")}
                </Text>
              </View>
            );
          }
          const links = skill.agentLinks;
          const prefix = `${skill.name}::`;
          const busyMaster = [...pending].some((key) => key.startsWith(prefix));
          const pairs: [string, boolean][] = targetable.map((agent) => [
            agent.id,
            profileIsLinked(agent, links),
          ]);
          const allOn = pairs.every(([, on]) => on);
          return (
            <>
              <View
                testID={`drawer-agent-all-${skill.name}`}
                style={{
                  display: "flex",
                  alignItems: "center",
                  justifyContent: "space-between",
                  paddingHorizontal: 8,
                  paddingVertical: 6,
                  borderRadius: 6,
                  borderBottomWidth: 1,
                  borderBottomColor: paletteToken("border"),
                }}
              >
                <View style={{ minWidth: 0 }}>
                  <Text style={{ fontSize: 12, fontWeight: "500", color: paletteToken("fg") }}>
                    {t("detailPanel.allAgents")}
                  </Text>
                </View>
                <SlideSwitch
                  testID={`drawer-toggle-all-${skill.name}`}
                  checked={allOn}
                  disabled={busyMaster}
                  label={t(allOn ? "detailPanel.unlinkAll" : "detailPanel.linkAll")}
                  onChange={() => runMasterToggle(ops, skill.name, targetable)}
                />
              </View>
              {targetable.map((agent) => {
                const linked = profileIsLinked(agent, links);
                return (
                  <View
                    testID={`drawer-agent-row-${skill.name}-${agent.id}`}
                    style={{
                      display: "flex",
                      alignItems: "center",
                      justifyContent: "space-between",
                      paddingHorizontal: 8,
                      paddingVertical: 6,
                      borderRadius: 6,
                    }}
                    hoverStyle={{ backgroundColor: paletteToken("card_hover") }}
                  >
                    <View style={{ display: "flex", alignItems: "center", gap: 6, minWidth: 0 }}>
                      <AgentBadge
                        id={agent.id}
                        name={agent.displayName}
                        icon={agent.icon}
                        active={linked}
                        size={18}
                      />
                      <View style={{ minWidth: 0, overflow: "hidden" }}>
                        <Text
                          style={{
                            fontSize: 12,
                            fontWeight: "500",
                            color: paletteToken("fg"),
                          }}
                        >
                          {agent.displayName}
                        </Text>
                      </View>
                    </View>
                    <SlideSwitch
                      testID={`drawer-toggle-${skill.name}-${agent.id}`}
                      checked={linked}
                      disabled={false}
                      label={`${skill.name} · ${agent.displayName}`}
                      onChange={() => runToggle(ops, skill.name, agent.id, !linked)}
                    />
                  </View>
                );
              })}
            </>
          );
        })()}
      </View>
    </View>
  );
}

/** chrome::slide_switch 的 nsh 手绘（设置开关同款 28×16 轨道，开启态 ok
 * 绿显式）。滑块只用 transform 的 80ms 过渡（motion.ts 通道二）走到另一端：
 * 轨道颜色零帧换色，系统降低动态效果时直接落位。 */
function SlideSwitch(props: {
  testID: string;
  checked: boolean;
  disabled: boolean;
  label: string;
  onChange(): void;
}): JSX.Element {
  return (
    <Switch
      testID={props.testID}
      checked={props.checked}
      disabled={props.disabled}
      accessibilityLabel={props.label}
      trackStyle={{
        width: 28,
        height: 16,
        borderRadius: 8,
        borderWidth: 1,
        backgroundColor: props.checked ? paletteToken("ok") : paletteToken("input"),
        borderColor: props.checked ? paletteToken("ok_border") : paletteToken("edge"),
      }}
      thumbStyle={{
        width: 12,
        height: 12,
        borderRadius: 6,
        backgroundColor: paletteToken("on_accent"),
        transform: props.checked ? "translateX(12px)" : "translateX(0px)",
        ...motionTransition(),
      }}
      onChange={props.onChange}
    />
  );
}

// ---- 动作脚（drawer_actions + detail_uninstall） ------------------------------

function drawerActions(props: DetailDrawerSlotProps, ops: DrawerOps): JSX.Element {
  return (
    <View
      style={{
        display: "flex",
        flexDirection: "column",
        gap: 8,
        flexShrink: 0,
        paddingHorizontal: 16,
        paddingVertical: 12,
        borderTopWidth: 1,
        borderTopColor: paletteToken("border"),
      }}
    >
      {(() => {
        const skill = props.skill;
        if (skill === null) return null;
        const updating = skillUpdateInFlight(skill, props.busyToken, props.updatingBatch);
        const isBusy = props.busyToken === skill.name;
        const overwritesLocal = skill.upstreamChange?.kind === "local_changes";
        const name = skill.name;
        return (
          <>
            <ActionButton
              testID="drawer-view-skill-md"
              glyph="fileText"
              label={t("detailPanel.viewSkillMd")}
              onPress={() => (props.onOpenReader ?? openSkillReader)(name)}
            />
            {skill.updateAvailable ? (
              <ActionButton
                testID="drawer-update-btn"
                glyph="circleArrowUp"
                label={
                  updating
                    ? t("common.updating")
                    : overwritesLocal
                      ? t("skillCard.updateOverwritesLocal")
                      : t("common.update")
                }
                bg={paletteToken("warn")}
                fg={paletteToken("on_accent")}
                border={paletteToken("warn")}
                emphasized
                disabled={updating}
                onPress={() => {
                  if (updating) return;
                  (props.onUpdate ?? updateSkill)(name);
                }}
              />
            ) : null}
            <ActionButton
              testID="drawer-open-folder-btn"
              glyph="folder"
              label={t("detailPanel.openFolder")}
              onPress={() => (props.onOpenFolder ?? openSkillFolder)(name)}
            />
            <UninstallButton
              name={name}
              label={isBusy ? t("common.uninstalling") : t("common.uninstall")}
              onPress={() => {
                openUninstallConfirm(uninstallTitleNamed(name), () => {
                  // on_ok：先收抽屉再执行（GPUI select_detail(None) 顺序）。
                  props.onClose();
                  runUninstall(ops, name);
                  return true;
                });
              }}
            />
          </>
        );
      })()}
    </View>
  );
}

/** chrome::action_button：描边命令钮；emphasize 的悬停走透明度（更新钮
 * 的 warn 填充分支）。 */
function ActionButton(props: {
  testID: string;
  glyph: "fileText" | "circleArrowUp" | "folder";
  label: string;
  bg?: string;
  fg?: string;
  border?: string;
  emphasized?: boolean;
  disabled?: boolean;
  onPress(): void;
}): JSX.Element {
  const bg = props.bg ?? paletteToken("card");
  const fg = props.fg ?? paletteToken("fg");
  const border = props.border ?? paletteToken("border");
  const disabled = props.disabled ?? false;
  return (
    <View
      testID={props.testID}
      disabled={disabled}
      style={{
        display: "flex",
        flexDirection: "row",
        alignItems: "center",
        justifyContent: "center",
        gap: 8,
        width: "100%",
        paddingVertical: 8,
        paddingHorizontal: 8,
        borderRadius: 10,
        borderWidth: 1,
        borderColor: border,
        backgroundColor: bg,
        opacity: disabled ? 0.7 : 1,
        cursor: "pointer",
      }}
      hoverStyle={
        disabled
          ? undefined
          : props.emphasized
            ? { opacity: 0.92 }
            : { backgroundColor: paletteToken("card_hover") }
      }
      onPress={props.onPress}
    >
      {props.glyph === "circleArrowUp" ? (
        <Icon name="circleArrowUp" size={14} color={fg} />
      ) : props.glyph === "folder" ? (
        <Icon name="folder" size={14} color={fg} />
      ) : (
        <Icon name="fileText" size={14} color={fg} />
      )}
      <Text style={{ fontSize: 12, fontWeight: props.emphasized ? "600" : "500", color: fg }}>
        {props.label}
      </Text>
    </View>
  );
}

/** detail_uninstall.rs 的按钮：全页唯一允许颜色缓动的控件（~80ms 临界
 * 阻尼弹簧的对位；Reduce Motion 直接落位）。悬停面在 danger 色阶内插值
 * （hover_face 的 bg/border mix_rgb；字色通道 nsh 的 hoverStyle 够不到
 * 子 Text，保持 danger 常量）。 */
function UninstallButton(props: { name: string; label: string; onPress(): void }): JSX.Element {
  const rest = {
    bg: paletteToken("danger_bg"),
    border: paletteToken("danger_border"),
    fg: paletteToken("danger"),
  };
  const hover = {
    backgroundColor: mixHex(paletteToken("danger_bg"), paletteToken("danger_hover"), 1),
    borderColor: mixHex(paletteToken("danger_border"), paletteToken("danger"), 1),
  };
  return (
    <View
      testID="drawer-uninstall-btn"
      accessibilityLabel={props.label}
      style={{
        display: "flex",
        flexDirection: "row",
        alignItems: "center",
        justifyContent: "center",
        gap: 8,
        width: "100%",
        paddingVertical: 8,
        paddingHorizontal: 8,
        borderRadius: 10,
        borderWidth: 1,
        borderColor: rest.border,
        backgroundColor: rest.bg,
        cursor: "pointer",
        ...(motionEnabled()
          ? {
              transitionProperty: "background-color, border-color",
              transitionDuration: 80,
            }
          : {}),
      }}
      hoverStyle={hover}
      activeStyle={{ opacity: 0.88 }}
      onPress={props.onPress}
    >
      <Icon name="trash" size={14} color={rest.fg} />
      <Text
        style={{
          fontSize: 12,
          fontWeight: "600",
          color: rest.fg,
          textAlign: "center",
        }}
      >
        {props.label}
      </Text>
    </View>
  );
}

/** index.tsx 的轨道渲染助手：selectedSkill 非空时按名查卡并挂抽屉。读取
 * 全部落在 JSX 子表达式里（组件体只执行一次，体级读取会冻死抽屉）。 */
export function DetailDrawerTrack(): JSX.Element {
  return (
    <>
      {(() => {
        const name = selectedSkill();
        if (name === null) return null;
        const skill = skills().find((record) => record.name === name) ?? null;
        return (
          <View
            testID="my-skills-detail-track"
            style={{ width: "100%", height: "100%", display: "flex" }}
          >
            <DetailDrawerSlot
              skill={skill}
              agents={profiles()}
              busyToken={busy()}
              updatingBatch={updatingBatch()}
              descriptionChoices={descriptionChoices()}
              onClose={() => selectedSkill.set(null)}
            />
          </View>
        );
      })()}
    </>
  );
}
