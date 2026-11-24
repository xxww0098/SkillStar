import { List, Text, View, computed, type Accessor, type JSX } from "nsh";
import { Icon } from "../../chrome/icons";
import { paletteToken } from "../../services/themeMode";
import { t, tf } from "../../services/translation";
import {
  error,
  installShareNow,
  share,
  shareBack,
  shareExisting,
  shareRemoteLabel,
} from "./importModalState";
import { CountPill, GhostButton } from "./importModalPhases";

// 分享码预览相 — import_modal/share.rs 对位：deck 头 + 警示横幅（内嵌/私有/
// 已安装）+ 只读技能列表 + 返回/安装条。data===null 时渲染错误块（解析失败
// 的落点；密码重试分支无 Rust 解码支持，不可达——照实不移植）。

/** 6 行后列表滚动（SHARE_VISIBLE）。 */
const SHARE_VISIBLE = 6;
/** 滚动盒视口高（SHARE_VIEW_H）。 */
const SHARE_VIEW_H = 280;

/** 警示/信息条（hint_banner：ink 5% 底 + 20% 边）。 */
function HintBanner(props: { glyph: "package" | "alertTriangle" | "check"; text: string; ink: string }): JSX.Element {
  return (
    <View
      style={{
        display: "flex",
        flexDirection: "row",
        alignItems: "center",
        gap: 8,
        borderRadius: 8,
        backgroundColor: props.ink + "0d",
        borderWidth: 1,
        borderColor: props.ink + "33",
        paddingHorizontal: 10,
        paddingVertical: 8,
      }}
    >
      <Icon name={props.glyph} size={14} color={props.ink} />
      <Text style={{ fontSize: 12, color: props.ink }}>{props.text}</Text>
    </View>
  );
}

export function SharePreviewBody(): JSX.Element {
  return (
    <View style={{ display: "flex", flexDirection: "column" }}>
      {/* 解析失败：错误块 + 返回（share.rs 的 let-else 分支）。 */}
      <If when={share() === null}>
        <View testID="import-share-error" style={{ display: "flex", flexDirection: "column", paddingHorizontal: 24, paddingVertical: 24, gap: 16 }}>
          <View style={{ display: "flex", flexDirection: "column", alignItems: "center", gap: 12, paddingVertical: 8 }}>
            <View
              style={{
                width: 56,
                height: 56,
                borderRadius: 16,
                backgroundColor: paletteToken("warn") + "1a",
                display: "flex",
                alignItems: "center",
                justifyContent: "center",
              }}
            >
              <Icon name="share" size={28} color={paletteToken("warn")} />
            </View>
            <Text testID="import-share-error-message" style={{ fontSize: 14, color: paletteToken("danger"), textAlign: "center" }}>
              {error() ?? ""}
            </Text>
          </View>
          <View style={{ width: "100%" }}>
            <GhostButton testID="import-share-back" label={t("common.back")} onPress={() => shareBack(false)} />
          </View>
        </View>
      </If>
      <SharePreviewPane />
    </View>
  );
}

interface ShareSkillItem {
  skill: { n: string; u: string; c?: string | null; p?: boolean | null };
  index: number;
}

