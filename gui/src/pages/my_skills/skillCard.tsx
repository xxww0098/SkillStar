import { Text, View, computed, effect, openUrl, untracked, type Accessor, type JSX } from "nsh";
import { isLight, paletteToken } from "../../services/themeMode";
import { t, tf } from "../../services/translation";
import { AgentFooterBar, AgentRail, type AgentRailSlot } from "../../chrome/agentRail";
import {
  distinctSource,
  skillCardFrameStyle,
  skillCardHoverStyle,
} from "../../chrome/cardShell";
import { RepoAvatar, githubOwnerForSkill, sanitizeGithubOwner } from "../../chrome/repoAvatar";
import { Tip } from "../../chrome/tooltip";
import { Icon } from "../../chrome/icons";
import { IconSpin } from "../../chrome/feedback";
import { SKILL_CARD_W } from "../../layout";
import { type SkillRecord } from "./filters";
import { etf } from "./extrasStrings";
import { cardTranslatedText, ensureCardDescriptionTranslation } from "./cardTranslate";
import { openPublishSkill } from "./publishModal";
import { pt } from "./publishStrings";
import {
  busy,
  descriptionChoices,
  openDetail,
  openSkillFolder,
  selectedBatch,
  selectedSkill,
  targetableProfiles,
  toggleBatchMember,
  updateSkill,
  updatingBatch,
} from "./state";
import { drawerOps, pendingAgents, runToggle } from "./detailDrawerActions";

// 我的技能工作台卡身：
// 统一使用 16px 圆角 `skillCardFrameStyle` 与 38px `AgentFooterBar` 底栏。
// 交互面：
// - 主卡点击 → 开/收详情抽屉；
// - 显式勾选框（`skill-card-check-${name}`）→ 进出批量选择；
// - 底栏 `AgentRail` 品牌 SVG 轮播开关 → 直接切换该技能与智能体的链接；
// - 状态胶囊 → 单卡更新或上游状态提示；
// - 本地创建徽章 → 打开技能目录。
//
// 卡面不放保存状态行（保存中/已保存/失败）：底栏开关乐观翻转本身就是
// 结果，失败仍由通知带说明，重试就是再点一次同一个图标。
//
// nsh 组件体只执行一次：一切信号与 `props.skill()` 派生读取全部落在 JSX 表达式内。

export interface SkillCardHandle {
  onOpen(name: string): void;
  onSelect(name: string): void;
  onUpdate(name: string): void;
  onOpenFolder(name: string): void;
}

export const cardHandle: SkillCardHandle = {
  onOpen: openDetail,
  onSelect: toggleBatchMember,
  onUpdate: updateSkill,
  onOpenFolder: openSkillFolder,
};

export function SkillCard(props: {
  skill: () => SkillRecord;
  listMode: boolean;
  cardWidth?: number;
}): JSX.Element {
  const skillName = computed(() => props.skill().name);
  const batchSelected = computed(() => selectedBatch().has(skillName()));
  const drawerSelected = computed(() => selectedSkill() === skillName());
  const cardSelected = computed(() => batchSelected() || drawerSelected());
  return (
    <View
      testID={`skill-card-${skillName()}`}
      style={{
        ...skillCardFrameStyle(cardSelected()),
        width: props.listMode ? "100%" : (props.cardWidth ?? SKILL_CARD_W),
        justifyContent: "space-between",
      }}
      hoverStyle={skillCardHoverStyle(cardSelected())}
      onPress={() => cardHandle.onOpen(skillName())}
    >
      <View
        style={{
          display: "flex",
          flexDirection: "column",
          gap: 6,
          flex: 1,
          minWidth: 0,
          minHeight: 0,
          paddingHorizontal: 14,
          paddingTop: 12,
          paddingBottom: 8,
        }}
      >
        <CardHeader skill={props.skill} skillName={skillName} batchSelected={batchSelected} />
        <Description skill={props.skill} skillName={skillName} />
      </View>
      <CardFooter skill={props.skill} skillName={skillName} />
    </View>
  );
}

