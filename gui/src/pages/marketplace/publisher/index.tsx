import { Text, View, effect, type JSX } from "nsh";
import { Marketplace, MarketplacePublisher } from "skillstar-gui/native";
import { paletteToken } from "../../../services/themeMode";
import { DETAIL_COLUMN_W, PAGE_PAD, SKILL_SCROLL_PT } from "../../../layout";
import { Icon } from "../../../chrome/icons";
import { BarIconButton, BarSearchInput, PageBar } from "../../../chrome/toolbar";
import { RepoAvatar, sanitizeGithubOwner } from "../../../chrome/repoAvatar";
import { openUninstallConfirm, uninstallTitleNamed } from "../../../chrome/dialog";
import { publishToast, selectPage } from "../../../shell/events";
import type { PublisherRef } from "../../../shell/events";
import { MarketDetailColumn } from "../detail_drawer";
import { mt, mtf } from "../marketStrings";
import { formatInstalls, type MarketSkill } from "../types";
import type { MarketNotice } from "../readSide";
import { PublisherBoardPane } from "./board";
import {
  createPublisherSide,
  heroCounts,
  type PublisherRepoRow,
  type PublisherSide,
} from "./readSide";

// 发布者详情页 — marketplace/publisher/（mod.rs + board.rs + card.rs +
// hero.rs）对位，波 2 发布者切片。
//
// 结构：顶栏（返回 + 发布者名 crumb + 当前仓库 mono、搜索框、同步状态文
// 案、刷新）+ hero 横幅（头像 + 名 + 官方徽章 + 计数）+ 板面（仓库清单的
// 紧凑行卡，或当前仓库的技能网格——复用市场卡身 MarketCard）+ 右侧详情列
// 固定轨道（复用 detail_drawer 的 MarketDetailColumn）。
//
// 读侧状态机在 ./readSide.ts（本地优先 + epoch 丢弃迟到响应 + 描述回填
// watch）；页面只持模块级单例（keep-alive 挂载一次 = 页面实体）并把信号绑
// 进 JSX——组件体只执行一次，signal 消费必须落在 JSX 响应式绑定里。
//
// 验收红线：本页是市场下钻——侧栏选中框留在市场项（nav.ts railPage 已映
// 射，本页不改导航状态）；入口唯一（市场页官方 tab 发 selectPublisher），
// 返回用 selectPage.publish("marketplace")。

export interface MarketPublisherPageProps {
  publisher: PublisherRef | null;
}

/** 状态机通知 → NoticeBand toast（文案在本层映射，键同市场页）。 */
function notifyToast(notice: MarketNotice): void {
  switch (notice.kind) {
    case "installed":
      publishToast({ tone: "success", message: mtf("marketplace.installedName", { name: notice.name }) });
      break;
    case "uninstalled":
      publishToast({ tone: "success", message: mtf("marketplace.uninstalledName", { name: notice.name }) });
      break;
    case "installFailed":
      publishToast({ tone: "error", message: mtf("marketplace.installFailedDetail", { err: notice.error }) });
      break;
    case "uninstallFailed":
      publishToast({ tone: "error", message: mtf("marketplace.uninstallFailedDetail", { err: notice.error }) });
      break;
  }
}

const side: PublisherSide = createPublisherSide({
  reposLoader: (name) => MarketplacePublisher.publisherRepos(name),
  skillsLoader: (source) => MarketplacePublisher.repoSkills(source),
  rereader: (source) => MarketplacePublisher.rereadRepoSkills(source),
  descriptionState: (source) =>
    MarketplacePublisher.repoDescriptionState(source) as { progress: number; running: boolean },
  syncer: (scope) => Marketplace.syncScope(scope),
  installer: (url, name) => Marketplace.installSkill(url, name),
  uninstaller: (name) => Marketplace.uninstallSkill(name),
  schedule: (fn, ms) => {
    const timer = setTimeout(fn, ms);
    return () => clearTimeout(timer);
  },
  notify: notifyToast,
  confirmUninstall: (name, commit) => {
    openUninstallConfirm(uninstallTitleNamed(name), () => {
      commit();
      return true;
    });
  },
  leaveMarket: () => selectPage.publish("marketplace"),
});

export function MarketPublisherPage(props: MarketPublisherPageProps): JSX.Element {
  // 换发布者（或进入）重开读侧：props 读取在 effect 里，publisher 变化即
  // 重置并重读（set_publisher 对位）。
  effect(() => {
    side.open(props.publisher);
  });
  return <PublisherBody />;
}