/** 预览本体（share.rs render_share_preview 主分支）。 */
export function SharePreviewPane(): JSX.Element {
  const preview = computed(() => share());
  const existingSet = computed(
    () => new Set(shareExisting().map((name) => name.trim().toLowerCase())),
  );
  const indexedSkills = computed<ShareSkillItem[]>(() =>
    (preview()?.skills ?? []).map((skill, index) => ({ skill, index })),
  );
  const installableCount = computed(
    () =>
      (preview()?.skills ?? []).filter(
        (skill) => !existingSet().has(skill.n.trim().toLowerCase()),
      ).length,
  );
  const hasEmbedded = computed(() =>
    (preview()?.skills ?? []).some((skill) => (skill.c ?? "") !== ""),
  );
  const hasPrivate = computed(() =>
    (preview()?.skills ?? []).some((skill) => skill.p === true),
  );
  const scrollable = computed(() => (preview()?.skills.length ?? 0) > SHARE_VISIBLE);

  return (
    <If when={preview() !== null}>
      <View testID="import-share-preview" style={{ display: "flex", flexDirection: "column" }}>
        <View style={{ display: "flex", flexDirection: "column", paddingHorizontal: 24, paddingTop: 20, paddingBottom: 12, gap: 12 }}>
          <View style={{ display: "flex", flexDirection: "row", alignItems: "center", gap: 12 }}>
            <View
              style={{
                width: 40,
                height: 40,
                borderRadius: 12,
                display: "flex",
                alignItems: "center",
                justifyContent: "center",
                backgroundImage: `linear-gradient(135deg, ${paletteToken("accent")}26, ${paletteToken("violet")}26)`,
              }}
            >
              <Text style={{ fontSize: 18 }}>{(preview()?.icon ?? "").trim() === "" ? "⭐" : preview()?.icon}</Text>
            </View>
            <View style={{ flex: 1, minWidth: 0 }}>
              <Text
                style={{
                  fontSize: 14,
                  fontWeight: "600",
                  overflow: "hidden",
                  textOverflow: "ellipsis",
                }}
              >
                {preview()?.name ?? ""}
              </Text>
              <Text
                style={{
                  fontSize: 12,
                  color: paletteToken("fg_muted"),
                  overflow: "hidden",
                  textOverflow: "ellipsis",
                }}
              >
                {preview()?.description ?? ""}
              </Text>
            </View>
            <CountPill label={tf("skillCards.shareCodeCount", { count: preview()?.skills.length ?? 0 })} />
          </View>
          <If when={hasEmbedded()}>
            <HintBanner glyph="package" text={t("importShareCodeModal.embeddedDesc")} ink={paletteToken("accent")} />
          </If>
          <If when={hasPrivate()}>
            <HintBanner glyph="alertTriangle" text={t("importShareCodeModal.privateRepoDesc")} ink={paletteToken("warn")} />
          </If>
          <If when={shareExisting().length > 0}>
            <HintBanner
              glyph="check"
              text={tf("shareCodeImport.alreadyDetected", { count: shareExisting().length })}
              ink={paletteToken("ok")}
            />
          </If>
        </View>

        <If when={!scrollable()}>
          <View style={{ paddingHorizontal: 24, paddingBottom: 8, display: "flex", flexDirection: "column", gap: 2 }}>
            <Each of={indexedSkills()} key={(row) => `${row.index}:${row.skill.n}`}>
              {(row: () => ShareSkillItem) => (
                <ShareSkillRow item={row} existingSet={existingSet} />
              )}
            </Each>
          </View>
        </If>
        <If when={scrollable()}>
          <View style={{ paddingHorizontal: 24, paddingBottom: 8, height: SHARE_VIEW_H, width: "100%", flexShrink: 0 }}>
            <List
              testID="import-share-list"
              each={indexedSkills()}
              keyExtractor={(row) => `${row.index}:${row.skill.n}`}
              estimatedItemSize={40}
              style={{ width: "100%", height: "100%" }}
            >
              {(row) => <ShareSkillRow item={row} existingSet={existingSet} />}
            </List>
          </View>
        </If>

        {/* 底部：返回 + 安装 N */}
        <View
          style={{
            paddingHorizontal: 24,
            paddingVertical: 14,
            borderTopWidth: 1,
            borderTopColor: paletteToken("border_soft"),
            display: "flex",
            flexDirection: "row",
            alignItems: "center",
            justifyContent: "space-between",
          }}
        >
          <GhostButton testID="import-share-back2" label={t("common.back")} onPress={() => shareBack(true)} />
          <View
            testID="import-share-install"
            style={{
              display: "flex",
              flexDirection: "row",
              alignItems: "center",
              gap: 6,
              height: 28,
              paddingHorizontal: 20,
              borderRadius: 6,
              backgroundColor: paletteToken("accent"),
              cursor: "pointer",
            }}
            hoverStyle={{ backgroundColor: paletteToken("accent_hover") }}
            onPress={() => installShareNow()}
          >
            <Icon name="download" size={14} color={paletteToken("on_accent")} />
            <Text style={{ fontSize: 12, fontWeight: "500", color: paletteToken("on_accent") }}>
              {tf("shareCodeImport.installSkills", { count: installableCount() })}
            </Text>
          </View>
        </View>
      </View>
    </If>
  );
}

