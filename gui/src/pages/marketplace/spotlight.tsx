import { Overlay, Text, View, effect, keymap, signal, type ViewRef, type JSX } from "nsh";
import { Input } from "nsh/base";
import { paletteToken } from "../../services/themeMode";
import { Icon } from "../../chrome/icons";
import { Tip } from "../../chrome/tooltip";
import { currentPage } from "../../shell/nav";
import { mt } from "./marketStrings";
import type { MarketSkill } from "./types";

// ⌘F spotlight — spotlight.rs 对位。
//
// 触发器住在工具栏搜索位：未激活显示放大镜 + ⌘F 键帽 chip；激活（query
// 非空）显示截断的当前词 + 清除 X。点开浮层：输入框 + 最多 8 条建议
// （当前列表按前缀过滤），选中一行 = query 设为该名并立即原位重载
// （load_now(false)）。⌘F 只在市场页是当前页时开浮层（GPUI 的
// track_focus 是焦点域，这里用页面判定等效）。
//
// 浮层是自绘下拉（弹层用卡片色——视觉契约），点透明幕/再点触发器/选中
// 行/Esc 收起。建议多于 9 行时列表内滚动（固定高度视口框，不给滚动包
// 装写 max_h）。

/** 建议行超过此数浮层内部滚动（spotlight.rs SPOTLIGHT_VISIBLE）。 */
const SPOTLIGHT_VISIBLE = 9;
/** 建议列表视口高（spotlight.rs SPOTLIGHT_LIST_H）。 */
const SPOTLIGHT_LIST_H = 300;

export const spotlightOpen = signal(false);

/** ⌘F 绑定：仅市场页激活（页面挂载即绑定，keep-alive 一次）。 */
export function bindSpotlightKeys(onOpen: () => void): () => void {
  return keymap.bind(
    {
      "mod+f": () => {
        if (currentPage() === "marketplace") onOpen();
      },
      escape: () => {
        if (currentPage() === "marketplace" && spotlightOpen()) spotlightOpen.set(false);
      },
    },
    { when: "always" }
  );
}

/** spotlight 建议行（纯函数，测试直接断言）：当前技能按词过滤取前 8。 */
export function spotlightNames(skills: MarketSkill[], query: string): string[] {
  const needle = query.trim().toLowerCase();
  return skills
    .filter((skill) => needle.length === 0 || skill.name.toLowerCase().includes(needle))
    .slice(0, 8)
    .map((skill) => skill.name);
}

export interface SpotlightProps {
  query: string;
  skills: MarketSkill[];
  onPanelInput: (text: string) => void;
  onPick: (name: string) => void;
  onClear: () => void;
}

/** 触发器 + 浮层宿主。挂进 PageBar 的 search 槽。浮层走原生 Overlay：
 * 面板以触发器 ref 为锚（bottom/start，间距 6），透明幕是不带锚的
 * fixed Overlay 垫在它下面（同为 root 层，后挂的面板压住幕），点外收
 * 起。Overlay 在 root 视图之上，普通 zIndex 层级压不住它。 */
export function Spotlight(props: SpotlightProps): JSX.Element {
  return (
    <SpotlightHost
      query={props.query}
      skills={props.skills}
      onPanelInput={props.onPanelInput}
      onPick={props.onPick}
      onClear={props.onClear}
    />
  );
}

const triggerRef = signal<ViewRef | null>(null);

function SpotlightHost(props: SpotlightProps): JSX.Element {
  return (
    <View style={{ position: "relative", display: "flex" }}>
      <View
        ref={(node) => {
          triggerRef.set(node);
        }}
      >
        <SpotlightTrigger query={props.query} onClear={props.onClear} />
      </View>
      <If when={spotlightOpen()}>
        <Overlay style={{ top: 0, left: 0, right: 0, bottom: 0 }}>
          <View
            testID="mk-spotlight-veil"
            style={{ position: "absolute", top: 0, left: 0, right: 0, bottom: 0 }}
            onPress={() => spotlightOpen.set(false)}
          />
        </Overlay>
        <Overlay anchor={triggerRef()} placement="bottom" align="start" offset={6}>
          <SpotlightPanelBody
            query={props.query}
            skills={props.skills}
            onPanelInput={props.onPanelInput}
            onPick={props.onPick}
          />
        </Overlay>
      </If>
    </View>
  );
}

/** 触发器。注意：nsh 组件体只执行一次，active 的派生读取全部落在 JSX
 * 表达式里（波 1 的 NoticeBand 教训——体内 const 读取后分支会卡死）。 */
