import { Scroll, Text, View, effect, openUrl, signal, type JSX } from "nsh";
import { Marketplace, MarketplaceDrawer } from "skillstar-gui/native";
import { paletteToken } from "../../../services/themeMode";
import { Icon } from "../../../chrome/icons";
import { Tip } from "../../../chrome/tooltip";
import { Sheet } from "../../../chrome/sheet";
import { IconSpin, IconSweep } from "../../../chrome/feedback";
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

  // 组件体只执行一次（keep-alive 单挂载）：设置保存后 bump 会话代数，
  // keep-alive 页面从缓存重绘（shell/events 的频道契约）。
  translationPrefsChanged.subscribe(() => {
    session.generation.set(session.generation() + 1);
  });

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
      session.ensureTranslation(text, session.resolveTranslate(name));
    }
  });

  // 「查看 SKILL.md…」悬浮窗（skill_reader 的市场侧最小对位）。
  const md = signal<{ title: string; markdown: string } | null>(null);

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
            onRetry={() => drawer.retry()}
            onToggleTranslate={(showing) => session.applyChoice(props.identity.name, showing)}
          />
          <MetadataCard skill={props.skill} phase={drawer.phase()} />
          <AuditSection phase={drawer.phase()} />
          {(() => {
            const phaseValue = drawer.phase();
            const skill = props.skill;
            if (skill === null) return null;
            const description = shownDescription(skill, phaseValue);
            const readme = shownReadme(phaseValue, description);
            if (readme === null) return null;
            return (
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
                onPress={() => md.set({ title: skill.name, markdown: readme })}
              >
                <Icon name="fileText" size={14} color={paletteToken("fg")} />
                <Text style={{ fontSize: 12, fontWeight: "600", color: paletteToken("fg") }}>
                  {mt("detailPanel.viewSkillMd")}
                </Text>
              </View>
            );
          })()}
        </Scroll>
        <DetailActions
          skill={props.skill}
          identity={props.identity}
          busy={props.busy}
          onToggleInstall={props.onToggleInstall}
        />
      </View>
      <If when={md() !== null}>
        {() => {
          const doc = md();
          return doc === null ? (
            <View />
          ) : (
            <SkillMdOverlay
              title={doc.title}
              markdown={doc.markdown}
              onClose={() => md.set(null)}
            />
          );
        }}
      </If>
    </Sheet>
  );
}

// ---- 头部（detail_header 对位）---------------------------------------------