function ShareSkillRow(props: {
  item: Accessor<ShareSkillItem>;
  existingSet: Accessor<Set<string>>;
}): JSX.Element {
  const isExisting = computed(() =>
    props.existingSet().has(props.item().skill.n.trim().toLowerCase()),
  );
  const hasRemote = computed(() => props.item().skill.u.length > 0);
  const remote = computed(() =>
    hasRemote() ? shareRemoteLabel(props.item().skill.u) : null,
  );
  const hasEmbedded = computed(() => (props.item().skill.c ?? "") !== "");
  const isPrivate = computed(() => props.item().skill.p === true);
  return (
    <View
      testID={`share-skill-${props.item().index}`}
      style={{
        display: "flex",
        flexDirection: "row",
        alignItems: "center",
        gap: 12,
        paddingHorizontal: 12,
        paddingVertical: 8,
        borderRadius: 12,
      }}
      hoverStyle={{ backgroundColor: paletteToken("panel_hover") }}
    >
      <View
        style={{
          width: 16,
          height: 16,
          flexShrink: 0,
          borderRadius: 4,
          borderWidth: 1,
          display: "flex",
          alignItems: "center",
          justifyContent: "center",
          backgroundColor: isExisting() ? paletteToken("ok") + "33" : paletteToken("accent"),
          borderColor: isExisting() ? paletteToken("ok") + "66" : paletteToken("accent"),
        }}
      >
        <Icon name="check" size={10} color={paletteToken("on_accent")} />
      </View>
      <View style={{ flex: 1, minWidth: 0, display: "flex", flexDirection: "row", alignItems: "center", gap: 6 }}>
        <Text
          style={{
            fontSize: 14,
            fontWeight: "500",
            overflow: "hidden",
            textOverflow: "ellipsis",
          }}
        >
          {props.item().skill.n}
        </Text>
        <If when={isExisting()}>
          <View style={{ paddingHorizontal: 6, paddingVertical: 2, borderRadius: 999, backgroundColor: paletteToken("ok_bg") }}>
            <Text style={{ fontSize: 12, fontWeight: "500", color: paletteToken("ok") }}>
              {t("githubImportModal.installed")}
            </Text>
          </View>
        </If>
        <If when={hasEmbedded()}>
          <View style={{ paddingHorizontal: 6, paddingVertical: 2, borderRadius: 999, backgroundColor: paletteToken("info_bg") }}>
            <Text style={{ fontSize: 12, fontWeight: "500", color: paletteToken("info") }}>embedded</Text>
          </View>
        </If>
        <If when={isPrivate()}>
          <View style={{ paddingHorizontal: 6, paddingVertical: 2, borderRadius: 999, backgroundColor: paletteToken("warn_bg") }}>
            <Text style={{ fontSize: 12, fontWeight: "500", color: paletteToken("warn") }}>private</Text>
          </View>
        </If>
      </View>
      <If when={hasRemote()}>
        <If
          when={remote() === null}
          else={
            <View style={{ display: "flex", flexDirection: "row", alignItems: "center", gap: 4, maxWidth: 220, flexShrink: 0 }}>
              <Icon name="gitBranch" size={12} color={paletteToken("fg_muted")} />
              <Text
                style={{
                  fontSize: 12,
                  color: paletteToken("fg_muted"),
                  overflow: "hidden",
                  textOverflow: "ellipsis",
                }}
              >
                {remote()}
              </Text>
            </View>
          }
        >
          <View style={{ paddingHorizontal: 6, paddingVertical: 2, borderRadius: 999, backgroundColor: paletteToken("danger_bg"), flexShrink: 0 }}>
            <Text style={{ fontSize: 12, fontWeight: "500", color: paletteToken("danger") }}>
              {t("shareCodeImport.unsupportedSource")}
            </Text>
          </View>
        </If>
      </If>
    </View>
  );
}
