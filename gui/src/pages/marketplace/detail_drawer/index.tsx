import { Scroll, Text, View, computed, effect, openUrl, signal, type JSX, type Signal } from "nsh";
import { Marketplace, MarketplaceDrawer } from "skillstar-gui/native";
import { paletteToken } from "../../../services/themeMode";
import { bindDelayedFlag } from "../../../chrome/delayedFlag";
import { Icon } from "../../../chrome/icons";
import { Tip } from "../../../chrome/tooltip";
import { Sheet } from "../../../chrome/sheet";
import { IconSpin, IconSweep, Pulse } from "../../../chrome/feedback";
import { translationPrefsChanged } from "../../../shell/events";
import { mt } from "../marketStrings";
import { formatInstalls, type MarketSkill } from "../types";
import type { MarketSkillId } from "../readSide";
import {
  createDetailReader,
  parseDetailPayload,
  resolveIdentity,
  shownDescription,
  shownReadme,
  type DetailPhase,
  type DetailReader,
} from "./detailState";
import { createTranslateSession, facadeBridge, type TranslateSession } from "./translate";
import { SkillMdOverlay } from "./skillMd";

// 市场详情列 — marketplace/detail_drawer/（mod.rs + column.rs +
// translate.rs）切片。基座约定：页面（../index.tsx）在 readSide.selected
// 非空时于固定宽 DETAIL_COLUMN_W 轨道渲染本组件（一次重绘出现、主网格
// 同帧退一列），关闭 = props.onClose（readSide.clearSelection）；安装/
// 卸载复用 props.onToggleInstall（确认弹窗+通知带+busy 已在基座）。
//
// 数据流（mod.rs 对位）：打开按身份（source 优先，git URL 回退）读取
// MarketplaceDrawer.skillDetail；迟到响应按 epoch+身份丢弃；stale/miss
// 后台再同步一次不循环；翻译选择与实时 fill 属本切片（translate.ts）。
//
// nsh 组件体只执行一次：一切从 props/signal 派生的读取都落在 JSX 表达
// 式里（NoticeBand/DialogHost 的教训），翻译桥的同步读以 generation 代
// 数信号配对（选择写入/译文落地/拒绝变化都 bump 它）。

// 页面实体（keep-alive 挂载一次）：读取状态机 + 翻译会话各一份。
const detail = createDetailReader({
  reader: async (source, name) =>
    parseDetailPayload(await MarketplaceDrawer.skillDetail(source, name)),
  syncer: (scope) => Marketplace.syncScope(scope),
});
const translateSession = createTranslateSession(facadeBridge);

export interface MarketDetailColumnProps {
  skill: MarketSkill | null;
  identity: MarketSkillId;
  busy: boolean;
  onClose: () => void;
  onToggleInstall: (skill: MarketSkill) => void;
  /** 读取状态机注入位（detailDrawer.tsx 的 ops? 同款约定）：缺省用页面
   * 单例（真实 facade 桥）；无头测试注入可控假件。 */
  reader?: DetailReader;
  /** 翻译会话注入位，同上。 */
  session?: TranslateSession;
}

