import { memo } from "react";
import type { AgentCliInfo } from "../hooks/useAgentClis";
import type { LayoutNode, SplitDirection } from "../hooks/useLaunchConfig";
import { countPanes } from "../hooks/useLayoutTree";
import { PaneCell } from "./PaneCell";
import { SplitHandle } from "./SplitHandle";

interface PaneLayoutEditorProps {
  node: LayoutNode;
  agents: AgentCliInfo[];
  isMulti: boolean;
  onSplit: (paneId: string, direction: SplitDirection) => void;
  onAssign: (paneId: string, agentId: string) => void;
  onRemove: (paneId: string) => void;
  onResize: (paneId: string, ratio: number) => void;
}

/** Recursive layout renderer — renders the binary tree as nested flex containers. */
export const PaneLayoutEditor = memo(function PaneLayoutEditor({
  node,
  agents,
  isMulti,
  onSplit,
  onAssign,
  onRemove,
  onResize,
}: PaneLayoutEditorProps) {
  if (node.type === "pane") {
    return (
      <div className="group flex-1 min-w-0 min-h-0">
        <PaneCell
          pane={node}
          agents={agents}
          isMulti={isMulti}
          canRemove={false}
          onSplit={onSplit}
          onAssign={onAssign}
          onRemove={onRemove}
        />
      </div>
    );
  }

  const isHorizontal = node.direction === "h";
  const leftChild = node.children[0];
  const rightChild = node.children[1];

  // Find first pane ID in each child tree for resize callback
  const firstPaneIdLeft = findFirstPaneId(leftChild);

  return (
    <div
      className={`flex flex-1 min-w-0 min-h-0 gap-0 ${isHorizontal ? "flex-row" : "flex-col"}`}
      style={{
        ["--split-ratio" as string]: String(node.ratio),
      }}
    >
      <div
        className="min-w-0 min-h-0 flex"
        style={{
          flex: `calc(var(--split-ratio, ${node.ratio}) * 100) 0 0`,
        }}
      >
        <LayoutNodeRenderer
          node={leftChild}
          agents={agents}
          isMulti={isMulti}
          rootNode={node}
          onSplit={onSplit}
          onAssign={onAssign}
          onRemove={onRemove}
          onResize={onResize}
        />
      </div>
      <SplitHandle
        direction={node.direction}
        onRatioChange={(ratio) => {
          if (firstPaneIdLeft) onResize(firstPaneIdLeft, ratio);
        }}
      />
      <div
        className="min-w-0 min-h-0 flex"
        style={{
          flex: `calc((1 - var(--split-ratio, ${node.ratio})) * 100) 0 0`,
        }}
      >
        <LayoutNodeRenderer
          node={rightChild}
          agents={agents}
          isMulti={isMulti}
          rootNode={node}
          onSplit={onSplit}
          onAssign={onAssign}
          onRemove={onRemove}
          onResize={onResize}
        />
      </div>
    </div>
  );
});

/** Inner recursive renderer that knows about the parent context. */
const LayoutNodeRenderer = memo(function LayoutNodeRenderer({
  node,
  agents,
  isMulti,
  rootNode,
  onSplit,
  onAssign,
  onRemove,
  onResize,
}: PaneLayoutEditorProps & { rootNode: LayoutNode }) {
  if (node.type === "pane") {
    const totalPanes = countPanes(rootNode);
    return (
      <div className="group flex-1 min-w-0 min-h-0">
        <PaneCell
          pane={node}
          agents={agents}
          isMulti={isMulti}
          canRemove={isMulti && totalPanes > 1}
          onSplit={onSplit}
          onAssign={onAssign}
          onRemove={onRemove}
        />
      </div>
    );
  }

  return (
    <PaneLayoutEditor
      node={node}
      agents={agents}
      isMulti={isMulti}
      onSplit={onSplit}
      onAssign={onAssign}
      onRemove={onRemove}
      onResize={onResize}
    />
  );
});

/** Find the first pane ID in a layout tree (DFS). */
function findFirstPaneId(node: LayoutNode): string | null {
  if (node.type === "pane") return node.id;
  return findFirstPaneId(node.children[0]);
}
