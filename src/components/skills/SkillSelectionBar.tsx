import { useState } from "react";
import { motion } from "framer-motion";
import { useTranslation } from "react-i18next";
import { Layers, RefreshCw, Rocket, Trash2, X, CheckSquare, Square, Link2 } from "lucide-react";
import { Badge } from "../ui/badge";
import { Button } from "../ui/button";
import type { AgentProfile } from "../../types";

interface SkillSelectionBarProps {
  selectedCount: number;
  totalCount?: number;
  disabled?: boolean;
  onDeploy: () => void;
  onSaveGroup: () => void;
  onPackSkills?: () => void;
  onUpdate: () => void;
  onUninstall: () => void;
  onSelectAll?: () => void;
  onClear: () => void;
  /** Agent profiles for the "Link to Agent" action */
  agentProfiles?: AgentProfile[];
  /** Batch-link selected skills to an agent */
  onBatchLink?: (agentId: string) => void;
}

export function SkillSelectionBar({
  selectedCount,
  totalCount,
  disabled,
  onDeploy,
  onSaveGroup,
  onPackSkills,
  onUpdate,
  onUninstall,
  onSelectAll,
  onClear,
  agentProfiles,
  onBatchLink,
}: SkillSelectionBarProps) {
  const { t } = useTranslation();
  const [linkMenuOpen, setLinkMenuOpen] = useState(false);
  const enabledProfiles = agentProfiles?.filter((p) => p.enabled) ?? [];

  return (
    <motion.div
      initial={{ height: 0, opacity: 0 }}
      animate={{ height: "auto", opacity: 1 }}
      exit={{ height: 0, opacity: 0 }}
      className="relative z-[60] flex items-center gap-3 px-6 py-2.5 border-b border-white/10 bg-primary/5 backdrop-blur-sm shadow-[0_4px_20px_-8px_rgba(0,0,0,0.3)]"
    >
      <div className="flex items-center gap-2">
        <Badge variant="default">{t("selectionBar.selected", { count: selectedCount })}</Badge>
        {onPackSkills && (
          <Button
            size="sm"
            variant="outline"
            onClick={onPackSkills}
            className="h-6 px-2 text-xs"
            disabled={disabled}
          >
            <Layers className="w-3.5 h-3.5 mr-1" />
            {t("selectionBar.packSkills")}
          </Button>
        )}
        {onSelectAll && totalCount !== undefined && (
          selectedCount < totalCount ? (
            <Button size="sm" variant="ghost" onClick={onSelectAll} className="h-6 px-2 text-xs text-muted-foreground hover:text-foreground">
              <CheckSquare className="w-3.5 h-3.5 mr-1" />
              {t("selectionBar.selectAll")}
            </Button>
          ) : (
            <Button size="sm" variant="ghost" onClick={onClear} className="h-6 px-2 text-xs text-muted-foreground hover:text-foreground">
              <Square className="w-3.5 h-3.5 mr-1" />
              {t("selectionBar.deselectAll")}
            </Button>
          )
        )}
      </div>
      <div className="w-px h-4 bg-border/50 mx-1" />

      {/* Link to Agent */}
      {enabledProfiles.length > 0 && onBatchLink && (
        <div className="relative">
          <Button
            size="sm"
            variant="outline"
            onClick={() => setLinkMenuOpen(!linkMenuOpen)}
            className="h-7"
          >
            <Link2 className="w-3.5 h-3.5 mr-1.5" />
            {t("selectionBar.linkToAgent")}
          </Button>
          {linkMenuOpen && (
            <>
              <div
                className="fixed inset-0 z-40"
                onClick={() => setLinkMenuOpen(false)}
              />
              <motion.div
                initial={{ opacity: 0, scale: 0.95 }}
                animate={{ opacity: 1, scale: 1 }}
                className="absolute left-0 top-full mt-1.5 p-1 rounded-xl border border-border/80 bg-card shadow-lg z-50 min-w-[140px]"
              >
                {enabledProfiles.map((profile) => (
                  <button
                    key={profile.id}
                    onClick={() => {
                      onBatchLink(profile.id);
                      setLinkMenuOpen(false);
                    }}
                    className="w-full flex items-center gap-2 px-3 py-1.5 rounded-lg text-xs hover:bg-muted transition-colors cursor-pointer"
                  >
                    <img
                      src={`/${profile.icon}`}
                      alt={profile.display_name}
                      className="w-3.5 h-3.5"
                    />
                    {profile.display_name}
                  </button>
                ))}
              </motion.div>
            </>
          )}
        </div>
      )}

      <Button size="sm" onClick={onDeploy}>
        <Rocket className="w-3.5 h-3.5 mr-1.5" />
        {t("selectionBar.deployToProject")}
      </Button>
      <Button size="sm" variant="outline" onClick={onSaveGroup}>
        <Layers className="w-3.5 h-3.5 mr-1.5" />
        {t("selectionBar.saveAsGroup")}
      </Button>
      <Button size="sm" variant="outline" onClick={onUpdate} disabled={disabled}>
        <RefreshCw className="w-3.5 h-3.5 mr-1.5" />
        {t("selectionBar.updateAll")}
      </Button>
      <Button size="sm" variant="destructive" onClick={onUninstall} disabled={disabled}>
        <Trash2 className="w-3.5 h-3.5 mr-1.5" />
        {t("selectionBar.uninstall")}
      </Button>
      <button
        onClick={onClear}
        className="ml-auto text-xs text-muted-foreground hover:text-foreground flex items-center gap-1 transition-colors cursor-pointer"
      >
        <X className="w-3 h-3" />
        {t("selectionBar.clear")}
      </button>
    </motion.div>
  );
}
