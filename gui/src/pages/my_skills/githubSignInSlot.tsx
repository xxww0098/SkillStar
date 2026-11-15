import { Clipboard, Overlay, Text, View, effect, openUrl, signal, untracked, type JSX, type ViewStyle } from "nsh";
import { Icon } from "../../chrome/icons";
import { IconSpin } from "../../chrome/feedback";
import { promptVeil, SHEET_SHADOW } from "../../chrome/dialog";
import { fadeTransition } from "../../chrome/motion";
import { isLight, paletteToken } from "../../services/themeMode";
import { t, tf } from "../../services/translation";
import { startJob, type JobRun } from "../../services/domainJob";
import { DomainJob, MySkillsGitHub } from "skillstar-gui/native";
import { github } from "./state";

// GitHub 设备授权登录对话框 — my_skills/github_sign_in.rs 对位。波 4 异步
// 桥基座预注册的落点：登录切片的组件体（本文件替换 GitHubSignInBody）。
//
// 【桥规矩（bridge_task.rs + services/domainJob.ts + my_skills_github.rs）】
// 1. 轮询是「一个 async 任务内的循环」，不是回调链：`MySkillsGitHub
//    .signIn` 的 Rust 任务内 `timer(retry_after_seconds)` →
//    `poll_device_flow()` → DeviceFlowPoll 分派（Pending 与 SlowDown 同臂，
//    退避秒数来自域层，github_sign_in.rs:141-179 对位）。
// 2. 每次登录一个 DomainJob（startJob 绑定）：waiting 阶段（userCode/
//    verificationUri/remainingSecs/expiresAt）经进度流一次性 post；关闭
//    对话框（Escape/遮罩）一律 run.stop() —— Rust 侧 cancel_device_flow
//    () + epoch 门双保险（GPUI 的 Drop → cancel 对位，github_sign_in.rs
//    :110-113/:416-420）。
// 3. Denied/Expired 以 locale key reject（settings.githubAuthDenied/
//    githubAuthExpired），域错误以英文消息 reject：两者都过一遍 t()——
//    命中 key 转译文，否则原样显示（GPUI error.to_string() 对位，
//    github_sign_in.rs:168-176）。
// 4. Connected 写回页面会话信号（page.github = Some(connection) + revise
//    对位）并自动关窗（close_pending 对位）。
//
// 【nsh 纪律】组件体只执行一次：phase/copied 的读取全部落在 JSX 表达式
// （phaseBody 以 JSX 子表达式调用，波 1 教训）；waiting 事件经
// activeRun→phase 的模块级 effect 镜像（dialog.tsx topEntry 同款）；done
// 结算比对 activeRun，迟到/过期结算不落屏。

/** waiting 进度事件的载荷（Rust 侧一次性 post）。 */
export interface SignInWaiting {
  phase: "waiting";
  userCode: string;
  verificationUri: string;
  remainingSecs: number;
  expiresAt: string;
}

/** 登录任务的终态：connected 携带 login；cancelled 是关闭路径。 */
export interface SignInOutcome {
  connected?: boolean;
  cancelled?: boolean;
  login?: string;
}

/** 域桥：登录切片只依赖这一个调用（skillReaderSlot 的 ReaderBridge 同款
 * 注入位——真桥走 MySkillsGitHub facade，无头测试注入假桥逐分支驱动）。 */
export interface SignInBridge {
  /** 一次设备授权任务（Rust 侧循环内完成 start→post waiting→轮询）。 */
  signIn(job: DomainJob, onEvent: (event: SignInWaiting) => void): Promise<SignInOutcome>;
}

/** 真桥：MySkillsGitHub facade（Callback<JsonValue> 在 TS 是
 * (event: unknown) => void——调用边界 as 收窄，基座约定）。 */
export const facadeBridge: SignInBridge = {
  signIn: (job, onEvent) =>
    MySkillsGitHub.signIn(job, onEvent as unknown as (event: unknown) => void) as Promise<
      SignInOutcome
    >,
};

/** 当前生效的桥（Host 挂载时安装一次；测试注入假桥）。 */
let bridge: SignInBridge = facadeBridge;

// ---- 页面实体信号 ------------------------------------------------------------

/** 登录对话框打开信号（null = 关闭）。 */
export const signInOpen = signal<string | null>(null);

