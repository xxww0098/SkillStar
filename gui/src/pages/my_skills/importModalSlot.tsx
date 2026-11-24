import { Overlay, Text, View, type JSX } from "nsh";
import { Icon } from "../../chrome/icons";
import { promptVeil, SHEET_SHADOW } from "../../chrome/dialog";
import { fadeTransition } from "../../chrome/motion";
import { isLight, paletteToken } from "../../services/themeMode";
import {
  closeImportModal,
  importModalOpen,
  installBridges,
  openImportModal,
  phase,
  type ImportBridge,
  type PackBridge,
} from "./importModalState";
import {
  CompletedBody,
  FailedBody,
  InputBody,
  LoadingBody,
  headerTitle,
} from "./importModalPhases";
import { SelectBody } from "./importModalSelect";
import { SharePreviewBody } from "./importModalShare";
import { PackBody } from "./importModalPack";

// 导入对话框槽位 — my_skills/import_modal.rs（+ import_modal/
// {phases,select,share,progress,helpers}）对位。波 4 异步桥基座预注册的
// 落点：本文件持有打开信号与浮层宿主，状态机在 importModalState，各相渲
// 染器在 importModalPhases/Select/Share/Pack。
//
// 【桥规矩（bridge_task.rs + services/domainJob.ts + my_skills_import.rs）】
// 扫描/安装是 blocking 池 worker + 120ms 最新值进度泵（节流在 Rust 桥层）；
// 未收进后台时，关闭（Escape/遮罩）一律 run.stop()——Rust 侧 facade.cancel()
// （git 会话自身的取消标志）+ epoch 门双保险，过期进度与迟到结算不落屏。
// parked 的关闭留着任务，重新打开后再关才取消。
//
// 【浮层】chrome::open_centered（toolbar.rs open_import）：512 宽、量高后
// 窗口正中；Escape/遮罩 = X = close（on_close → cancel_active）。
//
// 【nsh 纪律】组件体只执行一次：phase 的读取落在 JSX 表达式（phaseBody
// 以 JSX 子表达式调用）；bridge 在 Host 挂载时安装一次（测试可注入假桥）。

export { closeImportModal, openImportModal } from "./importModalState";
export type {
  AuditRow,
  GitProgressEvent,
  ImportBridge,
  PackBridge,
  ScanView,
  ShareView,
  ShareOutcome,
  SkillRow,
} from "./importModalState";

/**
 * 导入对话框宿主：页根常挂，importModalOpen 非空时渲染浮层。`bridge` /
 * `packBridge` 注入位供无头测试换假桥（真桥默认）。
 */
export function ImportModalHost(props: {
  bridge?: ImportBridge;
  packBridge?: PackBridge;
}): JSX.Element | null {
  installBridges(props.bridge, props.packBridge);
  return (
    <If when={importModalOpen()}>
      {() => <ImportModalCard />}
    </If>
  );
}

// ---- 浮层与卡片（open_centered 512 宽 + render_header + Phase 分派） -------

/** 卡宽（open_import builder 的 512.0）。 */
const CARD_W = 512;

/** 卡底：亮色 card / 暗色 panel（open_import 的 surface 选择）。 */
function cardSurface(): string {
  return isLight() ? paletteToken("card") : paletteToken("panel");
}

function ImportModalCard(): JSX.Element {
  return (
    <Overlay
      style={{ position: "fixed", top: 0, left: 0, right: 0, bottom: 0, zIndex: 60 }}
      onCancel={closeImportModal}
    >
      {/* Veil（点外关闭 → cancel_active）。 */}
      <View
        testID="import-modal-veil"
        style={{
          position: "absolute",
          top: 0,
          left: 0,
          right: 0,
          bottom: 0,
          backgroundColor: promptVeil(),
        }}
        onPress={closeImportModal}
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
          testID="import-modal-card"
          style={{
            display: "flex",
            flexDirection: "column",
            width: CARD_W,
            borderRadius: 12,
            borderWidth: 1,
            borderColor: paletteToken("border"),
            backgroundColor: cardSurface(),
            boxShadow: SHEET_SHADOW,
            overflow: "hidden",
            transform: "translateY(0)",
            ...fadeTransition(true),
          }}
          onPress={() => {
            // 吞掉按压，veil 不关窗。
          }}
        >
          <Header />
          <View style={{ display: "flex", flexDirection: "column" }}>
            <If when={phase() === "input"}>
              <InputBody />
            </If>
            <If when={phase() === "scanning" || phase() === "installing" || phase() === "shareInstalling"}>
              <LoadingBody />
            </If>
            <If when={phase() === "select"}>
              <SelectBody />
            </If>
            <If when={phase() === "completed"}>
              <CompletedBody />
            </If>
            <If when={phase() === "failed"}>
              <FailedBody />
            </If>
            <If when={phase() === "sharePreview"}>
              <SharePreviewBody />
            </If>
            <If when={phase() === "pack"}>
              <PackBody />
            </If>
          </View>
        </View>
      </View>
    </Overlay>
  );
}

/** 头部（render_header）：px-6 pt-4 pb-3 + 下边框；非 pack 相有 32px accent
 * 井 + Download 图标；pack 相是卡组标题（无井）。 */
function Header(): JSX.Element {
  const pack = () => phase() === "pack";
  return (
    <View
      testID="import-modal-header"
      style={{
        display: "flex",
        flexDirection: "row",
        alignItems: "center",
        justifyContent: "space-between",
        width: "100%",
        paddingHorizontal: 24,
        paddingTop: 16,
        paddingBottom: 12,
        flexShrink: 0,
        borderBottomWidth: 1,
        borderBottomColor: paletteToken("border_soft"),
      }}
    >
      <View style={{ display: "flex", flexDirection: "row", alignItems: "center", gap: 10 }}>
        <If when={!pack()}>
          <View
            style={{
              width: 32,
              height: 32,
              borderRadius: 12,
              display: "flex",
              alignItems: "center",
              justifyContent: "center",
              backgroundColor: paletteToken("accent") + "1a",
            }}
          >
            <Icon name="download" size={16} color={paletteToken("accent")} />
          </View>
        </If>
        <Text style={{ fontSize: 16, fontWeight: "600", color: paletteToken("fg") }}>
          {headerTitle(phase())}
        </Text>
      </View>
    </View>
  );
}