function SpotlightTrigger(props: { query: string; onClear: () => void }): JSX.Element {
  return (
    <View
      testID="mk-spotlight-trigger"
      tooltip={<Tip text={mt("marketplace.spotlightTitle")} />}
      style={{
        display: "flex",
        flexDirection: "row",
        alignItems: "center",
        height: 32,
        gap: 6,
        paddingHorizontal: 10,
        borderRadius: 10,
        cursor: "pointer",
        borderWidth: 1,
        borderColor:
          props.query.trim().length > 0 ? paletteToken("accent") : paletteToken("border"),
        backgroundColor:
          props.query.trim().length > 0 ? paletteToken("accent_soft") : paletteToken("bg"),
      }}
      hoverStyle={{ backgroundColor: paletteToken("panel_hover") }}
      onPress={() => spotlightOpen.set(!spotlightOpen())}
    >
      <Icon
        name="search"
        size={14}
        color={props.query.trim().length > 0 ? paletteToken("accent") : paletteToken("fg")}
      />
      <If
        when={props.query.trim().length > 0}
        else={
          <View
            style={{
              paddingHorizontal: 6,
              borderRadius: 8,
              borderWidth: 1,
              borderColor: paletteToken("border"),
              backgroundColor: paletteToken("well"),
            }}
          >
            <Text
              style={{
                fontSize: 10,
                color: paletteToken("fg"),
                fontFamily: "monospace",
                fontWeight: "700",
              }}
            >
              ⌘F
            </Text>
          </View>
        }
      >
        <View style={{ maxWidth: 88, overflow: "hidden" }}>
          <Text
            style={{
              fontSize: 12,
              color: props.query.trim().length > 0 ? paletteToken("accent") : paletteToken("fg"),
              numberOfLines: 1,
            }}
          >
            {props.query.trim()}
          </Text>
        </View>
      </If>
      <If when={props.query.trim().length > 0}>
        <View
          testID="mk-spotlight-clear"
          style={{
            display: "flex",
            alignItems: "center",
            justifyContent: "center",
            padding: 4,
            marginRight: 4,
            borderRadius: 4,
            cursor: "pointer",
          }}
          hoverStyle={{ backgroundColor: paletteToken("panel_hover") }}
          onPress={() => {
            spotlightOpen.set(false);
            props.onClear();
          }}
        >
          <Icon name="x" size={12} color={paletteToken("accent")} />
        </View>
      </If>
    </View>
  );
}

function SpotlightPanelBody(props: {
  query: string;
  skills: MarketSkill[];
  onPanelInput: (text: string) => void;
  onPick: (name: string) => void;
}): JSX.Element {
  return (
    // 面板卡（Overlay 已锚定在触发器下方）：卡片色，自身吞点击。建议行
    // 的派生在 JSX 里现算（组件体只执行一次）。
    <View
      testID="mk-spotlight-panel"
      style={{
        display: "flex",
        flexDirection: "column",
        gap: 4,
        width: 320,
        padding: 8,
        borderRadius: 14,
        borderWidth: 1,
        borderColor: paletteToken("border"),
        backgroundColor: paletteToken("card"),
        boxShadow: "0px 12px 24px -6px rgba(0,0,0,0.18)",
      }}
      onPress={() => {
        // 面板吞掉点击，不落幕。
      }}
    >
      <Input
        testID="mk-spotlight-input"
        value={props.query}
        placeholder={mt("marketplace.searchPlaceholder")}
        onChange={(e) => props.onPanelInput(e.value)}
        style={{
          fontSize: 12,
          color: paletteToken("fg"),
          backgroundColor: paletteToken("input"),
        }}
      />
      <If
        when={spotlightNames(props.skills, props.query).length > 0}
        else={<MutedLine text={mt("marketplace.spotlightEmpty")} />}
      >
        <SpotlightRows
          names={spotlightNames(props.skills, props.query)}
          onPick={props.onPick}
        />
      </If>
    </View>
  );
}

/** 建议行列表：map 直接在 JSX 表达式里（行随 props.names 响应式更新）。
 * 超过视口行数时套固定高度视口框内部滚动（不写 max_h）。 */
function SpotlightRows(props: { names: string[]; onPick: (name: string) => void }): JSX.Element {
  const rows = () => (
    <View style={{ display: "flex", flexDirection: "column", gap: 4 }}>
      {props.names.map((name, index) => (
        <View
          testID={`mk-spot-${index}`}
          style={{
            paddingHorizontal: 8,
            paddingVertical: 4,
            borderRadius: 8,
            cursor: "pointer",
          }}
          hoverStyle={{ backgroundColor: paletteToken("panel_hover") }}
          onPress={() => {
            spotlightOpen.set(false);
            props.onPick(name);
          }}
        >
          <Text style={{ fontSize: 12, color: paletteToken("fg"), numberOfLines: 1 }}>
            {name}
          </Text>
        </View>
      ))}
    </View>
  );
  return (
    <If
      when={props.names.length > SPOTLIGHT_VISIBLE}
      else={rows()}
    >
      <View style={{ height: SPOTLIGHT_LIST_H, width: "100%", flexShrink: 0 }}>{rows()}</View>
    </If>
  );
}

function MutedLine(props: { text: string }): JSX.Element {
  return (
    <Text style={{ fontSize: 12, color: paletteToken("fg_muted"), paddingHorizontal: 8 }}>
      {props.text}
    </Text>
  );
}

// keep-alive 页面只挂载一次。键绑定由页面组件体调用本函数注册：
// keymap 文档语义——组件渲染期建立的绑定随组件移除（effect 兜底）。
export function useSpotlightKeys(): void {
  effect(() => {
    return bindSpotlightKeys(() => spotlightOpen.set(true));
  });
}
