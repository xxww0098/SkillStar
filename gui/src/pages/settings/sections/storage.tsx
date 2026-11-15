// 「存储」分区 UI — 态机在 ./storageState.ts（保持单文件 < 800 行）。

import { Text, View, effect, type JSX, type ViewStyle } from "nsh";
import { paletteToken, type PaletteToken } from "../../../services/themeMode";
import { t, tf } from "../../../services/translation";
import { AgentBadge } from "../../../chrome/agentRail";
import { Icon, type IconName } from "../../../chrome/icons";
import { IconSpin } from "../../../chrome/feedback";
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
  createStorageState,
  hubRelative,
  realStorageBridge,
  type DeleteTarget,
  type StorageOverviewDto,
  type StorageState,
} from "./storageState";

export {
  asStorageOverview,
  confirmKeyOf,
  createStorageState,
  hubDeleteConfirm,
  hubDeleteStatus,
  hubRelative,
  realStorageBridge,
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
        {!state.storageLoading() && hasHealthOrIntake(state.storage()) ? <HealthCard state={state} /> : null}
        {state.storage() !== null && !state.storageLoading() ? <DirectoriesCard state={state} /> : null}
        {state.backupsAvailable() && state.storage() !== null && !state.storageLoading() ? (
          <StorageBackupsCard state={state} />
        ) : null}
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
  return (
    <View style={card({ display: "flex", flexDirection: "column" })}>
      <SummaryBar state={state} />
      {state.storage() !== null && !state.storageLoading() ? breakdownSection(state) : null}
      {state.pathStructureOpen() && state.storage() !== null && !state.storageLoading() ? (
        <PathStructure state={state} />
      ) : null}
      {state.storageLoading() ? <LoadingRow /> : null}
      {state.storageStatus() !== null ? <StatusFooter state={state} /> : null}
    </View>
  );
}

function SummaryBar(props: { state: StorageState }): JSX.Element {
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
        {state.storage() !== null && !state.storageLoading() ? (
          <TotalBytes state={state} />
        ) : !state.storageLoading() ? (
          <Text style={{ fontSize: 12, color: paletteToken("fg_muted") }}>{st("settings.storageNoData")}</Text>
        ) : null}
      </View>
      <CleanButton state={state} />
    </View>
  );
}

function totalBytesLabel(state: StorageState): string {
  const overview = state.storage();
  if (overview === null) return formatBytes(0);
  return formatBytes(overview.config_bytes + overview.hub_bytes + overview.local_bytes + overview.cache_bytes);
}

/** 总存储字节数（config+hub+local+cache）——在 JSX 表达式内响应式重算。 */
function TotalBytes(props: { state: StorageState }): JSX.Element {
  const state = props.state;
  return (
    <Text style={{ fontSize: 20, fontWeight: "600", fontFamily: MONO_FONT, fontVariant: ["tabular-nums"], color: paletteToken("fg") }}>
      {totalBytesLabel(state)}
    </Text>
  );
}

