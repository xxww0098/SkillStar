// GitHub 登录对话框的无头测试装置（skillReader.test.host.tsx 样板）：假
// SignInBridge 注入 GitHubSignInHost 的注入位，装置按钮换脚本（waiting
// 事件、connected/denied/expired/error 结算、外部取消）、显示调用计数与
// 取消状态供 app.texts() 断言。真实退避序列（Pending/SlowDown 的 delay
// 数列）由 native crate 的 cargo test 用脚本化 transport 覆盖
// （my_skills_github.rs）；本装置不触文件系统、不打真网、不写 $HOME。

import { Button } from "nsh/base";
import { Text, View, host, mount, signal, type JSX } from "nsh";
import {
  GitHubSignInHost,
  cancelGitHubSignIn,
  githubAuthPolling,
  githubAuthSummary,
  openGitHubSignIn,
  type SignInBridge,
  type SignInOutcome,
  type SignInWaiting,
} from "./githubSignInSlot";
import { github } from "./state";
import type { DomainJob } from "skillstar-gui/native";

// ---- 假域件 -------------------------------------------------------------------

/** 待结算的一次 signIn 调用（装置按钮驱动事件与终态）。 */
interface PendingCall {
  job: DomainJob;
  postEvent: (event: SignInWaiting) => void;
  resolve: (outcome: SignInOutcome) => void;
  reject: (reason: string) => void;
}

const queue: PendingCall[] = [];
let calls = 0;
const callCount = signal(0);
/** 最近一次调用时 job.cancelled 的读数（按钮触发刷新）。 */
const cancelledText = signal("unknown");
/** 迟到事件是否被 post 过（stop 之后 deliver 仍可 post，落屏由 epoch 门挡）；
 * signal 驱动装置文本刷新（普通变量不触发重渲染）。 */
const lateCount = signal(0);

function waitingEvent(code: string, remainingSecs: number): SignInWaiting {
  return {
    phase: "waiting",
    userCode: code,
    verificationUri: "https://github.com/device",
    remainingSecs,
    expiresAt: new Date(Date.now() + remainingSecs * 1000).toISOString(),
  };
}

const bridge: SignInBridge = {
  signIn: (job, onEvent) =>
    new Promise<SignInOutcome>((resolve, reject) => {
      calls += 1;
      callCount.set(calls);
      queue.push({ job, postEvent: onEvent, resolve, reject });
    }),
};

// ---- 装置动作 -------------------------------------------------------------------

function deliverWaiting(code = "ABCD-1234", remainingSecs = 900): void {
  const call = queue[queue.length - 1];
  if (call === undefined) return;
  call.postEvent(waitingEvent(code, remainingSecs));
}

/** 结算最旧一笔；resolve/reject 后从队列移除。 */
function settle(action: (call: PendingCall) => void): void {
  const call = queue.shift();
  if (call === undefined) return;
  action(call);
}

function refreshCancelled(): void {
  const call = queue[queue.length - 1] ?? queue[0];
  cancelledText.set(call === undefined ? "no-call" : String(call.job.cancelled));
}

// ---- 装置动作表 ---------------------------------------------------------------

// 0.5 起 nsh test 的 `press` 会做命中测试：模态遮罩（见 githubSignInSlot 的
// position:fixed 覆盖层）之下的装置按钮会以 ERR_NSH_TEST_COVERED 拒绝。装置
// 因此把同一批动作同时挂到 host 事件通道（nsh 给测试的驱动面，见
// nsh/test 的 AppHost.send），测试用 app.host.send("harness", {action}) 驱动，
// 按钮保留给看得见的路径与同在装置内的断言。
const harnessActions: Record<string, () => void> = {
  open: () => openGitHubSignIn(),
  "deliver-waiting": () => deliverWaiting(),
  "resolve-connected": () =>
    settle((call) => call.resolve({ connected: true, login: "octocat" })),
  "resolve-denied": () => settle((call) => call.reject("settings.githubAuthDenied")),
  "resolve-error": () =>
    settle((call) =>
      call.reject("Unable to reach GitHub; check the network and proxy settings"),
    ),
  "late-waiting": () => {
    const call = queue[0];
    if (call === undefined) return;
    lateCount.set(lateCount() + 1);
    call.postEvent(waitingEvent("LATE-0000", 900));
  },
  cancelled: () => refreshCancelled(),
  reset: () => {
    cancelGitHubSignIn();
    queue.length = 0;
    calls = 0;
    callCount.set(0);
    lateCount.set(0);
    cancelledText.set("unknown");
    github.set(null);
  },
};

host.on("harness", (data) => {
  const action = (data as { action?: string } | null)?.action;
  if (action !== undefined) harnessActions[action]?.();
});

// ---- 装置 ---------------------------------------------------------------------

function App(): JSX.Element {
  return (
    <View style={{ display: "flex", flexDirection: "column", height: "100%", gap: 6 }}>
      <View style={{ display: "flex", flexDirection: "row", gap: 6, flexWrap: "wrap" }}>
        <Button testID="host-open" onPress={() => openGitHubSignIn()}>
          <Text>open</Text>
        </Button>
        <Button testID="host-deliver-waiting" onPress={() => deliverWaiting()}>
          <Text>waiting</Text>
        </Button>
        <Button
          testID="host-resolve-connected"
          onPress={() => settle((call) => call.resolve({ connected: true, login: "octocat" }))}
        >
          <Text>connected</Text>
        </Button>
        <Button
          testID="host-resolve-denied"
          onPress={() => settle((call) => call.reject("settings.githubAuthDenied"))}
        >
          <Text>denied</Text>
        </Button>
        <Button
          testID="host-resolve-error"
          onPress={() =>
            settle((call) =>
              call.reject("Unable to reach GitHub; check the network and proxy settings"),
            )
          }
        >
          <Text>error</Text>
        </Button>
        <Button
          testID="host-late-waiting"
          onPress={() => {
            const call = queue[0];
            if (call === undefined) return;
            lateCount.set(lateCount() + 1);
            call.postEvent(waitingEvent("LATE-0000", 900));
          }}
        >
          <Text>late</Text>
        </Button>
        <Button testID="host-cancelled" onPress={refreshCancelled}>
          <Text>cancelled</Text>
        </Button>
        <Button
          testID="host-reset"
          onPress={() => {
            queue.length = 0;
            calls = 0;
            callCount.set(0);
            lateCount.set(0);
            cancelledText.set("unknown");
            github.set(null);
          }}
        >
          <Text>reset</Text>
        </Button>
      </View>
      <Text testID="host-counters" style={{ fontSize: 12 }}>
        {`calls:${callCount()} cancelled:${cancelledText()} polling:${githubAuthPolling()} late:${lateCount()} session:${
          (() => {
            const session = github();
            const sum = githubAuthSummary();
            return session === null ? `none:${sum.polling}` : `${session.state}@${session.login ?? ""}`;
          })()
        }`}
      </Text>
      <View style={{ flex: 1, minHeight: 0 }}>
        <GitHubSignInHost bridge={bridge} />
      </View>
    </View>
  );
}

mount(App);
