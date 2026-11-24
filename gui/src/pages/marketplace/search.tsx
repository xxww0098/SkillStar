import { effect, keymap, signal, type InputRef, type JSX } from "nsh";
import { BarSearchInput } from "../../chrome/toolbar";
import { currentPage } from "../../shell/nav";
import { mt } from "./marketStrings";

// 市场工具栏搜索框 — 与技能页同一个原语（chrome/toolbar.tsx 的
// BarSearchInput）：放大镜 + 占位词 + 即时清除 X，190 宽、32 高、10 圆角，
// 不再自绘触发器或键帽。输入直接落在 readSide.query 上，由状态机做 200ms
// 防抖读取（与发布者页同一条路径）；清空走 clearSearch，回全量。
//
// ⌘F 保留为「把焦点交给搜索框」（nsh 的 Input 经 ref 聚焦）：只在市场页是
// 当前页时触发，页面挂载即绑定（keep-alive 一次）。

/** 搜索框的输入节点：键盘快捷键把焦点交给它。 */
const marketSearchRef = signal<InputRef | null>(null);

export interface MarketSearchProps {
  query: string;
  onChange: (text: string) => void;
  onClear: () => void;
}

export function MarketSearch(props: MarketSearchProps): JSX.Element {
  return (
    <BarSearchInput
      boxTestID="mk-search"
      inputTestID="mk-search-input"
      clearTestID="mk-search-clear"
      value={props.query}
      placeholder={mt("marketplace.searchPlaceholder")}
      onChange={(next) => props.onChange(next)}
      onClear={props.onClear}
      inputRef={(node) => marketSearchRef.set(node)}
    />
  );
}

/** ⌘F 绑定：仅市场页是当前页时聚焦搜索框。 */
export function bindMarketSearchKeys(): () => void {
  return keymap.bind(
    {
      "mod+f": () => {
        if (currentPage() === "marketplace") marketSearchRef()?.focus();
      },
    },
    { when: "always" }
  );
}

// keep-alive 页面只挂载一次。键绑定由页面组件体调用本函数注册：
// keymap 文档语义——组件渲染期建立的绑定随组件移除（effect 兜底）。
export function useMarketSearchKeys(): void {
  effect(() => {
    return bindMarketSearchKeys();
  });
}
