import { AnimatePresence, motion } from "framer-motion";
import { Loader2, Plus, Search, X } from "lucide-react";
import { useMemo, useState } from "react";
import type { ProviderEntry } from "../hooks/useModelProviders";
import { useProviderPresets } from "../hooks/useProviderPresets";
import type { ModelAppId } from "./AppCapsuleSwitcher";

interface AddProviderDialogProps {
  open: boolean;
  appId: ModelAppId;
  onClose: () => void;
  onSelectDraft: (entry: ProviderEntry) => void;
}

export function AddProviderDialog({ open, appId, onClose, onSelectDraft }: AddProviderDialogProps) {
  const [search, setSearch] = useState("");
  const [customName, setCustomName] = useState("");
  const [showCustom, setShowCustom] = useState(false);
  const { loading: presetsLoading, presets } = useProviderPresets(appId);

  const builtInPresets = useMemo(() => presets.filter((preset) => preset.category !== "custom"), [presets]);

  const filtered = useMemo(() => {
    const q = search.trim().toLowerCase();
    if (!q) return builtInPresets;
    return builtInPresets.filter((p) => p.name.toLowerCase().includes(q));
  }, [builtInPresets, search]);

  // Group by category
  const grouped = useMemo(() => {
    const map: Record<string, typeof filtered> = {};
    const order = ["official", "cn_official", "aggregator", "third_party"];
    for (const p of filtered) {
      const cat = p.category || "custom";
      if (!map[cat]) map[cat] = [];
      map[cat].push(p);
    }
    return order
      .filter((k) => map[k]?.length)
      .map((k) => ({
        key: k,
        label:
          k === "official" ? "官方" : k === "cn_official" ? "国产官方" : k === "aggregator" ? "聚合平台" : "第三方",
        items: map[k],
      }));
  }, [filtered]);

  const handlePresetClick = (preset: ProviderEntry) => {
    const entry: ProviderEntry = {
      id: `${preset.id}_${Date.now()}`,
      name: preset.name,
      category: preset.category as ProviderEntry["category"],
      settingsConfig: preset.settingsConfig,
      websiteUrl: preset.websiteUrl,
      apiKeyUrl: preset.apiKeyUrl,
      iconColor: preset.iconColor,
      createdAt: Date.now(),
      sortIndex: 999,
    };
    onSelectDraft(entry);
    onClose();
    setSearch("");
  };

  const handleCustomAdd = () => {
    if (!customName.trim()) return;
    const entry: ProviderEntry = {
      id: `custom_${Date.now()}`,
      name: customName.trim(),
      category: "custom",
      settingsConfig: appId === "claude" ? { env: {} } : appId === "codex" ? { auth: {}, config: "" } : {},
      createdAt: Date.now(),
      sortIndex: 999,
    };
    onSelectDraft(entry);
    onClose();
    setCustomName("");
    setShowCustom(false);
  };

  return (
    <AnimatePresence>
      {open && (
        <motion.div
          initial={{ opacity: 0 }}
          animate={{ opacity: 1 }}
          exit={{ opacity: 0 }}
          className="fixed inset-0 z-[60] flex items-center justify-center bg-black/40 backdrop-blur-sm"
          onClick={(e) => {
            if (e.target === e.currentTarget) onClose();
          }}
        >
          <motion.div
            initial={{ opacity: 0, scale: 0.95, y: 20 }}
            animate={{ opacity: 1, scale: 1, y: 0 }}
            exit={{ opacity: 0, scale: 0.95, y: 20 }}
            transition={{ type: "spring", stiffness: 400, damping: 30 }}
            className="w-full max-w-md rounded-2xl border border-border bg-card shadow-2xl overflow-hidden"
          >
            {/* Header */}
            <div className="flex items-center justify-between px-5 py-4 border-b border-border">
              <h2 className="text-base font-semibold text-foreground flex items-center gap-2">
                <Plus className="w-5 h-5 text-primary" />
                添加供应商
              </h2>
              <button
                type="button"
                onClick={onClose}
                className="p-1.5 rounded-lg text-muted-foreground hover:text-foreground hover:bg-muted/50 transition-colors"
              >
                <X className="w-4 h-4" />
              </button>
            </div>

            {/* Search */}
            <div className="px-5 pt-4">
              <div className="relative">
                <Search className="absolute left-3 top-1/2 -translate-y-1/2 w-4 h-4 text-muted-foreground pointer-events-none" />
                <input
                  type="text"
                  value={search}
                  onChange={(e) => setSearch(e.target.value)}
                  placeholder="搜索供应商预设..."
                  className="w-full h-9 pl-9 pr-3 rounded-lg bg-background/60 border border-border text-sm text-foreground placeholder:text-muted-foreground/50 focus:outline-none focus:ring-1 focus:ring-primary/50"
                  autoFocus
                />
              </div>
            </div>

            {/* Preset list */}
            <div className="px-5 py-4 max-h-[50vh] overflow-y-auto scrollbar-thin space-y-4">
              {presetsLoading && (
                <div className="flex items-center justify-center py-6 text-muted-foreground">
                  <Loader2 className="w-4 h-4 animate-spin" />
                </div>
              )}

              {grouped.map((group) => (
                <div key={group.key}>
                  <h4 className="text-[10px] font-semibold text-muted-foreground uppercase tracking-wider mb-2">
                    {group.label}
                  </h4>
                  <div className="space-y-1">
                    {group.items.map((preset) => (
                      <button
                        key={preset.id}
                        type="button"
                        onClick={() => handlePresetClick(preset)}
                        className="w-full flex items-center gap-3 px-3 py-2.5 rounded-lg hover:bg-muted/50 transition-colors text-left group/item"
                      >
                        <span
                          className="w-2.5 h-2.5 rounded-full shrink-0"
                          style={{ backgroundColor: preset.iconColor || "#888" }}
                        />
                        <span className="flex-1 min-w-0">
                          <span className="text-sm font-medium text-foreground">{preset.name}</span>
                          {preset.websiteUrl && (
                            <span className="block text-[11px] text-muted-foreground truncate">
                              {preset.websiteUrl}
                            </span>
                          )}
                        </span>
                        <Plus className="w-4 h-4 text-muted-foreground/40 group-hover/item:text-primary transition-colors shrink-0" />
                      </button>
                    ))}
                  </div>
                </div>
              ))}

              {!presetsLoading && filtered.length === 0 && !showCustom && (
                <p className="text-sm text-muted-foreground text-center py-4">没有匹配的预设</p>
              )}
            </div>

            {/* Custom add */}
            <div className="px-5 py-4 border-t border-border">
              {showCustom ? (
                <div className="flex gap-2">
                  <input
                    type="text"
                    value={customName}
                    onChange={(e) => setCustomName(e.target.value)}
                    onKeyDown={(e) => e.key === "Enter" && handleCustomAdd()}
                    placeholder="供应商名称"
                    className="flex-1 h-9 px-3 rounded-lg bg-background/60 border border-border text-sm focus:outline-none focus:ring-1 focus:ring-primary/50"
                    autoFocus
                  />
                  <button
                    type="button"
                    onClick={handleCustomAdd}
                    disabled={!customName.trim()}
                    className="px-4 h-9 rounded-lg bg-primary text-primary-foreground text-sm font-medium disabled:opacity-40 transition-colors"
                  >
                    添加
                  </button>
                </div>
              ) : (
                <button
                  type="button"
                  onClick={() => setShowCustom(true)}
                  className="w-full flex items-center justify-center gap-2 py-2 rounded-lg border border-dashed border-border text-sm text-muted-foreground hover:text-foreground hover:border-primary/30 transition-colors"
                >
                  <Plus className="w-4 h-4" />
                  自定义供应商
                </button>
              )}
            </div>
          </motion.div>
        </motion.div>
      )}
    </AnimatePresence>
  );
}