type Phase =
  | { kind: "starting" }
  | { kind: "waiting"; userCode: string; verificationUri: string; remainingSecs: number }
  | { kind: "failed"; message: string };

const phase = signal<Phase>({ kind: "starting" });
/** 复制按钮的「已复制」反馈（render_waiting 的 copied）。 */
const copied = signal(false);
/** 在跑的桥任务绑定（startJob 的 latest/stopped 是它的信号，phase 镜像
 * waiting 事件；done 结算比对它丢弃过期任务）。 */
const activeRun = signal<JobRun<SignInWaiting> | null>(null);

/** 从共享频道空状态打开登录对话框的唯一入口（open_sign_in 对位）。 */
export function openGitHubSignIn(): void {
  signInOpen.set("sign-in");
}

/** 关闭：Escape/遮罩共用（on_close → cancel；在途取消由模块级 effect
 * 的 run.stop() 执行——先关 epoch 门再 cancel，过期事件不落屏）。 */
export function closeGitHubSignIn(): void {
  signInOpen.set(null);
}

// ---- 打开/关闭/重试驱动（github_sign_in.rs new/start/retry/cancel） ----------

/** 打开信号驱动任务生命周期：开 = 新任务；关 = stop（Drop→cancel 兜底）。 */
effect(() => {
  const open = signInOpen();
  if (open === null) {
    untracked(() => {
      activeRun()?.stop();
      activeRun.set(null);
      // 复位：下次打开从 Starting 开始（GPUI 关闭即丢弃实体）。
      phase.set({ kind: "starting" });
      copied.set(false);
    });
    return;
  }
  untracked(() => startFlow());
});

/** waiting 事件镜像进 phase（dialog.tsx topEntry 同款：外层 signal 变化
 * 重挂 latest 的追踪，重试换新 run 后事件仍能落屏）。 */
effect(() => {
  const run = activeRun();
  if (run === null) return;
  const event = run.latest();
  if (event !== null && event.phase === "waiting") {
    phase.set({
      kind: "waiting",
      userCode: event.userCode,
      verificationUri: event.verificationUri,
      remainingSecs: event.remainingSecs,
    });
  }
});

function startFlow(): void {
  phase.set({ kind: "starting" });
  copied.set(false);
  const run = startJob<SignInWaiting>("github-sign-in", (job, onEvent) =>
    bridge.signIn(job, onEvent),
  );
  activeRun.set(run);
  run.done.then(
    (raw) => {
      // 过期结算（重试/已关闭后）不落屏。
      if (activeRun() !== run) return;
      activeRun.set(null);
      const outcome = (raw ?? {}) as SignInOutcome;
      if (outcome.cancelled === true) return;
      if (outcome.connected === true) {
        // 写回页面会话（page.github = Some(connection) + revise）并自动
        // 关窗（close_pending 对位；signInOpen 置空会走 stop 兜底）。
        github.set({ state: "connected", login: outcome.login ?? "" });
        signInOpen.set(null);
        return;
      }
      // 契约外的 resolve 形状（防御）：按失败显示，不卡 Starting。
      phase.set({ kind: "failed", message: t("settings.githubAuthExpired") });
    },
    (err: unknown) => {
      if (activeRun() !== run) return;
      activeRun.set(null);
      // Denied/Expired 以 key reject → t() 转译文；域错误原样显示
      // （github_sign_in.rs:168-176 对位）。
      phase.set({ kind: "failed", message: t(String(err)) });
    },
  );
}

/** 失败卡的重试（retry：弃旧链再 start，github_sign_in.rs:181-185）。 */
function retryFlow(): void {
  activeRun()?.stop();
  startFlow();
}

// ---- 挂载点（index.tsx 页根渲染一次） ----------------------------------------

/**
 * 登录对话框宿主：页根常挂，signInOpen 非空时渲染浮层（open_centered 420
 * 宽、量高后窗口正中——chrome::open_centered 对位）。`bridge` 注入位供无头
 * 测试换假桥（真桥默认）。
 */
export function GitHubSignInHost(props: { bridge?: SignInBridge }): JSX.Element | null {
  bridge = props.bridge ?? facadeBridge;
  return (
    <If when={signInOpen()}>
      {() => <SignInCard />}
    </If>
  );
}

// ---- 浮层与卡片（open_centered + render_header/render_body） -------------------

/** 卡宽（open_sign_in 的 420.0）。 */
const CARD_W = 420;