function breakdownSection(state: StorageState): JSX.Element {
  const overview = state.storage();
  if (overview === null) return <View />;
  const segments: { label: string; bytes: number; token: PaletteToken }[] = [
    { label: t("settings.storageHub"), bytes: Math.max(0, overview.hub_bytes), token: "accent" },
    { label: t("settings.repoCache"), bytes: Math.max(0, overview.cache_bytes), token: "amber" },
    { label: t("settings.storageLocal"), bytes: Math.max(0, overview.local_bytes), token: "cyan" },
    { label: t("settings.storageConfig"), bytes: Math.max(0, overview.config_bytes), token: "info" },
  ];
  const total = segments.reduce((sum, seg) => sum + seg.bytes, 0);
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
        {total === 0
          ? null
          : segments
              .filter((seg) => seg.bytes > 0)
              .map((seg) => (
                <View
                  style={{
                    height: "100%",
                    flexGrow: (seg.bytes / total) * 100,
                    flexShrink: 1,
                    flexBasis: 0,
                    minWidth: 3,
                    borderRadius: 2,
                    backgroundColor: paletteToken(seg.token),
                  }}
                />
              ))}
      </View>
      <View style={{ display: "flex", flexDirection: "row", flexWrap: "wrap", alignItems: "center", gap: 14 }}>
        {segments.map((seg) => (
          <View style={{ display: "flex", flexDirection: "row", alignItems: "center", gap: 6 }}>
            <View style={{ width: 8, height: 8, borderRadius: 4, backgroundColor: paletteToken(seg.token), flexShrink: 0 }} />
            <Text style={{ fontSize: 12, color: paletteToken("fg_muted") }}>{seg.label}</Text>
            <Text
              style={{
                fontSize: 12,
                fontWeight: "500",
                fontFamily: MONO_FONT,
                fontVariant: ["tabular-nums"],
                color: paletteToken("fg"),
              }}
            >
              {formatBytes(seg.bytes)}
            </Text>
          </View>
        ))}
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
      label={state.cleaning() ? t("settings.cleaning") : t("settings.cleanAllCaches")}
      icon={state.cleaning() ? "loader" : "eraser"}
      spinning={state.cleaning()}
      variant="secondary"
      size="sm"
      disabled={state.storageLoading()}
      busy={state.cleaning()}
      onPress={() => state.cleanCaches()}
    />
  );
}

