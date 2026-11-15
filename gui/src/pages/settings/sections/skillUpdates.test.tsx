// 「技能更新」分区测试 — settings/skill_updates.rs 的语义对位：模式滑块点
// 击即写盘（manual/auto），自动模式才挂频率行，频率 pill 提交分钟值（档位
// 归一在 native facade，见 facades/settings_updates.rs 的 cargo 测试）。不
// 写真实 $HOME。
import { expect, open, test } from "nsh/test";
import {
  createSkillUpdatesState,
  type SkillUpdatesBridge,
  type SkillUpdatesDto,
} from "./skillUpdates";

const HOST = "./skillUpdates.test.host.tsx";

// ---- 状态机（假桥；save_skill_updates 的读写语义） -------------------------

function fakeBridge(saveError: string | null = null): {
  bridge: SkillUpdatesBridge;
  saves: SkillUpdatesDto[];
} {
  const saves: SkillUpdatesDto[] = [];
  let disk: SkillUpdatesDto = { auto_update: false, interval_minutes: 60 };
  return {
    saves,
    bridge: {
      load: () => ({ ...disk }),
      save(next) {
        if (saveError !== null) return saveError;
        disk = { ...next };
        saves.push({ ...next });
        return null;
      },
    },
  };
}

test("start() reads the disk once and every pick persists immediately", () => {
  const { bridge, saves } = fakeBridge();
  const state = createSkillUpdatesState(bridge);
  state.start();
  expect(state.updates()).toEqual({ auto_update: false, interval_minutes: 60 });

  state.setAutoUpdate(true);
  state.setIntervalMinutes(360);
  // 每次点击都是一次整写（save_skill_updates 对位，无暂存）。
  expect(saves).toHaveLength(2);
  expect(saves[0]).toEqual({ auto_update: true, interval_minutes: 60 });
  expect(saves[1]).toEqual({ auto_update: true, interval_minutes: 360 });
  expect(state.updates()).toEqual({ auto_update: true, interval_minutes: 360 });
});

test("a failed save warns but keeps the picked value in memory", () => {
  const { bridge } = fakeBridge("disk full");
  const state = createSkillUpdatesState(bridge);
  state.start();
  state.setIntervalMinutes(30);
  // GPUI 侧 tracing::warn 后页面继续（skill_updates.rs save 分支）。
  expect(state.updates().interval_minutes).toBe(30);
});

// ---- host UI（假桥装置；模式分支与 pills 的渲染期挂载） ---------------------

async function openSection() {
  return open(HOST);
}

test("the mode slider switches and auto reveals the interval pills", async () => {
  const app = await openSection();
  await app.waitUntil(() => app.shows("id:skill-update-mode-auto"), { timeout: 2000 });
  // 手动是默认：频率行不挂。
  expect(app.shows("id:skill-update-interval-360")).toBe(false);

  await app.press("id:skill-update-mode-auto");
  await app.waitUntil(() => app.texts().some((s) => s.includes("updates:true|60")));
  await app.waitUntil(() => app.shows("id:skill-update-interval-360"));

  await app.press("id:skill-update-interval-360");
  await app.waitUntil(() => app.texts().some((s) => s.includes("updates:true|360")));

  await app.press("id:skill-update-mode-manual");
  await app.waitUntil(() => app.texts().some((s) => s.includes("updates:false|360")));
  // 回手动：频率行卸载（when(current == "auto") 的 JSX 分支）。
  expect(app.shows("id:skill-update-interval-360")).toBe(false);
});
