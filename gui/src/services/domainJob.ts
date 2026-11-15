import { signal, type Signal } from "nsh";
import { DomainJob } from "skillstar-gui/native";

// 波 4 异步桥基座的 TSX 半边——Rust→TSX 任务纪律的定型写法。
//
// 【机制】（native/src/facades/bridge_task.rs 的对位物）
// Rust 侧每个长任务持有三件东西：blocking 池上的 worker、进度通道、
// `DomainJob` 取消句柄（drop 即取消）。进度经 `Callback.post` 回推——
// 事件在引擎线程按 120ms 节流窗折叠（每窗只留最新，pump_progress），
// TS 侧只需接住「最新值」。取消走 `DomainJob.cancel()`/`dispose()`；
// Rust 侧停发 + 本模块的 epoch 门双保险，保证过期事件不落屏。
//
// 【nsh 纪律】回调里不要「读信号再写同信号」（自写自读会被跟踪成循
// 环，skillReaderSlot 同款教训）：计数用普通变量镜像进 signal。组件里
// 消费 `latest`/`count` 必须落在 JSX 表达式（组件体只执行一次，波 1
// 教训）。关闭 UI 时先 `stop()` 再卸载；`job` 被 GC/dispose 时 Rust 侧
// Drop 也会取消（兜底，不替代显式 stop）。

/** 一个在跑的桥任务在 TS 侧的绑定。 */
export interface JobRun<E> {
  /** 最新进度事件（null = 尚无）。消费必须落在 JSX 表达式。 */
  readonly latest: Signal<E | null>;
  /** 已落屏事件数（节流后——比域侧原始事件数小）。 */
  readonly count: Signal<number>;
  /** 任务是否已被 stop()（epoch 门的只读镜像）。 */
  readonly stopped: Signal<boolean>;
  /** Rust 取消句柄。`job.cancelled` / `job.token`（epoch 键）可读。 */
  readonly job: DomainJob;
  /** 任务 promise：resolve 值原样透传（取消后仍会 settle，属过期态，消费方按 epoch 丢弃）。 */
  readonly done: Promise<unknown>;
  /** 停止：cancel + 封 epoch——此后一切 post（含已在 JS 队列里的）不落屏。 */
  stop(): void;
}

/**
 * 启动一个桥任务并绑定进度流。`run` 收取消除句柄与进度回调——传给
 * Rust facade 的 async 方法（如 `MySkillsImport.scanRepo(..., job,
 * onEvent)`）。`onEvent` 在 JS 下一轮被调（Callback.post），本模块保证：
 * stop() 之后到达的事件被 epoch 门丢弃（过期进度不落屏）。
 */
export function startJob<E>(
  label: string,
  run: (job: DomainJob, onEvent: (event: E) => void) => Promise<unknown>,
): JobRun<E> {
  const job = new DomainJob(label);
  const latest = signal<E | null>(null);
  const count = signal(0);
  const stopped = signal(false);
  let stoppedNow = false;
  let seen = 0;
  const onEvent = (event: E): void => {
    if (stoppedNow) return; // epoch 门：迟到/过期事件不落屏。
    seen += 1;
    count.set(seen);
    latest.set(event);
  };
  const done = run(job, onEvent);
  return {
    latest,
    count,
    stopped,
    job,
    done,
    stop(): void {
      if (stoppedNow) return;
      // 顺序有讲究：先关门（epoch），再取消 Rust 任务，最后才动信号
      // ——信号 set 可能同步触发重渲染，渲染时必须已能读到
      // job.cancelled === true。
      stoppedNow = true;
      job.cancel();
      stopped.set(true);
      // latest 保持停时值：调用方即将卸载 UI，冻结比清空更符合「过
      // 期进度不落屏」——新任务永远开新 run（token 单调递增）。
    },
  };
}
