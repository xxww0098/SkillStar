import { Text, View, type JSX, type ViewStyle } from "nsh";
import { paletteToken, type PaletteToken } from "../../services/themeMode";
import { settleTransition } from "../../chrome/motion";
import { Icon, type IconName } from "../../chrome/icons";
import { IconSpin } from "../../chrome/feedback";
import { st } from "./settingsStrings";
import type { SettingsSectionEntry, StandaloneSectionEntry } from "./sections";

// 设置分区共享设计与布局原语：
// - 等宽字体常量 MONO_FONT（路径 / URL / 版本 / 命令行 / 毫秒计数）
// - 分区外壳 SectionShell（36×36 图标底座 + 18px 标题 + 副标题导语 + 分组/单卡模式）
// - 分组卡片 SettingsGroup、设置行 SettingRow（支持行首图标底座）、统一开关 SettingsSwitch
// - 语义徽标 StatusPill / MetaChip、统一按钮 SettingsButton、ChoicePills、字节格式化

export const MONO_FONT = "JetBrains Mono, Menlo, Consolas, monospace";

export type StatusTone =
  | "neutral"
  | "accent"
  | "ok"
  | "warn"
  | "danger"
  | "info"
  | "amber"
  | "cyan";

export type ButtonVariant = "secondary" | "accent" | "warn" | "danger" | "ghost";

const TONE_MAP: Record<
  StatusTone,
  { bg: PaletteToken; border: PaletteToken; fg: PaletteToken; icon: PaletteToken }
> = {
  neutral: { bg: "well", border: "border", fg: "fg_muted", icon: "fg_muted" },
  accent: { bg: "accent_soft", border: "accent_soft_edge", fg: "accent_fg", icon: "accent" },
  ok: { bg: "ok_bg", border: "ok_border", fg: "ok", icon: "ok" },
  warn: { bg: "warn_bg", border: "warn_border", fg: "warn", icon: "warn" },
  danger: { bg: "danger_bg", border: "danger_border", fg: "danger_fg", icon: "danger" },
  info: { bg: "info_bg", border: "info_border", fg: "info", icon: "info" },
  amber: { bg: "amber_bg", border: "amber_border", fg: "amber_fg", icon: "amber" },
  cyan: { bg: "cyan_bg", border: "cyan_border", fg: "cyan_fg", icon: "cyan" },
};

const BUTTON_MAP: Record<
  ButtonVariant,
  {
    bg: PaletteToken;
    border: PaletteToken;
    fg: PaletteToken;
    icon: PaletteToken;
    hoverBg: PaletteToken;
    activeBg: PaletteToken;
  }
> = {
  secondary: {
    bg: "card",
    border: "border",
    fg: "fg",
    icon: "fg_muted",
    hoverBg: "panel_hover",
    activeBg: "well",
  },
  accent: {
    bg: "accent_soft",
    border: "accent_soft_edge",
    fg: "accent_fg",
    icon: "accent_fg",
    hoverBg: "panel_active",
    activeBg: "card_active",
  },
  warn: {
    bg: "warn_bg",
    border: "warn_border",
    fg: "warn",
    icon: "warn",
    hoverBg: "warn_hover",
    activeBg: "warn_bg",
  },
  danger: {
    bg: "danger_bg",
    border: "danger_border",
    fg: "danger_fg",
    icon: "danger",
    hoverBg: "danger_hover",
    activeBg: "danger_bg",
  },
  ghost: {
    bg: "well",
    border: "border_soft",
    fg: "fg_muted",
    icon: "fg_muted",
    hoverBg: "panel_hover",
    activeBg: "border_soft",
  },
};

/** B/KB/MB/GB 字节格式化（去尾零）。 */
export function formatBytes(bytes: number): string {
  if (bytes === 0) return "0 B";
  const units = ["B", "KB", "MB", "GB"];
  const exp = Math.min(Math.floor(Math.log(bytes) / Math.log(1024)), units.length - 1);
  const value = bytes / 1024 ** exp;
  const text = value.toFixed(2).replace(/\.?0+$/, "");
  return `${text} ${units[exp]}`;
}

