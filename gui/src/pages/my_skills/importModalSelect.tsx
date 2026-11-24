import { List, Text, View, computed, type Accessor, type JSX } from "nsh";
import { Checkbox, Input } from "nsh/base";
import { Icon } from "../../chrome/icons";
import { Tip } from "../../chrome/tooltip";
import { paletteToken } from "../../services/themeMode";
import { t, tf } from "../../services/translation";
import {
  auditPending,
  audits,
  deepScan,
  filterQuery,
  fullDepth,
  installSelected,
  refreshUpstream,
  scan,
  selectAllInstallable,
  selected,
  toggleSelectAll,
  toggleSkill,
  type SkillRow,
} from "./importModalState";
import { CountPill } from "./importModalPhases";

// 选择相 — import_modal/select.rs 对位：源行 + 计数、筛选、插件提示、审计
// 井、勾选列表、安装条（Quick Pack / 全选 / 全深度 / 全部重装 / 安装）。

/** 7 行后列表开始滚动（SELECT_VISIBLE）。滚动盒高度取视口 38%（GPUI 的
 * list_max_h = viewport.height * 0.38；nsh 无视口 API，用 380px 定值近似，
 * 分歧记录于 gaps）。 */
const SELECT_VISIBLE = 7;
const SELECT_VIEW_H = 380;

/** 审计行需要警示的结果子串（audit_needs_attention）。 */
function auditNeedsAttention(result: string): boolean {
  const lower = result.toLowerCase();
  return (
    lower.includes("critical") ||
    lower.includes("high") ||
    lower.includes("medium") ||
    lower.includes("fail")
  );
}

/** 审计井（audit_well：信息井里最多 4 行 + 余数 pill；pending 态只有
 * checkingAudits 一行，空结果整体隐藏——缺数据不是通过）。 */
function AuditWell(): JSX.Element | null {
  const pending = computed(() => auditPending() && audits().length === 0);
  const rows = computed(() => {
    const picked = selected();
    return audits()
      .filter((audit) => picked.has(audit.skill))
      .map((audit) => {
        const joined = audit.audits.map((row) => `${row.name} ${row.result}`).join(" · ");
        const attention = audit.audits.some((row) => auditNeedsAttention(row.result));
        return { attention, text: `${audit.skill}  ${joined}` };
      });
  });
  const topRows = computed(() =>
    [...rows()]
      .sort(
        (left, right) =>
          Number(right.attention) - Number(left.attention) || left.text.localeCompare(right.text),
      )
      .slice(0, 4),
  );
  return (
    <View style={{ paddingHorizontal: 24, paddingBottom: 8, flexShrink: 0 }}>
      <View
        testID="import-audits"
        style={{
          display: "flex",
          flexDirection: "row",
          alignItems: "flex-start",
          gap: 8,
          borderRadius: 8,
          backgroundColor: paletteToken("well") + "99",
          paddingHorizontal: 12,
          paddingVertical: 8,
        }}
      >
        <Icon name="info" size={14} color={paletteToken("fg_muted")} />
        <View style={{ display: "flex", flexDirection: "column", gap: 2, minWidth: 0 }}>
          <If when={pending()}>
            <Text style={{ fontSize: 12, color: paletteToken("fg_muted") }}>
              {t("githubImportModal.checkingAudits")}
            </Text>
          </If>
          <If when={!pending() && rows().length > 0}>
            <View style={{ display: "flex", flexDirection: "column", gap: 2, minWidth: 0 }}>
              <View style={{ display: "flex", flexDirection: "row", alignItems: "center", gap: 8 }}>
                <Text style={{ fontSize: 12, fontWeight: "500", color: paletteToken("fg_muted") }}>
                  {t("marketplace.detailAudits")}
                </Text>
                <If when={rows().length > 4}>
                  <CountPill label={String(rows().length)} />
                </If>
              </View>
              <Each of={topRows()} key={(row) => row.text}>
                {(row: () => { attention: boolean; text: string }) => (
                  <Text
                    style={{
                      fontSize: 12,
                      color: row().attention ? paletteToken("danger") : paletteToken("fg_muted"),
                    }}
                  >
                    {row().text}
                  </Text>
                )}
              </Each>
            </View>
          </If>
        </View>
      </View>
    </View>
  );
}

