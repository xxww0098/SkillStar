import { useCallback } from "react";
import type { LayoutNode, PaneNode, SplitDirection } from "./useLaunchConfig";

let paneCounter = 0;
function nextPaneId(): string {
  paneCounter += 1;
  return `pane-${paneCounter}`;
}

/** Reset pane counter (for testing) */
export function resetPaneCounter() {
  paneCounter = 0;
}

// ── Immutable tree operations ───────────────────────────────────────

/** Split a pane into two children. */
export function splitPane(tree: LayoutNode, paneId: string, direction: SplitDirection): LayoutNode {
  if (tree.type === "pane") {
    if (tree.id === paneId) {
      const newPane: PaneNode = {
        type: "pane",
        id: nextPaneId(),
        agentId: "",
        safeMode: false,
        extraArgs: [],
      };
      return {
        type: "split",
        direction,
        ratio: 0.5,
        children: [{ ...tree }, newPane],
      };
    }
    return tree;
  }

  return {
    ...tree,
    children: [splitPane(tree.children[0], paneId, direction), splitPane(tree.children[1], paneId, direction)],
  };
}

/** Remove a pane from the tree, promoting its sibling. */
export function removePane(tree: LayoutNode, paneId: string): LayoutNode | null {
  if (tree.type === "pane") {
    return tree.id === paneId ? null : tree;
  }

  const [left, right] = tree.children;

  // Check if the target pane is a direct child
  if (left.type === "pane" && left.id === paneId) return right;
  if (right.type === "pane" && right.id === paneId) return left;

  // Recurse
  const newLeft = removePane(left, paneId);
  const newRight = removePane(right, paneId);

  if (newLeft === null) return newRight;
  if (newRight === null) return newLeft;

  return { ...tree, children: [newLeft, newRight] };
}

/** Update the ratio of a split node containing a given pane. */
export function resizeAtPane(tree: LayoutNode, paneId: string, newRatio: number): LayoutNode {
  const clamped = Math.max(0.15, Math.min(0.85, newRatio));

  if (tree.type === "pane") return tree;

  const [left, right] = tree.children;

  // If either direct child is the target pane, update this split's ratio
  const leftContains = containsPane(left, paneId);
  const rightContains = containsPane(right, paneId);

  if (leftContains || rightContains) {
    // Only update if we're at the split that directly has the pane as one of the children
    if ((left.type === "pane" && left.id === paneId) || (right.type === "pane" && right.id === paneId)) {
      return { ...tree, ratio: clamped, children: [left, right] };
    }
  }

  return {
    ...tree,
    children: [resizeAtPane(left, paneId, newRatio), resizeAtPane(right, paneId, newRatio)],
  };
}

/** Assign an agent to a pane. */
export function assignAgent(
  tree: LayoutNode,
  paneId: string,
  agentId: string,
  providerId?: string,
  providerName?: string,
  modelId?: string,
): LayoutNode {
  if (tree.type === "pane") {
    if (tree.id === paneId) {
      return { ...tree, agentId, providerId, providerName, modelId };
    }
    return tree;
  }

  return {
    ...tree,
    children: [
      assignAgent(tree.children[0], paneId, agentId, providerId, providerName, modelId),
      assignAgent(tree.children[1], paneId, agentId, providerId, providerName, modelId),
    ],
  };
}

/** Check if a tree contains a pane with the given id. */
function containsPane(tree: LayoutNode, paneId: string): boolean {
  if (tree.type === "pane") return tree.id === paneId;
  return containsPane(tree.children[0], paneId) || containsPane(tree.children[1], paneId);
}

/** Count total panes in a tree. */
export function countPanes(tree: LayoutNode): number {
  if (tree.type === "pane") return 1;
  return countPanes(tree.children[0]) + countPanes(tree.children[1]);
}

/** Get the first pane in the tree (DFS left-first). */
export function firstPane(tree: LayoutNode): PaneNode {
  if (tree.type === "pane") return tree;
  return firstPane(tree.children[0]);
}

// ── React Hook ──────────────────────────────────────────────────────

export function useLayoutTree(tree: LayoutNode | null, onUpdate: (tree: LayoutNode) => void) {
  const split = useCallback(
    (paneId: string, direction: SplitDirection) => {
      if (!tree) return;
      onUpdate(splitPane(tree, paneId, direction));
    },
    [tree, onUpdate],
  );

  const remove = useCallback(
    (paneId: string) => {
      if (!tree) return;
      const result = removePane(tree, paneId);
      if (result) onUpdate(result);
    },
    [tree, onUpdate],
  );

  const resize = useCallback(
    (paneId: string, ratio: number) => {
      if (!tree) return;
      onUpdate(resizeAtPane(tree, paneId, ratio));
    },
    [tree, onUpdate],
  );

  const assign = useCallback(
    (paneId: string, agentId: string, providerId?: string, providerName?: string, modelId?: string) => {
      if (!tree) return;
      onUpdate(assignAgent(tree, paneId, agentId, providerId, providerName, modelId));
    },
    [tree, onUpdate],
  );

  return { split, remove, resize, assign };
}
