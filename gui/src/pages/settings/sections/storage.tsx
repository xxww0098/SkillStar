// 「存储」分区 UI — 态机在 ./storageState.ts（保持单文件 < 800 行）。

import { Text, View, computed, effect, type JSX, type Signal, type ViewStyle } from "nsh";
import { paletteToken, type PaletteToken } from "../../../services/themeMode";
import { t, tf } from "../../../services/translation";
import { AgentBadge } from "../../../chrome/agentRail";
import { Icon, type IconName } from "../../../chrome/icons";
import { IconSpin, Pulse } from "../../../chrome/feedback";
import { bindDelayedFlag } from "../../../chrome/delayedFlag";
import {
  card,
  formatBytes,
  MONO_FONT,
  SectionShell,
  SettingsButton,
  SettingsGroup,
  StatusPill,
  type StatusTone,
} from "../parts";
import { SETTINGS_SECTIONS } from "../sections";
import { st } from "../settingsStrings";
import { StorageBackupsCard } from "./storageBackups";
import {
  breakdownByteCounts,
  createStorageState,
  hubRelative,
  realStorageBridge,
  totalStorageBytes,
  type DeleteTarget,
  type StorageOverviewDto,
  type StorageState,
} from "./storageState";

export {
  asStorageOverview,
  breakdownByteCounts,
  confirmKeyOf,
  createStorageState,
  hubDeleteConfirm,
  hubDeleteStatus,
  hubRelative,
  realStorageBridge,
  totalStorageBytes,
  type ApplyReportDto,
  type BackupEntryDto,
  type BackupReason,
  type CacheCleanDto,
  type ClearBackupsReportDto,
  type DeleteTarget,
  type ForceDeleteReportDto,
  type IntakeItemDto,
  type IntakePreviewDto,
  type RepairPreviewDto,
  type StorageBridge,
  type StorageOverviewDto,
  type StorageState,
} from "./storageState";

// ---- 页面（SectionShell grouped 卡片栈） -------------------------------------

const pageState = createStorageState(realStorageBridge);

const ICON_WELL_TONE: Record<StatusTone, { bg: PaletteToken; border: PaletteToken; icon: PaletteToken }> = {
  neutral: { bg: "well", border: "border", icon: "fg_muted" },
  accent: { bg: "accent_soft", border: "accent_soft_edge", icon: "accent" },
  ok: { bg: "ok_bg", border: "ok_border", icon: "ok" },
  warn: { bg: "warn_bg", border: "warn_border", icon: "warn" },
  danger: { bg: "danger_bg", border: "danger_border", icon: "danger" },
  info: { bg: "info_bg", border: "info_border", icon: "info" },
  amber: { bg: "amber_bg", border: "amber_border", icon: "amber" },
  cyan: { bg: "cyan_bg", border: "cyan_border", icon: "cyan" },
};

function hasHealthOrIntake(overview: StorageOverviewDto | null): boolean {
  if (overview === null) return false;
  return overview.broken_count > 0 || overview.health_issue_count > 0 || overview.intake.length > 0;
}

function actionableIntakeCount(overview: StorageOverviewDto): number {
  return overview.intake.filter((item) => item.kind === "adopt" || item.kind === "relink").length;
}

function wellDrawerStyle(): ViewStyle {
  return {
    borderTopWidth: 1,
    borderTopColor: paletteToken("border_soft"),
    backgroundColor: paletteToken("well"),
    paddingHorizontal: 18,
    paddingVertical: 14,
    display: "flex",
    flexDirection: "column",
    gap: 10,
    width: "100%",
  };
}

export function StorageSection(): JSX.Element {
  return <StorageWorkbench state={pageState} />;
}