export function MarketDetailColumn(props: MarketDetailColumnProps): JSX.Element {
  const drawer = props.reader ?? detail;
  const session = props.session ?? translateSession;
  const detailLoading = bindDelayedFlag(() => drawer.phase().kind === "loading");

  // 组件体只执行一次：设置保存后清失败/拒绝态并 bump 会话代数，随组件卸载退订。
  effect(() =>
    translationPrefsChanged.subscribe(() => {
      session.onPrefsChanged();
    }),
  );

  // 打开身份跟随页面选择（props.skill 由页面在 JSX 里重算，effect 跟踪
  // 它驱动读取状态机的 open/close/换卡——mod.rs toggle 的相位半）。
  effect(() => {
    drawer.open(
      props.skill === null ? null : resolveIdentity(props.skill, props.identity)
    );
  });

  // 翻译排程：当前显示行开着译文而磁盘未命中时补一次引擎调用
  // （boards schedule translation for the shown text 的列侧对位）。读
  // generation：按下翻译按钮/设置保存/译文落地后都要重问一次。
  effect(() => {
    void session.generation();
    const phaseValue = drawer.phase();
    const skill = props.skill;
    const name = props.identity.name;
    if (skill === null) return;
    const text = shownDescription(skill, phaseValue);
    if (text !== null) {
      session.ensureTranslation(name, text, session.resolveTranslate(name));
    }
  });

  // 「查看 SKILL.md…」悬浮窗（skill_reader 的市场侧最小对位）。
  const md = signal<{ title: string; markdown: string } | null>(null);
  const readmeText = computed(() => {
    const skill = props.skill;
    if (skill === null) return null;
    const phaseValue = drawer.phase();
    const description = shownDescription(skill, phaseValue);
    return shownReadme(phaseValue, description);
  });

  return (
    <Sheet style={{ width: "100%", height: "100%" }}>
      <View
        testID="mk-detail-column"
        style={{ display: "flex", flexDirection: "column", flex: 1, minHeight: 0 }}
      >
        <DetailHeader skill={props.skill} onClose={props.onClose} />
        {/* 列内滚轮不外泄：正文是唯一的 Scroll（chrome/sheet 的约定）。 */}
        <Scroll
          style={{ flex: 1, minHeight: 0 }}
          contentStyle={{
            paddingHorizontal: 16,
            paddingVertical: 12,
            display: "flex",
            flexDirection: "column",
            gap: 16,
          }}
        >
          <DescriptionSection
            skill={props.skill}
            phase={drawer.phase()}
            session={session}
            delayedLoading={detailLoading}
            onRetry={() => drawer.retry()}
            onToggleTranslate={(showing) => session.applyChoice(props.identity.name, showing)}
          />
          <MetadataCard skill={props.skill} phase={drawer.phase()} />
          <AuditSection phase={drawer.phase()} />
          <If when={readmeText() !== null}>
            <View
              testID="mk-view-skill-md"
              style={{
                display: "flex",
                flexDirection: "row",
                alignItems: "center",
                justifyContent: "center",
                gap: 8,
                width: "100%",
                paddingVertical: 8,
                borderRadius: 10,
                borderWidth: 1,
                borderColor: paletteToken("border"),
                backgroundColor: paletteToken("card"),
                cursor: "pointer",
              }}
              hoverStyle={{ backgroundColor: paletteToken("card_hover") }}
              onPress={() => {
                const skill = props.skill;
                const readme = readmeText();
                if (skill !== null && readme !== null) {
                  md.set({ title: skill.name, markdown: readme });
                }
              }}
            >
              <Icon name="fileText" size={14} color={paletteToken("fg")} />
              <Text style={{ fontSize: 12, fontWeight: "600", color: paletteToken("fg") }}>
                {mt("detailPanel.viewSkillMd")}
              </Text>
            </View>
          </If>
        </Scroll>
        <DetailActions
          skill={props.skill}
          identity={props.identity}
          busy={props.busy}
          onToggleInstall={props.onToggleInstall}
        />
      </View>
      <If when={md() !== null}>
        <SkillMdOverlay
          title={md()?.title ?? ""}
          markdown={md()?.markdown ?? ""}
          onClose={() => md.set(null)}
        />
      </If>
    </Sheet>
  );
}

// ---- 头部（detail_header 对位）---------------------------------------------

