import { View, computed, effect, signal, type JSX } from "nsh";
import { currentPage } from "../../shell/nav";
import { t } from "../../services/translation";
import { Sheet } from "../../chrome/sheet";
import {
  BarCount,
  BarIconButton,
  BarPrimary,
  BarSearchInput,
  BarSegmentSlot,
  BarSegmented,
  PageBar,
} from "../../chrome/toolbar";
import { realMcpBridge } from "./bridge";
import { McpDetailDrawer } from "./detailDrawer";
import { McpModalHost } from "./editorModal";
import { McpLibraryPane } from "./libraryPane";
import { ms } from "./mcpStrings";
import { ProjectSnippetsView } from "./projectSnippets";
import { createMcpState, type McpState } from "./state";
import { McpStoreView } from "./storeView";

const pageState = createMcpState(realMcpBridge);

export function McpPage(): JSX.Element {
  return <McpWorkbench state={pageState} active={() => currentPage() === "mcp"} />;
}

function McpScopePane(props: {
  shown: () => boolean;
  children: JSX.Element;
}): JSX.Element {
  return (
    <View
      style={{
        position: "absolute",
        top: 0,
        left: 0,
        right: 0,
        bottom: 0,
        display: "flex",
        flexDirection: "column",
        opacity: props.shown() ? 1 : 0,
        pointerEvents: props.shown() ? "auto" : "none",
        zIndex: props.shown() ? 1 : 0,
      }}
    >
      {props.children}
    </View>
  );
}

export function McpWorkbench(props: {
  state: McpState;
  active?: () => boolean;
}): JSX.Element {
  const visitedStore = signal(props.state.tab() === "store");
  const visitedProject = signal(props.state.tab() === "project");
  const showSearch = computed(() => props.state.tab() !== "project");

  effect(() => {
    const isActive = props.active ? props.active() : true;
    props.state.pageActive.set(isActive);
    if (isActive) {
      void props.state.ensureStarted();
    }
  });

  effect(() => {
    const cur = props.state.tab();
    if (cur === "store" && !visitedStore()) visitedStore.set(true);
    if (cur === "project" && !visitedProject()) visitedProject.set(true);
  });

  return (
    <View
      testID="mcp-workbench"
      style={{
        position: "relative",
        display: "flex",
        flexDirection: "column",
        flex: 1,
        minHeight: 0,
        width: "100%",
        height: "100%",
      }}
    >
      <PageBar
        title={t("sidebar.mcp")}
        dragId="mcp-toolbar-drag"
        titleExtras={[<McpTabBar state={props.state} />]}
        search={showSearch() ? <McpSearchBox state={props.state} /> : undefined}
        filters={showSearch() ? [<McpCountChip state={props.state} />] : []}
        actions={[
          <BarIconButton
            testID="mcp-btn-backups"
            glyph="rotateCcw"
            disabled={props.state.busy()}
            tip={ms().btnBackups}
            onPress={() => props.state.openBackupsModal()}
          />,
          <BarIconButton
            testID="mcp-btn-sync"
            glyph="refresh"
            loading={props.state.busy()}
            tip={ms().btnSync}
            onPress={() => void props.state.syncAll()}
          />,
          <BarIconButton
            testID="mcp-btn-paste"
            glyph="clipboard"
            disabled={props.state.busy()}
            tip={ms().btnPaste}
            onPress={() => props.state.openPasteModal()}
          />,
          <BarPrimary
            testID="mcp-btn-new"
            glyph="plus"
            label={ms().btnNew}
            disabled={props.state.busy()}
            onPress={() => props.state.openNewModal()}
          />,
        ]}
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
        <View
          style={{
            position: "relative",
            flex: 1,
            minWidth: 0,
            minHeight: 0,
          }}
        >
          <McpScopePane shown={() => props.state.tab() === "managed"}>
            <McpLibraryPane state={props.state} />
          </McpScopePane>
          <If when={visitedStore()}>
            <McpScopePane shown={() => props.state.tab() === "store"}>
              <McpStoreView state={props.state} />
            </McpScopePane>
          </If>
          <If when={visitedProject()}>
            <McpScopePane shown={() => props.state.tab() === "project"}>
              <ProjectSnippetsView state={props.state} />
            </McpScopePane>
          </If>
        </View>

        <If when={props.state.tab() === "managed" && props.state.selectedServer() !== null}>
          <Sheet>
            <McpDetailDrawer state={props.state} />
          </Sheet>
        </If>
      </View>

      <McpModalHost state={props.state} />
    </View>
  );
}

function McpTabBar(props: { state: McpState }): JSX.Element {
  return (
    <BarSegmented
      testID="mcp-tabs"
      slots={[
        <BarSegmentSlot
          testID="mcp-tab-managed"
          tone="mode"
          label={ms().tabManaged}
          selected={props.state.tab() === "managed"}
          onPress={() => props.state.tab.set("managed")}
        />,
        <BarSegmentSlot
          testID="mcp-tab-store"
          tone="mode"
          label={ms().tabStore}
          selected={props.state.tab() === "store"}
          onPress={() => {
            props.state.tab.set("store");
            if (props.state.storeItems().length === 0) {
              void props.state.searchStore();
            }
          }}
        />,
        <BarSegmentSlot
          testID="mcp-tab-project"
          tone="mode"
          label={ms().tabProject}
          selected={props.state.tab() === "project"}
          onPress={() => {
            props.state.tab.set("project");
            void props.state.loadProjectSnippets();
          }}
        />,
      ]}
    />
  );
}

function McpSearchBox(props: { state: McpState }): JSX.Element {
  return (
    <If
      when={props.state.tab() === "store"}
      else={
        <BarSearchInput
          inputTestID="mcp-search-input"
          clearTestID="mcp-search-clear"
          value={props.state.search()}
          placeholder={ms().searchManaged}
          onChange={(v) => props.state.search.set(v)}
        />
      }
    >
      <BarSearchInput
        inputTestID="mcp-store-search-input"
        clearTestID="mcp-store-search-clear"
        value={props.state.storeQuery()}
        placeholder={ms().searchStore}
        onChange={(v) => props.state.onStoreSearchInput(v)}
      />
    </If>
  );
}

function McpCountChip(props: { state: McpState }): JSX.Element {
  return (
    <BarCount
      glyph={props.state.tab() === "store" ? "store" : "server"}
      label={
        props.state.tab() === "store"
          ? `${props.state.filteredStoreItems().length} ${ms().countSuffix}`
          : `${props.state.servers().length} ${ms().countSuffix}`
      }
    />
  );
}
