import { List, Scroll, Text, View, effect, signal, type JSX } from "nsh";
import { Button, Checkbox, Input } from "nsh/base";
import { SkillCardsCreateGroup } from "skillstar-gui/native";
import { SHEET_SHADOW, promptVeil } from "../../chrome/dialog";
import { IconSpin } from "../../chrome/feedback";
import { Icon } from "../../chrome/icons";
import { groupsChanged } from "../../shell/events";
import { t, tf } from "../../services/translation";
import { isLight, paletteToken } from "../../services/themeMode";
import {
  EMOJI_OPTIONS,
  PILLS_VIEW_H,
  PILLS_VISIBLE,
  PICKER_VIEW_H,
  PICKER_VISIBLE,
  allMarked,
  canCreate,
  commitMembers,
  filterRows,
  isDuplicateName,
  sortedPillMembers,
  toggleAll,
  toggleMember,
  type DeckDraft,
  type DeckSkillRow,
} from "./createGroup.logic";

// 新建/编辑卡组对话框（crates/ss-gpui/src/skill_cards/create_group.rs 对位）：
// emoji + 名称 + 描述、已选成员 pill 条、筛选 + checkbox 列表（超过阈值
// 才出现的固定像素高度滚动盒）、取消/创建栏。两处挂载共用这一个实体：
// 卡组页直开走 openCreateGroup/CreateGroupDialogHost（遮罩 + 居中 512px 卡
// + 标题行，open_create_group/open_editor 对位）；导入对话框的 Quick Pack
// 步直接渲染 <CreateGroupDialog>（open editor 的量高放正中由 embedder 的
// 弹层承担，标题行也由 embedder 画）。

/** 域桥：默认真实 facade，无头测试注入假实现（隔离 realm，不走真盘）。 */
export interface CreateGroupBridge {
  installedSkillRows(): Promise<DeckSkillRow[]>;
  existingDeckNames(excludeId?: string | null): Promise<string[]>;
  commitDeck(
    editingId: string | null,
    name: string,
    description: string,
    icon: string,
    members: string[]
  ): Promise<void>;
}

const nativeBridge: CreateGroupBridge = {
  installedSkillRows: () => SkillCardsCreateGroup.installedSkillRows(),
  existingDeckNames: (excludeId) => SkillCardsCreateGroup.existingDeckNames(excludeId ?? null),
  commitDeck: (editingId, name, description, icon, members) =>
    SkillCardsCreateGroup.commitDeck(editingId, name, description, icon, members),
};

/** 对话框内容列的宽度（open_editor 的 w(512)）。 */
export const CREATE_GROUP_W = 512;
/** 成员行高（虚拟化列表的确定行高）。 */
const ROW_H = 32;

export interface CreateGroupDialogProps {
  /** 预填名称（Quick Pack 用扫描来源的仓库名，deck_import.rs Phase::Pack）。 */
  seed?: string;
  /** 预选成员（Quick Pack 传刚装好的技能）。 */
  initialMembers?: string[];
  /** 编辑态：预填图标/名称/描述/成员，提交走 update。 */
  editing?: DeckDraft;
  /** 提交成功（on_created 对位；刷新由调用方负责，页面直开形态额外发 groupsChanged）。 */
  onCreated?: () => void;
  /** 请求关闭所在弹层（取消按钮、成功后的自动关闭）。 */
  onClose: () => void;
  /** busy 翻转上报（弹层 chrome 据 `overlay_closable(!busy)` 拦截误关）。 */
  onBusyChange?: (busy: boolean) => void;
  /** 域桥（测试注入）。 */
  bridge?: CreateGroupBridge;
}

/**
 * 表单体本体（render_body 对位）：icon 行 + 重名提示 + 描述 + pill 条 +
 * 筛选行 + 成员列表 + 错误行 + 取消/创建栏 + emoji 浮层。嵌入方（Quick
 * Pack）直接渲染它并画自己的标题行。
 */
