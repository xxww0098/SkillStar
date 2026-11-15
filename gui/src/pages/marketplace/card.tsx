import { Text, View, type JSX } from "nsh";
import { paletteToken } from "../../services/themeMode";
import { CARD_GAP, SKILL_CARD_H, SKILL_CARD_W } from "../../layout";
import { AgentFooterBar } from "../../chrome/agentRail";
import {
  distinctSource,
  SourceMeta,
  skillCardFrameStyle,
  skillCardHoverStyle,
} from "../../chrome/cardShell";
import { RepoAvatar, githubOwnerForSkill, sanitizeGithubOwner } from "../../chrome/repoAvatar";
import { Icon } from "../../chrome/icons";
import { Pulse } from "../../chrome/feedback";
import {
  formatInstalls,
  skillBlurb,
  type MarketSkill,
  type PublisherRecord,
} from "./types";
import { mt, mtf } from "./marketStrings";

// 市场卡与发布者卡 — 与已安装卡/卡组共用 16px 圆角卡壳与 40px well 底栏：
// 悬停高亮边框为强调色 + card_hover；选中时固定强调色边框 + card_active。
// 卡身只保留身份、两行描述摘要、主安装动作与底栏来源/安装量/状态徽章。

export interface MarketCardProps {
  skill: MarketSkill;
  selected: boolean;
  busy: boolean;
  listMode?: boolean;
  cardWidth?: number;
  onPress: () => void;
  onToggleInstall: () => void;
}

export function MarketCard(props: MarketCardProps): JSX.Element {
  return (
    <View
      testID={`mk-card-${props.skill.name}`}
      role="button"
      accessibilityLabel={props.skill.name}
      style={{
        ...skillCardFrameStyle(props.selected),
        width: props.listMode ? "100%" : (props.cardWidth ?? SKILL_CARD_W),
        flexShrink: 0,
        justifyContent: "space-between",
      }}
      hoverStyle={skillCardHoverStyle(props.selected)}
      onPress={props.onPress}
    >
      <View
        style={{
          display: "flex",
          flexDirection: "column",
          gap: 8,
          flex: 1,
          minWidth: 0,
          minHeight: 0,
          padding: 14,
          paddingBottom: 8,
        }}
      >
        <View style={{ display: "flex", flexDirection: "row", alignItems: "center", gap: 10 }}>
          <RepoAvatar
            owner={githubOwnerForSkill(props.skill)}
            selected={props.selected}
            size={32}
            radius={9}
          />
          <View
            style={{
              display: "flex",
              flexDirection: "row",
              alignItems: "center",
              gap: 6,
              flex: 1,
              minWidth: 0,
            }}
          >
            <If when={props.skill.rank !== null && props.skill.rank <= 100}>
              <RankBadge rank={props.skill.rank ?? 0} />
            </If>
            <View style={{ minWidth: 0, flex: 1 }}>
              <Text
                style={{
                  fontSize: 14,
                  fontWeight: "600",
                  color: paletteToken("fg"),
                  numberOfLines: 1,
                }}
              >
                {props.skill.name}
              </Text>
            </View>
          </View>
          <InstallAction
            skill={props.skill}
            busy={props.busy}
            onPress={props.onToggleInstall}
          />
        </View>
        <Text
          style={{
            fontSize: 12,
            lineHeight: 17,
            color: paletteToken("fg_muted"),
            numberOfLines: 2,
          }}
        >
          {skillBlurb(props.skill) ?? mt("skillCard.noDescription")}
        </Text>
      </View>
      <AgentFooterBar>
        <View
          style={{
            display: "flex",
            flexDirection: "row",
            alignItems: "center",
            gap: 6,
            flex: 1,
            minWidth: 0,
          }}
        >
          <If when={distinctSource(props.skill.source, props.skill.name) !== null}>
            <SourceMeta
              label={distinctSource(props.skill.source, props.skill.name) ?? ""}
              maxWidth={168}
            />
          </If>
          <If
            when={props.skill.upstreamRemoved}
            else={
              <If when={props.skill.updateAvailable}>
                <ExceptionChip label={mt("common.update")} tone="warn" />
              </If>
            }
          >
            <ExceptionChip label={mt("marketplace.removed")} tone="danger" />
          </If>
        </View>
        <If when={props.skill.stars > 0}>
          <Text
            style={{
              fontSize: 12,
              fontWeight: "600",
              color: paletteToken("warn"),
              flexShrink: 0,
            }}
          >
            {`★ ${formatInstalls(props.skill.stars)}`}
          </Text>
        </If>
      </AgentFooterBar>
    </View>
  );
}

