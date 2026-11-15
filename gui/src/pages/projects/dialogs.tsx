// 项目弹窗 — crates/ss-gpui/src/projects/dialogs.rs 对位。共享 ConfirmCard
// （chrome::open_confirm/open_form_dialog 的硬规矩）：路径弹窗走 fields
// 挂输入框，移除走破坏性确认；一次提交的 busy/error 留在弹窗自己的信号
// 律）。提交在跑时页面 busy=true：注册按钮、行内开关、看护按钮全部静默
// 拒绝（dialogs.rs run_mutation 的页面半边）。成功经 token 关卡（对位
// close_if_current：用户已关掉的弹窗不被晚到的成功再关，也不误关后来打
// 开的新弹窗）；失败保留弹窗与输入，输入框下方给出原因。
//
// 注：GPUI 的「选择目录…」按钮打开系统目录选择器；nsh 当前没有系统选
// 目录 API（.nsh/types 只有 fs 读写），路径输入框即提交通道——能力到齐
// 再补按钮，不造半吊子替身。

import { signal, type Signal } from "nsh";
import { Text, View, type JSX } from "nsh";
import { Input } from "nsh/base";
import { closeConfirm, openConfirm } from "../../chrome/dialog";
import { paletteToken } from "../../services/themeMode";
import { t, tf } from "../../services/translation";
import type { CopyRecord, ProjectEntry } from "./bridge";
import type { ProjectsState } from "./state";

/** 一次路径弹窗的全部本地状态（DialogState 的 TS 半边）。 */
interface PathDialogState {
  input: Signal<string>;
  error: Signal<string | null>;
  running: Signal<boolean>;
}

interface MutationDialogState {
  error: Signal<string | null>;
  running: Signal<boolean>;
}

/** dialogs.rs render_feedback：busy 行 + 错误行（错误白话保留换行）。 */
function DialogFeedback(props: {
  local: MutationDialogState;
  errorTestId: string;
}): JSX.Element {
  return (
    <>
      <If when={props.local.running()}>
        <Text style={{ fontSize: 12, color: paletteToken("fg_muted") }}>
          {t("projects.operationRunning")}
        </Text>
      </If>
      <If when={props.local.error()}>
        <Text
          testID={props.errorTestId}
          style={{ fontSize: 12, color: paletteToken("danger") }}
        >
          {props.local.error()}
        </Text>
      </If>
    </>
  );
}

function PathDialogBody(props: {
  local: PathDialogState;
  promptKey: string;
  errorTestId: string;
}): JSX.Element {
  return (
    <View style={{ display: "flex", flexDirection: "column", gap: 8, width: "100%" }}>
      <Text style={{ fontSize: 12, color: paletteToken("fg") }}>
        {t(props.promptKey)}
      </Text>
      <Input
        testID={props.errorTestId === "projects-register-error" ? "projects-path-input" : "project-change-path-input"}
        value={props.local.input()}
        placeholder={t(props.promptKey)}
        disabled={props.local.running()}
        onChange={(e: { value: string }) => props.local.input.set(e.value)}
        style={{
          fontSize: 12,
          color: paletteToken("fg"),
          padding: 8,
          backgroundColor: paletteToken("input"),
          borderWidth: 1,
          borderColor: paletteToken("border"),
          borderRadius: 6,
        }}
      />
      <DialogFeedback local={props.local} errorTestId={props.errorTestId} />
    </View>
  );
}

/** dialogs.rs open_path：注册项目与更改路径共用一块路径表单。 */
function openPathDialog(
  state: ProjectsState,
  kind: "register" | "change",
  entry: ProjectEntry | null
): void {
  if (state.mutationBlocked()) return; // dialogs.rs mutation_blocked
  const local: PathDialogState = {
    input: signal(kind === "change" ? (entry?.path ?? "") : ""),
    error: signal<string | null>(null),
    running: signal(false),
  };
  const token = `projects-path-${kind}-${Date.now()}-${Math.random()}`;
  const isRegister = kind === "register";
  const promptKey = isRegister ? "projects.chooseDir" : "projects.chooseNewDir";
  const errorTestId = isRegister ? "projects-register-error" : "project-change-path-error";

  openConfirm({
    token,
    title: t(isRegister ? "projects.registerDialogTitle" : "projects.changePathTitle"),
    okText: t(isRegister ? "projects.registerProject" : "common.update"),
    fields: () => (
      <PathDialogBody local={local} promptKey={promptKey} errorTestId={errorTestId} />
    ),
    // 提交与成功关卡都由弹窗自理：onOk 永远返回 false（成功经 token 关）。
    onOk: () => {
      if (local.running() || state.mutationBlocked()) return false;
      const path = local.input().trim();
      if (path === "") {
        local.error.set(t("projects.registerEmptyPath"));
        return false;
      }
      local.running.set(true);
      local.error.set(null);
      void (isRegister
        ? state.commitRegister(path)
        : state.commitUpdatePath(entry?.name ?? "", path)
      ).then((result) => {
        local.running.set(false);
        if (result.ok) {
          closeConfirm(token); // 成功才关（close_if_current）
          return;
        }
        local.error.set(result.error); // 失败：保留输入与弹窗，给出原因
      });
      return false;
    },
  });
}

/** 注册项目（工具栏「注册项目」，dialogs.rs open_register）。 */
export function openRegisterDialog(state: ProjectsState): void {
  openPathDialog(state, "register", null);
}

/** 更改路径（详情头「更改路径」，dialogs.rs open_change_path）。 */
export function openChangePathDialog(state: ProjectsState, entry: ProjectEntry): void {
  openPathDialog(state, "change", entry);
}

/** 移除项目（详情头「移除项目」，dialogs.rs open_remove）：破坏性确认。 */
export function openRemoveDialog(state: ProjectsState, name: string): void {
  if (state.mutationBlocked()) return;
  const local: MutationDialogState = { error: signal<string | null>(null), running: signal(false) };
  const token = `projects-remove-${Date.now()}-${Math.random()}`;
  openConfirm({
    token,
    title: tf("projects.removeConfirmTitle", { name }),
    body: t("projects.unregisterHint"),
    okText: t("projects.removeProject"),
    danger: true,
    fields: () => <DialogFeedback local={local} errorTestId="project-remove-error" />,
    onOk: () => {
      if (local.running() || state.mutationBlocked()) return false;
      local.running.set(true);
      local.error.set(null);
      void state.commitRemove(name).then((result) => {
        local.running.set(false);
        if (result.ok) {
          closeConfirm(token);
          return;
        }
        local.error.set(result.error);
      });
      return false;
    },
  });
}

/** 清理记录确认（upkeep.rs 行内 open_confirm）：只清元数据，文件保留；
 * 确认即关卡，域结果走页脚状态行（GPUI on_ok 返回 true 直接关）。 */
export function openForgetCopyConfirm(state: ProjectsState, record: CopyRecord): void {
  const rel = record.relativePath;
  const skill = record.skillName;
  openConfirm({
    title: tf("projects.forgetCopyTitle", { name: skill }),
    body: tf("projects.forgetCopyBody", { path: `${rel}/${skill}` }),
    okText: t("projects.forgetCopyConfirm"),
    danger: true,
    onOk: () => {
      state.forgetCopy(record.projectName, rel, skill);
    },
  });
}
