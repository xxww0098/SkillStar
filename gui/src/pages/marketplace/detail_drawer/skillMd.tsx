import { Overlay, Scroll, Text, View, type JSX } from "nsh";
import { paletteToken } from "../../../services/themeMode";
import { Icon } from "../../../chrome/icons";
import { Tip } from "../../../chrome/tooltip";
import { SHEET_SHADOW, promptVeil } from "../../../chrome/dialog";
import { fadeTransition } from "../../../chrome/motion";
import { mt } from "../marketStrings";

// 「查看 SKILL.md…」悬浮窗 — my_skills/skill_reader 的市场侧最小对位
// （column.rs skill_md_button 打开的那个窗口）。
//
// GPUI 里它与技能页共享 chrome 的阅读器悬浮窗（量高后放窗口正中、veil、
// Escape/点外关闭）。共享阅读器（中英对照、主题样式）属 my_skills 波次；
// 本切片先给同 chrome 的纯文本版：fixed Overlay 居中、卡片底、标题行 +
// 关闭、正文整段滚动（列内滚轮不外泄的同款约束在这里同样成立——正文是
// 唯一的 Scroll）。共享阅读器落地后此文件让位。

/** 卡片占窗口的比例上限（chrome/dialog.rs 量高后居中的无测量近似）。 */
const MD_MAX_HEIGHT = "78%";

export function SkillMdOverlay(props: { title: string; markdown: string; onClose(): void }): JSX.Element {
  return (
    <Overlay
      style={{
        position: "fixed",
        top: 0,
        left: 0,
        right: 0,
        bottom: 0,
        display: "flex",
        alignItems: "center",
        justifyContent: "center",
      }}
      onCancel={props.onClose}
    >
      {/* Veil：Kit 对话框遮罩（prompt_veil），页面保持可见。 */}
      <View
        style={{
          position: "absolute",
          top: 0,
          left: 0,
          right: 0,
          bottom: 0,
          backgroundColor: promptVeil(),
        }}
        onPress={props.onClose}
      />
      <View
        testID="mk-skill-md-overlay"
        style={{
          display: "flex",
          flexDirection: "column",
          width: 560,
          maxWidth: "92%",
          maxHeight: MD_MAX_HEIGHT,
          minHeight: 0,
          borderRadius: 12,
          borderWidth: 1,
          borderColor: paletteToken("border"),
          backgroundColor: paletteToken("panel"),
          boxShadow: SHEET_SHADOW,
          transform: "translateY(0)",
          ...fadeTransition(true),
        }}
        onPress={() => {
          // 吞掉按压，veil 不关窗。
        }}
      >
        <View
          style={{
            display: "flex",
            flexDirection: "row",
            alignItems: "center",
            justifyContent: "space-between",
            gap: 8,
            flexShrink: 0,
            paddingHorizontal: 16,
            paddingTop: 14,
            paddingBottom: 10,
            borderBottomWidth: 1,
            borderBottomColor: paletteToken("border"),
          }}
        >
          <View style={{ minWidth: 0 }}>
            <Text style={{ fontSize: 14, color: paletteToken("fg"), numberOfLines: 1 }}>
              {props.title}
            </Text>
          </View>
          <View
            testID="mk-skill-md-close"
            tooltip={<Tip text={mt("detailPanel.dismissDrawer")} />}
            style={{
              display: "flex",
              alignItems: "center",
              justifyContent: "center",
              width: 24,
              height: 24,
              flexShrink: 0,
              borderRadius: 6,
              cursor: "pointer",
            }}
            hoverStyle={{ backgroundColor: paletteToken("panel_hover") }}
            onPress={props.onClose}
          >
            <Icon name="x" size={14} color={paletteToken("fg_muted")} />
          </View>
        </View>
        <Scroll style={{ flex: 1, minHeight: 0 }} contentStyle={{ padding: 16 }}>
          <MdLines markdown={props.markdown} />
        </Scroll>
      </View>
    </Overlay>
  );
}

/** text_block（column.rs:761-786）对位：按行渲染，空行占一格。 */
function MdLines(props: { markdown: string }): JSX.Element {
  const lines = props.markdown.split("\n");
  return (
    <View style={{ display: "flex", flexDirection: "column", gap: 4 }}>
      {lines.map((line, index) =>
        index === 0 ? (
          // 首行带 View testID：无头测试按 id: 寻址（Text 不参与命中）。
          <View testID="mk-skill-md-body">
            <Text style={{ fontSize: 12, lineHeight: 18, color: paletteToken("fg") }}>
              {line.length === 0 ? " " : line}
            </Text>
          </View>
        ) : (
          <Text style={{ fontSize: 12, lineHeight: 18, color: paletteToken("fg") }}>
            {line.length === 0 ? " " : line}
          </Text>
        )
      )}
    </View>
  );
}
