import { Text, View, mount, signal, type JSX } from "nsh";
import { Button } from "nsh/base";
import { MarketBoard } from "./board";
import { createReadSide } from "./readSide";

// 市场卡并发安装的无头装置（detailDrawer.test.host.tsx 样板）：真实
// MarketBoard 接真实读侧状态机，只把安装调用换成可控 Promise。测试按卡面
// 上真实的安装按钮驱动，断言两张卡的转圈同时在画面上
// （installBusy.test.tsx）。描述留空、来源记 local：不进描述翻译队列，
// 也不取 GitHub 头像。

/** 在飞的安装调用；装置按钮放行或驳回最旧一笔。 */
const installCalls: {
  name: string;
  resolve: (name: string) => void;
  reject: (reason: unknown) => void;
}[] = [];
/** 在飞调用数（装置文本暴露；在飞的卡再点一次不会让它变多）。 */
const callCount = signal(0);

const machine = createReadSide({
  loader: async () => ({
    kind: "skills" as const,
    payload: {
      data: ["alpha", "beta", "gamma"].map((name, index) => ({
        name,
        description: "",
        stars: 0,
        rank: index + 1,
        source: "local",
        gitUrl: "https://example.invalid/owner/repo",
      })),
      snapshotStatus: "fresh",
      snapshotUpdatedAt: null,
      error: null,
    },
  }),
  syncer: async () => {},
  installer: (_url, name) =>
    new Promise<string>((resolve, reject) => {
      installCalls.push({ name, resolve, reject });
      callCount.set(installCalls.length);
    }),
  uninstaller: async (name) => name,
  // 右上角任务卡片不挂在本装置里：计时器只收集不触发。
  schedule: () => () => {},
  notify: () => {},
  confirmUninstall: () => {},
});

function settleFirst(reject: boolean): void {
  const call = installCalls.shift();
  if (!call) return;
  callCount.set(installCalls.length);
  if (reject) call.reject(new Error("boom"));
  else call.resolve(call.name);
}

function App(): JSX.Element {
  return (
    <View style={{ width: "100%", height: "100%", display: "flex", flexDirection: "column" }}>
      <View style={{ display: "flex", flexDirection: "row", height: 32, alignItems: "center" }}>
        <Button testID="host-resolve-first" onPress={() => settleFirst(false)}>
          <Text>resolve-first</Text>
        </Button>
        <Button testID="host-reject-first" onPress={() => settleFirst(true)}>
          <Text>reject-first</Text>
        </Button>
        <Text testID="host-calls">calls:{callCount()}</Text>
      </View>
      <View style={{ flex: 1, minHeight: 0, width: "100%" }}>
        <MarketBoard
          loading={false}
          tab="all"
          skills={machine.skills()}
          publishers={[]}
          query=""
          viewList={false}
          busy={machine.busy()}
          selectedName={machine.selected()?.name ?? null}
          onOpenSkill={machine.openSkill}
          onToggleInstall={machine.toggleInstalled}
          onOpenPublisher={() => {}}
          onClearSearch={() => {}}
        />
      </View>
    </View>
  );
}

mount(App);
machine.start();
