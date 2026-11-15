import { Overlay, Scroll, Text, View, signal, type JSX, type ViewRef } from "nsh";
import { paletteToken } from "../../services/themeMode";
import { t, tf } from "../../services/translation";
import { Icon } from "../../chrome/icons";
import { Tip } from "../../chrome/tooltip";
import { IconSpin } from "../../chrome/feedback";
import {
  SOURCE_FILTERS,
  needsAttention,
  orderRepos,
  type SourceFilter,
} from "./filters";
import {
  currentRepos,
  currentScoped,
  favoriteRepos,
  favoritesLocked,
  onlyUpdates,
  reinstallingRepo,
  repoFilter,
  reinstallRepoSource,
  sourceFilter,
  toggleFavoriteRepo,
  uninstallRepoSource,
} from "./state";

// 来源/仓库菜单 — my_skills/toolbar/origin_menu.rs 对位。紧凑触发器
// （Layers + 计数 + 下箭头；激活时 accent_soft 底）打开弹层：
// - 来源分段 All/Hub/Local（source_segment；选 Local 时收起仓库区并清
//   repo 筛选）；
// - 仓库区：标题行（GitBranch + 「仓库」+ 计数 pill）+「全部仓库」行 +
//   逐仓库行（选中 Check / 未选 GitBranch 前缀，owner 弱化 + name，
//   末尾 星标/重装/移除 三钮）；
// - 星标即时翻面并写盘（mod.rs toggle_repo_favorite：乐观更新 + epoch
//   保护 + NewerVersion 锁定）；
// - 重装一仓一跑（reinstalling_repo 旋转，其他行锁住）；移除走共享卸载
//   确认弹窗（uninstall_title_count）。
// 弹层为手绘浮层：fixed 点外幕收起（kit Popover 的 click-away 对位），
// 仓库列表 > 8 行时固定 280px 高视口框 + <Scroll> 内滚（视口框写明确像素
// 高度、滚动交给 <Scroll>，见界面约定「滚动盒」；只写高度不挂 <Scroll>，
// 溢出内容会被面板的 overflow:hidden 裁掉，滚轮滚不动）。

/** Repo rows (the all-repos entry included) that fit before the origin menu
 * has to scroll (ORIGIN_REPOS_VISIBLE/VIEW_H)。 */
const ORIGIN_REPOS_VISIBLE = 8;
const ORIGIN_REPOS_VIEW_H = 280;

const menuOpen = signal(false);
const triggerRef = signal<ViewRef | null>(null);

/** 触发器 + 弹层宿主（spotlight 同款 Overlay 模式）：面板以触发器 ref
 * 为锚（bottom/start，间距 6）挂在 root 层——PageBar 的命令区
 * overflow:hidden 会裁掉普通 absolute 子树；透明幕是不带锚的 fixed
 * Overlay 垫在面板下面，点外收起。计数口径 = toolbar.rs
 * filtered_count；repos = repo_sources(skills)：本组件无 props，一切动
 * 态量落在自家 JSX 表达式里（nsh 组件体只执行一次的纪律）。 */
export function OriginMenuButton(): JSX.Element {
  return (
    <View style={{ position: "relative", flexShrink: 0, display: "flex" }}>
      <View
        ref={(node) => {
          triggerRef.set(node);
        }}
      >
        <OriginTrigger />
      </View>
      <If when={menuOpen()}>
        <Overlay style={{ top: 0, left: 0, right: 0, bottom: 0 }}>
          <View
            testID="selection-menu-dismiss"
            style={{ position: "absolute", top: 0, left: 0, right: 0, bottom: 0 }}
            onPress={() => menuOpen.set(false)}
          />
        </Overlay>
        <Overlay anchor={triggerRef()} placement="bottom" align="start" offset={6}>
          <OriginPanel onClose={() => menuOpen.set(false)} />
        </Overlay>
      </If>
    </View>
  );
}