export function CreateGroupDialog(props: CreateGroupDialogProps): JSX.Element {
  const bridge = props.bridge ?? nativeBridge;
  const editing = props.editing ?? null;
  const name = signal(editing?.name ?? props.seed ?? "");
  const desc = signal(editing?.description ?? "");
  const filter = signal("");
  const icon = signal(editing?.icon ?? "💻");
  const emojiOpen = signal(false);
  const members = signal<string[]>(editing?.skills ?? props.initialMembers ?? []);
  const all = signal<DeckSkillRow[]>([]);
  const existingNames = signal<string[]>([]);
  const busy = signal(false);
  const error = signal<string | null>(null);
  // busy 上报：弹层遮罩据此在提交期间拦截误关（overlay_closable(!busy)）。
  effect(() => props.onBusyChange?.(busy()));

  // 打开即后台加载成员候选与重名表（CreateGroupDialog::new 的 spawn_domain
  // 对位）。失败静默为空列表，与 GPUI 的 unwrap_or_default 一致——读不到
  // hub 不阻塞卡组编辑。
  bridge
    .installedSkillRows()
    .then((rows) => all.set(rows))
    .catch(() => all.set([]));
  bridge
    .existingDeckNames(editing?.id ?? null)
    .then((names) => existingNames.set(names))
    .catch(() => existingNames.set([]));

  const duplicate = () => isDuplicateName(name(), existingNames());
  const filtered = () => filterRows(all(), filter());
  const createEnabled = () => canCreate(name(), members(), duplicate(), busy());
  const installedSet = () => new Set(all().map((row) => row.name));

  const toggle = (skill: string) => {
    if (busy()) return;
    members.set(toggleMember(members(), skill));
  };

  const pickEmoji = (emoji: string) => {
    icon.set(emoji);
    emojiOpen.set(false);
  };

  const create = () => {
    if (!createEnabled()) return;
    busy.set(true);
    error.set(null);
    emojiOpen.set(false);
    const trimmedName = name().trim();
    const trimmedDesc = desc().trim();
    const sorted = commitMembers(members());
    bridge
      .commitDeck(editing?.id ?? null, trimmedName, trimmedDesc, icon(), sorted)
      .then(() => {
        busy.set(false);
        props.onCreated?.();
        props.onClose();
      })
      .catch((reason: unknown) => {
        // 失败保留输入并显示原因（create_group.rs 的 this.error）。
        busy.set(false);
        error.set(reason instanceof Error ? reason.message : String(reason));
      });
  };

  return (
    <View style={{ position: "relative", display: "flex", flexDirection: "column" }}>
      {emojiOpen() ? (
        // emoji 网格打开时铺满内容列的一层透明幕：点外收起网格
        // （on_action Cancel 对位的可点击部分）。
        <View
          style={{ position: "absolute", top: 0, left: 0, right: 0, bottom: 0, zIndex: 5 }}
          onPress={() => emojiOpen.set(false)}
        />
      ) : null}

      <View style={{ display: "flex", flexDirection: "column", padding: 16, paddingHorizontal: 24, gap: 12 }}>
        {/* ── Icon 按钮 + 名称 ─────────────────────────────────── */}
        <View style={{ display: "flex", flexDirection: "column", gap: 6 }}>
          <View style={{ display: "flex", flexDirection: "row", alignItems: "flex-end", gap: 12 }}>
            <Button
              testID="pack-icon"
              disabled={busy()}
              accessibilityLabel={t("createGroupModal.icon")}
              onPress={() => {
                if (!busy()) emojiOpen.set(!emojiOpen());
              }}
              style={{
                width: 44,
                height: 44,
                display: "flex",
                alignItems: "center",
                justifyContent: "center",
                borderRadius: 12,
                borderWidth: 1,
                borderColor: emojiOpen()
                  ? paletteToken("accent")
                  : duplicate()
                    ? paletteToken("danger")
                    : paletteToken("border"),
                backgroundColor: emojiOpen() ? paletteToken("accent") + "0d" : "transparent",
                flexShrink: 0,
              }}
              hoverStyle={{ backgroundColor: paletteToken("panel_hover") }}
            >
              <Text style={{ fontSize: 20 }}>{icon()}</Text>
            </Button>
            <DialogInput
              testID="pack-name"
              value={name()}
              placeholder={t("createGroupModal.groupName")}
              busy={busy()}
              autoFocus
              onChange={(next) => name.set(next)}
              onSubmit={create}
            />
          </View>
          {duplicate() ? (
            <View
              style={{
                display: "flex",
                flexDirection: "row",
                alignItems: "center",
                gap: 6,
                paddingLeft: 56,
              }}
            >
              <Icon name="circleAlert" size={14} color={paletteToken("danger")} />
              <Text
                style={{ fontSize: 12, fontWeight: "500", color: paletteToken("danger") }}
              >
                {t("createGroupModal.nameExists")}
              </Text>
            </View>
          ) : null}
        </View>

        <DialogInput
          testID="pack-desc"
          value={desc()}
          placeholder={t("createGroupModal.description")}
          busy={busy()}
          onChange={(next) => desc.set(next)}
          onSubmit={create}
        />

        {/* ── 已选成员 pill 条 ─────────────────────────────────── */}
        {members().length > 0 ? (
          <PillStrip
            members={members}
            installedSet={installedSet}
            busy={busy}
            onRemove={(member) => {
              if (!busy()) members.set(members().filter((m) => m !== member));
            }}
          />
        ) : null}

        {/* ── 搜索 + 全选 ──────────────────────────────────────── */}
        <View style={{ display: "flex", flexDirection: "row", alignItems: "center", gap: 8 }}>
          <View style={{ flex: 1, minWidth: 0, display: "flex", flexDirection: "row", alignItems: "center", gap: 4 }}>
            <View style={{ paddingLeft: 4, display: "flex" }}>
              <Icon name="search" size={14} color={paletteToken("fg_muted")} />
            </View>
            <DialogInput
              testID="pack-filter"
              value={filter()}
              placeholder={t("createGroupModal.searchSkills")}
              busy={busy()}
              onChange={(next) => filter.set(next)}
            />
          </View>
          {filtered().length > 0 ? (
            <View
              testID="pack-select-all"
              style={{
                height: 32,
                paddingHorizontal: 12,
                display: "flex",
                alignItems: "center",
                justifyContent: "center",
                borderRadius: 6,
                backgroundColor: paletteToken("well"),
                cursor: "pointer",
                flexShrink: 0,
              }}
              hoverStyle={{ backgroundColor: paletteToken("panel_hover") }}
              onPress={() => {
                if (busy()) return;
                const rows = filtered();
                members.set(toggleAll(rows, members(), allMarked(rows, members())));
              }}
            >
              <Text style={{ fontSize: 12, fontWeight: "500", color: paletteToken("fg") }}>
                {allMarked(filtered(), members())
                  ? t("common.deselectAll")
                  : t("common.selectAll")}
              </Text>
            </View>
          ) : null}
        </View>

        {/* ── 成员列表 ─────────────────────────────────────────── */}
        <MemberPicker
          all={all}
          filter={filter}
          members={members}
          busy={busy}
          onToggle={toggle}
        />

        {error() ? (
          <Text testID="pack-error" style={{ fontSize: 14, color: paletteToken("danger") }}>
            {error()}
          </Text>
        ) : null}
      </View>

      {/* ── 取消/创建栏 ───────────────────────────────────────── */}
      <View
        style={{
          display: "flex",
          flexDirection: "row",
          alignItems: "center",
          justifyContent: "flex-end",
          gap: 8,
          paddingHorizontal: 24,
          paddingVertical: 14,
          borderTopWidth: 1,
          borderTopColor: paletteToken("border_soft"),
        }}
      >
        <Button
          testID="pack-cancel"
          disabled={busy()}
          onPress={() => {
            if (!busy()) props.onClose();
          }}
          style={{
            height: 32,
            paddingHorizontal: 16,
            display: "flex",
            alignItems: "center",
            justifyContent: "center",
            borderRadius: 6,
          }}
          hoverStyle={{ backgroundColor: paletteToken("panel_hover") }}
        >
          <Text style={{ fontSize: 12, color: paletteToken("fg") }}>
            {t("createGroupModal.cancel")}
          </Text>
        </Button>
        <Button
          testID="pack-create"
          disabled={!createEnabled()}
          onPress={create}
          style={{
            height: 32,
            minWidth: 88,
            paddingHorizontal: 16,
            display: "flex",
            flexDirection: "row",
            alignItems: "center",
            justifyContent: "center",
            gap: 6,
            borderRadius: 6,
            backgroundColor: paletteToken("accent"),
          }}
          hoverStyle={{ backgroundColor: paletteToken("accent_hover") }}
          disabledStyle={{ backgroundColor: paletteToken("well") }}
        >
          {busy() ? (
            <IconSpin name="refresh" size={12} color={paletteToken("on_accent")} spin />
          ) : null}
          <Text
            style={{
              fontSize: 12,
              color: createEnabled() ? paletteToken("on_accent") : paletteToken("fg_muted"),
            }}
          >
            {t(editing ? "createGroupModal.save" : "createGroupModal.create")}
          </Text>
        </Button>
      </View>

      {/* ── emoji 浮层（浮在名称输入上方）───────────────────── */}
      {emojiOpen() ? (
        <View
          style={{
            position: "absolute",
            // body py16 + 44px icon 按钮 + 8px 行距，对位 React 的 top-full mt-2。
            top: 64,
            left: 24,
            zIndex: 6,
          }}
        >
          <View
            style={{
              width: 196,
              display: "flex",
              flexDirection: "row",
              flexWrap: "wrap",
              gap: 2,
              padding: 8,
              borderRadius: 12,
              borderWidth: 1,
              borderColor: paletteToken("border"),
              backgroundColor: paletteToken("card"),
              boxShadow: SHEET_SHADOW,
            }}
          >
            {EMOJI_OPTIONS.map((emoji, ix) => (
              <Button
                testID={`pack-emoji-${ix}`}
                selected={icon() === emoji}
                onPress={() => pickEmoji(emoji)}
                style={{
                  width: 36,
                  height: 36,
                  display: "flex",
                  alignItems: "center",
                  justifyContent: "center",
                  borderRadius: 8,
                  backgroundColor: icon() === emoji ? paletteToken("accent") + "1a" : "transparent",
                }}
                hoverStyle={{ backgroundColor: paletteToken("panel_hover") }}
              >
                <Text style={{ fontSize: 18 }}>{emoji}</Text>
              </Button>
            ))}
          </View>
        </View>
      ) : null}
    </View>
  );
}

