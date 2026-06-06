import { AlertDialog, ContextMenu } from "radix-ui";
import { useCallback, useState } from "react";
import { toast } from "sonner";
import { cn } from "../../../lib/utils";
import type { ProviderEntryFlat } from "../../../types";

// ---------------------------------------------------------------------------
// Props
// ---------------------------------------------------------------------------

export interface ProviderContextMenuProps {
  children: React.ReactNode;
  provider: ProviderEntryFlat;
  onActivate: (toolId: string) => Promise<void>;
  onActivateAll: () => Promise<void>;
  onDuplicate: () => Promise<void>;
  onDelete: () => void; // opens confirmation dialog
}

export interface DeleteConfirmDialogProps {
  open: boolean;
  onOpenChange: (open: boolean) => void;
  providerName: string;
  affectedTools: string[]; // tool names that will be deactivated
  onConfirm: () => void;
}

// ---------------------------------------------------------------------------
// Tool display names
// ---------------------------------------------------------------------------

const TOOL_DISPLAY_NAMES: Record<string, string> = {
  "claude-code": "Claude",
  codex: "Codex",
};

// ---------------------------------------------------------------------------
// ProviderContextMenu
// ---------------------------------------------------------------------------

export function ProviderContextMenu({
  children,
  provider,
  onActivate,
  onActivateAll,
  onDuplicate,
  onDelete,
}: ProviderContextMenuProps) {
  const [activating, setActivating] = useState(false);

  const handleActivate = useCallback(
    async (toolId: string) => {
      if (activating) return;
      setActivating(true);
      try {
        await onActivate(toolId);
        toast.success(`已将 ${provider.name} 应用到 ${TOOL_DISPLAY_NAMES[toolId] ?? toolId}`);
      } catch (err) {
        const message = err instanceof Error ? err.message : String(err);
        toast.error(`应用失败: ${message}`);
      } finally {
        setActivating(false);
      }
    },
    [activating, onActivate, provider.name],
  );

  const handleActivateAll = useCallback(async () => {
    if (activating) return;
    setActivating(true);
    try {
      await onActivateAll();
      toast.success(`已将 ${provider.name} 应用到全部工具`);
    } catch (err) {
      const message = err instanceof Error ? err.message : String(err);
      toast.error(`应用失败: ${message}`);
    } finally {
      setActivating(false);
    }
  }, [activating, onActivateAll, provider.name]);

  const handleDuplicate = useCallback(async () => {
    try {
      await onDuplicate();
      toast.success(`已复制 ${provider.name}`);
    } catch (err) {
      const message = err instanceof Error ? err.message : String(err);
      toast.error(`复制失败: ${message}`);
    }
  }, [onDuplicate, provider.name]);

  return (
    <ContextMenu.Root>
      <ContextMenu.Trigger asChild>{children}</ContextMenu.Trigger>
      <ContextMenu.Portal>
        <ContextMenu.Content
          className={cn(
            "z-50 min-w-[180px] overflow-hidden rounded-xl border border-border/60",
            "bg-popover/95 backdrop-blur-md p-1 shadow-lg",
            "animate-in fade-in-0 zoom-in-95 data-[state=closed]:animate-out data-[state=closed]:fade-out-0 data-[state=closed]:zoom-out-95",
          )}
        >
          {/* Activate to Claude Code */}
          <ContextMenu.Item
            className={cn(
              "relative flex items-center rounded-lg px-2.5 py-1.5 text-[12px] text-foreground",
              "outline-none select-none cursor-pointer",
              "data-[highlighted]:bg-accent/10 data-[highlighted]:text-accent-foreground",
              "data-[disabled]:pointer-events-none data-[disabled]:opacity-50",
            )}
            disabled={activating}
            onSelect={() => void handleActivate("claude-code")}
          >
            应用到 Claude Code
          </ContextMenu.Item>

          {/* Activate to Codex */}
          <ContextMenu.Item
            className={cn(
              "relative flex items-center rounded-lg px-2.5 py-1.5 text-[12px] text-foreground",
              "outline-none select-none cursor-pointer",
              "data-[highlighted]:bg-accent/10 data-[highlighted]:text-accent-foreground",
              "data-[disabled]:pointer-events-none data-[disabled]:opacity-50",
            )}
            disabled={activating}
            onSelect={() => void handleActivate("codex")}
          >
            应用到 Codex
          </ContextMenu.Item>

          {/* Activate to all */}
          <ContextMenu.Item
            className={cn(
              "relative flex items-center rounded-lg px-2.5 py-1.5 text-[12px] text-foreground",
              "outline-none select-none cursor-pointer",
              "data-[highlighted]:bg-accent/10 data-[highlighted]:text-accent-foreground",
              "data-[disabled]:pointer-events-none data-[disabled]:opacity-50",
            )}
            disabled={activating}
            onSelect={() => void handleActivateAll()}
          >
            应用到全部
          </ContextMenu.Item>

          <ContextMenu.Separator className="my-1 h-px bg-border/50" />

          {/* Duplicate */}
          <ContextMenu.Item
            className={cn(
              "relative flex items-center rounded-lg px-2.5 py-1.5 text-[12px] text-foreground",
              "outline-none select-none cursor-pointer",
              "data-[highlighted]:bg-accent/10 data-[highlighted]:text-accent-foreground",
            )}
            onSelect={() => void handleDuplicate()}
          >
            复制
          </ContextMenu.Item>

          {/* Delete */}
          <ContextMenu.Item
            className={cn(
              "relative flex items-center rounded-lg px-2.5 py-1.5 text-[12px] text-destructive",
              "outline-none select-none cursor-pointer",
              "data-[highlighted]:bg-destructive/10 data-[highlighted]:text-destructive",
            )}
            onSelect={onDelete}
          >
            删除
          </ContextMenu.Item>
        </ContextMenu.Content>
      </ContextMenu.Portal>
    </ContextMenu.Root>
  );
}