function DetailHeader(props: { skill: MarketSkill | null; onClose(): void }): JSX.Element {
  const showRank = () =>
    props.skill !== null && props.skill.rank !== null && props.skill.rank > 0 && props.skill.rank <= 100;
  const showHot = () => props.skill?.category === "hot";
  const showInstalled = () => props.skill?.installed === true;
  const showBadges = () => props.skill !== null && (showInstalled() || showHot() || showRank());

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
        <View
          style={{ minWidth: 0 }}
          tooltip={props.skill === null ? undefined : <Tip text={props.skill.name} />}
        >
          <Text
            testID="mk-detail-name"
            style={{ fontSize: 14, color: paletteToken("fg"), numberOfLines: 1 }}
          >
            {props.skill === null ? "" : props.skill.name}
          </Text>
        </View>
        {/* 徽章行只在有内容时占位（badge_row 的 show_badges）。 */}
        <If when={showBadges()}>
          <View style={{ display: "flex", flexWrap: "wrap", alignItems: "center", gap: 4 }}>
            <If when={showRank()}>
              <Chip bg={paletteToken("warn_bg")} fg={paletteToken("warn")} label={`#${props.skill?.rank ?? 0}`} />
            </If>
            <If when={showHot()}>
              <Chip bg={paletteToken("danger_bg")} fg={paletteToken("danger")} label={mt("marketplace.hot")} />
            </If>
            <If when={showInstalled()}>
              <Chip
                bg={paletteToken("ok_bg")}
                fg={paletteToken("ok")}
                label={mt("marketplace.detailInstalled")}
                testID="mk-detail-installed-chip"
              />
            </If>
          </View>
        </If>
      </View>
      <View
        testID="close-market-detail-drawer"
        accessibilityLabel={mt("detailPanel.dismissDrawer")}
        tooltip={<Tip text={mt("detailPanel.dismissDrawer")} />}
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
        hoverStyle={{ backgroundColor: paletteToken("panel_hover") }}
        onPress={props.onClose}
      >
        <Icon name="x" size={14} color={paletteToken("fg_muted")} />
      </View>
    </View>
  );
}

function Chip(props: { bg: string; fg: string; label: string; testID?: string }): JSX.Element {
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
      <Text style={{ fontSize: 10, fontWeight: "600", color: props.fg }}>{props.label}</Text>
    </View>
  );
}

// ---- 描述段（description_section 对位）-------------------------------------

function DescriptionSection(props: {
  skill: MarketSkill | null;
  phase: DetailPhase;
  session: TranslateSession;
  delayedLoading: Signal<boolean>;
  onRetry(): void;
  onToggleTranslate(showing: boolean): void;
}): JSX.Element {
  const description = computed(() => {
    const skill = props.skill;
    if (skill === null) return null;
    return shownDescription(skill, props.phase);
  });
  const showing = computed(() => {
    void props.session.generation();
    const skill = props.skill;
    return skill === null ? false : props.session.resolveTranslate(skill.name);
  });
  const gate = computed(() => {
    void props.session.generation();
    const desc = description();
    return desc === null ? null : props.session.buttonGate(desc, showing());
  });
  const showRetry = () => {
    const desc = description();
    const transport = props.session.transportFailed();
    return desc !== null && transport !== null && transport === desc.trim();
  };
  const failure = computed(() => {
    void props.session.generation();
    const desc = description();
    if (desc === null) return null;
    return showing() || showRetry() ? props.session.failureLine(desc) : null;
  });
  const isLoading = () => props.phase.kind === "loading";
  const showPulses = () => isLoading() && props.delayedLoading();
  const failedError = () => (props.phase.kind === "failed" ? props.phase.error : null);
  const shownText = computed(() => {
    void props.session.generation();
    const desc = description();
    if (desc === null) return "";
    return props.session.descriptionLine(desc, showing());
  });
  const isTranslated = () => {
    const desc = description();
    return desc !== null && shownText() !== desc;
  };

  return (
    <If when={props.skill !== null}>
      <View testID="mk-detail-description" style={{ display: "flex", flexDirection: "column", gap: 8 }}>
        <View style={{ display: "flex", flexDirection: "row", alignItems: "center", justifyContent: "space-between", gap: 8 }}>
          <Text style={{ fontSize: 12, fontWeight: "600", color: paletteToken("fg_muted") }}>
            {mt("detailPanel.description")}
          </Text>
          <If when={gate()?.visible === true}>
            <TranslateButton
              pending={gate()?.pending === true}
              tip={showing() ? mt("detailPanel.showOriginal") : mt("detailPanel.translate")}
              onPress={() => props.onToggleTranslate(showing())}
            />
          </If>
        </View>
        <If
          when={description() !== null}
          else={
            <View style={{ display: "flex", flexDirection: "column", gap: 8 }}>
              <If when={showPulses()}>
                <DetailPulses />
              </If>
              <If when={!isLoading() && failedError() === null}>
                <MutedLine text={mt("detailPanel.noDescription")} />
              </If>
            </View>
          }
        >
          <View style={{ display: "flex", flexDirection: "column", gap: 4 }}>
            <View testID={isTranslated() ? "mk-drawer-translation" : "mk-drawer-original"}>
              <TextBlock text={shownText()} />
            </View>
            <If when={failure() !== null}>
              <MutedLine text={failure() ?? ""} />
            </If>
            <If when={showRetry()}>
              <TranslateRetry
                onPress={() => {
                  const skill = props.skill;
                  const desc = description();
                  if (skill !== null && desc !== null) {
                    props.session.retryTransport(skill.name, desc);
                  }
                }}
              />
            </If>
            <If when={showPulses()}>
              <DetailPulses />
            </If>
          </View>
        </If>
        <If when={failedError() !== null}>
          <FailureRow error={failedError() ?? ""} onRetry={props.onRetry} />
        </If>
      </View>
    </If>
  );
}