export function StorageWorkbench(props: { state: StorageState }): JSX.Element {
  const state = props.state;
  effect(() => state.start());
  return (
    <SectionShell
      section={SETTINGS_SECTIONS.find((section) => section.id === "storage")!}
      grouped
      action={<PathToggleButton state={state} />}
    >
      <>
        <OverviewCard state={state} />
        <If when={hasHealthOrIntake(state.storage())}>
          <HealthCard state={state} />
        </If>
        <If when={state.storage() !== null}>
          <DirectoriesCard state={state} />
        </If>
        <If when={state.backupsAvailable() && state.storage() !== null}>
          <StorageBackupsCard state={state} />
        </If>
      </>
    </SectionShell>
  );
}

function PathToggleButton(props: { state: StorageState }): JSX.Element {
  const state = props.state;
  return (
    <View
      testID="storage-path-toggle"
      role="button"
      style={{
        height: 30,
        paddingHorizontal: 11,
        display: "flex",
        flexDirection: "row",
        alignItems: "center",
        gap: 6,
        borderRadius: 9,
        borderWidth: 1,
        borderColor: state.pathStructureOpen() ? paletteToken("accent_soft_edge") : paletteToken("border"),
        backgroundColor: state.pathStructureOpen() ? paletteToken("card_active") : paletteToken("card"),
        cursor: "pointer",
      }}
      hoverStyle={{ backgroundColor: paletteToken("panel_hover") }}
      activeStyle={{ backgroundColor: paletteToken("well") }}
      onPress={() => state.togglePathStructure()}
    >
      <Text
        style={{
          fontSize: 12,
          fontWeight: "500",
          color: state.pathStructureOpen() ? paletteToken("accent_fg") : paletteToken("fg_muted"),
          numberOfLines: 1,
        }}
      >
        {state.pathStructureOpen() ? t("common.hide") : t("settings.viewPathStructure")}
      </Text>
      <Icon
        name={state.pathStructureOpen() ? "chevronDown" : "chevronRight"}
        size={14}
        color={state.pathStructureOpen() ? paletteToken("accent_fg") : paletteToken("fg_muted")}
      />
    </View>
  );
}

// ---- 卡 1：空间总览与占比条卡片 -----------------------------------------------

function OverviewCard(props: { state: StorageState }): JSX.Element {
  const state = props.state;
  const showSkeleton = bindDelayedFlag(() => state.storage() === null && state.storageLoading());
  return (
    <View style={card({ display: "flex", flexDirection: "column" })}>
      <SummaryBar state={state} skeleton={showSkeleton} />
      <If when={state.storage() !== null}>
        <BreakdownSection state={state} />
      </If>
      <If when={state.pathStructureOpen() && state.storage() !== null}>
        <PathStructure state={state} />
      </If>
      <If when={state.storage() === null && showSkeleton()}>
        <StorageSkeleton />
      </If>
      <If when={state.storageStatus() !== null}>
        <StatusFooter state={state} />
      </If>
    </View>
  );
}

function SummaryBar(props: { state: StorageState; skeleton: Signal<boolean> }): JSX.Element {
  const state = props.state;
  return (
    <View
      style={{
        paddingHorizontal: 18, paddingTop: 16, paddingBottom: 14,
        display: "flex", flexDirection: "row", alignItems: "center", justifyContent: "space-between", gap: 12,
      }}
    >
      <View style={{ display: "flex", flexDirection: "column", gap: 4, minWidth: 0, flex: 1 }}>
        <Text style={{ fontSize: 14, fontWeight: "600", color: paletteToken("fg") }}>{t("settings.storageTotal")}</Text>
        <If
          when={state.storage() !== null}
          else={
            <If when={!props.skeleton() && !state.storageLoading()}>
              <Text style={{ fontSize: 12, color: paletteToken("fg_muted") }}>{st("settings.storageNoData")}</Text>
            </If>
          }
        >
          <TotalBytes state={state} />
        </If>
      </View>
      <CleanButton state={state} />
    </View>
  );
}

function totalBytesLabel(state: StorageState): string {
  const overview = state.storage();
  if (overview === null) return formatBytes(0);
  return formatBytes(totalStorageBytes(overview));
}