function OriginTrigger(): JSX.Element {
  return (
    <View>
      <View
        testID="my-skills-origin-trigger"
        tooltip={<Tip text={`${t("toolbar.source")}: ${repoLabel()}`} />}
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
            sourceFilter() !== "all" || repoFilter() !== null
              ? paletteToken("accent_soft_edge")
              : paletteToken("border"),
          backgroundColor:
            sourceFilter() !== "all" || repoFilter() !== null
              ? paletteToken("accent_soft")
              : paletteToken("bg"),
        }}
        hoverStyle={{ backgroundColor: paletteToken("card_hover") }}
        onPress={() => menuOpen.set(!menuOpen())}
      >
        <Icon
          name="layers"
          size={14}
          color={
            sourceFilter() !== "all" || repoFilter() !== null
              ? paletteToken("accent")
              : paletteToken("fg_muted")
          }
        />
        <Text
          style={{
            fontSize: 12,
            color:
              sourceFilter() !== "all" || repoFilter() !== null
                ? paletteToken("accent")
                : paletteToken("fg"),
          }}
        >
          {originCount()}
        </Text>
        <Icon
          name="chevronDown"
          size={11}
          color={
            sourceFilter() !== "all" || repoFilter() !== null
              ? paletteToken("accent")
              : paletteToken("fg_muted")
          }
        />
      </View>
    </View>
  );
}

/** 计数口径（toolbar.rs filtered_count）。 */
function originCount(): number {
  const scoped = currentScoped();
  return onlyUpdates() ? scoped.filter(needsAttention).length : scoped.length;
}

function repoLabel(): string {
  const filter = sourceFilter();
  const label =
    filter === "all" ? t("toolbar.all") : filter === "hub" ? t("toolbar.hub") : t("toolbar.local");
  const repo = repoFilter();
  return repo === null ? label : `${label} · ${repo}`;
}

function OriginPanel(props: { onClose: () => void }): JSX.Element {
  const repos = currentRepos();
  const showRepos =
    sourceFilter() !== "local" && !(repos.length === 0 && repoFilter() === null);
  const width = showRepos ? 340 : 220;
  const ordered = orderRepos(repos, favoriteRepos());
  return (
    <View
      testID="my-skills-origin-panel"
      style={{
        width,
        borderRadius: 12,
        borderWidth: 1,
        borderColor: paletteToken("border"),
        backgroundColor: paletteToken("card"),
        boxShadow: "0 8px 30px rgba(0,0,0,0.35)",
        overflow: "hidden",
        display: "flex",
        flexDirection: "column",
      }}
    >
        <View style={{ padding: 8, display: "flex", flexDirection: "row", gap: 2 }}>
          <SourceSegment onClose={props.onClose} />
        </View>
        {(() => {
          if (!showRepos) return null;
          const rows = ordered.length + 1;
          return (
            <View
              style={{
                display: "flex",
                flexDirection: "column",
                paddingHorizontal: 6,
                paddingTop: 6,
                paddingBottom: 6,
              }}
            >
              <View
                style={{
                  display: "flex",
                  flexDirection: "row",
                  alignItems: "center",
                  justifyContent: "space-between",
                  paddingHorizontal: 8,
                  paddingTop: 2,
                  paddingBottom: 4,
                }}
              >
                <View style={{ display: "flex", flexDirection: "row", alignItems: "center", gap: 5 }}>
                  <Icon name="gitBranch" size={12} color={paletteToken("fg_muted")} />
                  <Text
                    style={{
                      fontSize: 12,
                      fontWeight: "600",
                      color: paletteToken("fg_muted"),
                    }}
                  >
                    {t("toolbar.repo")}
                  </Text>
                </View>
                <View
                  style={{
                    paddingHorizontal: 6,
                    paddingVertical: 1,
                    borderRadius: 999,
                    backgroundColor: paletteToken("well"),
                  }}
                >
                  <Text
                    style={{ fontSize: 10, fontWeight: "500", color: paletteToken("fg_faint") }}
                  >
                    {ordered.length}
                  </Text>
                </View>
              </View>
              {(() => {
                const list = (
                  <View style={{ display: "flex", flexDirection: "column", gap: 2 }}>
                    <PlainRepoRow
                      testID="origin-repo-all"
                      label={t("toolbar.allRepos")}
                      active={repoFilter() === null}
                      onPress={() => {
                        repoFilter.set(null);
                        props.onClose();
                      }}
                    />
                    {ordered.map((repo) => (
                      <RepoSourceRow repo={repo} onClose={props.onClose} />
                    ))}
                  </View>
                );
                if (rows <= ORIGIN_REPOS_VISIBLE) return list;
                // 高度写在视口框上，<Scroll> 撑满：滚轮留在名单里（import 列表
                // 同款滚动盒契约）。只写高度不挂 <Scroll> 时内容溢出视口被面板
                // 的 overflow:hidden 裁掉，滚轮无处可滚。
                return (
                  <View
                    testID="origin-repo-scroll"
                    style={{
                      height: ORIGIN_REPOS_VIEW_H,
                      width: "100%",
                      minWidth: 0,
                      flexShrink: 0,
                    }}
                  >
                    <Scroll testID="origin-repo-scroll-list" style={{ width: "100%", height: "100%" }}>
                      {list}
                    </Scroll>
                  </View>
                );
              })()}
            </View>
          );
        })()}
    </View>
  );
}