/** 卡底：亮色 card / 暗色 panel（open_sign_in 的 surface 选择）。 */
function cardSurface(): string {
  return isLight() ? paletteToken("card") : paletteToken("panel");
}

function SignInCard(): JSX.Element {
  return (
    <Overlay
      style={{ position: "fixed", top: 0, left: 0, right: 0, bottom: 0, zIndex: 60 }}
      onCancel={closeGitHubSignIn}
    >
      {/* Veil（点外关闭 → cancel）。 */}
      <View
        testID="github-sign-in-veil"
        style={{
          position: "absolute",
          top: 0,
          left: 0,
          right: 0,
          bottom: 0,
          backgroundColor: promptVeil(),
        }}
        onPress={closeGitHubSignIn}
      />
      <View
        style={{
          position: "absolute",
          top: 0,
          left: 0,
          right: 0,
          bottom: 0,
          display: "flex",
          alignItems: "center",
          justifyContent: "center",
        }}
      >
        <View
          testID="github-sign-in-card"
          style={{
            display: "flex",
            flexDirection: "column",
            width: CARD_W,
            borderRadius: 12,
            borderWidth: 1,
            borderColor: paletteToken("border"),
            backgroundColor: cardSurface(),
            boxShadow: SHEET_SHADOW,
            transform: "translateY(0)",
            ...fadeTransition(true),
          }}
          onPress={() => {
            // 吞掉按压，veil 不关窗。
          }}
        >
          <Header />
          {/* Phase 分派必须以 JSX 子表达式调用（组件体只执行一次）。 */}
          {phaseBody()}
        </View>
      </View>
    </Overlay>
  );
}

/** 头部（render_header）：32 圆角方 accent 软底 + gitBranch + 标题。 */
function Header(): JSX.Element {
  return (
    <View
      style={{
        display: "flex",
        flexDirection: "row",
        alignItems: "center",
        gap: 10,
        width: "100%",
        paddingHorizontal: 24,
        paddingTop: 16,
        paddingBottom: 12,
        flexShrink: 0,
        borderBottomWidth: 1,
        borderBottomColor: paletteToken("border_soft"),
      }}
    >
      <View
        style={{
          width: 32,
          height: 32,
          borderRadius: 12,
          display: "flex",
          alignItems: "center",
          justifyContent: "center",
          backgroundColor: paletteToken("accent_soft"),
        }}
      >
        <Icon name="gitBranch" size={16} color={paletteToken("accent")} />
      </View>
      <Text style={{ fontSize: 16, fontWeight: "600", color: paletteToken("fg") }}>
        {t("sharedChannels.signInTitle")}
      </Text>
    </View>
  );
}

/** Phase 分派（render_body）。 */
function phaseBody(): JSX.Element {
  const current = phase();
  if (current.kind === "starting") {
    return <StartingBody />;
  }
  if (current.kind === "waiting") {
    return <WaitingBody event={current} />;
  }
  return <FailedBody message={current.message} />;
}

/** 正文容器样式（body()：px24 py16 gap_3）。 */
function bodyStyle(): ViewStyle {
  return {
    display: "flex",
    flexDirection: "column",
    width: "100%",
    gap: 12,
    paddingHorizontal: 24,
    paddingVertical: 16,
  };
}

/** Starting（render_starting）：Loader 旋转 + 加载文案。 */
function StartingBody(): JSX.Element {
  return (
    <View style={bodyStyle()}>
      <View style={{ display: "flex", flexDirection: "row", alignItems: "center", gap: 8 }}>
        <IconSpin name="loader" size={14} color={paletteToken("fg_muted")} spin />
        <Text style={{ fontSize: 14, color: paletteToken("fg_muted") }}>
          {t("common.loading")}
        </Text>
      </View>
    </View>
  );
}

/** mm:ss 倒计时快照（render_waiting :263-268；静态，不逐秒走）。 */
function expiryLine(remainingSecs: number): string {
  const minutes = Math.floor(remainingSecs / 60);
  const seconds = remainingSecs % 60;
  const time = `${String(minutes).padStart(2, "0")}:${String(seconds).padStart(2, "0")}`;
  return tf("settings.githubAuthExpiresIn", { time });
}

/** Waiting（render_waiting）：两步说明 + 设备码卡（复制）+ 打开 GitHub +
 * 等待与剩余有效期。 */