function RankBadge(props: { rank: number }): JSX.Element {
  return (
    <View
      style={{
        flexShrink: 0,
        paddingHorizontal: 5,
        paddingVertical: 1,
        borderRadius: 6,
        backgroundColor: paletteToken("warn_bg"),
        display: "flex",
      }}
    >
      <Text style={{ fontSize: 10, fontWeight: "700", color: paletteToken("warn") }}>
        {`#${props.rank}`}
      </Text>
    </View>
  );
}

function ExceptionChip(props: { label: string; tone: "danger" | "warn" }): JSX.Element {
  return (
    <View
      style={{
        display: "flex",
        alignItems: "center",
        flexShrink: 0,
        paddingHorizontal: 6,
        paddingVertical: 2,
        borderRadius: 6,
        borderWidth: 1,
        borderColor:
          props.tone === "danger" ? paletteToken("danger_border") : paletteToken("warn_border"),
        backgroundColor:
          props.tone === "danger" ? paletteToken("danger_bg") : paletteToken("warn_bg"),
      }}
    >
      <Text
        style={{
          fontSize: 10,
          fontWeight: "600",
          color: props.tone === "danger" ? paletteToken("danger") : paletteToken("warn"),
        }}
      >
        {props.label}
      </Text>
    </View>
  );
}

/** 安装/卸载主动作：未装 = accent 实底「安装」；已装 = danger 弱底描边「卸载」；
 * busy = 禁用态显示省略号。 */
function InstallAction(props: { skill: MarketSkill; busy: boolean; onPress: () => void }): JSX.Element {
  return (
    <View
      testID={`mk-act-${props.skill.name}`}
      style={{
        display: "flex",
        flexDirection: "row",
        alignItems: "center",
        justifyContent: "center",
        flexShrink: 0,
        height: 26,
        paddingHorizontal: 10,
        borderRadius: 7,
        cursor: props.busy ? "default" : "pointer",
        borderWidth: 1,
        backgroundColor: props.busy
          ? paletteToken("card_hover")
          : props.skill.installed
            ? paletteToken("danger_bg")
            : paletteToken("accent"),
        borderColor:
          !props.busy && props.skill.installed ? paletteToken("danger_border") : "transparent",
      }}
      hoverStyle={
        props.busy
          ? undefined
          : {
              backgroundColor: props.skill.installed
                ? paletteToken("danger_hover")
                : paletteToken("accent_hover"),
            }
      }
      onPress={props.busy ? undefined : props.onPress}
    >
      <Text
        style={{
          fontSize: 12,
          fontWeight: props.busy ? "500" : "600",
          color: props.busy
            ? paletteToken("fg_muted")
            : props.skill.installed
              ? paletteToken("danger")
              : paletteToken("on_accent"),
        }}
      >
        {props.busy
          ? "…"
          : props.skill.installed
            ? mt("common.uninstall")
            : mt("common.install")}
      </Text>
    </View>
  );
}