export function SelectBody(): JSX.Element {
  const current = computed(() => scan());
  const query = computed(() => filterQuery().toLowerCase());
  const filtered = computed(() => {
    const value = current();
    if (value === null) return [];
    const q = query();
    if (q.length === 0) return value.skills;
    return value.skills.filter(
      (skill) =>
        skill.id.toLowerCase().includes(q) || skill.description.toLowerCase().includes(q),
    );
  });
  // fresh 优先；全装完时回退可批量集（selectableFiltered 的回退）。
  const selectableAll = computed(() => {
    const rows = filtered();
    const fresh = rows.filter(
      (skill) => skill.installable && !skill.internal && !skill.alreadyInstalled,
    );
    if (fresh.length > 0) return fresh;
    return rows.filter((skill) => skill.installable && !skill.internal);
  });
  const allSelected = computed(() => {
    const rows = selectableAll();
    const picked = selected();
    return rows.length > 0 && rows.every((skill) => picked.has(skill.id));
  });
  const allInstalled = computed(() => {
    const value = current();
    if (value === null) return false;
    return value.skills.length > 0 && value.skills.every((skill) => skill.alreadyInstalled);
  });
  const emptyFiltered = computed(() => filtered().length === 0);
  const emptyScan = computed(() => (current()?.skills.length ?? 0) === 0);
  const scrollable = computed(() => filtered().length > SELECT_VISIBLE);
  const cachedAt = computed(() => current()?.cachedAt ?? null);
  const cacheHit = computed(() => current()?.cacheHit ?? false);
  const isPlugin = computed(() => current()?.plugin === true);
  const selectedCount = computed(() => selected().size);
  const showAudits = computed(
    () => auditPending() || audits().some((audit) => selected().has(audit.skill)),
  );

  return (
    <View testID="import-select" style={{ display: "flex", flexDirection: "column" }}>
      {/* 头部：源 + 计数 | quick pack + 全选 */}
      <View style={{ display: "flex", flexDirection: "column", gap: 12, paddingHorizontal: 24, paddingTop: 16, paddingBottom: 8, flexShrink: 0 }}>
        <View style={{ display: "flex", flexDirection: "row", alignItems: "center", justifyContent: "space-between" }}>
          <View style={{ display: "flex", flexDirection: "row", alignItems: "center", gap: 8, minWidth: 0 }}>
            <Icon name="gitBranch" size={14} color={paletteToken("fg_muted")} />
            <Text
              style={{
                fontSize: 12,
                fontWeight: "500",
                color: paletteToken("fg_muted"),
                overflow: "hidden",
                textOverflow: "ellipsis",
              }}
            >
              {current()?.short ?? ""}
            </Text>
            <CountPill label={`${current()?.skills.length ?? 0} skills`} />
          </View>
          <View style={{ display: "flex", flexDirection: "row", alignItems: "center", gap: 12, flexShrink: 0 }}>
            <If when={selectedCount() > 0}>
              <View
                testID="import-quick-pack"
                style={{
                  display: "flex",
                  flexDirection: "row",
                  alignItems: "center",
                  gap: 4,
                  paddingHorizontal: 8,
                  paddingVertical: 4,
                  borderRadius: 6,
                  cursor: "pointer",
                  backgroundColor: paletteToken("warn") + "1a",
                }}
                hoverStyle={{ backgroundColor: paletteToken("warn") + "33" }}
                onPress={() => installSelected(true)}
              >
                <Icon name="package" size={14} color={paletteToken("warn")} />
                <Text style={{ fontSize: 12, fontWeight: "500", color: paletteToken("warn") }}>
                  {t("githubImportModal.quickPack")}
                </Text>
              </View>
            </If>
            <View
              testID="import-select-all"
              style={{ cursor: "pointer" }}
              onPress={() => toggleSelectAll()}
            >
              <Text style={{ fontSize: 12, fontWeight: "500", color: paletteToken("accent") }}>
                {allSelected() ? t("common.deselectAll") : t("common.selectAll")}
              </Text>
            </View>
          </View>
        </View>
        <View style={{ marginTop: 12 }}>
          <Input
            testID="import-filter"
            value={filterQuery()}
            placeholder={t("common.search")}
            style={{
              width: "100%",
              height: 32,
              paddingHorizontal: 10,
              borderRadius: 6,
              borderWidth: 1,
              borderColor: paletteToken("border"),
              backgroundColor: paletteToken("input"),
              fontSize: 14,
              color: paletteToken("fg"),
              placeholderColor: paletteToken("fg_muted"),
            }}
            onChange={(event: { value: string }) => filterQuery.set(event.value)}
          />
        </View>
      </View>

      {/* 缓存源与上游刷新 */}
      <If when={cachedAt() !== null}>
        <View
          style={{
            paddingHorizontal: 24,
            paddingBottom: 8,
            display: "flex",
            flexDirection: "row",
            alignItems: "center",
            justifyContent: "space-between",
          }}
        >
          <Text style={{ fontSize: 12, color: paletteToken("fg_muted") }}>
            {tf(cacheHit() ? "githubImportModal.cachedSource" : "githubImportModal.fetchedSource", {
              time: cachedAt() ?? "",
            })}
          </Text>
          <View testID="import-refresh-upstream" style={{ cursor: "pointer" }} onPress={() => refreshUpstream()}>
            <Text style={{ fontSize: 12, color: paletteToken("accent") }}>
              {t("githubImportModal.refreshUpstream")}
            </Text>
          </View>
        </View>
      </If>

      {/* Claude Code 插件提示 */}
      <If when={isPlugin()}>
        <View style={{ paddingHorizontal: 24, paddingBottom: 8, flexShrink: 0 }}>
          <View
            style={{
              display: "flex",
              flexDirection: "row",
              alignItems: "center",
              gap: 8,
              borderRadius: 8,
              backgroundColor: paletteToken("well") + "99",
              paddingHorizontal: 12,
              paddingVertical: 8,
            }}
          >
            <Icon name="info" size={14} color={paletteToken("fg_muted")} />
            <Text style={{ fontSize: 12, color: paletteToken("fg_muted") }}>
              {t("githubImportModal.claudePluginHint")}
            </Text>
          </View>
        </View>
      </If>

      <If when={showAudits()}>
        <AuditWell />
      </If>

      {/* 技能行 */}
      <If when={emptyFiltered()}>
        <View style={{ paddingHorizontal: 24, paddingBottom: 8 }}>
          <Text
            testID="import-select-empty"
            style={{ paddingVertical: 32, textAlign: "center", fontSize: 12, color: paletteToken("fg_muted") }}
          >
            {t(emptyScan() ? "githubImportModal.noSkillsFound" : "common.noResults")}
          </Text>
        </View>
      </If>
      <If when={!emptyFiltered() && !scrollable()}>
        <View style={{ paddingHorizontal: 24, paddingBottom: 8, display: "flex", flexDirection: "column", gap: 2 }}>
          <Each of={filtered()} key={(skill) => skill.id}>
            {(skill: () => SkillRow) => <SkillRowView skill={skill} />}
          </Each>
        </View>
      </If>
      <If when={!emptyFiltered() && scrollable()}>
        <View style={{ paddingHorizontal: 24, paddingBottom: 8, height: SELECT_VIEW_H, width: "100%", flexShrink: 0 }}>
          <List
            testID="import-select-list"
            each={filtered()}
            keyExtractor={(skill) => skill.id}
            estimatedItemSize={54}
            style={{ width: "100%", height: "100%" }}
          >
            {(skill) => <SkillRowView skill={skill} />}
          </List>
        </View>
      </If>

      {/* 安装条 */}
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
          flexShrink: 0,
        }}
      >
        <Text style={{ fontSize: 12, color: paletteToken("fg_muted") }}>
          {tf("githubImportModal.selected", { count: selectedCount() })}
        </Text>
        <View style={{ display: "flex", flexDirection: "row", alignItems: "center", gap: 8 }}>
          <View
            testID="import-deep-scan"
            tooltip={<Tip text={t("githubImportModal.deepScanHint")} />}
            style={{
              display: "flex",
              flexDirection: "row",
              alignItems: "center",
              gap: 6,
              height: 28,
              paddingHorizontal: 12,
              borderRadius: 6,
              borderWidth: 1,
              borderColor: paletteToken("border"),
              cursor: "pointer",
            }}
            hoverStyle={{ backgroundColor: paletteToken("panel_hover") }}
            onPress={() => deepScan()}
          >
            <Icon name="layers" size={14} color={paletteToken("fg_muted")} />
            <Text style={{ fontSize: 12, color: paletteToken("fg") }}>
              {fullDepth() ? t("githubImportModal.rescanFullDepth") : t("githubImportModal.fullDepthLabel")}
            </Text>
          </View>
          <If when={allInstalled()}>
            <View
              testID="import-reinstall-all"
              style={{
                display: "flex",
                flexDirection: "row",
                alignItems: "center",
                gap: 6,
                height: 28,
                paddingHorizontal: 12,
                borderRadius: 6,
                cursor: "pointer",
              }}
              hoverStyle={{ backgroundColor: paletteToken("panel_hover") }}
              onPress={() => selectAllInstallable()}
            >
              <Icon name="rotateCcw" size={12} color={paletteToken("fg_muted")} />
              <Text style={{ fontSize: 12, color: paletteToken("fg") }}>
                {t("githubImportModal.reinstallAll")}
              </Text>
            </View>
          </If>
          <View
            testID="import-install"
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
              opacity: selectedCount() === 0 ? 0.5 : 1,
            }}
            hoverStyle={{
              backgroundColor:
                selectedCount() > 0 ? paletteToken("accent_hover") : paletteToken("accent"),
            }}
            onPress={() => {
              if (selectedCount() === 0) return;
              installSelected(false);
            }}
          >
            <Icon name="download" size={14} color={paletteToken("on_accent")} />
            <Text style={{ fontSize: 12, fontWeight: "500", color: paletteToken("on_accent") }}>
              {t("githubImportModal.install")}
            </Text>
          </View>
        </View>
      </View>
    </View>
  );
}