function LoadingRow(): JSX.Element {
  return (
    <View
      style={{
        paddingHorizontal: 18, paddingVertical: 14, borderTopWidth: 1, borderTopColor: paletteToken("border_soft"),
        display: "flex", flexDirection: "row", alignItems: "center", gap: 8,
      }}
    >
      <IconSpin name="loader" size={14} color={paletteToken("fg_muted")} spin />
      <Text style={{ fontSize: 12, color: paletteToken("fg_muted") }}>{t("common.loading")}</Text>
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
  return (
    <View testID="storage-path-structure" style={wellDrawerStyle()}>
      {pathStructureContent(state)}
    </View>
  );
}

function pathStructureContent(state: StorageState): JSX.Element {
  const overview = state.storage();
  if (overview === null) return <View />;
  return (
    <>
      <Text style={{ fontSize: 12, color: paletteToken("fg_muted"), lineHeight: 18 }}>
        {t("settings.storagePathModelHint")}
      </Text>
      <View style={{ display: "flex", flexDirection: "row", gap: 10, width: "100%" }}>
        <RootCard
          title={t("settings.storageRootData")}
          tag={t("settings.storageSourceData")}
          path={overview.data_root_path}
          includes={[t("settings.storageConfig"), t("settings.storageHistory")].join(" · ")}
        />
        <RootCard
          title={t("settings.storageRootHub")}
          tag={t("settings.storageSourceHub")}
          path={overview.hub_root_path}
          includes={[t("settings.storageHub"), t("settings.storageLocal"), t("settings.repoCache")].join(" · ")}
        />
      </View>
      <Text style={{ fontSize: 12, color: paletteToken("fg_muted") }}>
        {overview.is_hub_under_data
          ? tf("settings.storagePathRelationNested", {
              relative: hubRelative(overview.data_root_path, overview.hub_root_path),
            })
          : t("settings.storagePathRelationIndependent")}
      </Text>
    </>
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
  return (
    <View style={card({ borderColor: paletteToken("warn_border"), display: "flex", flexDirection: "column" })}>
      {healthCardContent(state)}
    </View>
  );
}

function healthCardContent(state: StorageState): JSX.Element {
  const overview = state.storage();
  if (overview === null) return <View />;
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
  const warnHighlight = overview.health_issue_count > 0 || actionable > 0;
  return (
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
              {overview.broken_count > 0 ? (
                <StatusPill text={tf("settings.healthBroken", { count: String(overview.broken_count) })} tone="warn" />
              ) : null}
            </View>
            <Text style={{ fontSize: 12, color: paletteToken("fg_muted"), lineHeight: 18 }}>
              {t("settings.repairHint")}
            </Text>
            <Text style={{ fontSize: 12, fontWeight: "500", color: warnHighlight ? paletteToken("warn") : paletteToken("fg_muted") }}>
              {detail}
            </Text>
          </View>
        </View>
        <View style={{ display: "flex", flexDirection: "row", alignItems: "center", gap: 6, flexShrink: 0 }}>
          <SettingsButton
            testID="storage-repair-preview"
            label={state.previewing() ? t("settings.repairPreviewing") : t("settings.repairPreview")}
            icon={state.previewing() ? "loader" : undefined}
            spinning={state.previewing()}
            variant="secondary"
            size="sm"
            busy={state.busyRepair()}
            onPress={() => state.previewSkillRepair()}
          />
          <SettingsButton
            testID="storage-repair"
            label={state.repairing() ? t("settings.repairing") : t("settings.repairAll")}
            icon={state.repairing() ? "loader" : "wrench"}
            spinning={state.repairing()}
            variant="warn"
            size="sm"
            busy={state.busyRepair()}
            onPress={() => state.repairSkills()}
          />
        </View>
      </View>
      {overview.intake.length > 0 ? <IntakeLines state={state} /> : null}
    </>
  );
}

/** intake_lines：前 8 条 + 折叠计数 + 纳管两钮。 */
function IntakeLines(props: { state: StorageState }): JSX.Element {
  const state = props.state;
  return <View style={wellDrawerStyle()}>{intakeLinesContent(state)}</View>;
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

function intakeLinesContent(state: StorageState): JSX.Element {
  const overview = state.storage();
  if (overview === null) return <View />;
  const actionable = actionableIntakeCount(overview);
  return (
    <>
      <Text style={{ fontSize: 12, color: paletteToken("fg_muted"), lineHeight: 18 }}>
        {t("settings.intakeHint")}
      </Text>
      <View style={{ display: "flex", flexDirection: "column", gap: 6, width: "100%" }}>
        {overview.intake.slice(0, 8).map((item) => (
          <View style={{ display: "flex", flexDirection: "row", flexWrap: "wrap", alignItems: "center", gap: 8, paddingVertical: 2 }}>
            <AgentBadge id={item.agentId} name={item.agentId} size={22} />
            <Text style={{ fontSize: 12, fontWeight: "500", color: paletteToken("fg_muted") }}>{item.agentId}</Text>
            <Text style={{ fontSize: 12, fontWeight: "600", fontFamily: MONO_FONT, color: paletteToken("fg") }}>
              {item.skill}
            </Text>
            <Text style={{ fontSize: 12, color: paletteToken("fg_muted"), flex: 1, minWidth: 120 }}>
              {`— ${intakeNoteOf(item.kind)}`}
            </Text>
          </View>
        ))}
      </View>
      {overview.intake.length > 8 ? (
        <Text style={{ fontSize: 12, color: paletteToken("fg_muted") }}>
          {tf("settings.intakeMore", { count: String(overview.intake.length - 8) })}
        </Text>
      ) : null}
      <View style={{ display: "flex", flexDirection: "row", alignItems: "center", gap: 6, marginTop: 2 }}>
        <SettingsButton
          testID="storage-intake-preview"
          label={state.previewingIntake() ? t("settings.intakePreviewing") : t("settings.intakePreview")}
          icon={state.previewingIntake() ? "loader" : undefined}
          spinning={state.previewingIntake()}
          variant="secondary"
          size="sm"
          busy={state.busyRepair()}
          onPress={() => state.previewAgentIntake()}
        />
        {actionable > 0 ? (
          <SettingsButton
            testID="storage-intake"
            label={state.adopting() ? t("settings.intakeApplying") : t("settings.intakeApply")}
            icon={state.adopting() ? "loader" : "wrench"}
            spinning={state.adopting()}
            variant="warn"
            size="sm"
            busy={state.busyRepair()}
            onPress={() => state.adoptAgentSkills()}
          />
        ) : null}
      </View>
    </>
  );
}

// ---- 卡 3：目录明细与维护 5 行列表 --------------------------------------------

function DirectoriesCard(props: { state: StorageState }): JSX.Element {
  const state = props.state;
  return (
    <SettingsGroup title={st("settings.group.storageDirectories")}>
      <View style={{ display: "flex", flexDirection: "column", width: "100%" }}>
        {directoriesContent(state)}
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

function directorySpecs(overview: StorageOverviewDto): DirectoryRowSpec[] {
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
    { folderId: "history", icon: "fileClock", tone: "ok", label: t("settings.storageHistory"), source: t("settings.storageSourceData"), detail: historyDetail, highlight: overview.history_count > 0, path: overview.config_path },
  ];
}

function directoriesContent(state: StorageState): JSX.Element {
  const overview = state.storage();
  if (overview === null) return <View />;
  return (
    <>
      {directorySpecs(overview).map((row) => (
        <PlainRow
          first={row.first}
          folderId={row.folderId}
          icon={row.icon}
          tone={row.tone}
          label={row.label}
          source={row.source}
          detail={row.detail}
          highlight={row.highlight}
          onOpen={() => state.reveal(row.path)}
        >
          {row.deleteTarget ? <DeleteButton state={state} target={row.deleteTarget} /> : undefined}
        </PlainRow>
      ))}
    </>
  );
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
  return (
    <View
      testID={`storage-del-${props.target}`}
      role="button"
      accessibilityState={{ busy: state.storageBusy() === props.target }}
      style={{
        height: 30, minWidth: 30, paddingHorizontal: state.storageConfirm() === props.target ? 10 : 6,
        display: "flex", flexDirection: "row", alignItems: "center", justifyContent: "center",
        borderRadius: 8, borderWidth: 1,
        borderColor: state.storageConfirm() === props.target ? paletteToken("danger_border") : "transparent",
        backgroundColor: state.storageConfirm() === props.target ? paletteToken("danger_bg") : "transparent",
        cursor: "pointer",
      }}
      hoverStyle={
        state.storageConfirm() === props.target
          ? { backgroundColor: paletteToken("danger_hover") }
          : { backgroundColor: paletteToken("panel_hover") }
      }
      activeStyle={{
        backgroundColor: state.storageConfirm() === props.target ? paletteToken("danger_bg") : paletteToken("well"),
      }}
      onPress={() => state.armDelete(props.target)}
    >
      {state.storageBusy() === props.target ? (
        <IconSpin name="loader" size={14} color={paletteToken("danger")} spin />
      ) : state.storageConfirm() === props.target ? (
        <Text style={{ fontSize: 12, fontWeight: "500", color: paletteToken("danger_fg") }}>{t("common.delete")}</Text>
      ) : (
        <Icon name="trash" size={14} color={paletteToken("fg_muted")} />
      )}
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
        paddingHorizontal: 18, paddingVertical: 14, display: "flex", flexDirection: "row", alignItems: "center", gap: 12,
        ...(props.first ? {} : { borderTopWidth: 1, borderTopColor: paletteToken("border_soft") }),
      }}
    >
      <IconBox icon={props.icon} tone={props.tone} />
      <View style={{ flex: 1, minWidth: 0, display: "flex", flexDirection: "column", gap: 3 }}>
        <View style={{ display: "flex", flexDirection: "row", alignItems: "center", gap: 8 }}>
          <Text style={{ fontSize: 14, fontWeight: "500", color: paletteToken("fg") }}>{props.label}</Text>
          {props.source !== null ? <StatusPill text={props.source} tone="neutral" /> : null}
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