/** 详情读取的占位：晚 300ms 才出现，替换「正在读取」文字行。 */
function DetailPulses(): JSX.Element {
  return (
    <View
      testID="mk-detail-loading"
      accessibilityState={{ busy: true }}
      style={{ display: "flex", flexDirection: "column", gap: 8 }}
    >
      <Pulse id="mk-detail-pulse-1" style={{ width: 220, height: 12 }} />
      <Pulse id="mk-detail-pulse-2" style={{ width: 180, height: 12 }} secondary />
      <Pulse id="mk-detail-pulse-3" style={{ width: 140, height: 12 }} secondary />
    </View>
  );
}

/** 传输失败后的段落重试：清失败再 fill。 */
function TranslateRetry(props: { onPress(): void }): JSX.Element {
  return (
    <View
      testID="mk-translate-retry"
      style={{
        display: "flex",
        flexDirection: "row",
        alignItems: "center",
        alignSelf: "flex-start",
        gap: 4,
        paddingHorizontal: 8,
        paddingVertical: 4,
        borderRadius: 6,
        borderWidth: 1,
        borderColor: paletteToken("border"),
        backgroundColor: paletteToken("card"),
        cursor: "pointer",
      }}
      hoverStyle={{ backgroundColor: paletteToken("card_hover") }}
      onPress={props.onPress}
    >
      <Text style={{ fontSize: 12, fontWeight: "500", color: paletteToken("fg") }}>
        {mt("common.retry")}
      </Text>
    </View>
  );
}

/** 语言按钮（translate_button 对位）：目标语言不需要翻译的文案不画。 */
function TranslateButton(props: { pending: boolean; tip: string; onPress(): void }): JSX.Element {
  return (
    <View
      testID="mk-detail-translate"
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
        testID="mk-detail-translate-sweep"
      />
    </View>
  );
}

/** 失败行（failure_row 对位）：标题 + 可折叠原因 + 重试。 */
function FailureRow(props: { error: string; onRetry(): void }): JSX.Element {
  return (
    <View style={{ display: "flex", flexDirection: "column", gap: 8 }}>
      <Text style={{ fontSize: 12, color: paletteToken("danger") }}>
        {mt("marketplace.detailFailed")}
      </Text>
      <If when={props.error.trim().length > 0}>
        <View testID="mk-detail-error" tooltip={<Tip text={props.error} />}>
          <Text style={{ fontSize: 12, color: paletteToken("fg_muted"), numberOfLines: 2 }}>
            {props.error}
          </Text>
        </View>
      </If>
      <View
        testID="mk-detail-retry"
        style={{
          display: "flex",
          flexDirection: "row",
          alignItems: "center",
          alignSelf: "flex-start",
          gap: 4,
          paddingHorizontal: 8,
          paddingVertical: 4,
          borderRadius: 6,
          borderWidth: 1,
          borderColor: paletteToken("border"),
          backgroundColor: paletteToken("card"),
          cursor: "pointer",
        }}
        hoverStyle={{ backgroundColor: paletteToken("card_hover") }}
        onPress={props.onRetry}
      >
        <Text style={{ fontSize: 12, fontWeight: "500", color: paletteToken("fg") }}>
          {mt("common.retry")}
        </Text>
      </View>
    </View>
  );
}