/** 总存储字节数（config+hub+cache，local 已计入 hub_bytes）——在 JSX 表达式内响应式重算。 */
function TotalBytes(props: { state: StorageState }): JSX.Element {
  const state = props.state;
  return (
    <Text style={{ fontSize: 20, fontWeight: "600", fontFamily: MONO_FONT, fontVariant: ["tabular-nums"], color: paletteToken("fg") }}>
      {totalBytesLabel(state)}
    </Text>
  );
}

function BreakdownSection(props: { state: StorageState }): JSX.Element {
  const state = props.state;
  const segments = computed((): { key: string; label: string; bytes: number; token: PaletteToken }[] => {
    const overview = state.storage();
    if (overview === null) return [];
    const counts = breakdownByteCounts(overview);
    return [
      { key: "hub", label: t("settings.storageHub"), bytes: counts.hub, token: "accent" },
      { key: "cache", label: t("settings.repoCache"), bytes: counts.cache, token: "amber" },
      { key: "local", label: t("settings.storageLocal"), bytes: counts.local, token: "cyan" },
      { key: "config", label: t("settings.storageConfig"), bytes: counts.config, token: "info" },
    ];
  });
  const total = computed(() => segments().reduce((sum, seg) => sum + seg.bytes, 0));
  const nonZeroSegments = computed(() => segments().filter((seg) => seg.bytes > 0));
  return (
    <View style={{ paddingHorizontal: 18, paddingBottom: 16, display: "flex", flexDirection: "column", gap: 10, width: "100%" }}>
      <View
        style={{
          width: "100%",
          height: 8,
          borderRadius: 999,
          backgroundColor: paletteToken("well"),
          overflow: "hidden",
          display: "flex",
          flexDirection: "row",
          gap: 2,
        }}
      >
        <If when={total() > 0}>
          <Each of={nonZeroSegments()} key={(seg) => seg.key}>
            {(seg) => (
              <View
                style={{
                  height: "100%",
                  flexGrow: total() > 0 ? (seg().bytes / total()) * 100 : 0,
                  flexShrink: 1,
                  flexBasis: 0,
                  minWidth: 3,
                  borderRadius: 2,
                  backgroundColor: paletteToken(seg().token),
                }}
              />
            )}
          </Each>
        </If>
      </View>
      <View style={{ display: "flex", flexDirection: "row", flexWrap: "wrap", alignItems: "center", gap: 14 }}>
        <Each of={segments()} key={(seg) => seg.key}>
          {(seg) => (
            <View style={{ display: "flex", flexDirection: "row", alignItems: "center", gap: 6 }}>
              <View style={{ width: 8, height: 8, borderRadius: 4, backgroundColor: paletteToken(seg().token), flexShrink: 0 }} />
              <Text style={{ fontSize: 12, color: paletteToken("fg_muted") }}>{seg().label}</Text>
              <Text
                style={{
                  fontSize: 12,
                  fontWeight: "500",
                  fontFamily: MONO_FONT,
                  fontVariant: ["tabular-nums"],
                  color: paletteToken("fg"),
                }}
              >
                {formatBytes(seg().bytes)}
              </Text>
            </View>
          )}
        </Each>
      </View>
    </View>
  );
}

/** 常规清理（非破坏）。 */
function CleanButton(props: { state: StorageState }): JSX.Element {
  const state = props.state;
  return (
    <SettingsButton
      testID="storage-clean"
      label={t("settings.cleanAllCaches")}
      icon="eraser"
      spinning={state.cleaning()}
      variant="secondary"
      size="sm"
      disabled={state.storage() === null && state.storageLoading()}
      busy={state.cleaning()}
      onPress={() => state.cleanCaches()}
    />
  );
}

