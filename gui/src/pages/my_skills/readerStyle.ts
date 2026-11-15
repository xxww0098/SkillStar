import type { TextStyle, ViewStyle } from "nsh";
import type { TextViewStyles } from "nsh/base";
import { hexAlpha, paletteToken } from "../../services/themeMode";

// SKILL.md 阅读器样式 — reader_style.rs + 排版语义对位。分两半：
//
// (1) readerTypography()：nsh TextView 的逐部件样式映射，把 nsh 默认排版
//     （body 15/h2 20）拉回 GPUI 现阅读器的数值。每个数字都出自 GPUI 源
//     码（波 3 spike 的 GPIOMapping 表逐项验证过，13/13 无头断言 +
//     macOS 像素截图通过；此处是它的正式落位）：
//       body 16px            gpui-component theme/mod.rs:771 font_size px(16.)
//       段距 12px            gpui-base text/style.rs paragraph_gap rems(0.75)
//       标题比例             gpui-base text/node.rs heading_metrics :2867
//                            h1 1.8 h2 4/3 h3 17/15 h4 1 h5 14/15 h6 13/15，
//                            全 SEMIBOLD，h6 muted（node.rs :3913）
//       标题行高             h1 1.25，其余 1.35（heading_metrics）
//       标题下间距           HEADING_BOTTOM_GAP 0.35 × size（node.rs :2879）
//       表格字号 14.93px     body × 14/15（node.rs :4004）
//       代码块               px16/pt14/pb14、mono13、行高 1.6、边框圆角
//                            （node.rs :2226-2236）
//       行内码               mono，0.875 × 16 = 14（node.rs :2422）
//       引用                 左边线 2、pl 0.9×body、muted（node.rs :3925）
//       分隔线               上下各 body × 0.6 的空间（node.rs :2891）
//     GPUI 复刻不了的一点：GPUI 标题上间距与前块的段距折叠（node.rs
//     :3874 注释），nsh 的 margin 相加，故 marginTop 记「节距 − 段距」的
//     净值逼近同一节奏（spike 已注明）。
//     代码块底色：kit 用 theme.muted 而 palette 无此 token，用 well 近似
//     （spike 同款；palette 归一波对表）。
//
// (2) readerLineTheme：译文行的 18 款保存样式（translation.rs paint
//     :383-507 + reader_style.rs 的样式菜单）。两款（dot 的点阵行、
//     grad/color 的逐字着色）在 GPUI 里是逐字符绘制，nsh 文本装饰的近
//     似见各 case 注释。

function headingStyle(scale: number, lineHeight: number, sectionGap: number): TextStyle {
  const size = 16 * scale;
  return {
    fontSize: size,
    fontWeight: 600,
    lineHeight: Math.round(size * lineHeight),
    marginTop: Math.max(0, size * sectionGap - 12),
    marginBottom: size * 0.35,
  };
}

/** TextView 的 GPUI 排版映射（GPIOMapping 的正式落位）。
 *
 * 是**函数不是常量**：paletteToken() 每次都读 themeMode() 才算得出颜色，
 * 存进模块级常量的话在模块导入那一刻就取完值（默认 current 深色），之后
 * 切到 paper 只有外围重画、这张表仍是深色值 —— 白卡片上 #f2efe9 的正文
 * 与整条深色代码块就是这么来的（见 docs/errors.md）。调用点必须写在 JSX
 * 表达式里（`styles={readerTypography()}`），nsh 换主题时才重求值。 */
export function readerTypography(): TextViewStyles {
  return {
    root: { fontSize: 16, color: paletteToken("fg") },
    paragraph: { marginBottom: 12, lineHeight: 24 },
    heading1: headingStyle(1.8, 1.25, 1.1),
    heading2: headingStyle(4 / 3, 1.35, 1.6),
    heading3: headingStyle(17 / 15, 1.35, 1.4),
    heading4: headingStyle(1.0, 1.35, 1.4),
    heading5: headingStyle(14 / 15, 1.35, 1.4),
    heading6: { ...headingStyle(13 / 15, 1.35, 1.4), color: paletteToken("fg_muted") },
    codeBlock: {
      backgroundColor: paletteToken("well"),
      borderWidth: 1,
      borderColor: paletteToken("border"),
      borderRadius: 6,
      paddingHorizontal: 16,
      paddingTop: 14,
      paddingBottom: 14,
      fontSize: 13,
      lineHeight: 21,
      fontFamily: "monospace",
    },
    code: {
      fontFamily: "monospace",
      fontSize: 14,
      color: paletteToken("fg"),
    },
    table: {
      borderWidth: 1,
      borderColor: paletteToken("border"),
      borderRadius: 6,
      fontSize: 16 * (14 / 15),
    },
    tableHead: { backgroundColor: paletteToken("card_hover") },
    blockquote: {
      borderLeftWidth: 2,
      borderLeftColor: paletteToken("border"),
      paddingLeft: 16 * 0.9,
      color: paletteToken("fg_muted"),
    },
    rule: { marginTop: 16 * 0.6, marginBottom: 16 * 0.6 },
  };
}

