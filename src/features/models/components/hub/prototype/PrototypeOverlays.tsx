import { Trash2 } from "lucide-react";
import { Button } from "../../../../../components/ui/button";
import { ModalShell } from "../../../../../components/ui/ModalShell";
import { getAgent } from "../../../lib/agentRegistry";
import { activeEntry } from "../../../lib/toolBinding";
import type { PrototypeHubData } from "./types";

/**
 * Only confirmations / tiny pickers use floating chrome.
 * Create / edit / settings / App AI are main-pane pages (see EditorPage).
 * No side drawers.
 */
export function DeleteConfirmModal({ data }: { data: PrototypeHubData }) {
  const { overlay, closeOverlay, providers, toolActivations, stub } = data;
  if (overlay.type !== "delete") return null;

  const provider = providers.find((p) => p.id === overlay.providerId);
  const affected = Object.entries(toolActivations)
    .filter(([, binding]) => activeEntry(binding)?.provider_id === overlay.providerId)
    .map(([toolId]) => getAgent(toolId)?.displayName ?? toolId);

  return (
    <ModalShell open onClose={closeOverlay} ariaLabel="Delete provider" panelClassName="max-w-md">
      <div className="space-y-4 p-5">
        <div className="flex items-start gap-3">
          <div className="flex h-10 w-10 items-center justify-center rounded-xl bg-destructive/15 text-destructive">
            <Trash2 className="h-5 w-5" />
          </div>
          <div>
            <h3 className="text-sm font-semibold">Delete {provider?.name ?? "provider"}?</h3>
            <p className="mt-1 text-xs text-muted-foreground">
              {affected.length > 0 ? `Disconnects: ${affected.join(", ")}` : "No agents currently bound."}
            </p>
          </div>
        </div>
        <div className="flex justify-end gap-2">
          <Button variant="ghost" size="sm" onClick={closeOverlay}>
            Cancel
          </Button>
          <Button
            variant="destructive"
            size="sm"
            onClick={() => {
              stub("deleteProvider", { id: overlay.providerId });
              closeOverlay();
            }}
          >
            Delete
          </Button>
        </div>
      </div>
    </ModalShell>
  );
}

/** Optional centered create picker (Variant B) — still not a drawer. */
export function CreateModal({ data }: { data: PrototypeHubData }) {
  if (data.overlay.type !== "create") return null;
  const presets = [
    { id: "deepseek", name: "DeepSeek", color: "#4D6BFE" },
    { id: "kimi", name: "Kimi", color: "#5B45E0" },
    { id: "glm", name: "智谱 GLM", color: "#3366FF" },
    { id: "openrouter", name: "OpenRouter", color: "#6366F1" },
    { id: "custom", name: "Custom", color: "#64748B" },
  ];
  return (
    <ModalShell open onClose={data.closeOverlay} ariaLabel="Add provider" panelClassName="max-w-md">
      <div className="space-y-3 p-5">
        <h2 className="text-sm font-semibold">Add provider</h2>
        <div className="space-y-1.5">
          {presets.map((preset) => (
            <button
              key={preset.id}
              type="button"
              onClick={() => {
                data.stub("createProvider", { preset: preset.id });
                const id = data.providers[0]?.id;
                if (id) data.setOverlay({ type: "edit", providerId: id });
                else data.closeOverlay();
              }}
              className="flex w-full items-center gap-3 rounded-xl border border-border/50 px-3 py-2.5 text-left text-sm hover:bg-muted/40"
            >
              <span className="h-8 w-8 rounded-lg" style={{ background: preset.color }} />
              {preset.name}
            </button>
          ))}
        </div>
      </div>
    </ModalShell>
  );
}
