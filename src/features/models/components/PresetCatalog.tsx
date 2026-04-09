import { AnimatePresence, motion } from "framer-motion";
import { Check, ChevronDown, Loader2, Plus, Search } from "lucide-react";
import { useCallback, useMemo, useState } from "react";
import { toast } from "sonner";
import { cn } from "../../../lib/utils";
import { useCodexAccounts } from "../hooks/useCodexAccounts";
import { useGeminiOAuth } from "../hooks/useGeminiOAuth";
import type { ProviderEntry } from "../hooks/useModelProviders";
import { claudePresets } from "../presets/claudePresets";
import { codexPresets } from "../presets/codexPresets";
import { geminiPresets } from "../presets/geminiPresets";
import { opencodePresets } from "../presets/opencodePresets";
import type { ModelAppId } from "./AppCapsuleSwitcher";
import { ProviderIcon } from "./shared/ProviderIcon";

interface PresetCatalogProps {
  appId: ModelAppId;
  appColor: string;
  onAddPreset: (entry: ProviderEntry) => void;
  /** Existing providers — used to prevent duplicate additions */
  existingProviders?: ProviderEntry[];
}

export function PresetCatalog({ appId, appColor, onAddPreset, existingProviders = [] }: PresetCatalogProps) {
  const [expanded, setExpanded] = useState(false);
  const [search, setSearch] = useState("");
  const [customName, setCustomName] = useState("");
  const [showCustom, setShowCustom] = useState(false);
  const codexState = useCodexAccounts();
  const geminiState = useGeminiOAuth({ onAccountAdded: onAddPreset });

  // Build a Set of existing provider names (lowercase) for O(1) dedup lookups
  const existingNames = useMemo(() => new Set(existingProviders.map((p) => p.name.toLowerCase())), [existingProviders]);

  // Build preset list based on app
  const presets = useMemo(() => {
    switch (appId) {
      case "claude":
        return claudePresets.map((p) => ({
          id: p.name.toLowerCase().replace(/[^a-z0-9]/g, "_"),
          name: p.name,
          category: p.category,
          websiteUrl: p.websiteUrl,
          apiKeyUrl: p.apiKeyUrl,
          iconColor: p.iconColor,
          settingsConfig: { env: p.env },
        }));
      case "codex":
        return codexPresets.map((p) => ({
          id: p.name.toLowerCase().replace(/[^a-z0-9]/g, "_"),
          name: p.name,
          category: p.category,
          websiteUrl: p.websiteUrl,
          apiKeyUrl: p.apiKeyUrl,
          iconColor: p.iconColor,
          settingsConfig: { config: p.config },
        }));
      case "opencode":
        return opencodePresets.map((p) => ({
          id: p.name.toLowerCase().replace(/[^a-z0-9]/g, "_"),
          name: p.name,
          category: p.category,
          websiteUrl: p.websiteUrl,
          apiKeyUrl: p.apiKeyUrl,
          iconColor: p.iconColor,
          settingsConfig: { provider: { [p.name.toLowerCase().replace(/[^a-z0-9]/g, "_")]: p.settingsConfig } },
        }));
      case "gemini":
        return geminiPresets.map((p) => ({
          id: p.name.toLowerCase().replace(/[^a-z0-9]/g, "_"),
          name: p.name,
          category: p.category,
          websiteUrl: p.websiteUrl,
          apiKeyUrl: p.apiKeyUrl,
          iconColor: p.iconColor,
          settingsConfig: { env: p.env },
        }));
      default:
        return [];
    }
  }, [appId]);

  const filtered = useMemo(() => {
    const q = search.trim().toLowerCase();
    if (!q) return presets;
    return presets.filter((p) => p.name.toLowerCase().includes(q));
  }, [presets, search]);

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
      .filter((k) => map[k]?.length || (appId === "codex" && k === "official"))
      .map((k) => ({
        key: k,
        label: k === "official" ? "官方" : k === "cn_official" ? "国产" : k === "aggregator" ? "聚合" : "第三方",
        items: map[k] || [],
      }));
  }, [filtered]);

  const handlePresetClick = useCallback(
    (preset: (typeof presets)[0]) => {
      // ── Dedup check: block adding providers with the same name ──
      if (existingNames.has(preset.name.toLowerCase())) {
        toast.warning(`${preset.name} 已存在，无需重复添加`);
        return;
      }

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
      onAddPreset(entry);
      setSearch("");
    },
    [onAddPreset, existingNames, appId],
  );

  const handleCustomAdd = useCallback(() => {
    if (!customName.trim()) return;
    const safeId =
      customName
        .trim()
        .toLowerCase()
        .replace(/[^a-z0-9_]/g, "_")
        .replace(/^_+|_+$/g, "") || "custom";
    const nameStr = customName.trim();

    // Generate valid starting config for Codex to avoid empty "" breaking the apply merger
    const codexConfig = `model_provider = "${safeId}"\nmodel = "gpt-5.4"\n\n[model_providers.${safeId}]\nname = "${nameStr}"\nbase_url = ""\nrequires_openai_auth = true`;

    const entry: ProviderEntry = {
      id: `custom_${Date.now()}`,
      name: nameStr,
      category: "custom",
      settingsConfig:
        appId === "claude" || appId === "gemini" ? { env: {} } : appId === "codex" ? { config: codexConfig } : {},
      createdAt: Date.now(),
      sortIndex: 999,
    };
    onAddPreset(entry);
    setCustomName("");
    setShowCustom(false);
  }, [appId, customName, onAddPreset]);

  return (
    <div
      className={cn(
        "rounded-2xl border transition-all duration-200",
        expanded ? "border-border bg-card/60 backdrop-blur-sm" : "border-dashed border-border/70 hover:border-border",
      )}
    >
      {/* Toggle header */}
      <button
        type="button"
        onClick={() => setExpanded(!expanded)}
        className={cn(
          "w-full flex items-center justify-center gap-2 py-3 text-sm transition-colors rounded-2xl",
          expanded ? "text-foreground font-medium" : "text-muted-foreground hover:text-foreground",
        )}
      >
        <Plus className="w-4 h-4" />
        添加供应商
        <ChevronDown className={cn("w-3.5 h-3.5 transition-transform duration-200", expanded && "rotate-180")} />
      </button>

      {/* Expanded preset catalog */}
      <AnimatePresence>
        {expanded && (
          <motion.div
            initial={{ height: 0, opacity: 0 }}
            animate={{ height: "auto", opacity: 1 }}
            exit={{ height: 0, opacity: 0 }}
            transition={{ duration: 0.2, ease: [0.22, 1, 0.36, 1] }}
            className="overflow-hidden"
          >
            <div className="px-4 pb-4 space-y-3">
              {/* Search */}
              <div className="relative">
                <Search className="absolute left-3 top-1/2 -translate-y-1/2 w-3.5 h-3.5 text-muted-foreground pointer-events-none" />
                <input
                  type="text"
                  value={search}
                  onChange={(e) => setSearch(e.target.value)}
                  placeholder="搜索预设..."
                  className="w-full h-8 pl-8 pr-3 rounded-lg bg-background/60 border border-border text-xs text-foreground placeholder:text-muted-foreground/50 focus:outline-none focus:ring-1 focus:ring-primary/50"
                />
              </div>

              {/* Grouped preset chips */}
              {grouped.map((group) => (
                <div key={group.key}>
                  <span className="text-[10px] font-semibold text-muted-foreground/70 uppercase tracking-wider">
                    {group.label}
                  </span>
                  <div className="flex flex-wrap gap-1.5 mt-1.5">
                    {group.items.map((preset) => {
                      const alreadyAdded = existingNames.has(preset.name.toLowerCase());
                      return (
                        <button
                          key={preset.id}
                          type="button"
                          onClick={() => handlePresetClick(preset)}
                          disabled={alreadyAdded}
                          className={cn(
                            "group/chip flex items-center gap-1.5 px-2.5 py-1.5 rounded-lg text-xs font-medium border transition-all",
                            alreadyAdded
                              ? "text-muted-foreground/40 border-border/30 bg-muted/10 cursor-not-allowed"
                              : "text-muted-foreground border-border/50 bg-muted/20 hover:bg-muted/50 hover:text-foreground hover:border-border",
                          )}
                        >
                          <ProviderIcon name={preset.name} fallbackColor={preset.iconColor || "#888"} size="w-4 h-4" />
                          {preset.name}
                          {alreadyAdded ? (
                            <Check className="w-3 h-3 text-emerald-500/60" />
                          ) : (
                            <Plus className="w-3 h-3 text-muted-foreground/30 group-hover/chip:text-primary transition-colors" />
                          )}
                        </button>
                      );
                    })}

                    {/* Codex OAuth Injection */}
                    {appId === "codex" && group.key === "official" && (
                      <button
                        type="button"
                        onClick={(e) => {
                          e.stopPropagation();
                          codexState.startOAuth();
                        }}
                        disabled={codexState.oauthLoading}
                        className={cn(
                          "group/chip flex items-center gap-1.5 px-2.5 py-1.5 rounded-lg text-xs font-medium border transition-all",
                          codexState.oauthLoading
                            ? "text-[#00A67E] border-[#00A67E]/30 bg-[#00A67E]/10 animate-pulse"
                            : "text-[#00A67E] border-[#00A67E]/20 bg-[#00A67E]/10 hover:bg-[#00A67E]/20 hover:border-[#00A67E]/40",
                        )}
                      >
                        <ProviderIcon name="OpenAI OAuth" fallbackColor="#00A67E" size="w-4 h-4" />
                        OpenAI OAuth
                        {codexState.oauthLoading ? (
                          <Loader2 className="w-3 h-3 animate-spin text-[#00A67E]" />
                        ) : (
                          <Plus className="w-3 h-3 text-[#00A67E]/50 group-hover/chip:text-[#00A67E] transition-colors" />
                        )}
                      </button>
                    )}

                    {/* Gemini OAuth Injection */}
                    {appId === "gemini" && group.key === "official" && (
                      <button
                        type="button"
                        onClick={(e) => {
                          e.stopPropagation();
                          geminiState.startOAuth();
                        }}
                        disabled={geminiState.oauthLoading}
                        className={cn(
                          "group/chip flex items-center gap-1.5 px-2.5 py-1.5 rounded-lg text-xs font-medium border transition-all",
                          geminiState.oauthLoading
                            ? "text-[#4285F4] border-[#4285F4]/30 bg-[#4285F4]/10 animate-pulse"
                            : "text-[#4285F4] border-[#4285F4]/20 bg-[#4285F4]/10 hover:bg-[#4285F4]/20 hover:border-[#4285F4]/40",
                        )}
                      >
                        <ProviderIcon name="Google OAuth" fallbackColor="#4285F4" size="w-4 h-4" />
                        Google OAuth
                        {geminiState.oauthLoading ? (
                          <Loader2 className="w-3 h-3 animate-spin text-[#4285F4]" />
                        ) : (
                          <Plus className="w-3 h-3 text-[#4285F4]/50 group-hover/chip:text-[#4285F4] transition-colors" />
                        )}
                      </button>
                    )}
                  </div>
                </div>
              ))}

              {filtered.length === 0 && (
                <p className="text-xs text-muted-foreground text-center py-2">没有匹配的预设</p>
              )}

              {/* Custom add */}
              <div className="pt-1 border-t border-border/30">
                {showCustom ? (
                  <div className="flex gap-2">
                    <input
                      type="text"
                      value={customName}
                      onChange={(e) => setCustomName(e.target.value)}
                      onKeyDown={(e) => e.key === "Enter" && handleCustomAdd()}
                      placeholder="自定义供应商名称"
                      className="flex-1 h-8 px-3 rounded-lg bg-background/60 border border-border text-xs focus:outline-none focus:ring-1 focus:ring-primary/50"
                    />
                    <button
                      type="button"
                      onClick={handleCustomAdd}
                      disabled={!customName.trim()}
                      className="px-3 h-8 rounded-lg text-xs font-medium text-white disabled:opacity-40 transition-colors"
                      style={{ backgroundColor: appColor }}
                    >
                      添加
                    </button>
                    <button
                      type="button"
                      onClick={() => setShowCustom(false)}
                      className="px-2 h-8 rounded-lg text-xs text-muted-foreground hover:text-foreground transition-colors"
                    >
                      取消
                    </button>
                  </div>
                ) : (
                  <button
                    type="button"
                    onClick={() => setShowCustom(true)}
                    className="flex items-center gap-1.5 text-xs text-muted-foreground hover:text-foreground transition-colors"
                  >
                    <Plus className="w-3 h-3" />
                    自定义供应商
                  </button>
                )}
              </div>
            </div>
          </motion.div>
        )}
      </AnimatePresence>
    </div>
  );
}