// ---------------------------------------------------------------------------
// DeleteConfirmDialog
// ---------------------------------------------------------------------------

export function DeleteConfirmDialog({
  open,
  onOpenChange,
  providerName,
  affectedTools,
  onConfirm,
}: DeleteConfirmDialogProps) {
  return (
    <AlertDialog.Root open={open} onOpenChange={onOpenChange}>
      <AlertDialog.Portal>
        <AlertDialog.Overlay className="fixed inset-0 z-50 bg-black/40 backdrop-blur-sm animate-in fade-in-0" />
        <AlertDialog.Content
          className={cn(
            "fixed left-1/2 top-1/2 z-50 w-full max-w-md -translate-x-1/2 -translate-y-1/2",
            "rounded-2xl border border-border/60 bg-card/95 backdrop-blur-md p-6 shadow-xl",
            "animate-in fade-in-0 zoom-in-95",
          )}
        >
          <AlertDialog.Title className="text-base font-semibold text-foreground">确认删除供应商</AlertDialog.Title>
          <AlertDialog.Description className="mt-2 text-sm text-muted-foreground leading-relaxed">
            确定要删除 <span className="font-medium text-foreground">{providerName}</span> 吗？
            {affectedTools.length > 0 && (
              <>
                <br />
                <span className="mt-2 block">删除后以下工具将恢复到启用前的状态 (backup)：</span>
                <span className="mt-1.5 flex flex-wrap gap-1.5">
                  {affectedTools.map((tool) => (
                    <span
                      key={tool}
                      className="inline-flex items-center px-2 py-0.5 rounded-md text-xs font-medium bg-destructive/10 text-destructive border border-destructive/20"
                    >
                      {TOOL_DISPLAY_NAMES[tool] ?? tool}
                    </span>
                  ))}
                </span>
              </>
            )}
          </AlertDialog.Description>

          <div className="mt-5 flex items-center justify-end gap-3">
            <AlertDialog.Cancel
              className={cn(
                "inline-flex items-center justify-center rounded-lg px-4 py-2 text-sm font-medium",
                "bg-muted/50 text-muted-foreground hover:bg-muted/80 transition-colors cursor-pointer",
                "focus:outline-none focus:ring-2 focus:ring-ring/40",
              )}
            >
              取消
            </AlertDialog.Cancel>
            <AlertDialog.Action
              onClick={onConfirm}
              className={cn(
                "inline-flex items-center justify-center rounded-lg px-4 py-2 text-sm font-medium",
                "bg-destructive text-destructive-foreground hover:bg-destructive/90 transition-colors cursor-pointer",
                "focus:outline-none focus:ring-2 focus:ring-destructive/40",
              )}
            >
              删除
            </AlertDialog.Action>
          </div>
        </AlertDialog.Content>
      </AlertDialog.Portal>
    </AlertDialog.Root>
  );
}