function StorageSkeleton(): JSX.Element {
  return (
    <View
      testID="storage-skeleton"
      style={{
        paddingHorizontal: 18,
        paddingVertical: 14,
        borderTopWidth: 1,
        borderTopColor: paletteToken("border_soft"),
        display: "flex",
        flexDirection: "column",
        gap: 8,
        width: "100%",
      }}
    >
      <Pulse id="storage-skel-total" style={{ width: "36%", height: 14, borderRadius: 6 }} />
      <Pulse id="storage-skel-bar" secondary style={{ width: "100%", height: 8, borderRadius: 999 }} />
      <Pulse id="storage-skel-meta" secondary style={{ width: "62%", height: 10, borderRadius: 5 }} />
    </View>
  );
}

function StatusFooter(props: { state: StorageState }): JSX.Element {
  const state = props.state;
  return (
    <View style={{ paddingHorizontal: 18, paddingVertical: 11, borderTopWidth: 1, borderTopColor: paletteToken("border_soft"), backgroundColor: paletteToken("well") }}>
      <Text
        testID="storage-status"
        style={{
          fontSize: 12,
          lineHeight: 18,
          color:
            state.storageConfirm() !== null || state.storageStatus() === t("settings.confirmForceDelete")
              ? paletteToken("warn")
              : state.storageStatus() === t("settings.forceDeleteFailed") ||
                  state.storageStatus() === t("settings.cacheCleanFailed")
                ? paletteToken("danger")
                : paletteToken("fg_muted"),
        }}
      >
        {state.storageStatus()}
      </Text>
    </View>
  );
}

/** 路径模型抽屉：嵌于总览卡底部（Data/Hub 两张根路径卡 + 嵌套关系）。 */
function PathStructure(props: { state: StorageState }): JSX.Element {
  const state = props.state;
  const overview = computed(() => state.storage());
  return (
    <View testID="storage-path-structure" style={wellDrawerStyle()}>
      <If when={overview() !== null}>
        <>
          <Text style={{ fontSize: 12, color: paletteToken("fg_muted"), lineHeight: 18 }}>
            {t("settings.storagePathModelHint")}
          </Text>
          <View style={{ display: "flex", flexDirection: "row", gap: 10, width: "100%" }}>
            <RootCard
              title={t("settings.storageRootData")}
              tag={t("settings.storageSourceData")}
              path={overview()?.data_root_path ?? ""}
              includes={[t("settings.storageConfig"), t("settings.storageHistory")].join(" · ")}
            />
            <RootCard
              title={t("settings.storageRootHub")}
              tag={t("settings.storageSourceHub")}
              path={overview()?.hub_root_path ?? ""}
              includes={[t("settings.storageHub"), t("settings.storageLocal"), t("settings.repoCache")].join(" · ")}
            />
          </View>
          <Text style={{ fontSize: 12, color: paletteToken("fg_muted") }}>
            {overview()?.is_hub_under_data
              ? tf("settings.storagePathRelationNested", {
                  relative: hubRelative(overview()?.data_root_path ?? "", overview()?.hub_root_path ?? ""),
                })
              : t("settings.storagePathRelationIndependent")}
          </Text>
        </>
      </If>
    </View>
  );
}

function RootCard(props: { title: string; tag: string; path: string; includes: string }): JSX.Element {
  return (
    <View
      style={{
        flex: 1,
        minWidth: 0,
        borderRadius: 10,
        borderWidth: 1,
        borderColor: paletteToken("border_soft"),
        backgroundColor: paletteToken("panel"),
        padding: 12,
        display: "flex",
        flexDirection: "column",
        gap: 4,
      }}
    >
      <View style={{ display: "flex", flexDirection: "row", alignItems: "center", gap: 8 }}>
        <Text style={{ fontSize: 12, fontWeight: "600", color: paletteToken("fg") }}>{props.title}</Text>
        <StatusPill text={props.tag} tone="neutral" />
      </View>
      <Text style={{ fontSize: 12, fontFamily: MONO_FONT, color: paletteToken("fg"), numberOfLines: 2 }}>
        {props.path}
      </Text>
      <Text style={{ fontSize: 12, color: paletteToken("fg_muted") }}>
        {`${t("settings.storagePathContains")}: ${props.includes}`}
      </Text>
    </View>
  );
}