// ---- 元数据卡（metadata_card 对位）-----------------------------------------

function hasMetadata(
  skill: MarketSkill,
  source: string | null,
  weekly: string | null,
  githubStars: number | null,
  firstSeen: string | null
): boolean {
  return (
    source !== null ||
    (skill.author !== null && skill.author.length > 0) ||
    skill.stars > 0 ||
    weekly !== null ||
    githubStars !== null ||
    firstSeen !== null ||
    skill.lastUpdated.length > 0 ||
    skill.gitUrl.trim().length > 0
  );
}

function metaText(text: string | null): string | null {
  const trimmed = text?.trim();
  return trimmed !== undefined && trimmed.length > 0 ? trimmed : null;
}

function MetadataCard(props: { skill: MarketSkill | null; phase: DetailPhase }): JSX.Element {
  const meta = computed(() => {
    const skill = props.skill;
    if (skill === null) return null;
    const identity = resolveIdentity(skill, { source: null, name: skill.name });
    const details = props.phase.kind === "ready" ? props.phase.details : null;
    const weekly = details === null ? null : metaText(details.weeklyInstalls);
    const stars = details === null ? null : details.githubStars;
    const firstSeen = details === null ? null : metaText(details.firstSeen);
    if (!hasMetadata(skill, identity.source, weekly, stars, firstSeen)) return null;
    const author = skill.author !== null && skill.author.length > 0 ? skill.author : null;
    const gitUrl = skill.gitUrl.trim();
    return {
      source: identity.source,
      author,
      installs: skill.stars > 0 ? formatInstalls(skill.stars) : null,
      weekly,
      githubStars: stars !== null ? formatInstalls(stars) : null,
      firstSeen,
      lastUpdated: skill.lastUpdated.length > 0 ? skill.lastUpdated : null,
      gitUrl: gitUrl.length > 0 ? gitUrl : null,
    };
  });

  return (
    <If when={meta() !== null}>
      <View
        testID="mk-detail-metadata"
        style={{
          display: "flex",
          flexDirection: "column",
          gap: 8,
          padding: 12,
          borderRadius: 12,
          backgroundColor: paletteToken("card"),
          borderWidth: 1,
          borderColor: paletteToken("border"),
        }}
      >
        <If when={meta()?.source !== null}>
          <MetaRow label={mt("marketplace.detailSource")} value={meta()?.source ?? ""} testID="mk-detail-meta-source" />
        </If>
        <If when={meta()?.author !== null}>
          <MetaRow label={mt("detailPanel.author")} value={`@${meta()?.author ?? ""}`} testID="mk-detail-meta-author" />
        </If>
        <If when={meta()?.installs !== null}>
          <MetaRow label={mt("marketplace.detailInstalls")} value={meta()?.installs ?? ""} testID="mk-detail-meta-installs" />
        </If>
        <If when={meta()?.weekly !== null}>
          <MetaRow label={mt("marketplace.detailWeeklyInstalls")} value={meta()?.weekly ?? ""} testID="mk-detail-meta-weekly" />
        </If>
        <If when={meta()?.githubStars !== null}>
          <MetaRow label={mt("marketplace.detailGithubStars")} value={meta()?.githubStars ?? ""} testID="mk-detail-meta-stars" />
        </If>
        <If when={meta()?.firstSeen !== null}>
          <MetaRow label={mt("marketplace.detailFirstSeen")} value={meta()?.firstSeen ?? ""} testID="mk-detail-meta-seen" />
        </If>
        <If when={meta()?.lastUpdated !== null}>
          <MetaRow label={mt("detailPanel.lastUpdated")} value={meta()?.lastUpdated ?? ""} testID="mk-detail-meta-updated" />
        </If>
        <If when={meta()?.gitUrl !== null}>
          <LinkRow url={meta()?.gitUrl ?? ""} />
        </If>
      </View>
    </If>
  );
}