/** 副标题及其跳转目标（GPUI `skill_card/source_chip.rs`）：标签仓库路径
 * 优先、作者 handle 兜底；`url` 只在能推出真实页面时给出。 */
export interface SkillSource {
  label: string;
  url: string | null;
}

/** HTTP(S) 地址去掉尾斜杠与 `.git` 克隆后缀；不是地址时 null。 */
function browserUrl(value: string | null | undefined): string | null {
  let url = value?.trim() ?? "";
  const lower = url.toLowerCase();
  if (!lower.startsWith("https://") && !lower.startsWith("http://")) return null;
  while (url.endsWith("/")) url = url.slice(0, -1);
  if (url.toLowerCase().endsWith(".git")) url = url.slice(0, -4);
  while (url.endsWith("/")) url = url.slice(0, -1);
  return url.length === 0 ? null : url;
}

/** `owner/repo[/path]` → GitHub 页面地址；主机名、空段或非法字符一律
 * null（不是 GitHub 路径，不猜）。 */
function githubPageUrl(source: string): string | null {
  const path = source.replace(/^\/+|\/+$/g, "");
  if (path.length === 0) return null;
  const parts = path.split("/");
  if (parts.length < 2) return null;
  if (sanitizeGithubOwner(parts[0]) === null) return null;
  if (!parts.slice(1).every((part) => /^[A-Za-z0-9._-]+$/.test(part))) return null;
  return `https://github.com/${parts.join("/")}`;
}

/** 卡面副标题：非本地技能优先展示与技能名不同的仓库路径，退回 `@author`。
 * 仓库行链接已跟踪的 http(s) git URL，其次来源自身是地址，最后拼
 * GitHub；作者行链接个人页。推不出地址时 `url` 为 null，副标题只是文字。 */
export function skillSource(skill: SkillRecord): SkillSource | null {
  if (skill.skillType === "local") return null;
  const repo = distinctSource(skill.source, skill.name);
  if (repo !== null) {
    return {
      label: repo,
      url: browserUrl(skill.gitUrl) ?? browserUrl(repo) ?? githubPageUrl(repo),
    };
  }
  const author = skill.author?.trim().replace(/^@+/, "") ?? "";
  if (author.length === 0) return null;
  const handle = sanitizeGithubOwner(author);
  return {
    label: `@${author}`,
    url: handle === null ? null : `https://github.com/${handle}`,
  };
}

/** 卡面副标题文本（来源 chip 的标签）。 */
export function skillSubtitle(skill: SkillRecord): string | null {
  return skillSource(skill)?.label ?? null;
}

function CardHeader(props: {
  skill: () => SkillRecord;
  skillName: Accessor<string>;
  batchSelected: Accessor<boolean>;
}): JSX.Element {
  const sourceInfo = computed(() => skillSource(props.skill()));
  return (
    <View
      style={{
        display: "flex",
        flexDirection: "row",
        alignItems: "center",
        gap: 10,
        minWidth: 0,
        minHeight: 32,
      }}
    >
      <SkillRepoAvatar
        skill={props.skill}
        skillName={props.skillName}
        batchSelected={props.batchSelected}
      />
      <View
        style={{
          display: "flex",
          flexDirection: "column",
          gap: 2,
          flex: 1,
          minWidth: 0,
        }}
      >
        <View
          style={{
            display: "flex",
            flexDirection: "row",
            alignItems: "center",
            gap: 6,
            minWidth: 0,
          }}
        >
          <Text
            testID={`skill-card-name-${props.skillName()}`}
            style={{
              fontSize: 14,
              lineHeight: 18,
              fontWeight: "600",
              color: paletteToken("fg"),
              numberOfLines: 1,
              flexShrink: 1,
            }}
          >
            {props.skillName()}
          </Text>
          <If
            when={
              typeof props.skill().called === "string" &&
              props.skill().called !== "" &&
              props.skill().called !== props.skillName()
            }
          >
            <View
              testID={`skill-card-called-${props.skillName()}`}
              style={{
                paddingHorizontal: 6,
                paddingVertical: 1,
                borderRadius: 6,
                borderWidth: 1,
                borderColor: paletteToken("info_border"),
                backgroundColor: paletteToken("info_bg"),
                flexShrink: 0,
              }}
            >
              <Text
                style={{
                  fontSize: 10,
                  fontWeight: "600",
                  color: paletteToken("info"),
                  numberOfLines: 1,
                }}
              >
                {etf("mySkills.fromCalled", { called: props.skill().called ?? "" })}
              </Text>
            </View>
          </If>
        </View>
        <If when={sourceInfo() !== null}>
          <SourceSubtitle skillName={props.skillName} sourceInfo={sourceInfo} />
        </If>
      </View>
      <View
        style={{
          display: "flex",
          flexDirection: "row",
          alignItems: "center",
          gap: 6,
          flexShrink: 0,
        }}
      >
        <LocalPublishPill skill={props.skill} skillName={props.skillName} />
        <LocalBadge skill={props.skill} skillName={props.skillName} />
        <StatusPill skill={props.skill} skillName={props.skillName} />
      </View>
    </View>
  );
}