/** 来源分段（origin_menu.rs source_segment）：三段原地式，选中即收弹层；
 * 选 Local 清 repo 筛选。 */
function SourceSegment(props: { onClose: () => void }): JSX.Element {
  return (
    <View
      testID="source-filter"
      style={{
        display: "flex",
        flexDirection: "row",
        alignItems: "center",
        width: "100%",
        height: 32,
        borderRadius: 8,
        borderWidth: 1,
        borderColor: paletteToken("border"),
        backgroundColor: paletteToken("bg"),
        padding: 2,
        gap: 2,
      }}
    >
      {SOURCE_FILTERS.map((kind) => (
        <SourceSegTab
          kind={kind}
          selected={sourceFilter() === kind}
          onClose={props.onClose}
        />
      ))}
    </View>
  );
}

function SourceSegTab(props: {
  kind: SourceFilter;
  selected: boolean;
  onClose: () => void;
}): JSX.Element {
  const label =
    props.kind === "all"
      ? t("toolbar.all")
      : props.kind === "hub"
        ? t("toolbar.hub")
        : t("toolbar.local");
  return (
    <View
      testID={`source-filter-${props.kind}`}
      style={{
        display: "flex",
        alignItems: "center",
        justifyContent: "center",
        flex: 1,
        height: 26,
        borderRadius: 6,
        cursor: "pointer",
        backgroundColor: props.selected ? paletteToken("card") : "transparent",
        borderWidth: 1,
        borderColor: props.selected ? paletteToken("border") : "transparent",
      }}
      hoverStyle={{ backgroundColor: paletteToken("panel_hover") }}
      onPress={() => {
        sourceFilter.set(props.kind);
        if (props.kind === "local") repoFilter.set(null);
        props.onClose();
      }}
    >
      <Text
        style={{
          fontSize: 12,
          color: props.selected ? paletteToken("accent") : paletteToken("fg_muted"),
        }}
      >
        {label}
      </Text>
    </View>
  );
}

function PlainRepoRow(props: {
  testID: string;
  label: string;
  active: boolean;
  onPress: () => void;
}): JSX.Element {
  return (
    <MenuRow testID={props.testID} active={props.active} onPress={props.onPress}>
      <LeadingGlyph active={props.active} />
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
        {props.label}
      </Text>
    </MenuRow>
  );
}

function MenuRow(props: {
  testID: string;
  active: boolean;
  onPress: () => void;
  children: JSX.Element[];
}): JSX.Element {
  return (
    <View
      testID={props.testID}
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
      onPress={props.onPress}
    >
      {props.children}
    </View>
  );
}

function LeadingGlyph(props: { active: boolean; repo?: string }): JSX.Element {
  if (props.active) {
    return <Icon name="check" size={13} color={paletteToken("accent")} />;
  }
  return <Icon name="gitBranch" size={13} color={paletteToken("fg_muted")} />;
}

/** 逐仓库行（repo_source_row）：点击切换 repo 筛选（再点同仓清除）并收
 * 弹层；末尾 星标/重装/移除。nsh 无 group-hover：星标恒显（收藏 amber），
 * 重装/移除弱化恒显、悬停着色——与原版「悬停显形」的差异记入台账。 */