/** 分区卡片标准样式（16px 圆角，对齐 DESIGN.md）。 */
export function card(style?: ViewStyle): ViewStyle {
  return {
    width: "100%",
    borderRadius: 16,
    borderWidth: 1,
    borderColor: paletteToken("border"),
    backgroundColor: paletteToken("card"),
    overflow: "hidden",
    ...style,
  };
}

/** 语义状态胶囊。 */
export function StatusPill(props: {
  text: string;
  tone?: StatusTone;
  mono?: boolean;
  icon?: IconName;
  testID?: string;
}): JSX.Element {
  const tokens = () => TONE_MAP[props.tone ?? "neutral"];
  return (
    <View
      testID={props.testID}
      style={{
        height: 22,
        paddingHorizontal: 8,
        borderRadius: 999,
        borderWidth: 1,
        borderColor: paletteToken(tokens().border),
        backgroundColor: paletteToken(tokens().bg),
        display: "flex",
        flexDirection: "row",
        alignItems: "center",
        gap: 4,
        flexShrink: 0,
      }}
    >
      <If when={Boolean(props.icon)}>
        <Icon name={props.icon!} size={11} color={paletteToken(tokens().icon)} />
      </If>
      <Text
        style={{
          fontSize: 11,
          fontWeight: "600",
          color: paletteToken(tokens().fg),
          ...(props.mono ? { fontFamily: MONO_FONT, fontVariant: ["tabular-nums"] } : {}),
          numberOfLines: 1,
        }}
      >
        {props.text}
      </Text>
    </View>
  );
}

/** 截断元数据胶囊（例如活动代理地址、当前镜像 URL）。 */
export function MetaChip(props: {
  text: string;
  mono?: boolean;
  tone?: StatusTone;
}): JSX.Element {
  const tokens = () => TONE_MAP[props.tone ?? "neutral"];
  return (
    <View
      style={{
        maxWidth: 260,
        paddingHorizontal: 8,
        paddingVertical: 3,
        borderRadius: 7,
        borderWidth: 1,
        borderColor: paletteToken(tokens().border),
        backgroundColor: paletteToken(tokens().bg),
        flexShrink: 1,
        minHeight: 0,
      }}
    >
      <Text
        style={{
          fontSize: 12,
          color: paletteToken(tokens().fg),
          ...(props.mono ? { fontFamily: MONO_FONT, fontVariant: ["tabular-nums"] } : {}),
          numberOfLines: 1,
        }}
      >
        {props.text}
      </Text>
    </View>
  );
}

/** 折叠箭头：展开向下，收起向右。 */
export function CollapseChevron(props: { open: boolean; color?: string }): JSX.Element {
  return (
    <Icon
      name={props.open ? "chevronDown" : "chevronRight"}
      size={16}
      color={props.color ?? paletteToken("fg_muted")}
    />
  );
}

/** 字段标签 + 可选右侧附属槽 + 控件纵排容器。 */
export function FieldLabel(props: {
  title: string;
  trailing?: JSX.Element | null;
  children: JSX.Element;
  style?: ViewStyle;
}): JSX.Element {
  return (
    <View style={{ display: "flex", flexDirection: "column", gap: 6, minWidth: 0, ...props.style }}>
      <View
        style={{
          display: "flex",
          flexDirection: "row",
          alignItems: "center",
          justifyContent: "space-between",
          gap: 8,
          minWidth: 0,
        }}
      >
        <Text style={{ fontSize: 12, fontWeight: "500", color: paletteToken("fg_muted") }}>
          {props.title}
        </Text>
        {props.trailing ?? null}
      </View>
      {props.children}
    </View>
  );
}

