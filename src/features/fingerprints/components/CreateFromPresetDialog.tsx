import { AnimatePresence, motion } from "framer-motion";
import { Loader2, Sparkles, X } from "lucide-react";
import { useEffect, useMemo, useState } from "react";
import { toast } from "sonner";
import { Button } from "@/components/ui/button";
import { Input } from "@/components/ui/input";
import { cn } from "@/lib/utils";
import type { PresetTemplate } from "../types";

interface CreateFromPresetDialogProps {
  open: boolean;
  presets: PresetTemplate[];
  onClose: () => void;
  onSubmit: (presetId: string, name: string) => Promise<void>;
}

/**
 * Modal for creating a fingerprint from a built-in preset:
 * 1. user picks one of the preset cards (family-coloured)
 * 2. fills a name (auto-suggested from the preset label)
 * 3. clicks "Create"
 */
export function CreateFromPresetDialog({ open, presets, onClose, onSubmit }: CreateFromPresetDialogProps) {
  const [presetId, setPresetId] = useState<string | null>(null);
  const [name, setName] = useState("");
  const [busy, setBusy] = useState(false);

  // Auto-suggest a name when the user picks a new preset.
  useEffect(() => {
    if (!presetId) return;
    const p = presets.find((x) => x.id === presetId);
    if (p) setName(suggestName(p));
  }, [presetId, presets]);

  // Reset when dialog opens/closes.
  useEffect(() => {
    if (open) {
      setPresetId(presets[1]?.id ?? presets[0]?.id ?? null);
    } else {
      setName("");
      setBusy(false);
    }
  }, [open, presets]);

  const grouped = useMemo(() => groupByFamily(presets), [presets]);

  if (!open) return null;

  const submit = async () => {
    if (!presetId) {
      toast.error("请选择一个预设");
      return;
    }
    if (!name.trim()) {
      toast.error("请填写指纹名称");
      return;
    }
    setBusy(true);
    try {
      await onSubmit(presetId, name.trim());
      toast.success("已创建指纹", { description: name.trim() });
      onClose();
    } catch (e) {
      toast.error("创建失败", { description: e instanceof Error ? e.message : String(e) });
    } finally {
      setBusy(false);
    }
  };

  return (
    <AnimatePresence>
      {open && (
        <motion.div
          className="fixed inset-0 z-50 flex items-center justify-center bg-black/40 backdrop-blur-sm px-4"
          initial={{ opacity: 0 }}
          animate={{ opacity: 1 }}
          exit={{ opacity: 0 }}
          onClick={onClose}
        >
          <motion.div
            className="relative w-full max-w-2xl rounded-3xl border border-zinc-200/60 bg-white p-6 shadow-2xl"
            initial={{ scale: 0.94, opacity: 0 }}
            animate={{ scale: 1, opacity: 1 }}
            exit={{ scale: 0.96, opacity: 0 }}
            transition={{ duration: 0.18 }}
            onClick={(e) => e.stopPropagation()}
          >
            <button
              type="button"
              className="absolute right-4 top-4 inline-flex h-8 w-8 items-center justify-center rounded-md text-muted-foreground hover:bg-zinc-100 hover:text-foreground"
              onClick={onClose}
              aria-label="close"
            >
              <X className="h-4 w-4" />
            </button>

            <div className="mb-5 flex items-center gap-2.5">
              <div className="inline-flex h-9 w-9 items-center justify-center rounded-xl bg-violet-50 text-violet-600">
                <Sparkles className="h-4 w-4" />
              </div>
              <div>
                <h2 className="text-base font-semibold">从预设创建指纹</h2>
                <p className="text-xs text-muted-foreground">
                  挑一个浏览器画像，下次刷额度时自动套用它的 TLS / UA / 头部
                </p>
              </div>
            </div>

            <div className="max-h-[50vh] space-y-4 overflow-y-auto pr-1">
              {Object.entries(grouped).map(([family, list]) => (
                <div key={family}>
                  <div className="mb-2 text-[11px] font-medium uppercase tracking-wide text-zinc-500">{family}</div>
                  <div className="grid grid-cols-1 gap-2 sm:grid-cols-2">
                    {list.map((p) => (
                      <button
                        key={p.id}
                        type="button"
                        className={cn(
                          "flex flex-col gap-1 rounded-2xl border px-3 py-2.5 text-left transition-all",
                          presetId === p.id
                            ? "border-violet-400 bg-violet-50/50 ring-2 ring-violet-200"
                            : "border-zinc-200 bg-white hover:border-zinc-300 hover:bg-zinc-50",
                        )}
                        onClick={() => setPresetId(p.id)}
                      >
                        <span className="text-sm font-medium">{p.label}</span>
                        <span className="text-[11px] leading-snug text-muted-foreground line-clamp-2">
                          {p.description}
                        </span>
                      </button>
                    ))}
                  </div>
                </div>
              ))}
            </div>

            <div className="mt-5">
              <label className="mb-1.5 block text-xs font-medium text-zinc-700">指纹名称</label>
              <Input
                value={name}
                onChange={(e) => setName(e.target.value)}
                placeholder="例如：Chrome on Mac (Office)"
                disabled={busy}
              />
            </div>

            <div className="mt-5 flex justify-end gap-2">
              <Button variant="outline" onClick={onClose} disabled={busy}>
                取消
              </Button>
              <Button onClick={submit} disabled={busy || !presetId}>
                {busy ? (
                  <>
                    <Loader2 className="mr-2 h-3.5 w-3.5 animate-spin" />
                    创建中…
                  </>
                ) : (
                  "创建指纹"
                )}
              </Button>
            </div>
          </motion.div>
        </motion.div>
      )}
    </AnimatePresence>
  );
}

function suggestName(p: PresetTemplate): string {
  return p.label;
}

function groupByFamily(presets: PresetTemplate[]): Record<string, PresetTemplate[]> {
  const out: Record<string, PresetTemplate[]> = {};
  for (const p of presets) {
    if (!out[p.family]) {
      out[p.family] = [];
    }
    out[p.family].push(p);
  }
  return out;
}