/** 卡面副标题（来源 chip）：有可跳转页面时是外链——手型光标、悬停转
 * accent 并加下划线，点击经系统浏览器打开；没有可点目标时是纯文字。
 * 点击不再冒泡到卡身，详情抽屉不会被顺带打开。 */
function SourceSubtitle(props: {
  skillName: Accessor<string>;
  sourceInfo: Accessor<SkillSource | null>;
}): JSX.Element {
  const label = () => props.sourceInfo()?.label ?? "";
  const url = () => props.sourceInfo()?.url ?? null;
  return (
    <Text
      testID={`skill-card-source-${props.skillName()}`}
      role={url() === null ? undefined : "link"}
      style={{
        fontSize: 11,
        lineHeight: 14,
        color: paletteToken("fg_muted"),
        numberOfLines: 1,
        cursor: url() === null ? "default" : "pointer",
      }}
      hoverStyle={
        url() === null
          ? undefined
          : { color: paletteToken("accent"), textDecorationLine: "underline" }
      }
      tooltip={url() === null ? undefined : <Tip text={url() ?? ""} />}
      onPress={
        url() === null
          ? undefined
          : () => {
              const target = url();
              if (target !== null) void openUrl(target);
            }
      }
    >
      {label()}
    </Text>
  );
}

function SkillRepoAvatar(props: {
  skill: () => SkillRecord;
  skillName: Accessor<string>;
  batchSelected: Accessor<boolean>;
}): JSX.Element {
  return (
    <View
      testID={`skill-card-check-${props.skillName()}`}
      role="checkbox"
      accessibilityLabel={props.skillName()}
      style={{
        position: "relative",
        width: 32,
        height: 32,
        borderRadius: 9,
        overflow: "hidden",
        flexShrink: 0,
        cursor: "pointer",
      }}
      onPress={() => cardHandle.onSelect(props.skillName())}
    >
      <RepoAvatar
        owner={githubOwnerForSkill(props.skill())}
        local={props.skill().skillType === "local"}
        selected={props.batchSelected()}
        size={32}
        radius={9}
      />
      <View
        style={{
          position: "absolute",
          top: 0,
          right: 0,
          bottom: 0,
          left: 0,
          borderRadius: 9,
          display: "flex",
          alignItems: "center",
          justifyContent: "center",
          opacity: props.batchSelected() ? 1 : 0,
          backgroundColor: props.batchSelected()
            ? paletteToken("accent")
            : isLight()
              ? "rgba(22, 32, 47, 0.45)"
              : "rgba(6, 10, 18, 0.56)",
        }}
        hoverStyle={{ opacity: 1 }}
      >
        <If
          when={props.batchSelected()}
          else={
            <View
              style={{
                width: 16,
                height: 16,
                borderRadius: 4,
                borderWidth: 1,
                borderColor: paletteToken("edge"),
                backgroundColor: paletteToken("card"),
              }}
            />
          }
        >
          <Icon name="check" size={16} color={paletteToken("on_accent")} />
        </If>
      </View>
    </View>
  );
}