/** kit Input 的对话框形态：受控 + busy 时只读变灰。Enter 提交。 */
function DialogInput(props: {
  testID: string;
  value: string;
  placeholder: string;
  busy: boolean;
  autoFocus?: boolean;
  onChange: (next: string) => void;
  onSubmit?: () => void;
}): JSX.Element {
  return (
    <Input
      testID={props.testID}
      value={props.value}
      placeholder={props.placeholder}
      readonly={props.busy}
      autoFocus={props.autoFocus}
      submitBehavior="submit"
      style={{
        flex: 1,
        minWidth: 0,
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
      disabledStyle={{ opacity: 0.5 }}
      onChange={(event) => props.onChange(event.value)}
      onSubmit={() => props.onSubmit?.()}
    />
  );
}

/** 已选成员 pill 条：已安装在前、orphan（删除线 + danger）垫后；超过阈值才滚动。 */
function PillStrip(props: {
  members: () => string[];
  installedSet: () => ReadonlySet<string>;
  busy: () => boolean;
  onRemove: (name: string) => void;
}): JSX.Element {
  const pills = () => sortedPillMembers(props.members(), props.installedSet());
  const rows = () => (
    <View style={{ display: "flex", flexDirection: "row", flexWrap: "wrap", gap: 6, paddingRight: 4 }}>
      {pills().map((member) => {
        const orphan = !props.installedSet().has(member);
        const tone = orphan ? paletteToken("danger") : paletteToken("accent");
        return (
          <Button
            testID={`pack-pill-${member}`}
            disabled={props.busy()}
            accessibilityLabel={tf("createGroupModal.removeMember", { name: member })}
            onPress={() => props.onRemove(member)}
            style={{
              height: 24,
              display: "flex",
              flexDirection: "row",
              alignItems: "center",
              gap: 4,
              paddingHorizontal: 8,
              borderRadius: 6,
              backgroundColor: tone + "1a",
            }}
            hoverStyle={{ backgroundColor: tone + "33" }}
          >
            <Text
              style={{
                fontSize: 12,
                fontWeight: "500",
                color: tone,
                textDecorationLine: orphan ? "line-through" : "none",
              }}
            >
              {member}
            </Text>
            <Icon name="x" size={12} color={tone} />
          </Button>
        );
      })}
    </View>
  );
  // 分支读 members() 只在 JSX 表达式里（挂载后的数量变化也要切分支）。
  return (
    <View>
      {props.members().length > PILLS_VISIBLE ? (
        // 明确像素高度视口框，不给滚动包装写 max_h（滚动盒契约）。
        <View testID="pack-pills-viewport" style={{ height: PILLS_VIEW_H, width: "100%", flexShrink: 0 }}>
          <Scroll testID="pack-pills" style={{ width: "100%", height: "100%" }}>
            {rows()}
          </Scroll>
        </View>
      ) : (
        rows()
      )}
    </View>
  );
}

/**
 * 筛选后的成员选择列表：候选为空 → 「还没有安装技能」；筛后为空 → 「未找
 * 到技能」；行数超过阈值 → 固定 160px 视口的虚拟化列表；否则自然高度。
 * 行点击与 checkbox 都 toggle；checkbox 点击不冒泡到行（由 nsh 的 press
 * 命中模型保证，测试覆盖）。
 */
function MemberPicker(props: {
  all: () => DeckSkillRow[];
  filter: () => string;
  members: () => string[];
  busy: () => boolean;
  onToggle: (skill: string) => void;
}): JSX.Element {
  const rows = () => filterRows(props.all(), props.filter());

  const renderRow = (name: () => string) => {
    const picked = () => props.members().includes(name());
    return (
      <View
        testID={`pack-row-${name()}`}
        style={{
          height: ROW_H,
          display: "flex",
          flexDirection: "row",
          alignItems: "center",
          gap: 10,
          paddingHorizontal: 10,
          borderRadius: 8,
          backgroundColor: picked() ? paletteToken("accent") + "0d" : "transparent",
          cursor: "pointer",
        }}
        hoverStyle={{ backgroundColor: picked() ? paletteToken("accent") + "1a" : paletteToken("panel_hover") }}
        onPress={() => props.onToggle(name())}
      >
        <Checkbox
          checked={picked()}
          disabled={props.busy()}
          accessibilityLabel={name()}
          onChange={(event) => {
            if (!props.busy()) props.onToggle(name());
          }}
          style={{ width: 16, height: 16, borderRadius: 4, borderWidth: 1, borderColor: picked() ? paletteToken("accent") : paletteToken("edge") }}
          checkedStyle={{ backgroundColor: paletteToken("accent"), borderColor: paletteToken("accent") }}
          indicator={
            <View style={{ display: "flex", alignItems: "center", justifyContent: "center" }}>
              <Icon name="check" size={12} color={paletteToken("on_accent")} />
            </View>
          }
        />
        <Text
          style={{
            flex: 1,
            minWidth: 0,
            fontSize: 14,
            fontWeight: picked() ? "500" : "normal",
            color: picked() ? paletteToken("accent") : paletteToken("fg"),
            textOverflow: "ellipsis",
            numberOfLines: 1,
          }}
        >
          {name()}
        </Text>
      </View>
    );
  };

  // 三分支都读 signal，全部放 JSX 表达式里：空候选、筛后为空、自然高度、
  // 固定视口虚拟化（滚动盒契约：> PICKER_VISIBLE 行才出现 PICKER_VIEW_H 视口）。
  return (
    <View>
      {props.all().length === 0 ? (
        <EmptyHint text={t("skillCards.noSkillsInstalled")} />
      ) : rows().length === 0 ? (
        <EmptyHint text={t("createGroupModal.noSkillsFound")} />
      ) : rows().length > PICKER_VISIBLE ? (
        <View testID="pack-picker-viewport" style={{ height: PICKER_VIEW_H, width: "100%", flexShrink: 0 }}>
          <List
            testID="pack-picker"
            each={rows()}
            keyExtractor={(row) => row.name}
            estimatedItemSize={ROW_H}
            style={{ width: "100%", height: "100%", borderRadius: 8 }}
            contentClass="py-1"
          >
            {(item) => renderRow(() => item().name)}
          </List>
        </View>
      ) : (
        <View style={{ display: "flex", flexDirection: "column", gap: 2, borderRadius: 8 }}>
          {rows().map((row) => renderRow(() => row.name))}
        </View>
      )}
    </View>
  );
}

function EmptyHint(props: { text: string }): JSX.Element {
  return (
    <View style={{ paddingVertical: 24, display: "flex", alignItems: "center" }}>
      <Text style={{ fontSize: 14, color: paletteToken("fg_muted") }}>{props.text}</Text>
    </View>
  );
}

// ── 页面直开形态（open_create_group/open_editor 对位）───────────────────

interface GroupEditorEntry {
  seq: number;
  editing?: DeckDraft;
  seed?: string;
  initialMembers?: string[];
  onCreated?: () => void;
  bridge?: CreateGroupBridge;
}

const entries = signal<GroupEditorEntry[]>([]);
let nextSeq = 1;

/**
 * 打开新建卡组对话框（卡组页「新建卡组…」/空状态 CTA）。创建成功后发
 * groupsChanged——卡组页刷新挂该频道；`onCreated` 额外回调调用方。
 */
export function openCreateGroup(options: {
  seed?: string;
  initialMembers?: string[];
  onCreated?: () => void;
  bridge?: CreateGroupBridge;
} = {}): void {
  entries.set([
    ...entries(),
    { ...options, seq: nextSeq++ },
  ]);
}

/** 打开编辑卡组对话框（卡组详情列「编辑卡组…」复用同一表单）。 */
export function openEditGroup(
  group: DeckDraft,
  options: { onSaved?: () => void; bridge?: CreateGroupBridge } = {}
): void {
  entries.set([
    ...entries(),
    { editing: group, onCreated: options.onSaved, bridge: options.bridge, seq: nextSeq++ },
  ]);
}

/**
 * 挂在壳根（与 DialogHost 同层）：自带按钮体系的居中对话框由
 * chrome/dialog.rs 量高后放窗口正中——nsh 无量高 API，卡内容自适应、
 * 列表滚动盒限制高度，遮罩读 promptVeil，成功后自动关闭并发 groupsChanged
 * （Quick Pack 对位；卡组页自身刷新挂同一频道）。
 *
 * 响应式规则：signal 的读取只放 JSX 表达式或回调里——组件函数体只在
 * 挂载时执行一次（nsh/solid 细粒度绑定），顶层读取不会随 signal 重算。
 */
export function CreateGroupDialogHost(): JSX.Element {
  return (
    <If when={entries().length > 0}>
      <GroupEditorCard entry={() => entries()[entries().length - 1]} />
    </If>
  );
}

function GroupEditorCard(props: { entry: () => GroupEditorEntry }): JSX.Element {
  const entry = props.entry;
  // 提交期间拦截遮罩误关（open_editor 的 overlay_closable(!busy)）；成功后
  // 的自动关闭走 force（busy effect 的翻转可能晚于回调链）。
  let submitting = false;
  const close = (force = false) => {
    if (force || !submitting) entries.set(entries().filter((e) => e.seq !== entry().seq));
  };
  return (
    <View
      testID="group-editor-veil"
      style={{
        position: "absolute",
        top: 0,
        left: 0,
        right: 0,
        bottom: 0,
        display: "flex",
        alignItems: "center",
        justifyContent: "center",
        zIndex: 60,
      }}
      onPress={() => close()}
    >
      <View style={{ position: "absolute", top: 0, left: 0, right: 0, bottom: 0, backgroundColor: promptVeil() }} />
      <View
        style={{
          width: CREATE_GROUP_W,
          maxHeight: "85%",
          display: "flex",
          flexDirection: "column",
          borderRadius: 12,
          borderWidth: 1,
          borderColor: paletteToken("border"),
          // modal-surface：亮色用 card、暗色用 panel（open_editor）。
          backgroundColor: isLight() ? paletteToken("card") : paletteToken("panel"),
          boxShadow: SHEET_SHADOW,
        }}
        onPress={() => {
          // 卡片吞掉按压，点遮罩才关。
        }}
      >
        {/* 标题行（header 对位）：无图标井，border-b。 */}
        <View
          style={{
            display: "flex",
            flexDirection: "row",
            alignItems: "center",
            justifyContent: "space-between",
            paddingHorizontal: 24,
            paddingTop: 16,
            paddingBottom: 12,
            flexShrink: 0,
            borderBottomWidth: 1,
            borderBottomColor: paletteToken("border_soft"),
          }}
        >
          <Text style={{ fontSize: 16, fontWeight: "600", color: paletteToken("fg") }}>
            {t(entry().editing ? "createGroupModal.editGroup" : "createGroupModal.newGroup")}
          </Text>
        </View>
        <CreateGroupDialog
          seed={entry().seed}
          initialMembers={entry().initialMembers}
          editing={entry().editing}
          bridge={entry().bridge}
          onBusyChange={(busy) => {
            submitting = busy;
          }}
          onCreated={() => {
            entry().onCreated?.();
            // Quick Pack 的 GroupsChanged 对位：卡组页刷新挂该频道。
            groupsChanged.publish();
          }}
          onClose={() => close(true)}
        />
      </View>
    </View>
  );
}