/** 统一设置开关（开启 ok 绿，关闭 edge，滑块 on_accent 白，回弹落位）。 */
export function SettingsSwitch(props: {
  id?: string;
  testID?: string;
  checked: boolean;
  disabled?: boolean;
  onToggle: () => void;
}): JSX.Element {
  const trackW = 38;
  const trackH = 22;
  const thumb = 16;
  const pad = 3;
  const off = () => props.disabled === true;
  return (
    <View
      testID={props.testID ?? props.id}
      role="switch"
      accessibilityState={{ checked: props.checked, disabled: off() }}
      style={{
        width: trackW,
        height: trackH,
        borderRadius: trackH / 2,
        backgroundColor: props.checked ? paletteToken("ok") : paletteToken("edge"),
        display: "flex",
        alignItems: "center",
        padding: pad,
        flexShrink: 0,
        cursor: off() ? "default" : "pointer",
        opacity: off() ? 0.45 : 1,
      }}
      hoverStyle={off() ? undefined : { opacity: 0.9 }}
      onPress={() => {
        if (!off()) props.onToggle();
      }}
    >
      <View
        style={{
          width: thumb,
          height: thumb,
          borderRadius: thumb / 2,
          backgroundColor: paletteToken("on_accent"),
          transform: [{ translateX: props.checked ? trackW - thumb - pad * 2 : 0 }],
          ...settleTransition(),
        }}
      />
    </View>
  );
}

/** 统一动作按钮（零帧 hoverStyle / activeStyle，支持 IconSpin）。 */
export function SettingsButton(props: {
  testID?: string;
  label: string;
  icon?: IconName;
  spinning?: boolean;
  variant?: ButtonVariant;
  size?: "sm" | "md";
  disabled?: boolean;
  busy?: boolean;
  onPress: () => void;
}): JSX.Element {
  const tokens = () => BUTTON_MAP[props.variant ?? "secondary"];
  const blocked = () => props.disabled === true || props.busy === true;
  const turning = () => props.spinning === true || (props.busy === true && props.spinning !== false);
  const h = () => (props.size === "sm" ? 30 : 34);
  return (
    <View
      testID={props.testID}
      role="button"
      accessibilityState={{
        disabled: props.disabled === true,
        busy: props.busy === true || props.spinning === true,
      }}
      style={{
        height: h(),
        paddingHorizontal: props.size === "sm" ? 11 : 13,
        display: "flex",
        flexDirection: "row",
        alignItems: "center",
        gap: 6,
        flexShrink: 0,
        borderRadius: 9,
        borderWidth: 1,
        borderColor: paletteToken(tokens().border),
        backgroundColor: paletteToken(tokens().bg),
        opacity: props.disabled === true ? 0.6 : 1,
        cursor: blocked() ? "default" : "pointer",
      }}
      hoverStyle={blocked() ? undefined : { backgroundColor: paletteToken(tokens().hoverBg) }}
      activeStyle={blocked() ? undefined : { backgroundColor: paletteToken(tokens().activeBg) }}
      onPress={() => {
        if (!blocked()) props.onPress();
      }}
    >
      <If when={props.icon !== undefined || turning()}>
        <IconSpin
          name={props.icon ?? "loader"}
          size={props.size === "sm" ? 12 : 14}
          color={paletteToken(tokens().icon)}
          spin={turning()}
        />
      </If>
      <Text
        style={{
          fontSize: 12,
          fontWeight: "500",
          color: paletteToken(tokens().fg),
          numberOfLines: 1,
        }}
      >
        {props.label}
      </Text>
    </View>
  );
}