/** 发布者行卡：统一 16px 圆角卡片风格。 */
export function PublisherCard(props: {
  publisher: PublisherRecord;
  onPress: () => void;
}): JSX.Element {
  return (
    <View
      testID={`pub-card-${props.publisher.name}`}
      role="button"
      accessibilityLabel={props.publisher.name}
      style={{
        width: "100%",
        height: 76,
        display: "flex",
        flexDirection: "row",
        alignItems: "center",
        gap: 12,
        padding: 14,
        borderRadius: 16,
        borderWidth: 1,
        borderColor: paletteToken("border"),
        backgroundColor: paletteToken("card"),
        cursor: "pointer",
      }}
      hoverStyle={{ backgroundColor: paletteToken("card_hover"), borderColor: paletteToken("accent") }}
      onPress={props.onPress}
    >
      <RepoAvatar
        owner={sanitizeGithubOwner(props.publisher.name)}
        size={36}
        radius={10}
      />
      <View style={{ display: "flex", flexDirection: "column", gap: 2, flex: 1, minWidth: 0 }}>
        <View style={{ display: "flex", flexDirection: "row", alignItems: "center", gap: 6, minWidth: 0 }}>
          <Text style={{ fontSize: 14, fontWeight: "600", color: paletteToken("fg"), numberOfLines: 1 }}>
            {props.publisher.name}
          </Text>
          <View
            style={{
              flexShrink: 0,
              paddingHorizontal: 6,
              paddingVertical: 1,
              borderRadius: 6,
              backgroundColor: paletteToken("ok_bg"),
              borderWidth: 1,
              borderColor: paletteToken("ok_border"),
              display: "flex",
            }}
          >
            <Text style={{ fontSize: 10, fontWeight: "600", color: paletteToken("ok") }}>
              {mt("marketplace.officialBadge")}
            </Text>
          </View>
        </View>
        <View style={{ display: "flex", flexDirection: "row", alignItems: "center", gap: 10 }}>
          <View style={{ display: "flex", flexDirection: "row", alignItems: "center", gap: 4 }}>
            <Icon name="folder" size={12} color={paletteToken("fg_muted")} />
            <Text style={{ fontSize: 12, color: paletteToken("fg_muted") }}>
              {mtf("marketplace.repoCount", { count: props.publisher.repoCount })}
            </Text>
          </View>
          <View style={{ display: "flex", flexDirection: "row", alignItems: "center", gap: 4 }}>
            <Icon name="package" size={12} color={paletteToken("fg_muted")} />
            <Text style={{ fontSize: 12, color: paletteToken("fg_muted") }}>
              {mtf("marketplace.skillCount", { count: props.publisher.skillCount })}
            </Text>
          </View>
        </View>
      </View>
      <Icon name="chevronRight" size={14} color={paletteToken("fg_muted")} />
    </View>
  );
}

/** 骨架占位卡（与真实卡同 16px 轨道）。 */
export function MarketSkeletonTile(props: { index: number; cardWidth?: number }): JSX.Element {
  return (
    <View
      testID={`mk-skel-${props.index}`}
      style={{
        width: props.cardWidth ?? SKILL_CARD_W,
        height: SKILL_CARD_H,
        flexShrink: 0,
        display: "flex",
        flexDirection: "column",
        gap: 10,
        padding: 14,
        borderRadius: 16,
        borderWidth: 1,
        borderColor: paletteToken("border"),
        backgroundColor: paletteToken("card"),
      }}
    >
      <View style={{ display: "flex", flexDirection: "row", alignItems: "center", gap: 10 }}>
        <Pulse id={`mk-skel-${props.index}-mark`} style={{ width: 32, height: 32, borderRadius: 9 }} />
        <View style={{ display: "flex", flexDirection: "column", gap: 4, flex: 1 }}>
          <Pulse id={`mk-skel-${props.index}-title`} style={{ width: 120, height: 16 }} />
          <Pulse id={`mk-skel-${props.index}-sub`} style={{ width: 80, height: 12 }} secondary />
        </View>
      </View>
      <Pulse id={`mk-skel-${props.index}-line1`} style={{ width: 200, height: 12 }} secondary />
      <Pulse id={`mk-skel-${props.index}-line2`} style={{ width: 150, height: 12 }} secondary />
    </View>
  );
}

export const MARKET_CARD_GAP = CARD_GAP;
