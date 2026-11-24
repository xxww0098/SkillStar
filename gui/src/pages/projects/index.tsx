// 项目页 — crates/ss-gpui/src/projects/mod.rs 的 Render 对位：一条工具栏
// + 左列（固定 rail：搜索、项目卡）+ 右侧详情列。
//
// 工具栏（mod.rs render）：标题「项目」、搜索框（224px）、Layers 计数
// chip（过滤后项目数）、「注册项目」主按钮（busy 或 dirty 时压暗）。
//
// 左列 288px 固定 rail（max 320）：空注册表 → Empty 空态；过滤无果 →
// 居中「没有匹配的项目」；否则卡片列表（选中卡 accent 边 + accent_soft
// 底 + FolderOpen 图标 + semibold 名字）。
//
// 右侧：选中项目 → ProjectDetail（三卡 + 看护区 + 页脚）；未选中 →
// SelectProjectHint。keep-alive：页面只挂载一次，模块级 state 即页面实体。
//
// 【nsh 纪律】组件体只执行一次：PageBar 的 filters/actions 数组一次性
// 构建，动态量都在各元素自己的 JSX 表达式里（条件成员用 <If>）。

import { List, Text, View, computed, effect, type JSX } from "nsh";
import { paletteToken } from "../../services/themeMode";
import { t } from "../../services/translation";
import { bindDelayedFlag } from "../../chrome/delayedFlag";
import { Pulse } from "../../chrome/feedback";
import { Icon } from "../../chrome/icons";
import { BarCount, BarPrimary, BarSearchInput, PageBar } from "../../chrome/toolbar";
import { currentPage } from "../../shell/nav";
import { realProjectsBridge } from "./bridge";
import { createProjectsState, type ProjectsState } from "./state";
import { ProjectDetail, SelectProjectHint } from "./detail";
import { openRegisterDialog } from "./dialogs";
import type { ProjectEntry } from "./bridge";

// 页面实体：keep-alive 单挂载的真桥 state（测试装置自建假桥 state）。
const pageState = createProjectsState(realProjectsBridge);

export function ProjectsPage(): JSX.Element {
  return <ProjectsWorkbench state={pageState} lazy />;
}

/** 工作台本体（页面与测试装置共用；state 注入）。 */
export function ProjectsWorkbench(props: { state: ProjectsState; lazy?: boolean }): JSX.Element {
  const state = props.state;
  const pageActive = (): boolean => props.lazy !== true || currentPage() === "projects";
  effect(() => {
    if (props.lazy === true) {
      if (currentPage() !== "projects") return;
      state.start({ isActive: pageActive });
      state.onPageActivated();
      return;
    }
    state.start();
  });
  return (
    <View
      style={{
        display: "flex",
        flexDirection: "column",
        flex: 1,
        minHeight: 0,
        width: "100%",
      }}
    >
      <PageBar
        title={t("sidebar.projects")}
        dragId="projects-toolbar-drag"
        search={<ProjectsSearch state={state} />}
        filters={[<ProjectsCount state={state} />]}
        actions={[<RegisterButton state={state} />]}
      />
      <View
        style={{
          display: "flex",
          flexDirection: "row",
          alignItems: "stretch",
          flex: 1,
          minHeight: 0,
          width: "100%",
          overflow: "hidden",
        }}
      >
        <ProjectRail state={state} />
        <If when={state.selected() !== null} else={<SelectProjectHint />}>
          <ProjectDetail state={state} />
        </If>
      </View>
    </View>
  );
}

/** 216px 药丸搜索框（query 命中名字或路径）。 */
function ProjectsSearch(props: { state: ProjectsState }): JSX.Element {
  return (
    <BarSearchInput
      inputTestID="projects-search-input"
      value={props.state.searchQuery()}
      placeholder={t("projects.searchPlaceholder")}
      onChange={(value) => props.state.searchQuery.set(value)}
    />
  );
}

/** bar_count：过滤后项目数（mod.rs filtered_count）。 */
function ProjectsCount(props: { state: ProjectsState }): JSX.Element {
  return <BarCount glyph="layers" label={String(props.state.filteredProjects().length)} />;
}

/** bar_primary「注册项目」：busy 或 dirty 时置灰禁用。 */
function RegisterButton(props: { state: ProjectsState }): JSX.Element {
  return (
    <BarPrimary
      testID="projects-register-btn"
      glyph="plus"
      label={t("projects.registerProject")}
      disabled={props.state.mutationBlocked()}
      onPress={() => openRegisterDialog(props.state)}
    />
  );
}

/** mod.rs render_project_list：288px 固定 rail。
 * 首次刷新完成前不画空态；慢于 300ms 才出与项目卡同高的骨架。 */
function ProjectRail(props: { state: ProjectsState }): JSX.Element {
  const state = props.state;
  const showSkeleton = bindDelayedFlag(() => !state.ready());
  return (
    <View
      testID="projects-rail"
      style={{
        width: 288,
        minWidth: 288,
        maxWidth: 320,
        flexShrink: 0,
        height: "100%",
        minHeight: 0,
        display: "flex",
        flexDirection: "column",
        borderRightWidth: 1,
        borderRightColor: paletteToken("border"),
      }}
    >
      <If when={showSkeleton()}>
        <ProjectRailSkeleton />
      </If>
      <If when={!showSkeleton() && state.ready() && state.projects().length === 0}>
        <RailEmpty />
      </If>
      <If when={!showSkeleton() && state.projects().length > 0}>
        <ProjectRailList state={state} />
      </If>
    </View>
  );
}