/** 分组卡片：16px 圆角卡面 + 可选内嵌卡片标题头。 */
export function SettingsGroup(props: {
  title?: string;
  trailing?: JSX.Element | null;
  style?: ViewStyle;
  children: JSX.Element;
}): JSX.Element {
  return (
    <View style={card(props.style)}>
      <If when={Boolean(props.title)}>
        <View
          style={{
            paddingHorizontal: 18,
            paddingVertical: 11,
            borderBottomWidth: 1,
            borderBottomColor: paletteToken("border_soft"),
            backgroundColor: paletteToken("panel"),
            display: "flex",
            flexDirection: "row",
            alignItems: "center",
            justifyContent: "space-between",
            gap: 12,
            width: "100%",
          }}
        >
          <Text
            style={{
              fontSize: 13,
              fontWeight: "600",
              color: paletteToken("fg"),
              numberOfLines: 1,
            }}
          >
            {props.title}
          </Text>
          {props.trailing ?? null}
        </View>
      </If>
      {props.children}
    </View>
  );
}

/** 水平设置行：可选行首图标底座 + 左标题与说明 + 右控件 + 底部扩展区。 */
export function SettingRow(props: {
  icon?: IconName;
  iconTone?: StatusTone;
  title: string;
  hint?: string;
  borderTop?: boolean;
  control: JSX.Element;
  children?: JSX.Element | null;
}): JSX.Element {
  const toneTokens = () => TONE_MAP[props.iconTone ?? "neutral"];
  return (
    <View
      style={{
        display: "flex",
        flexDirection: "column",
        width: "100%",
        ...(props.borderTop
          ? { borderTopWidth: 1, borderTopColor: paletteToken("border_soft") }
          : {}),
      }}
    >
      <View
        style={{
          paddingHorizontal: 18,
          paddingVertical: 15,
          display: "flex",
          flexDirection: "row",
          alignItems: "center",
          justifyContent: "space-between",
          gap: 16,
          width: "100%",
        }}
      >
        <View
          style={{
            display: "flex",
            flexDirection: "row",
            alignItems: "center",
            gap: 12,
            flex: 1,
            minWidth: 0,
          }}
        >
          <If when={Boolean(props.icon)}>
            <View
              style={{
                width: 32,
                height: 32,
                borderRadius: 9,
                flexShrink: 0,
                borderWidth: 1,
                borderColor: paletteToken(toneTokens().border),
                backgroundColor: paletteToken(toneTokens().bg),
                display: "flex",
                alignItems: "center",
                justifyContent: "center",
              }}
            >
              <Icon name={props.icon!} size={15} color={paletteToken(toneTokens().icon)} />
            </View>
          </If>
          <View
            style={{
              display: "flex",
              flexDirection: "column",
              gap: 2,
              flex: 1,
              minWidth: 0,
            }}
          >
            <Text style={{ fontSize: 14, fontWeight: "500", color: paletteToken("fg") }}>
              {props.title}
            </Text>
            <If when={Boolean(props.hint)}>
              <Text style={{ fontSize: 12, color: paletteToken("fg_muted"), lineHeight: 18 }}>
                {props.hint}
              </Text>
            </If>
          </View>
        </View>
        <View style={{ flexShrink: 0 }}>{props.control}</View>
      </View>
      {props.children ?? null}
    </View>
  );
}

/**
 * 分区外壳：36×36 图标底座 + 18px 标题与导语副标题 + 可选 meta/action + 单卡或分组卡片栈。
 */