function DetailHeader(props: { skill: MarketSkill | null; onClose(): void }): JSX.Element {
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
        {(() => {
          const skill = props.skill;
          if (skill === null) return null;
          const showRank = skill.rank !== null && skill.rank > 0 && skill.rank <= 100;
          const showBadges = skill.installed || skill.category === "hot" || showRank;
          if (!showBadges) return null;
          return (
            <View style={{ display: "flex", flexWrap: "wrap", alignItems: "center", gap: 4 }}>
              {showRank ? (
                <Chip bg={paletteToken("warn_bg")} fg={paletteToken("warn")} label={`#${skill.rank ?? 0}`} />
              ) : null}
              {skill.category === "hot" ? (
                <Chip bg={paletteToken("danger_bg")} fg={paletteToken("danger")} label={mt("marketplace.hot")} />
              ) : null}
              {skill.installed ? (
                <Chip
                  bg={paletteToken("ok_bg")}
                  fg={paletteToken("ok")}
                  label={mt("marketplace.detailInstalled")}
                  testID="mk-detail-installed-chip"
                />
              ) : null}
            </View>
          );
        })()}
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
  onRetry(): void;
  onToggleTranslate(showing: boolean): void;
}): JSX.Element {
  return (
    <View style={{ display: "flex", flexDirection: "column", gap: 8 }}>
      {(() => {
        // 派生读取落在本表达式：phase/skill/generation 变化都重算。
        void props.session.generation();
        const skill = props.skill;
        const phaseValue = props.phase;
        if (skill === null) return null;
        const name = skill.name;
        const description = shownDescription(skill, phaseValue);
        const showing = props.session.resolveTranslate(name);
        const gate = description === null ? null : props.session.buttonGate(description, showing);
        const failure = showing ? props.session.failureLine() : null;
        const loading = phaseValue.kind === "loading";
        const failed = phaseValue.kind === "failed" ? phaseValue.error : null;
        return (
          <View testID="mk-detail-description" style={{ display: "flex", flexDirection: "column", gap: 8 }}>
            <View style={{ display: "flex", flexDirection: "row", alignItems: "center", justifyContent: "space-between", gap: 8 }}>
              <Text style={{ fontSize: 12, fontWeight: "600", color: paletteToken("fg_muted") }}>
                {mt("detailPanel.description")}
              </Text>
              {gate !== null && gate.visible ? (
                <TranslateButton
                  pending={gate.pending}
                  tip={showing ? mt("detailPanel.showOriginal") : mt("detailPanel.translate")}
                  onPress={() => props.onToggleTranslate(showing)}
                />
              ) : null}
            </View>
            {(() => {
              if (description === null) {
                if (loading) {
                  return <MutedLine testID="mk-detail-loading" text={mt("marketplace.detailLoading")} />;
                }
                if (failed === null) {
                  return <MutedLine text={mt("detailPanel.noDescription")} />;
                }
                return null;
              }
              // display_when：开着译文显示缓存行；这行未落地前显示原文。
              const shown = props.session.descriptionLine(description, showing);
              const translated = shown !== description;
              const body = (
                <View testID={translated ? "mk-drawer-translation" : "mk-drawer-original"}>
                  <TextBlock text={shown} />
                </View>
              );
              // failure_line：开着译文且会话记录了模型拒绝时，描述下方一
              // 行弱化提示（translation.rs failure_line）。
              if (showing && failure !== null) {
                return (
                  <View style={{ display: "flex", flexDirection: "column", gap: 4 }}>
                    {body}
                    <MutedLine text={failure} />
                  </View>
                );
              }
              return body;
            })()}
            {description !== null && loading ? (
              <MutedLine testID="mk-detail-loading" text={mt("marketplace.detailLoading")} />
            ) : null}
            {failed !== null ? <FailureRow error={failed} onRetry={props.onRetry} /> : null}
          </View>
        );
      })()}
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
      {props.error.trim().length > 0 ? (
        <View testID="mk-detail-error" tooltip={<Tip text={props.error} />}>
          <Text style={{ fontSize: 12, color: paletteToken("fg_muted"), numberOfLines: 2 }}>
            {props.error}
          </Text>
        </View>
      ) : null}
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
  return (
    <View style={{ display: "flex", flexDirection: "column", gap: 8 }}>
      {(() => {
        const skill = props.skill;
        const phaseValue = props.phase;
        if (skill === null) return null;
        const identity = resolveIdentity(skill, { source: null, name: skill.name });
        const details = phaseValue.kind === "ready" ? phaseValue.details : null;
        const weekly = details === null ? null : metaText(details.weeklyInstalls);
        const stars = details === null ? null : details.githubStars;
        const firstSeen = details === null ? null : metaText(details.firstSeen);
        if (!hasMetadata(skill, identity.source, weekly, stars, firstSeen)) return null;
        const author = skill.author !== null && skill.author.length > 0 ? skill.author : null;
        const gitUrl = skill.gitUrl.trim();
        return (
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
            {identity.source !== null ? (
              <MetaRow label={mt("marketplace.detailSource")} value={identity.source} testID="mk-detail-meta-source" />
            ) : null}
            {author !== null ? (
              <MetaRow label={mt("detailPanel.author")} value={`@${author}`} testID="mk-detail-meta-author" />
            ) : null}
            {skill.stars > 0 ? (
              <MetaRow
                label={mt("marketplace.detailInstalls")}
                value={formatInstalls(skill.stars)}
                testID="mk-detail-meta-installs"
              />
            ) : null}
            {weekly !== null ? (
              <MetaRow
                label={mt("marketplace.detailWeeklyInstalls")}
                value={weekly}
                testID="mk-detail-meta-weekly"
              />
            ) : null}
            {stars !== null ? (
              <MetaRow
                label={mt("marketplace.detailGithubStars")}
                value={formatInstalls(stars)}
                testID="mk-detail-meta-stars"
              />
            ) : null}
            {firstSeen !== null ? (
              <MetaRow
                label={mt("marketplace.detailFirstSeen")}
                value={firstSeen}
                testID="mk-detail-meta-seen"
              />
            ) : null}
            {skill.lastUpdated.length > 0 ? (
              <MetaRow
                label={mt("detailPanel.lastUpdated")}
                value={skill.lastUpdated}
                testID="mk-detail-meta-updated"
              />
            ) : null}
            {gitUrl.length > 0 ? <LinkRow url={gitUrl} /> : null}
          </View>
        );
      })()}
    </View>
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
  return (
    <View style={{ display: "flex", flexDirection: "column", gap: 8 }}>
      {(() => {
        const phaseValue = props.phase;
        if (phaseValue.kind !== "ready") return null;
        const audits = phaseValue.details.securityAudits;
        if (audits.length === 0) return null;
        return (
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
              <Each of={audits} key={(audit) => `${audit.name}:${audit.result}`}>
                {(audit: () => { name: string; result: string }) => {
                  const tone = auditTone(audit().result);
                  return (
                    <View
                      style={{ display: "flex", flexDirection: "row", alignItems: "center", justifyContent: "space-between", gap: 8, minWidth: 0 }}
                    >
                      <View style={{ minWidth: 0 }}>
                        <Text style={{ fontSize: 12, color: paletteToken("fg"), numberOfLines: 1 }}>
                          {audit().name}
                        </Text>
                      </View>
                      <Chip bg={tone.bg} fg={tone.fg} label={audit().result} />
                    </View>
                  );
                }}
              </Each>
            </View>
          </View>
        );
      })()}
    </View>
  );
}

// ---- 底部动作（detail_actions 对位）----------------------------------------

function DetailActions(props: {
  skill: MarketSkill | null;
  identity: MarketSkillId;
  busy: boolean;
  onToggleInstall(skill: MarketSkill): void;
}): JSX.Element {
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
        const installed = skill.installed;
        const busy = props.busy;
        const label = installed ? mt("common.uninstall") : mt("common.install");
        const bg = busy
          ? paletteToken("card_hover")
          : installed
            ? paletteToken("danger_bg")
            : paletteToken("accent");
        const fg = busy ? paletteToken("fg_muted") : installed ? paletteToken("danger") : paletteToken("on_accent");
        const hoverBg = busy
          ? paletteToken("card_hover")
          : installed
            ? paletteToken("danger_hover")
            : paletteToken("accent_hover");
        return (
          <View
            testID={`mk-detail-install-${skill.name}`}
            accessibilityLabel={label}
            disabled={busy}
            style={{
              display: "flex",
              flexDirection: "row",
              alignItems: "center",
              justifyContent: "center",
              gap: 8,
              width: "100%",
              paddingVertical: 8,
              borderRadius: 10,
              backgroundColor: bg,
              cursor: busy ? "default" : "pointer",
            }}
            hoverStyle={busy ? undefined : { backgroundColor: hoverBg }}
            onPress={() => {
              if (!busy) props.onToggleInstall(skill);
            }}
          >
            {busy ? (
              <IconSpin name={installed ? "trash" : "download"} size={14} color={fg} spin />
            ) : (
              <Icon name={installed ? "trash" : "download"} size={14} color={fg} />
            )}
            <Text style={{ fontSize: 12, fontWeight: "600", color: fg }}>{label}</Text>
          </View>
        );
      })()}
      {(() => {
        const skill = props.skill;
        if (skill === null) return null;
        const source = resolveIdentity(skill, props.identity).source;
        if (source === null) return null;
        const url = `https://skills.sh/${source}/${skill.name}`;
        return (
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
              openUrl(url);
            }}
          >
            <Icon name="externalLink" size={12} color={paletteToken("accent")} />
            <Text
              style={{ fontSize: 12, color: paletteToken("accent"), textDecorationLine: "underline" }}
            >
              {mt("detailPanel.viewOnSkillsSh")}
            </Text>
          </View>
        );
      })()}
    </View>
  );
}

// ---- 共用小件 ----------------------------------------------------------------

/** text_block（column.rs:761-786）：按行渲染，空行占一格。 */
function TextBlock(props: { text: string }): JSX.Element {
  const lines = props.text.split("\n");
  return (
    <View style={{ display: "flex", flexDirection: "column", gap: 4 }}>
      {lines.map((line) => (
        <Text style={{ fontSize: 12, lineHeight: 18, color: paletteToken("fg") }}>
          {line.length === 0 ? " " : line}
        </Text>
      ))}
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