function Description(props: {
  skill: () => SkillRecord;
  skillName: Accessor<string>;
}): JSX.Element {
  effect(() => {
    const s = props.skill();
    ensureCardDescriptionTranslation(s.name, s.description, s.localizedDescription);
  });
  const descChoice = computed(() => descriptionChoices()[props.skillName()]);
  const descText = computed(() => {
    const s = props.skill();
    const localized = s.localizedDescription ?? cardTranslatedText(s.description);
    const useLocalized = descChoice() !== false && typeof localized === "string";
    const text = (useLocalized ? (localized as string) : s.description).trim();
    return text.length > 0 ? text : t("skillCard.noDescription");
  });
  return (
    <View style={{ minWidth: 0 }}>
      <Text
        testID={`skill-card-desc-${props.skillName()}`}
        style={{
          fontSize: 12,
          lineHeight: 17,
          color: paletteToken("fg_muted"),
          numberOfLines: 2,
        }}
      >
        {descText()}
      </Text>
    </View>
  );
}

function LocalPublishPill(props: {
  skill: () => SkillRecord;
  skillName: Accessor<string>;
}): JSX.Element {
  return (
    <If when={props.skill().skillType === "local"}>
      <View
        testID={`skill-card-publish-${props.skillName()}`}
        role="button"
        accessibilityLabel={pt("publish.action")}
        tooltip={<Tip text={pt("publish.action")} />}
        style={{
          display: "flex",
          flexDirection: "row",
          alignItems: "center",
          justifyContent: "center",
          width: 22,
          height: 22,
          borderRadius: 999,
          backgroundColor: paletteToken("accent_soft"),
          borderWidth: 1,
          borderColor: paletteToken("border"),
          cursor: "pointer",
        }}
        hoverStyle={{ backgroundColor: paletteToken("panel_hover") }}
        onPress={() => openPublishSkill(props.skillName())}
      >
        <Icon name="gitBranch" size={11} color={paletteToken("accent")} />
      </View>
    </If>
  );
}

function LocalBadge(props: {
  skill: () => SkillRecord;
  skillName: Accessor<string>;
}): JSX.Element {
  return (
    <If when={props.skill().skillType === "local"}>
      <View
        testID={`skill-card-local-${props.skillName()}`}
        style={{
          display: "flex",
          flexDirection: "row",
          alignItems: "center",
          gap: 4,
          height: 22,
          paddingHorizontal: 7,
          borderRadius: 999,
          backgroundColor: paletteToken("ok_bg"),
          borderWidth: 1,
          borderColor: paletteToken("ok_border"),
          cursor: "pointer",
        }}
        hoverStyle={{ backgroundColor: paletteToken("ok_hover") }}
        onPress={() => cardHandle.onOpenFolder(props.skillName())}
      >
        <Icon name="hardDrive" size={10} color={paletteToken("ok")} />
        <Text style={{ fontSize: 10, fontWeight: "600", color: paletteToken("ok") }}>
          {t("mySkills.scopeLocal")}
        </Text>
      </View>
    </If>
  );
}

function upstreamProblemKind(skill: SkillRecord): "removed" | "identity_changed" | null {
  if (skill.skillType === "local") return null;
  const kind = skill.upstreamChange?.kind;
  return kind === "removed" || kind === "identity_changed" ? kind : null;
}

function overwritesLocalChanges(skill: SkillRecord): boolean {
  return skill.upstreamChange?.kind === "local_changes";
}