export function SectionShell(props: {
  section: SettingsSectionEntry | StandaloneSectionEntry;
  meta?: JSX.Element | null;
  action?: JSX.Element | null;
  grouped?: boolean;
  bodyStyle?: ViewStyle;
  children: JSX.Element;
}): JSX.Element {
  const section = props.section;
  return (
    <View style={{ width: "100%", flexShrink: 0, display: "flex", flexDirection: "column" }}>
      <View
        style={{
          marginBottom: 16,
          paddingHorizontal: 2,
          display: "flex",
          flexDirection: "row",
          alignItems: "center",
          justifyContent: "space-between",
          gap: 12,
          width: "100%",
        }}
      >
        <View
          style={{
            display: "flex",
            flexDirection: "row",
            alignItems: "center",
            gap: 12,
            minWidth: 0,
            flex: 1,
          }}
        >
          <View
            style={{
              width: 36,
              height: 36,
              borderRadius: 10,
              flexShrink: 0,
              borderWidth: 1,
              borderColor: paletteToken("accent_soft_edge"),
              backgroundColor: paletteToken("accent_soft"),
              display: "flex",
              alignItems: "center",
              justifyContent: "center",
            }}
          >
            <Icon name={section.icon} size={18} color={paletteToken("accent")} />
          </View>
          <View
            style={{
              display: "flex",
              flexDirection: "column",
              gap: 2,
              minWidth: 0,
              flex: 1,
            }}
          >
            <View
              style={{
                display: "flex",
                flexDirection: "row",
                alignItems: "center",
                gap: 8,
                minWidth: 0,
              }}
            >
              <Text
                style={{
                  fontSize: 18,
                  fontWeight: "600",
                  color: paletteToken("fg"),
                  numberOfLines: 1,
                }}
              >
                {st(section.labelKey)}
              </Text>
              {props.meta ?? null}
            </View>
            {section.subtitleKey ? (
              <Text
                style={{
                  fontSize: 12,
                  color: paletteToken("fg_muted"),
                  lineHeight: 18,
                  numberOfLines: 2,
                }}
              >
                {st(section.subtitleKey)}
              </Text>
            ) : null}
          </View>
        </View>
        {props.action ? <View style={{ flexShrink: 0 }}>{props.action}</View> : null}
      </View>
      {props.grouped === true ? (
        <View
          style={{
            width: "100%",
            display: "flex",
            flexDirection: "column",
            gap: 16,
            ...props.bodyStyle,
          }}
        >
          {props.children}
        </View>
      ) : (
        <View style={card(props.bodyStyle)}>{props.children}</View>
      )}
    </View>
  );
}

/**
 * 原地变色分段 pills（更新检查频率等互斥选择）。
 * selected 判断在 JSX 属性表达式里求值（遵守 nsh 组件体只跑一次纪律）。
 */
export function ChoicePills(props: {
  idPrefix: string;
  options: { id: string; label: string }[];
  current: string;
  onPick: (id: string) => void;
}): JSX.Element {
  return (
    <View
      style={{
        display: "flex",
        flexDirection: "row",
        flexWrap: "wrap",
        gap: 4,
        padding: 3,
        borderRadius: 10,
        borderWidth: 1,
        borderColor: paletteToken("border_soft"),
        backgroundColor: paletteToken("well"),
      }}
    >
      {props.options.map((option) => (
        <ChoicePill
          testID={`${props.idPrefix}-${option.id}`}
          label={option.label}
          selected={props.current === option.id || props.current.startsWith(option.id)}
          onPick={() => props.onPick(option.id)}
        />
      ))}
    </View>
  );
}

function ChoicePill(props: {
  testID: string;
  label: string;
  selected: boolean;
  onPick: () => void;
}): JSX.Element {
  return (
    <View
      testID={props.testID}
      role="button"
      style={{
        height: 30,
        paddingHorizontal: 12,
        display: "flex",
        alignItems: "center",
        justifyContent: "center",
        borderRadius: 7,
        borderWidth: 1,
        borderColor: props.selected ? paletteToken("accent_soft_edge") : "transparent",
        backgroundColor: props.selected ? paletteToken("card_active") : "transparent",
        cursor: "pointer",
      }}
      hoverStyle={
        props.selected
          ? undefined
          : {
              backgroundColor: paletteToken("panel_hover"),
            }
      }
      onPress={props.onPick}
    >
      <Text
        style={{
          fontSize: 12,
          fontWeight: props.selected ? "600" : "400",
          color: props.selected ? paletteToken("accent_fg") : paletteToken("fg_muted"),
        }}
      >
        {props.label}
      </Text>
    </View>
  );
}