// ---- 卡 2：健康与本地技能纳管卡 -----------------------------------------------

function HealthCard(props: { state: StorageState }): JSX.Element {
  const state = props.state;
  const summary = computed(() => {
    const overview = state.storage();
    if (overview === null) {
      return {
        ready: false,
        brokenCount: 0,
        intakeCount: 0,
        detail: "",
        warnHighlight: false,
      };
    }
    const actionable = actionableIntakeCount(overview);
    const reported = overview.intake.length - actionable;
    const detail = [
      tf("settings.healthSummary", {
        count: String(overview.health_issue_count),
        repairable: String(overview.health_repairable),
      }),
      ...(overview.intake.length > 0
        ? [tf("settings.intakeSummary", { actionable: String(actionable), reported: String(reported) })]
        : []),
    ].join(" · ");
    return {
      ready: true,
      brokenCount: overview.broken_count,
      intakeCount: overview.intake.length,
      detail,
      warnHighlight: overview.health_issue_count > 0 || actionable > 0,
    };
  });
  return (
    <View style={card({ borderColor: paletteToken("warn_border"), display: "flex", flexDirection: "column" })}>
      <If when={summary().ready}>
        <>
          <View
            style={{
              paddingHorizontal: 18,
              paddingVertical: 15,
              display: "flex",
              flexDirection: "row",
              alignItems: "flex-start",
              justifyContent: "space-between",
              gap: 12,
              flexWrap: "wrap",
            }}
          >
            <View style={{ display: "flex", flexDirection: "row", alignItems: "flex-start", gap: 12, flex: 1, minWidth: 220 }}>
              <IconBox icon="stethoscope" tone="warn" />
              <View style={{ flex: 1, minWidth: 0, display: "flex", flexDirection: "column", gap: 3 }}>
                <View style={{ display: "flex", flexDirection: "row", alignItems: "center", gap: 8 }}>
                  <Text style={{ fontSize: 14, fontWeight: "600", color: paletteToken("fg") }}>
                    {t("settings.skillHealth")}
                  </Text>
                  <If when={summary().brokenCount > 0}>
                    <StatusPill text={tf("settings.healthBroken", { count: String(summary().brokenCount) })} tone="warn" />
                  </If>
                </View>
                <Text style={{ fontSize: 12, color: paletteToken("fg_muted"), lineHeight: 18 }}>
                  {t("settings.repairHint")}
                </Text>
                <Text style={{ fontSize: 12, fontWeight: "500", color: summary().warnHighlight ? paletteToken("warn") : paletteToken("fg_muted") }}>
                  {summary().detail}
                </Text>
              </View>
            </View>
            <View style={{ display: "flex", flexDirection: "row", alignItems: "center", gap: 6, flexShrink: 0 }}>
              <SettingsButton
                testID="storage-repair-preview"
                label={t("settings.repairPreview")}
                spinning={state.previewing()}
                variant="secondary"
                size="sm"
                busy={state.busyRepair()}
                onPress={() => state.previewSkillRepair()}
              />
              <SettingsButton
                testID="storage-repair"
                label={t("settings.repairAll")}
                icon="wrench"
                spinning={state.repairing()}
                variant="warn"
                size="sm"
                busy={state.busyRepair()}
                onPress={() => state.repairSkills()}
              />
            </View>
          </View>
          <If when={summary().intakeCount > 0}>
            <IntakeLines state={state} />
          </If>
        </>
      </If>
    </View>
  );
}