/** 一行技能（select.rs skill_row）：checkbox + 名称徽章 + 描述 + 悬停重装
 * 图标；整行可点（可安装时）。 */
function SkillRowView(props: { skill: Accessor<SkillRow> }): JSX.Element {
  const picked = computed(() => {
    const skill = props.skill();
    return selected().has(skill.id) && skill.installable;
  });
  const issues = computed(() =>
    props.skill().frontmatterIssues.map((issue) => t(`githubImportModal.frontmatterIssues.${issue}`)),
  );
  const blocking = computed(() => issues().length > 0 && !props.skill().installable);
  return (
    <View
      testID={`import-skill-${props.skill().folderPath}`}
      style={{
        display: "flex",
        flexDirection: "row",
        alignItems: "center",
        justifyContent: "space-between",
        paddingHorizontal: 12,
        paddingVertical: 8,
        borderRadius: 12,
        cursor: props.skill().installable ? "pointer" : "default",
        opacity: props.skill().installable ? 1 : 0.7,
        backgroundColor: picked() ? paletteToken("accent") + "0d" : "transparent",
      }}
      hoverStyle={{
        backgroundColor: props.skill().installable
          ? picked()
            ? paletteToken("accent") + "0d"
            : paletteToken("panel_hover")
          : "transparent",
      }}
      onPress={() => {
        const skill = props.skill();
        if (!skill.installable) return;
        toggleSkill(skill.id);
      }}
    >
      <View style={{ display: "flex", flexDirection: "row", alignItems: "center", gap: 12, flex: 1, minWidth: 0 }}>
        <Checkbox
          checked={picked()}
          disabled={!props.skill().installable}
          accessibilityLabel={props.skill().id}
          onChange={() => {
            // checkbox 的点击不冒泡到整行 toggle（nsh 的 press 命中模型）。
            toggleSkill(props.skill().id);
          }}
          style={{
            width: 16,
            height: 16,
            borderRadius: 4,
            borderWidth: 1,
            borderColor: picked() ? paletteToken("accent") : paletteToken("edge"),
          }}
          checkedStyle={{
            backgroundColor: paletteToken("accent"),
            borderColor: paletteToken("accent"),
          }}
        />
        <View style={{ flex: 1, minWidth: 0, paddingRight: 16 }}>
          <View style={{ display: "flex", flexDirection: "row", alignItems: "center", gap: 8 }}>
            <Text
              style={{
                fontSize: 14,
                fontWeight: "500",
                color: picked() ? paletteToken("accent") : paletteToken("fg"),
                overflow: "hidden",
                textOverflow: "ellipsis",
              }}
            >
              {props.skill().id}
            </Text>
            <If when={props.skill().internal}>
              <View
                style={{
                  paddingHorizontal: 6,
                  paddingVertical: 2,
                  borderRadius: 999,
                  backgroundColor: paletteToken("border_soft"),
                }}
              >
                <Text style={{ fontSize: 12, fontWeight: "500", color: paletteToken("fg_muted") }}>
                  {t("githubImportModal.internal")}
                </Text>
              </View>
            </If>
            <If when={props.skill().alreadyInstalled && !picked()}>
              <View
                style={{
                  paddingHorizontal: 6,
                  paddingVertical: 2,
                  borderRadius: 999,
                  backgroundColor: paletteToken("ok_bg"),
                }}
              >
                <Text style={{ fontSize: 12, fontWeight: "500", color: paletteToken("ok") }}>
                  {t("githubImportModal.installed")}
                </Text>
              </View>
            </If>
            <If when={props.skill().alreadyInstalled && picked()}>
              <View
                style={{
                  paddingHorizontal: 6,
                  paddingVertical: 2,
                  borderRadius: 999,
                  backgroundColor: paletteToken("warn_bg"),
                }}
              >
                <Text style={{ fontSize: 12, fontWeight: "500", color: paletteToken("warn") }}>
                  {t("detailPanel.reinstall")}
                </Text>
              </View>
            </If>
            <If when={issues().length > 0}>
              <View
                testID={`import-issue-${props.skill().folderPath}`}
                tooltip={<Tip text={issues().join("; ")} />}
                style={{
                  display: "flex",
                  flexDirection: "row",
                  alignItems: "center",
                  gap: 4,
                  paddingHorizontal: 6,
                  paddingVertical: 2,
                  borderRadius: 999,
                  flexShrink: 0,
                  backgroundColor: blocking() ? paletteToken("danger_bg") : paletteToken("warn_bg"),
                }}
              >
                <Icon
                  name="alertTriangle"
                  size={10}
                  color={blocking() ? paletteToken("danger") : paletteToken("warn")}
                />
                <Text
                  style={{
                    fontSize: 12,
                    fontWeight: "500",
                    color: blocking() ? paletteToken("danger") : paletteToken("warn"),
                  }}
                >
                  {t(blocking() ? "githubImportModal.metadataIssue" : "githubImportModal.compatibilityWarning")}
                </Text>
              </View>
            </If>
          </View>
          <If when={props.skill().description.length > 0}>
            <Text
              style={{
                fontSize: 12,
                color: paletteToken("fg_muted"),
                marginTop: 2,
                overflow: "hidden",
                textOverflow: "ellipsis",
              }}
            >
              {props.skill().description}
            </Text>
          </If>
        </View>
      </View>
      <If when={props.skill().alreadyInstalled}>
        <View style={{ flexShrink: 0, display: "flex", alignItems: "center" }}>
          <Icon name="rotateCcw" size={12} color={paletteToken("fg_muted")} />
        </View>
      </If>
    </View>
  );
}