function PublisherBody(): JSX.Element {
  return (
    <View
      testID="pd-page"
      style={{ display: "flex", flexDirection: "column", flex: 1, minHeight: 0, width: "100%" }}
    >
      <PageBar
        title={mt("sidebar.market")}
        dragId="publisher-toolbar-drag"
        titleExtras={[
          <PublisherBackButton />,
          <PublisherCrumb name={side.publisherName() ?? ""} inRepo={side.activeRepo() !== null} />,
          <If when={side.activeRepo() !== null}>
            <View style={{ display: "flex", flexDirection: "row", alignItems: "center", gap: 8, minWidth: 0 }}>
              <Text style={{ fontSize: 14, color: paletteToken("fg_muted") }}>/</Text>
              <View style={{ overflow: "hidden", flexShrink: 1, minWidth: 0 }}>
                <Text
                  style={{
                    fontSize: 14,
                    color: paletteToken("fg"),
                    fontFamily: "monospace",
                    numberOfLines: 1,
                  }}
                >
                  {side.activeRepo() ?? ""}
                </Text>
              </View>
            </View>
          </If>,
        ]}
        search={
          <BarSearchInput
            inputTestID="pd-search"
            value={side.query()}
            placeholder={mtf("publisherDetail.searchPlaceholder", {
              name: side.publisherName() ?? "",
            })}
            onChange={(value) => side.onSearchInput(value)}
            width={216}
          />
        }
        actions={[
          <If when={side.refreshedFlash() && !side.refreshing()}>
            <Text style={{ fontSize: 12, color: paletteToken("ok"), flexShrink: 0 }}>
              {mt("marketplace.snapshotRefreshed")}
            </Text>
          </If>,
          <If when={side.refreshing()}>
            <Text style={{ fontSize: 12, color: paletteToken("fg_muted"), flexShrink: 0 }}>
              {mt("marketplace.refreshingSnapshot")}
            </Text>
          </If>,
          <View testID="pd-sync-btn">
            <BarIconButton
              glyph="refresh"
              loading={side.refreshing()}
              tip={mt("common.refresh")}
              onPress={() => side.syncSnapshot()}
            />
          </View>,
        ]}
      />
      <View
        style={{
          display: "flex",
          flexDirection: "row",
          flex: 1,
          minHeight: 0,
          minWidth: 0,
          width: "100%",
        }}
      >
        <View
          style={{
            display: "flex",
            flexDirection: "column",
            flex: 1,
            minHeight: 0,
            minWidth: 0,
          }}
        >
          <If when={side.publisherName() === null}>
            <View style={{ padding: PAGE_PAD, flex: 1 }}>
              <NoneSelected />
            </View>
          </If>
          <If when={side.publisherName() !== null}>
            <View style={{ paddingHorizontal: PAGE_PAD, paddingTop: SKILL_SCROLL_PT, paddingBottom: 12 }}>
              <PublisherHero name={side.publisherName() ?? ""} repos={side.repos()} />
            </View>
            <PublisherBoardPane side={side} />
          </If>
        </View>
        <If when={side.selected() !== null}>
          <View style={{ width: DETAIL_COLUMN_W, flexShrink: 0, height: "100%" }}>
            <MarketDetailColumn
              skill={findSkill(side.skills(), side.selected())}
              identity={side.selected() ?? { source: null, name: "" }}
              busy={side.busy() !== null && side.busy() === side.selected()?.name}
              onClose={() => side.clearSelection()}
              onToggleInstall={(skill) => side.toggleInstalled(skill)}
            />
          </View>
        </If>
      </View>
    </View>
  );
}

function findSkill(
  skills: MarketSkill[],
  identity: { name: string } | null
): MarketSkill | null {
  if (identity === null) return null;
  return skills.find((skill) => skill.name === identity.name) ?? null;
}

/** 返回按钮（mod.rs pd-back-btn）：go_up——仓库内退回仓库清单，仓库清单
 * 退回市场。 */
function PublisherBackButton(): JSX.Element {
  return (
    <View
      testID="pd-back-btn"
      style={{
        display: "flex",
        flexDirection: "row",
        alignItems: "center",
        gap: 6,
        paddingHorizontal: 8,
        height: 32,
        borderRadius: 10,
        cursor: "pointer",
        flexShrink: 0,
      }}
      hoverStyle={{ backgroundColor: paletteToken("card_hover") }}
      onPress={() => side.goUp()}
    >
      <Icon name="arrowLeft" size={16} color={paletteToken("fg_muted")} />
      <Text style={{ fontSize: 12, color: paletteToken("fg_muted") }}>
        {mt("publisherDetail.back")}
      </Text>
    </View>
  );
}

/** 标题里的发布者名（mod.rs publisher_crumb）：可点，与返回同一动作；仓
 * 库内弱化、列表态常规。当前仓库名是纯文本（上面的 mono 行）。 */