function intakeNoteOf(kind: string): string {
  switch (kind) {
    case "adopt":
      return t("settings.intakeAdopt");
    case "relink":
      return t("settings.intakeRelink");
    case "conflict":
      return t("settings.intakeConflict");
    case "excluded":
      return t("settings.intakeExcluded");
    case "foreign":
      return t("settings.intakeForeign");
    default:
      return t("settings.intakeOccupied");
  }
}

/** intake_lines：前 8 条 + 折叠计数 + 纳管两钮。 */
function IntakeLines(props: { state: StorageState }): JSX.Element {
  const state = props.state;
  const info = computed(() => {
    const overview = state.storage();
    if (overview === null) {
      return { ready: false, items: [] as StorageOverviewDto["intake"], total: 0, actionable: 0 };
    }
    return {
      ready: true,
      items: overview.intake.slice(0, 8),
      total: overview.intake.length,
      actionable: actionableIntakeCount(overview),
    };
  });
  return (
    <View style={wellDrawerStyle()}>
      <If when={info().ready}>
        <>
          <Text style={{ fontSize: 12, color: paletteToken("fg_muted"), lineHeight: 18 }}>
            {t("settings.intakeHint")}
          </Text>
          <View style={{ display: "flex", flexDirection: "column", gap: 6, width: "100%" }}>
            <Each of={info().items} key={(item) => `${item.agentId}:${item.skill}:${item.kind}`}>
              {(item) => (
                <View style={{ display: "flex", flexDirection: "row", flexWrap: "wrap", alignItems: "center", gap: 8, paddingVertical: 2 }}>
                  <AgentBadge id={item().agentId} name={item().agentId} size={22} />
                  <Text style={{ fontSize: 12, fontWeight: "500", color: paletteToken("fg_muted") }}>{item().agentId}</Text>
                  <Text style={{ fontSize: 12, fontWeight: "600", fontFamily: MONO_FONT, color: paletteToken("fg") }}>
                    {item().skill}
                  </Text>
                  <Text style={{ fontSize: 12, color: paletteToken("fg_muted"), flex: 1, minWidth: 120 }}>
                    {`— ${intakeNoteOf(item().kind)}`}
                  </Text>
                </View>
              )}
            </Each>
          </View>
          <If when={info().total > 8}>
            <Text style={{ fontSize: 12, color: paletteToken("fg_muted") }}>
              {tf("settings.intakeMore", { count: String(info().total - 8) })}
            </Text>
          </If>
          <View style={{ display: "flex", flexDirection: "row", alignItems: "center", gap: 6, marginTop: 2 }}>
            <SettingsButton
              testID="storage-intake-preview"
              label={t("settings.intakePreview")}
              spinning={state.previewingIntake()}
              variant="secondary"
              size="sm"
              busy={state.busyRepair()}
              onPress={() => state.previewAgentIntake()}
            />
            <If when={info().actionable > 0}>
              <SettingsButton
                testID="storage-intake"
                label={t("settings.intakeApply")}
                icon="wrench"
                spinning={state.adopting()}
                variant="warn"
                size="sm"
                busy={state.busyRepair()}
                onPress={() => state.adoptAgentSkills()}
              />
            </If>
          </View>
        </>
      </If>
    </View>
  );
}

// ---- 卡 3：目录明细与维护 5 行列表 --------------------------------------------

function DirectoriesCard(props: { state: StorageState }): JSX.Element {
  const state = props.state;
  const rows = computed(() => {
    const overview = state.storage();
    return overview === null ? [] : directorySpecs(overview);
  });
  return (
    <SettingsGroup title={st("settings.group.storageDirectories")}>
      <View style={{ display: "flex", flexDirection: "column", width: "100%" }}>
        <Each of={rows()} key={(row) => row.folderId}>
          {(row) => (
            <PlainRow
              first={row().first}
              folderId={row().folderId}
              icon={row().icon}
              tone={row().tone}
              label={row().label}
              source={row().source}
              detail={row().detail}
              highlight={row().highlight}
              onOpen={() => state.reveal(row().path)}
            >
              <If when={row().deleteTarget !== undefined}>
                <DeleteButton state={state} target={row().deleteTarget!} />
              </If>
            </PlainRow>
          )}
        </Each>
      </View>
    </SettingsGroup>
  );
}

