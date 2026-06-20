import { useCallback, useEffect } from "react";

interface SkillsSelectionShortcutsOptions {
  /** Whether anything is currently selected (gates all shortcuts) */
  hasSelection: boolean;
  /** Whether a batch operation / dialog blocks shortcuts */
  disabled?: boolean;
  /** Whether the "Link to Agent" menu is open (Esc closes it first) */
  linkMenuOpen?: boolean;
  onClear: () => void;
  onSelectAll: () => void;
  onToggleLinkMenu?: () => void;
  onCloseLinkMenu?: () => void;
  onUnlinkAll?: () => void;
  onDeploy: () => void;
  onUninstall?: () => void;
}

/**
 * Contextual single-letter shortcuts active only while a skill selection is
 * present (Finder/Gmail-style). Global ⌘-prefixed shortcuts live in
 * useKeyboardShortcuts; these are plain letters that never fire in inputs.
 *
 * Esc       → clear selection (exit selection mode)
 * A         → select all / deselect all
 * L         → open the "Link to Agent" menu
 * U         → unlink selected skills from all agents
 * Enter     → deploy (primary CTA)
 * Backspace → uninstall
 */
export function useSkillsSelectionShortcuts({
  hasSelection,
  disabled,
  linkMenuOpen,
  onClear,
  onSelectAll,
  onToggleLinkMenu,
  onCloseLinkMenu,
  onUnlinkAll,
  onDeploy,
  onUninstall,
}: SkillsSelectionShortcutsOptions) {
  const handleKeyDown = useCallback(
    (e: KeyboardEvent) => {
      if (!hasSelection || disabled) return;

      // Any modifier (⌘/Ctrl/Alt) → defer to global shortcuts.
      if (e.metaKey || e.ctrlKey || e.altKey) return;

      const target = e.target as HTMLElement;
      const isInput =
        target.tagName === "INPUT" ||
        target.tagName === "TEXTAREA" ||
        target.isContentEditable ||
        target.tagName === "SELECT";
      if (isInput) return;

      // Layered Esc: close the open Link menu first, then clear selection.
      if (e.key === "Escape") {
        if (linkMenuOpen && onCloseLinkMenu) {
          e.preventDefault();
          onCloseLinkMenu();
          return;
        }
        e.preventDefault();
        onClear();
        return;
      }

      // While the Link menu is open, swallow the other selection shortcuts so
      // they don't accidentally fire through the menu overlay.
      if (linkMenuOpen) return;

      switch (e.key) {
        case "a":
        case "A":
          e.preventDefault();
          onSelectAll();
          break;
        case "l":
        case "L":
          if (onToggleLinkMenu) {
            e.preventDefault();
            onToggleLinkMenu();
          }
          break;
        case "u":
        case "U":
          if (onUnlinkAll) {
            e.preventDefault();
            onUnlinkAll();
          }
          break;
        case "Enter":
          e.preventDefault();
          onDeploy();
          break;
        case "Backspace":
          if (onUninstall) {
            e.preventDefault();
            onUninstall();
          }
          break;
      }
    },
    [
      hasSelection,
      disabled,
      linkMenuOpen,
      onClear,
      onSelectAll,
      onToggleLinkMenu,
      onCloseLinkMenu,
      onUnlinkAll,
      onDeploy,
      onUninstall,
    ],
  );

  useEffect(() => {
    window.addEventListener("keydown", handleKeyDown);
    return () => window.removeEventListener("keydown", handleKeyDown);
  }, [handleKeyDown]);
}