/** 译文行的基础排版（skill_reader.rs :483-487：text_sm、行高 20、
 * 换行允许、全宽）。同 readerTypography()：函数，颜色渲染时读主题。 */
export function translationBase(): TextStyle {
  return {
    fontSize: 14,
    lineHeight: 20,
    color: paletteToken("fg_muted"),
  };
}

export interface ReaderLineTheme {
  /** 译文行外包 View 的样式（边框/填充/内边距类主题）。 */
  view: ViewStyle;
  /** 译文 Text 的样式（下划装饰/颜色/透明度类主题）。 */
  text: TextStyle;
}

const PLAIN: ReaderLineTheme = { view: {}, text: {} };

/** paint() 的 18 款（translation.rs :383-507）。id 未知 → none。 */
export function readerLineTheme(id: string): ReaderLineTheme {
  const accent = paletteToken("accent");
  switch (id) {
    case "line":
      return {
        view: {},
        text: {
          textDecorationLine: "underline",
          textDecorationColor: accent,
          textDecorationThickness: 1,
        },
      };
    case "dot":
      // GPUI dotted()：文字下方一行 2px 圆点（逐个绘制）。nsh 近似为
      // 点状下划线（textDecorationStyle dotted）。
      return {
        view: {},
        text: {
          textDecorationLine: "underline",
          textDecorationColor: accent,
          textDecorationStyle: "dotted",
          textDecorationThickness: 1,
        },
      };
    case "dash":
      // GPUI 是 border_b_1 虚线（盒子底边线）；文本下划虚线近似。
      return {
        view: {},
        text: {
          textDecorationLine: "underline",
          textDecorationColor: accent,
          textDecorationStyle: "dashed",
          textDecorationThickness: 1,
        },
      };
    case "dbold":
      return {
        view: {},
        text: {
          textDecorationLine: "underline",
          textDecorationColor: accent,
          textDecorationStyle: "dashed",
          textDecorationThickness: 2,
        },
      };
    case "wavy":
      return {
        view: {},
        text: {
          textDecorationLine: "underline",
          textDecorationColor: accent,
          textDecorationStyle: "wavy",
          textDecorationThickness: 1,
        },
      };
    case "wbold":
      return {
        view: {},
        text: {
          textDecorationLine: "underline",
          textDecorationColor: accent,
          textDecorationStyle: "wavy",
          textDecorationThickness: 2,
        },
      };
    case "box":
      return {
        view: {
          borderWidth: 1,
          borderStyle: "dashed",
          borderColor: accent,
          borderRadius: 6,
          paddingHorizontal: 4,
        },
        text: {},
      };
    case "xbold":
      return {
        view: {
          borderWidth: 2,
          borderStyle: "dashed",
          borderColor: accent,
          borderRadius: 6,
          paddingHorizontal: 4,
        },
        text: {},
      };
    case "mark":
      return {
        view: {},
        text: {
          backgroundColor: hexAlpha(accent, 0.45),
          borderRadius: 2,
        },
      };
    case "gmark":
      return {
        view: {},
        text: {
          backgroundColor: hexAlpha(paletteToken("violet"), 0.4),
          borderRadius: 2,
        },
      };
    case "fuzzy":
      return {
        view: { opacity: 0.55 },
        text: { color: paletteToken("fg_faint") },
      };
    case "hi":
      return {
        view: {
          backgroundColor: accent,
          borderRadius: 2,
          paddingHorizontal: 4,
        },
        text: { color: paletteToken("on_accent") },
      };
    case "quote":
      return {
        view: {
          borderLeftWidth: 4,
          borderLeftColor: accent,
          backgroundColor: paletteToken("accent_soft"),
          paddingLeft: 8,
          paddingRight: 4,
        },
        text: {},
      };
    case "grad":
      return { view: {}, text: { color: paletteToken("violet") } };
    case "blink":
      return { view: { opacity: 0.35 }, text: {} };
    case "glow":
      return {
        view: {
          backgroundColor: hexAlpha(paletteToken("danger"), 0.16),
          borderRadius: 6,
          paddingHorizontal: 4,
        },
        text: {},
      };
    case "color":
      // GPUI 是 45° 双色渐变填充；CSS 渐变背景近似。
      return {
        view: {
          backgroundImage: `linear-gradient(45deg, ${hexAlpha(
            paletteToken("ok"),
            0.55,
          )}, ${hexAlpha(accent, 0.55)})`,
          borderRadius: 2,
          paddingHorizontal: 4,
        },
        text: {},
      };
    default:
      return PLAIN;
  }
}