interface DirectoryRowSpec {
  first?: boolean;
  folderId: string;
  icon: IconName;
  tone: StatusTone;
  label: string;
  source: string;
  detail: string;
  highlight: boolean;
  path: string;
  deleteTarget?: DeleteTarget;
}

export function directorySpecs(overview: StorageOverviewDto): DirectoryRowSpec[] {
  const hubDetail = [
    formatBytes(overview.hub_bytes),
    tf("settings.storageHubCount", { count: String(overview.hub_count) }),
    ...(overview.broken_count > 0 ? [`(${tf("settings.healthBroken", { count: String(overview.broken_count) })})`] : []),
  ].join(" · ");
  const cacheDetail =
    overview.cache_count > 0
      ? [
          formatBytes(overview.cache_bytes),
          tf("settings.cacheRepos", { count: String(overview.cache_count) }),
          ...(overview.cache_unused_count > 0
            ? [tf("settings.cacheUnused", { count: String(overview.cache_unused_count) }), `(${formatBytes(overview.cache_unused_bytes)})`]
            : []),
        ].join(" · ")
      : t("settings.storageEmpty");
  const localDetail =
    overview.local_count > 0
      ? `${formatBytes(overview.local_bytes)} · ${tf("settings.storageLocalCount", { count: String(overview.local_count) })}`
      : t("settings.storageEmpty");
  const historyDetail =
    overview.history_count > 0
      ? tf("settings.storageHistoryCount", { count: String(overview.history_count) })
      : t("settings.storageEmpty");

  return [
    { first: true, folderId: "hub", icon: "database", tone: "accent", label: t("settings.storageHub"), source: t("settings.storageSourceHub"), detail: hubDetail, highlight: overview.broken_count > 0, path: overview.hub_path, deleteTarget: "hub" },
    { folderId: "cache", icon: "folderGit2", tone: "amber", label: t("settings.repoCache"), source: t("settings.storageSourceHub"), detail: cacheDetail, highlight: overview.cache_unused_count > 0, path: overview.cache_path, deleteTarget: "cache" },
    { folderId: "local", icon: "folderOpen", tone: "cyan", label: t("settings.storageLocal"), source: t("settings.storageSourceHub"), detail: localDetail, highlight: false, path: overview.local_path },
    { folderId: "config", icon: "sliders", tone: "neutral", label: t("settings.storageConfig"), source: t("settings.storageSourceData"), detail: formatBytes(overview.config_bytes), highlight: false, path: overview.config_path },
    { folderId: "history", icon: "fileClock", tone: "ok", label: t("settings.storageHistory"), source: t("settings.storageSourceData"), detail: historyDetail, highlight: overview.history_count > 0, path: overview.skills_data_path || overview.config_path },
  ];
}

function IconBox(props: { icon: IconName; tone: StatusTone }): JSX.Element {
  const tokens = () => ICON_WELL_TONE[props.tone];
  return (
    <View
      style={{
        width: 36, height: 36, borderRadius: 10, flexShrink: 0,
        borderWidth: 1, borderColor: paletteToken(tokens().border), backgroundColor: paletteToken(tokens().bg),
        display: "flex", alignItems: "center", justifyContent: "center",
      }}
    >
      <Icon name={props.icon} size={16} color={paletteToken(tokens().icon)} />
    </View>
  );
}