function PublisherCrumb(props: { name: string; inRepo: boolean }): JSX.Element {
  return (
    <View
      testID="pd-crumb-publisher"
      style={{
        display: "flex",
        alignItems: "center",
        paddingHorizontal: 4,
        height: 32,
        borderRadius: 10,
        cursor: "pointer",
        overflow: "hidden",
        flexShrink: 1,
        minWidth: 0,
      }}
      hoverStyle={{ backgroundColor: paletteToken("card_hover") }}
      onPress={() => side.goUp()}
    >
      <Text
        style={{
          fontSize: 14,
          color: props.inRepo ? paletteToken("fg_muted") : paletteToken("fg"),
          numberOfLines: 1,
        }}
      >
        {props.name}
      </Text>
    </View>
  );
}

/** 未选发布者空态（mod.rs:579-599）。 */
function NoneSelected(): JSX.Element {
  return (
    <View
      testID="pd-none"
      style={{
        display: "flex",
        flexDirection: "column",
        alignItems: "center",
        justifyContent: "center",
        gap: 12,
        paddingVertical: 80,
      }}
    >
      <Icon name="package" size={32} color={paletteToken("fg_muted")} />
      <Text style={{ fontSize: 14, color: paletteToken("fg_muted") }}>
        {mt("publisherDetail.noneSelected")}
      </Text>
    </View>
  );
}

/** hero 横幅（hero.rs）：56px 头像 + 名 + 官方徽章 + 计数行。计数从仓库
 * 行实时推导（hero.rs total_* 的本地和）——派生在 JSX 表达式里现算（组
 * 件体只执行一次）；发布者行自身计数仅在仓库表为空时兜底——PublisherRef
 * 只带 id/name，兜底值恒 0（见 gaps：载荷不含 url/repoCount/skillCount）。 */
function PublisherHero(props: { name: string; repos: PublisherRepoRow[] }): JSX.Element {
  return (
    <View
      testID="pd-hero"
      style={{
        display: "flex",
        flexDirection: "row",
        alignItems: "center",
        justifyContent: "space-between",
        gap: 16,
        padding: 16,
        borderRadius: 16,
        borderWidth: 1,
        borderColor: paletteToken("border"),
        backgroundColor: paletteToken("card"),
        flexShrink: 0,
      }}
    >
      <View
        style={{
          display: "flex",
          flexDirection: "row",
          alignItems: "center",
          gap: 16,
          flex: 1,
          minWidth: 0,
        }}
      >
        <RepoAvatar
          owner={sanitizeGithubOwner(props.name)}
          size={56}
          radius={16}
        />
        <View style={{ display: "flex", flexDirection: "column", gap: 4, flex: 1, minWidth: 0 }}>
          <View style={{ display: "flex", flexDirection: "row", alignItems: "center", gap: 8, minWidth: 0 }}>
            <Text style={{ fontSize: 14, color: paletteToken("fg"), numberOfLines: 1 }}>
              {props.name}
            </Text>
            <View
              style={{
                display: "flex",
                flexDirection: "row",
                alignItems: "center",
                gap: 2,
                flexShrink: 0,
                paddingHorizontal: 8,
                paddingVertical: 2,
                borderRadius: 999,
                backgroundColor: paletteToken("ok_bg"),
                borderWidth: 1,
                borderColor: paletteToken("ok_border"),
              }}
            >
              <Icon name="badgeCheck" size={12} color={paletteToken("ok")} />
              <Text style={{ fontSize: 12, fontWeight: "500", color: paletteToken("ok") }}>
                {mt("publisherDetail.official")}
              </Text>
            </View>
          </View>
          <View
            style={{
              display: "flex",
              flexDirection: "row",
              alignItems: "center",
              gap: 16,
              flexWrap: "wrap",
            }}
          >
            <View style={{ display: "flex", flexDirection: "row", alignItems: "center", gap: 4 }}>
              <Icon name="folder" size={13} color={paletteToken("fg_muted")} />
              <Text style={{ fontSize: 12, color: paletteToken("fg_muted") }}>
                {mtf("marketplace.repositoriesCount", { count: heroCounts(null, props.repos).repos })}
              </Text>
            </View>
            <View style={{ display: "flex", flexDirection: "row", alignItems: "center", gap: 4 }}>
              <Icon name="package" size={13} color={paletteToken("fg_muted")} />
              <Text style={{ fontSize: 12, color: paletteToken("fg_muted") }}>
                {mtf("marketplace.skillsTotal", { count: heroCounts(null, props.repos).skills })}
              </Text>
            </View>
          <If when={heroCounts(null, props.repos).installs > 0}>
            <View style={{ display: "flex", flexDirection: "row", alignItems: "center", gap: 4 }}>
              <Icon name="download" size={13} color={paletteToken("fg_muted")} />
              <Text style={{ fontSize: 12, color: paletteToken("fg_muted") }}>
                {mtf("marketplace.installsCount", {
                  count: formatInstalls(heroCounts(null, props.repos).installs),
                })}
              </Text>
            </View>
          </If>
        </View>
      </View>
    </View>
  </View>
  );
}