function RepoSourceRow(props: { repo: string; onClose: () => void }): JSX.Element {
  const repo = props.repo;
  const active = repoFilter() === repo;
  const spinning = reinstallingRepo() === repo;
  const locked = reinstallingRepo() !== null;
  const favorited = favoriteRepos().has(repo);
  const starTip = favoritesLocked()
    ? t("toolbar.favoriteSourceLocked")
    : favorited
      ? t("toolbar.unfavoriteSource")
      : t("toolbar.favoriteSource");
  return (
    <MenuRow
      testID={`origin-repo-row-${repo}`}
      active={active}
      onPress={() => {
        repoFilter.set(repoFilter() === repo ? null : repo);
        props.onClose();
      }}
    >
      <LeadingGlyph active={active} repo={repo} />
      <RepoName repo={repo} active={active} />
      <View style={{ display: "flex", flexDirection: "row", alignItems: "center", gap: 2, flexShrink: 0 }}>
        <RepoActionButton
          testID={`origin-repo-favorite-${repo}`}
          glyph="star"
          tip={starTip}
          ink={favorited ? paletteToken("amber_fg") : paletteToken("fg_faint")}
          hoverColor={paletteToken("amber_fg")}
          hoverBg={paletteToken("amber_bg")}
          onPress={() => void toggleFavoriteRepo(repo)}
        />
        <RepoActionButton
          testID={`origin-repo-reinstall-${repo}`}
          glyph="refresh"
          tip={t("toolbar.reinstallSource")}
          ink={locked && !spinning ? paletteToken("fg_faint") : paletteToken("fg_muted")}
          hoverColor={paletteToken("fg")}
          hoverBg={paletteToken("panel_hover")}
          dimmed={locked && !spinning}
          spin={spinning}
          onPress={() => void reinstallRepoSource(repo)}
        />
        <RepoActionButton
          testID={`origin-repo-remove-${repo}`}
          glyph="trash"
          tip={t("toolbar.removeSource")}
          ink={paletteToken("fg_muted")}
          hoverColor={paletteToken("danger")}
          hoverBg={paletteToken("danger_bg")}
          onPress={() => {
            props.onClose();
            uninstallRepoSource(repo);
          }}
        />
      </View>
    </MenuRow>
  );
}

/** `owner/name` 一行：owner 弱化、name 占余宽截断；无斜杠整段一个 run
 * （origin_menu.rs repo_name）。> 28 字符挂 tooltip。 */
function RepoName(props: { repo: string; active: boolean }): JSX.Element {
  const ink = props.active ? paletteToken("accent") : paletteToken("fg");
  const slash = props.repo.indexOf("/");
  const body =
    slash === -1 ? (
      <Text
        style={{
          flex: 1,
          minWidth: 0,
          fontSize: 12,
          fontWeight: "500",
          color: ink,
          numberOfLines: 1,
        }}
      >
        {props.repo}
      </Text>
    ) : (
      <View
        style={{
          flex: 1,
          minWidth: 0,
          display: "flex",
          flexDirection: "row",
          alignItems: "center",
          gap: 3,
        }}
      >
        <View style={{ maxWidth: 112, minWidth: 0 }}>
          <Text
            style={{ fontSize: 12, color: paletteToken("fg_muted"), numberOfLines: 1 }}
          >
            {props.repo.slice(0, slash)}
          </Text>
        </View>
        <Text style={{ flexShrink: 0, fontSize: 12, color: paletteToken("fg_faint") }}>/</Text>
        <View style={{ flex: 1, minWidth: 0 }}>
          <Text style={{ fontSize: 12, fontWeight: "500", color: ink, numberOfLines: 1 }}>
            {props.repo.slice(slash + 1)}
          </Text>
        </View>
      </View>
    );
  if (Array.from(props.repo).length > 28) {
    return (
      <View style={{ flex: 1, minWidth: 0 }} tooltip={<Tip text={props.repo} />}>
        {body}
      </View>
    );
  }
  return (
    <View style={{ flex: 1, minWidth: 0 }}>{body}</View>
  );
}

function RepoActionButton(props: {
  testID: string;
  glyph: "star" | "refresh" | "trash";
  tip: string;
  ink: string;
  hoverColor: string;
  hoverBg: string;
  dimmed?: boolean;
  spin?: boolean;
  onPress: () => void;
}): JSX.Element {
  const dimmed = props.dimmed === true;
  return (
    <View
      testID={props.testID}
      tooltip={<Tip text={props.tip} />}
      style={{
        width: 24,
        height: 24,
        display: "flex",
        alignItems: "center",
        justifyContent: "center",
        borderRadius: 6,
        flexShrink: 0,
        opacity: dimmed ? 0.4 : 1,
        cursor: dimmed ? "default" : "pointer",
      }}
      hoverStyle={dimmed ? undefined : { backgroundColor: props.hoverBg }}
      onPress={dimmed ? undefined : props.onPress}
    >
      {props.spin === true ? (
        <IconSpin name={props.glyph} size={13} color={paletteToken("accent")} spin />
      ) : (
        <Icon name={props.glyph} size={13} color={props.ink} />
      )}
    </View>
  );
}