/** 两步强删：第一击 arm（危险徽章确认态），第二击执行。 */
function DeleteButton(props: { state: StorageState; target: DeleteTarget }): JSX.Element {
  const state = props.state;
  const armed = () =>
    state.storageConfirm() === props.target || state.storageBusy() === props.target;
  return (
    <View
      testID={`storage-del-${props.target}`}
      role="button"
      accessibilityState={{ busy: state.storageBusy() === props.target }}
      style={{
        height: 30,
        minWidth: 30,
        paddingHorizontal: armed() ? 10 : 6,
        display: "flex",
        flexDirection: "row",
        alignItems: "center",
        justifyContent: "center",
        gap: armed() ? 4 : 0,
        borderRadius: 8,
        borderWidth: 1,
        borderColor: armed() ? paletteToken("danger_border") : "transparent",
        backgroundColor: armed() ? paletteToken("danger_bg") : "transparent",
        cursor: state.storageBusy() === props.target ? "default" : "pointer",
      }}
      hoverStyle={
        state.storageBusy() === props.target
          ? undefined
          : state.storageConfirm() === props.target
            ? { backgroundColor: paletteToken("danger_hover") }
            : { backgroundColor: paletteToken("panel_hover") }
      }
      activeStyle={{
        backgroundColor: armed() ? paletteToken("danger_bg") : paletteToken("well"),
      }}
      onPress={() => {
        if (state.storageBusy() !== props.target) state.armDelete(props.target);
      }}
    >
      <IconSpin
        name="trash"
        size={14}
        color={armed() ? paletteToken("danger") : paletteToken("fg_muted")}
        spin={state.storageBusy() === props.target}
      />
      <If when={armed()}>
        <Text style={{ fontSize: 12, fontWeight: "500", color: paletteToken("danger_fg") }}>
          {t("common.delete")}
        </Text>
      </If>
    </View>
  );
}

/** 图标底座 + 标题 + 来源胶囊 + 等宽明细 + 打开钮（+ extra）。 */
function PlainRow(props: {
  first?: boolean;
  folderId: string;
  icon: IconName;
  tone: StatusTone;
  label: string;
  source: string | null;
  detail: string;
  highlight: boolean;
  onOpen: () => void;
  children?: JSX.Element;
}): JSX.Element {
  return (
    <View
      style={{
        paddingHorizontal: 18,
        paddingVertical: 14,
        display: "flex",
        flexDirection: "row",
        alignItems: "center",
        gap: 12,
        borderTopWidth: props.first ? 0 : 1,
        borderTopColor: props.first ? "transparent" : paletteToken("border_soft"),
      }}
    >
      <IconBox icon={props.icon} tone={props.tone} />
      <View style={{ flex: 1, minWidth: 0, display: "flex", flexDirection: "column", gap: 3 }}>
        <View style={{ display: "flex", flexDirection: "row", alignItems: "center", gap: 8 }}>
          <Text style={{ fontSize: 14, fontWeight: "500", color: paletteToken("fg") }}>{props.label}</Text>
          <If when={props.source !== null}>
            <StatusPill text={props.source!} tone="neutral" />
          </If>
        </View>
        <Text
          style={{
            fontSize: 12, fontFamily: MONO_FONT, fontVariant: ["tabular-nums"], numberOfLines: 1,
            color: props.highlight ? paletteToken("warn") : paletteToken("fg_muted"),
          }}
        >
          {props.detail}
        </Text>
      </View>
      <View style={{ display: "flex", flexDirection: "row", alignItems: "center", gap: 6, flexShrink: 0 }}>
        {props.children}
        <View
          testID={`storage-open-${props.folderId}`}
          role="button"
          style={{
            width: 30, height: 30, display: "flex", alignItems: "center", justifyContent: "center",
            borderRadius: 8, borderWidth: 1, borderColor: paletteToken("border_soft"), backgroundColor: paletteToken("well"), cursor: "pointer",
          }}
          hoverStyle={{ backgroundColor: paletteToken("panel_hover"), borderColor: paletteToken("border") }}
          activeStyle={{ backgroundColor: paletteToken("border_soft") }}
          onPress={props.onOpen}
        >
          <Icon name="folderOpen" size={14} color={paletteToken("fg_muted")} />
        </View>
      </View>
    </View>
  );
}