function MetaRow(props: { label: string; value: string; testID: string }): JSX.Element {
  return (
    <View style={{ display: "flex", flexDirection: "row", alignItems: "center", justifyContent: "space-between", gap: 12, minWidth: 0 }}>
      <Text style={{ fontSize: 12, fontWeight: "600", color: paletteToken("fg_muted"), flexShrink: 0 }}>
        {props.label}
      </Text>
      <View style={{ minWidth: 0 }} tooltip={<Tip text={props.value} />}>
        <Text testID={props.testID} style={{ fontSize: 12, color: paletteToken("fg"), numberOfLines: 1 }}>
          {props.value}
        </Text>
      </View>
    </View>
  );
}

/** Git 来源行（link_row 对位）：外开系统浏览器。 */
function LinkRow(props: { url: string }): JSX.Element {
  return (
    <View style={{ display: "flex", flexDirection: "column", gap: 2, minWidth: 0 }}>
      <Text style={{ fontSize: 12, fontWeight: "600", color: paletteToken("fg_muted"), flexShrink: 0 }}>
        {mt("detailPanel.gitSource")}
      </Text>
      <View
        testID="mk-detail-git"
        tooltip={<Tip text={props.url} />}
        style={{ minWidth: 0, cursor: "pointer" }}
        onPress={() => {
          openUrl(props.url);
        }}
      >
        <Text
          style={{
            fontSize: 12,
            color: paletteToken("accent"),
            numberOfLines: 1,
            textDecorationLine: "underline",
          }}
        >
          {props.url}
        </Text>
      </View>
    </View>
  );
}

// ---- 安全审计（audit_section 对位）-----------------------------------------

/** audit_tone（column.rs:565-578）：结果文本决定色彩，未知保持中性。 */
function auditTone(result: string): { fg: string; bg: string } {
  const lower = result.toLowerCase();
  if (lower.includes("fail") || lower.includes("error") || lower.includes("high")) {
    return { fg: paletteToken("danger"), bg: paletteToken("danger_bg") };
  }
  if (
    lower.includes("pass") ||
    lower.includes("ok") ||
    lower.includes("safe") ||
    lower.includes("clean")
  ) {
    return { fg: paletteToken("ok"), bg: paletteToken("ok_bg") };
  }
  return { fg: paletteToken("fg"), bg: paletteToken("well") };
}

function AuditSection(props: { phase: DetailPhase }): JSX.Element {
  const audits = computed(() =>
    props.phase.kind === "ready" ? props.phase.details.securityAudits : [],
  );
  return (
    <If when={audits().length > 0}>
      <View style={{ display: "flex", flexDirection: "column", gap: 8 }}>
        <Text style={{ fontSize: 12, fontWeight: "600", color: paletteToken("fg_muted") }}>
          {mt("marketplace.detailAudits")}
        </Text>
        <View
          testID="mk-detail-audits"
          style={{
            display: "flex",
            flexDirection: "column",
            gap: 6,
            padding: 12,
            borderRadius: 12,
            backgroundColor: paletteToken("card"),
            borderWidth: 1,
            borderColor: paletteToken("border"),
          }}
        >
          <Each of={audits()} key={(audit) => `${audit.name}:${audit.result}`}>
            {(audit: () => { name: string; result: string }) => {
              const tone = () => auditTone(audit().result);
              return (
                <View
                  style={{ display: "flex", flexDirection: "row", alignItems: "center", justifyContent: "space-between", gap: 8, minWidth: 0 }}
                >
                  <View style={{ minWidth: 0 }}>
                    <Text style={{ fontSize: 12, color: paletteToken("fg"), numberOfLines: 1 }}>
                      {audit().name}
                    </Text>
                  </View>
                  <Chip bg={tone().bg} fg={tone().fg} label={audit().result} />
                </View>
              );
            }}
          </Each>
        </View>
      </View>
    </If>
  );
}