function StatusPill(props: {
  skill: () => SkillRecord;
  skillName: Accessor<string>;
}): JSX.Element {
  const problem = computed(() => upstreamProblemKind(props.skill()));
  const updating = computed(() => {
    const token = busy();
    if (token === null) return false;
    const name = props.skillName();
    if (token === "update_all" || token === "batch_update") {
      return updatingBatch().has(name);
    }
    return token === name;
  });
  return (
    <If
      when={problem() !== null}
      else={
        <If
          when={
            props.skill().skillType !== "local" &&
            problem() === null &&
            props.skill().updateAvailable
          }
        >
          <View
            testID={`skill-card-update-${props.skillName()}`}
            style={{
              display: "flex",
              flexDirection: "row",
              alignItems: "center",
              gap: 4,
              flexShrink: 0,
              height: 24,
              paddingHorizontal: 9,
              borderRadius: 999,
              backgroundColor: paletteToken("warn_bg"),
              borderWidth: 1,
              borderColor: paletteToken("warn_border"),
              cursor: updating() ? "default" : "pointer",
            }}
            hoverStyle={
              updating()
                ? undefined
                : { backgroundColor: paletteToken("warn_hover") }
            }
            onPress={() => {
              if (!updating()) cardHandle.onUpdate(props.skillName());
            }}
          >
            <IconSpin
              name="circleArrowUp"
              size={12}
              color={paletteToken("warn")}
              spin={updating()}
            />
            <Text style={{ fontSize: 11, fontWeight: "600", color: paletteToken("warn") }}>
              {updating()
                ? t("common.updating")
                : overwritesLocalChanges(props.skill())
                  ? t("skillCard.updateOverwritesLocal")
                  : t("common.update")}
            </Text>
          </View>
        </If>
      }
    >
      <View
        testID={`skill-card-upstream-${props.skillName()}`}
        style={{
          display: "flex",
          flexDirection: "row",
          alignItems: "center",
          gap: 4,
          flexShrink: 0,
          height: 24,
          paddingHorizontal: 9,
          borderRadius: 999,
          backgroundColor: paletteToken("danger_bg"),
          borderWidth: 1,
          borderColor: paletteToken("danger_border"),
        }}
      >
        <Icon name="alertTriangle" size={12} color={paletteToken("danger_fg")} />
        <Text style={{ fontSize: 11, fontWeight: "600", color: paletteToken("danger_fg") }}>
          {problem() === "removed"
            ? t("skillCard.upstreamRemoved")
            : t("skillCard.upstreamRenamed")}
        </Text>
      </View>
    </If>
  );
}

function CardFooter(props: {
  skill: () => SkillRecord;
  skillName: Accessor<string>;
}): JSX.Element {
  const cardPendingKey = computed(() => {
    const prefix = `${props.skillName()}::`;
    const set = pendingAgents();
    if (set.size === 0) return "";
    let hits = "";
    for (const key of set) {
      if (key.startsWith(prefix)) hits += `${key}|`;
    }
    return hits;
  });
  const railSlots = computed<AgentRailSlot[]>(() => {
    const s = props.skill();
    const pendingKey = cardPendingKey();
    const pendingSet = pendingKey.length > 0 ? untracked(pendingAgents) : null;
    const prefix = `${s.name}::`;
    const links = new Set(s.agentLinks);
    return targetableProfiles().map((profile) => {
      const linked = links.has(profile.id) || links.has(profile.displayName);
      const pending = pendingSet !== null && pendingSet.has(`${prefix}${profile.id}`);
      return {
        profile: { id: profile.id, displayName: profile.displayName, icon: profile.icon },
        linked,
        pending,
        label: tf(linked ? "skillCard.unlinkAgent" : "skillCard.linkAgent", {
          agent: profile.displayName,
        }),
      };
    });
  });
  return (
    <If when={targetableProfiles().length > 0}>
      <AgentFooterBar>
        <AgentRail
          scopeKey={props.skillName()}
          railPrefix="skill-rail"
          slotPrefix="skill-slot"
          slots={railSlots()}
          pending={false}
          onSlotClick={(slot) =>
            runToggle(drawerOps, props.skillName(), slot.profile.id, !slot.linked)
          }
        />
      </AgentFooterBar>
    </If>
  );
}