function WaitingBody(props: {
  event: { kind: "waiting"; userCode: string; verificationUri: string; remainingSecs: number };
}): JSX.Element {
  return (
    <View style={bodyStyle()}>
      <View style={{ display: "flex", flexDirection: "column", gap: 4 }}>
        <Text style={{ fontSize: 14, color: paletteToken("fg_muted") }}>
          {t("settings.githubAuthStepOpen")}
        </Text>
        <Text style={{ fontSize: 14, color: paletteToken("fg_muted") }}>
          {t("settings.githubAuthStepApprove")}
        </Text>
      </View>
      {/* 设备码卡：大号 mono 码 + 复制按钮（render_waiting :283-337）。 */}
      <View
        style={{
          display: "flex",
          flexDirection: "row",
          alignItems: "center",
          justifyContent: "center",
          gap: 8,
          paddingVertical: 12,
          paddingHorizontal: 16,
          borderRadius: 8,
          borderWidth: 1,
          borderColor: paletteToken("border"),
          backgroundColor: paletteToken("card"),
        }}
      >
        <Text
          testID="github-sign-in-code"
          style={{
            fontSize: 20,
            fontWeight: "600",
            fontFamily: "monospace",
            color: paletteToken("fg"),
          }}
        >
          {props.event.userCode}
        </Text>
        <View
          testID="github-sign-in-copy"
          style={{
            paddingHorizontal: 12,
            paddingVertical: 4,
            borderRadius: 6,
            borderWidth: 1,
            borderColor: paletteToken("border"),
            backgroundColor: paletteToken("card"),
            cursor: "pointer",
          }}
          hoverStyle={{ backgroundColor: paletteToken("card_hover") }}
          onPress={() => void copyCode(props.event.userCode)}
        >
          <Text style={{ fontSize: 14, color: paletteToken("fg") }}>{copyLabel()}</Text>
        </View>
      </View>
      {/* 打开 GitHub（open_external → openUrl；悬停降透明度）。 */}
      <View
        testID="github-sign-in-open"
        style={{
          display: "flex",
          flexDirection: "row",
          alignItems: "center",
          justifyContent: "center",
          gap: 8,
          width: "100%",
          paddingVertical: 8,
          borderRadius: 8,
          backgroundColor: paletteToken("accent"),
          cursor: "pointer",
        }}
        hoverStyle={{ opacity: 0.9 }}
        onPress={() => void openUrl(props.event.verificationUri)}
      >
        <Icon name="gitBranch" size={14} color={paletteToken("on_accent")} />
        <Text style={{ fontSize: 14, fontWeight: "500", color: paletteToken("on_accent") }}>
          {t("settings.githubAuthStart")}
        </Text>
      </View>
      <View style={{ display: "flex", flexDirection: "column", alignItems: "center", gap: 4 }}>
        <Text style={{ fontSize: 14, color: paletteToken("fg_muted") }}>
          {t("settings.githubAuthWaiting")}
        </Text>
        <Text style={{ fontSize: 14, color: paletteToken("fg_muted") }}>
          {expiryLine(props.event.remainingSecs)}
        </Text>
      </View>
    </View>
  );
}

/** 复制按钮文案（copied ? Copied : Copy）——JSX 表达式里读 signal。 */
function copyLabel(): string {
  return copied() ? t("settings.githubAuthCopied") : t("settings.githubAuthCopy");
}

async function copyCode(code: string): Promise<void> {
  await Clipboard.writeText(code);
  copied.set(true);
}

/** Failed（render_failed）：域文案 + 重试按钮。 */
function FailedBody(props: { message: string }): JSX.Element {
  return (
    <View style={bodyStyle()}>
      <Text testID="github-sign-in-error" style={{ fontSize: 14, color: paletteToken("fg_muted") }}>
        {props.message}
      </Text>
      <View
        testID="github-sign-in-retry"
        style={{
          display: "flex",
          flexDirection: "row",
          alignItems: "center",
          justifyContent: "center",
          width: "100%",
          paddingVertical: 8,
          borderRadius: 8,
          backgroundColor: paletteToken("accent"),
          cursor: "pointer",
        }}
        hoverStyle={{ opacity: 0.9 }}
        onPress={retryFlow}
      >
        <Text style={{ fontSize: 14, fontWeight: "500", color: paletteToken("on_accent") }}>
          {t("common.retry")}
        </Text>
      </View>
    </View>
  );
}