// ---- 底部动作（detail_actions 对位）----------------------------------------

function DetailActions(props: {
  skill: MarketSkill | null;
  identity: MarketSkillId;
  busy: boolean;
  onToggleInstall(skill: MarketSkill): void;
}): JSX.Element {
  const installed = () => props.skill?.installed === true;
  const label = () => (installed() ? mt("common.uninstall") : mt("common.install"));
  const bg = () => (installed() ? paletteToken("danger_bg") : paletteToken("accent"));
  const fg = () => (installed() ? paletteToken("danger") : paletteToken("on_accent"));
  const hoverBg = () => (installed() ? paletteToken("danger_hover") : paletteToken("accent_hover"));
  const sourceUrl = computed(() => {
    const skill = props.skill;
    if (skill === null) return null;
    const source = resolveIdentity(skill, props.identity).source;
    return source === null ? null : `https://skills.sh/${source}/${skill.name}`;
  });

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
      <If when={props.skill !== null}>
        <View
          testID={`mk-detail-install-${props.skill?.name ?? ""}`}
          accessibilityLabel={label()}
          disabled={props.busy}
          style={{
            display: "flex",
            flexDirection: "row",
            alignItems: "center",
            justifyContent: "center",
            gap: 8,
            width: "100%",
            paddingVertical: 8,
            borderRadius: 10,
            backgroundColor: bg(),
            cursor: props.busy ? "default" : "pointer",
          }}
          hoverStyle={props.busy ? undefined : { backgroundColor: hoverBg() }}
          onPress={() => {
            if (!props.busy && props.skill !== null) props.onToggleInstall(props.skill);
          }}
        >
          <IconSpin name={installed() ? "trash" : "download"} size={14} color={fg()} spin={props.busy} />
          <Text style={{ fontSize: 12, fontWeight: "600", color: fg() }}>{label()}</Text>
        </View>
      </If>
      <If when={sourceUrl() !== null}>
        <View
          testID="mk-detail-source"
          style={{
            display: "flex",
            flexDirection: "row",
            alignItems: "center",
            justifyContent: "center",
            gap: 4,
            width: "100%",
            paddingVertical: 4,
            cursor: "pointer",
          }}
          onPress={() => {
            const url = sourceUrl();
            if (url !== null) openUrl(url);
          }}
        >
          <Icon name="externalLink" size={12} color={paletteToken("accent")} />
          <Text
            style={{ fontSize: 12, color: paletteToken("accent"), textDecorationLine: "underline" }}
          >
            {mt("detailPanel.viewOnSkillsSh")}
          </Text>
        </View>
      </If>
    </View>
  );
}

// ---- 共用小件 ----------------------------------------------------------------

/** text_block（column.rs:761-786）：按行渲染，空行占一格。 */
function TextBlock(props: { text: string }): JSX.Element {
  return (
    <View style={{ display: "flex", flexDirection: "column", gap: 4 }}>
      <Each of={props.text.split("\n")} key="index">
        {(line: () => string) => (
          <Text style={{ fontSize: 12, lineHeight: 18, color: paletteToken("fg") }}>
            {line().length === 0 ? " " : line()}
          </Text>
        )}
      </Each>
    </View>
  );
}

function MutedLine(props: { text: string; testID?: string }): JSX.Element {
  // testID 挂 View：无头引擎按可命中视图寻址 id:（Text 不参与命中）。
  return (
    <View testID={props.testID}>
      <Text style={{ fontSize: 12, color: paletteToken("fg_muted") }}>{props.text}</Text>
    </View>
  );
}
