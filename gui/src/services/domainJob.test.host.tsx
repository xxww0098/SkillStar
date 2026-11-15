// 波 4 异步桥基座的机制证明装置：用真实 native DomainJob（new DomainJob /
// .token / .cancelled / .cancel() / .dispose()）驱动 src/services/domainJob.ts
// 的 startJob 与 epoch 门，把计数/最新事件/取消态/token/结局/dispose 状态渲染成
// 文本供 app.texts() 断言。不触文件系统、不写 $HOME。

import { Button } from "nsh/base";
import { Text, View, mount, signal, type JSX } from "nsh";
import { DomainJob } from "skillstar-gui/native";
import { startJob, type JobRun } from "./domainJob";

interface ActiveSink {
  emit?: (event: { step: number }) => void;
  resolve?: (value: { steps: number; cancelled: boolean }) => void;
}

/** 当前演示 run（null = 未起或已释放）。 */
const runState = signal<JobRun<{ step: number }> | null>(null);
/** 任务结局（steps/cancelled），正常结算后写入；stop() 后由 epoch 门拦截。 */
const outcome = signal<string>("pending");
/** token 唯一性检测回显。 */
const tokensText = signal<string>("tokens:idle");
/** dispose 状态回显（先清 runState 再 dispose，避免 JSX 读已释放对象）。 */
const disposeText = signal<string>("dispose:idle");

let sink: ActiveSink = {};
let emittedStep = 0;

function start(): void {
  outcome.set("pending");
  disposeText.set("dispose:idle");
  emittedStep = 0;
  sink = {};
  const run = startJob<{ step: number }>("domain-job-harness", (_job, onEvent) =>
    new Promise((resolve) => {
      sink = { emit: onEvent, resolve };
    }),
  );
  runState.set(run);
  void run.done.then((value) => {
    if (run.stopped()) return; // epoch 门：stop() 之后迟到的 promise 结算不落屏。
    const result = value as { steps: number; cancelled: boolean };
    outcome.set(`steps:${result.steps} cancelled:${result.cancelled}`);
  });
}

function emitNext(): void {
  emittedStep += 1;
  sink.emit?.({ step: emittedStep });
}

function emitLate(): void {
  sink.emit?.({ step: 999 });
}

function settleNormal(): void {
  sink.resolve?.({ steps: emittedStep, cancelled: runState()?.job.cancelled ?? false });
}

function settleLate(): void {
  sink.resolve?.({ steps: 999, cancelled: true });
}

function checkTokens(): void {
  const first = new DomainJob("token-a");
  const second = new DomainJob("token-b");
  const t1 = first.token;
  const t2 = second.token;
  const initCancelled = first.cancelled || second.cancelled;
  first.dispose();
  second.dispose();
  tokensText.set(
    `tokens:${t1}/${t2} unique:${t1 > 0 && t2 > 0 && t1 !== t2} initCancelled:${initCancelled}`,
  );
}

function disposeRun(): void {
  const run = runState();
  if (run === null) return;
  const job = run.job;
  const before = job.cancelled;
  // 先从 signal 清空引用再 dispose，避免重渲染时 JSX 访问已释放的 native 实例。
  runState.set(null);
  job.dispose();
  let throwsAfter = false;
  try {
    void job.cancelled;
  } catch {
    throwsAfter = true;
  }
  disposeText.set(`dispose:before=${before} throwsAfter=${throwsAfter}`);
}

function App(): JSX.Element {
  return (
    <View style={{ display: "flex", flexDirection: "column", gap: 8, padding: 12 }}>
      <View style={{ display: "flex", flexDirection: "row", gap: 6, flexWrap: "wrap" }}>
        <Button testID="host-start" onPress={start}>
          <Text>start</Text>
        </Button>
        <Button testID="host-emit" onPress={emitNext}>
          <Text>emit</Text>
        </Button>
        <Button testID="host-stop" onPress={() => runState()?.stop()}>
          <Text>stop</Text>
        </Button>
        <Button testID="host-late-emit" onPress={emitLate}>
          <Text>late-emit</Text>
        </Button>
        <Button testID="host-settle" onPress={settleNormal}>
          <Text>settle</Text>
        </Button>
        <Button testID="host-late-settle" onPress={settleLate}>
          <Text>late-settle</Text>
        </Button>
        <Button testID="host-tokens" onPress={checkTokens}>
          <Text>tokens</Text>
        </Button>
        <Button testID="host-dispose" onPress={disposeRun}>
          <Text>dispose</Text>
        </Button>
      </View>
      {/* 消费全部落在 JSX 表达式（组件体只执行一次）。 */}
      <Text testID="host-progress" style={{ fontSize: 12 }}>
        {`count:${runState()?.count() ?? 0} latest:${runState()?.latest()?.step ?? 0} stopped:${runState()?.stopped() ?? false} cancelled:${runState()?.job.cancelled ?? false} token:${runState()?.job.token ?? 0}`}
      </Text>
      <Text testID="host-outcome" style={{ fontSize: 12 }}>
        {outcome()}
      </Text>
      <Text testID="host-tokens-status" style={{ fontSize: 12 }}>
        {tokensText()}
      </Text>
      <Text testID="host-dispose-status" style={{ fontSize: 12 }}>
        {disposeText()}
      </Text>
    </View>
  );
}

mount(App);