/** 与项目卡同高的脉冲行。慢加载才挂上，快刷新不闪空态。 */
function ProjectRailSkeleton(): JSX.Element {
  return (
    <View
      testID="projects-rail-skeleton"
      accessibilityState={{ busy: true }}
      style={{
        display: "flex",
        flexDirection: "column",
        gap: 4,
        padding: 8,
        width: "100%",
      }}
    >
      {[0, 1, 2, 3, 4, 5].map((index) => (
        <View
          style={{
            display: "flex",
            flexDirection: "column",
            justifyContent: "center",
            gap: 6,
            width: "100%",
            height: 58,
            paddingHorizontal: 12,
            borderRadius: 12,
            borderWidth: 1,
            borderColor: paletteToken("border"),
            backgroundColor: paletteToken("card"),
            flexShrink: 0,
          }}
        >
          <Pulse
            id={`proj-skel-${index}-name`}
            style={{ width: "48%", height: 14, borderRadius: 6 }}
          />
          <Pulse
            id={`proj-skel-${index}-path`}
            secondary
            style={{ width: "72%", height: 12, borderRadius: 6 }}
          />
        </View>
      ))}
    </View>
  );
}

/** kit Empty 对位：媒体圆片 + 标题 + 描述（mod.rs:285-300）。 */
function RailEmpty(): JSX.Element {
  return (
    <View
      testID="projects-rail-empty"
      style={{
        display: "flex",
        flex: 1,
        width: "100%",
        flexDirection: "column",
        alignItems: "center",
        justifyContent: "center",
        gap: 12,
        paddingHorizontal: 12,
      }}
    >
      <View
        style={{
          width: 56,
          height: 56,
          borderRadius: 999,
          backgroundColor: paletteToken("well"),
          display: "flex",
          alignItems: "center",
          justifyContent: "center",
        }}
      >
        <Icon name="folder" size={28} color={paletteToken("fg_muted")} />
      </View>
      <Text style={{ fontSize: 14, fontWeight: "600", color: paletteToken("fg") }}>
        {t("projects.emptyTitle")}
      </Text>
      <Text
        style={{
          fontSize: 12,
          color: paletteToken("fg_muted"),
          textAlign: "center",
          maxWidth: 220,
        }}
      >
        {t("projects.emptyDesc")}
      </Text>
    </View>
  );
}

/** rail 列表：过滤无果的居中提示，或项目卡（<List> 虚拟滚动）。 */
function ProjectRailList(props: { state: ProjectsState }): JSX.Element {
  const state = props.state;
  const filtered = computed(() => state.filteredProjects());
  return (
    <If
      when={filtered().length === 0}
      else={
        <List
          testID="projects-rail-scroll"
          each={filtered()}
          keyExtractor={(entry: ProjectEntry) => entry.name}
          estimatedItemSize={62}
          scrollEventThrottle={32}
          style={{ flex: 1, minHeight: 0, width: "100%" }}
          contentStyle={{ padding: 8, paddingBottom: 4 }}
        >
          {(entry: () => ProjectEntry) => (
            <View style={{ paddingBottom: 4, width: "100%" }}>
              <ProjectCard state={state} entry={entry()} />
            </View>
          )}
        </List>
      }
    >
      <View
        style={{
          display: "flex",
          flex: 1,
          width: "100%",
          flexDirection: "column",
          alignItems: "center",
          justifyContent: "center",
          paddingHorizontal: 12,
        }}
      >
        <Text style={{ fontSize: 12, textAlign: "center", color: paletteToken("fg_muted") }}>
          {t("projects.noMatching")}
        </Text>
      </View>
    </If>
  );
}

/** 单张项目卡（mod.rs:340-428）：选中 accent 边/accent_soft 底 + FolderOpen。
 * 选中态经每卡 computed 窄化（换选中卡时仅新旧两张卡局部刷新）。 */
function ProjectCard(props: { state: ProjectsState; entry: ProjectEntry }): JSX.Element {
  const state = props.state;
  const isSelected = computed(() => state.selected()?.name === props.entry.name);
  return (
    <View
      testID={`proj-card-${props.entry.name}`}
      style={{
        display: "flex",
        flexDirection: "row",
        alignItems: "center",
        justifyContent: "space-between",
        width: "100%",
        paddingHorizontal: 12,
        paddingVertical: 10,
        borderRadius: 12,
        cursor: "pointer",
        borderWidth: 1,
        borderColor: isSelected() ? paletteToken("accent") : paletteToken("border"),
        backgroundColor: isSelected() ? paletteToken("card_active") : paletteToken("card"),
        flexShrink: 0,
      }}
      hoverStyle={{
        borderColor: paletteToken("accent"),
        backgroundColor: isSelected()
          ? paletteToken("card_active_hover")
          : paletteToken("card_hover"),
      }}
      onPress={() => state.selectProject(props.entry)}
    >
      <View
        style={{
          display: "flex",
          alignItems: "center",
          gap: 8,
          flex: 1,
          minWidth: 0,
        }}
      >
        <Icon
          name={isSelected() ? "folderOpen" : "folder"}
          size={16}
          color={isSelected() ? paletteToken("accent_fg") : paletteToken("fg_muted")}
        />
        <View style={{ display: "flex", flex: 1, minWidth: 0 }}>
          <Text
            style={{
              fontSize: 14,
              fontWeight: isSelected() ? "600" : "400",
              color: paletteToken("fg"),
              numberOfLines: 1,
            }}
          >
            {props.entry.name}
          </Text>
          <Text
            style={{
              fontSize: 12,
              color: paletteToken("fg_muted"),
              numberOfLines: 1,
            }}
          >
            {props.entry.path}
          </Text>
        </View>
      </View>
    </View>
  );
}
