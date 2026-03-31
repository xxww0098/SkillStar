import { useState } from "react";
import { motion } from "framer-motion";
import { useTranslation } from "react-i18next";
import {
  Layers,
  RefreshCw,
  Rocket,
  Trash2,
  X,
  Link2,
  Share2,
  CheckSquare,
  Square,
} from "lucide-react";
import { Loader2, Check } from "lucide-react";
import type { AgentProfile } from "../../types";

interface SkillSelectionBarProps {
  selectedCount: number;
  totalCount?: number;
  disabled?: boolean;
  onDeploy: () => void;
  onSaveGroup?: () => void;
  onShare?: () => void;
  onPackSkills?: () => void;
  onUpdate: () => Promise<void> | void;
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
  onShare,
  onUpdate,
  onUninstall,
  onSelectAll,
  onClear,
  agentProfiles,
  onBatchLink,
}: SkillSelectionBarProps) {
  const { t } = useTranslation();
  const [linkMenuOpen, setLinkMenuOpen] = useState(false);
  const [isUpdating, setIsUpdating] = useState(false);
  const [updateSuccess, setUpdateSuccess] = useState(false);
  const enabledProfiles = agentProfiles?.filter((p) => p.enabled) ?? [];
  const allSelected =
    totalCount !== undefined && selectedCount >= totalCount;

  const handleUpdate = async () => {
    if (disabled || isUpdating) return;
    setIsUpdating(true);
    setUpdateSuccess(false);
    try {
      await onUpdate();
      setUpdateSuccess(true);
      setTimeout(() => setUpdateSuccess(false), 2000);
    } finally {
      setIsUpdating(false);
    }
  };

  /* ── Shared icon-text button style ──────────────────────────── */
  const ghostBtn =
    "inline-flex items-center gap-1.5 px-2.5 h-7 rounded-lg text-[12px] font-medium text-muted-foreground hover:text-foreground hover:bg-white/[0.07] active:bg-white/10 transition-all duration-150 cursor-pointer select-none disabled:opacity-40 disabled:pointer-events-none whitespace-nowrap";

  /* Paper-mode overrides — detected via CSS variable */
  const ghostBtnPaper =
    "[html[data-bg-style=paper]_&]:hover:bg-black/[0.05] [html[data-bg-style=paper]_&]:active:bg-black/[0.08]";

  return (
    <motion.div
      initial={{ height: 0, opacity: 0 }}
      animate={{ height: "auto", opacity: 1 }}
      exit={{ height: 0, opacity: 0 }}
      transition={{ duration: 0.2, ease: "easeOut" }}
      className={`relative z-[60] ${linkMenuOpen ? "overflow-visible" : "overflow-hidden"}`}
    >
      {/* Glass bar */}
      <div
        className="flex items-center gap-1 px-4 py-1.5 border-b border-white/[0.06] bg-white/[0.03] backdrop-blur-xl
          [html[data-bg-style=paper]_&]:bg-black/[0.025] [html[data-bg-style=paper]_&]:border-black/[0.06]"
      >
        {/* ─── Zone 1: Selection state ─────────────────────────── */}
        <div className="flex items-center gap-1 shrink-0">
          {/* Counter pill */}
          <span className="inline-flex items-center h-6 px-2.5 rounded-full bg-primary/15 text-primary text-[12px] font-semibold tabular-nums tracking-tight whitespace-nowrap">
            {t("selectionBar.selected", { count: selectedCount })}
          </span>

          {/* Select-all / deselect-all toggle */}
          {onSelectAll && totalCount !== undefined && (
            <button
              onClick={allSelected ? onClear : onSelectAll}
              className={`${ghostBtn} ${ghostBtnPaper}`}
            >
              {allSelected ? (
                <Square className="w-3.5 h-3.5 shrink-0" />
              ) : (
                <CheckSquare className="w-3.5 h-3.5 shrink-0" />
              )}
              {allSelected
                ? t("common.deselectAll")
                : t("common.selectAll")}
            </button>
          )}
        </div>

        {/* Divider */}
        <div className="w-px h-3.5 bg-white/[0.08] mx-1 shrink-0 [html[data-bg-style=paper]_&]:bg-black/[0.08]" />

        {/* ─── Zone 2: Actions ─────────────────────────────────── */}
        <div className="flex items-center gap-1 flex-1 min-w-0">
          {/* Pack as deck */}
          {onPackSkills && (
            <button
              onClick={onPackSkills}
              disabled={disabled}
              className={`${ghostBtn} ${ghostBtnPaper}`}
            >
              <Layers className="w-3.5 h-3.5 shrink-0" />
              {t("selectionBar.packSkills")}
            </button>
          )}

          {/* Link to agent */}
          {enabledProfiles.length > 0 && onBatchLink && (
            <div className="relative">
              <button
                onClick={() => setLinkMenuOpen(!linkMenuOpen)}
                className={`${ghostBtn} ${ghostBtnPaper}`}
              >
                <Link2 className="w-3.5 h-3.5 shrink-0" />
                {t("selectionBar.linkToAgent")}
              </button>
              {linkMenuOpen && (
                <>
                  <div
                    className="fixed inset-0 z-40"
                    onClick={() => setLinkMenuOpen(false)}
                  />
                  <motion.div
                    initial={{ opacity: 0, scale: 0.95 }}
                    animate={{ opacity: 1, scale: 1 }}
                    className="absolute left-0 top-full mt-1.5 p-1 rounded-xl border border-border/80 bg-card shadow-lg z-50 min-w-[140px] max-h-[200px] overflow-y-auto overscroll-contain"
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
                          loading="lazy"
                          decoding="async"
                        />
                        {profile.display_name}
                      </button>
                    ))}
                  </motion.div>
                </>
              )}
            </div>
          )}

          {/* ★ Primary CTA: Deploy */}
          <button
            onClick={onDeploy}
            disabled={disabled}
            className="inline-flex items-center gap-1.5 px-3.5 h-7 rounded-lg text-[12px] font-semibold text-primary-foreground cursor-pointer select-none
              bg-gradient-to-r from-primary to-primary/85 shadow-[0_0_12px_rgba(var(--color-primary-rgb),0.25)] hover:shadow-[0_0_18px_rgba(var(--color-primary-rgb),0.35)]
              hover:brightness-110 active:brightness-95 transition-all duration-150
              disabled:opacity-40 disabled:pointer-events-none whitespace-nowrap"
          >
            <Rocket className="w-3.5 h-3.5 shrink-0" />
            {t("selectionBar.deployToProject")}
          </button>

          {/* Save as group */}
          {onSaveGroup && (
            <button
              onClick={onSaveGroup}
              disabled={disabled}
              className={`${ghostBtn} ${ghostBtnPaper}`}
            >
              <Layers className="w-3.5 h-3.5 shrink-0" />
              {t("selectionBar.saveAsGroup")}
            </button>
          )}

          {/* Share */}
          {onShare && (
            <button
              onClick={onShare}
              disabled={disabled || isUpdating}
              className={`${ghostBtn} ${ghostBtnPaper}`}
            >
              <Share2 className="w-3.5 h-3.5 shrink-0" />
              {t("selectionBar.share")}
            </button>
          )}

          {/* Update selected */}
          <button
            onClick={handleUpdate}
            disabled={disabled || isUpdating || updateSuccess}
            className={`${ghostBtn} ${ghostBtnPaper} ${
              updateSuccess
                ? "!text-success"
                : ""
            }`}
          >
            {isUpdating ? (
              <Loader2 className="w-3.5 h-3.5 shrink-0 animate-spin" />
            ) : updateSuccess ? (
              <Check className="w-3.5 h-3.5 shrink-0" />
            ) : (
              <RefreshCw className="w-3.5 h-3.5 shrink-0" />
            )}
            {isUpdating
              ? t("common.updating", { defaultValue: "Updating..." })
              : updateSuccess
                ? t("common.updated", { defaultValue: "Updated" })
                : t("selectionBar.updateAll")}
          </button>
        </div>

        {/* Divider */}
        <div className="w-px h-3.5 bg-white/[0.08] mx-1 shrink-0 [html[data-bg-style=paper]_&]:bg-black/[0.08]" />

        {/* ─── Zone 3: Danger + dismiss ────────────────────────── */}
        <div className="flex items-center gap-1 shrink-0">
          {/* Uninstall — danger ghost */}
          <button
            onClick={onUninstall}
            disabled={disabled || isUpdating}
            className="inline-flex items-center gap-1.5 px-2.5 h-7 rounded-lg text-[12px] font-medium text-destructive/70 hover:text-destructive hover:bg-destructive/10 active:bg-destructive/15 transition-all duration-150 cursor-pointer select-none disabled:opacity-40 disabled:pointer-events-none whitespace-nowrap"
          >
            <Trash2 className="w-3.5 h-3.5 shrink-0" />
            {t("selectionBar.uninstall")}
          </button>

          {/* Dismiss selection */}
          <button
            onClick={onClear}
            className="inline-flex items-center justify-center w-6 h-6 rounded-md text-muted-foreground/60 hover:text-foreground hover:bg-white/[0.07] active:bg-white/10 transition-all duration-150 cursor-pointer
              [html[data-bg-style=paper]_&]:hover:bg-black/[0.05]"
            aria-label={t("selectionBar.clear")}
          >
            <X className="w-3.5 h-3.5" />
          </button>
        </div>
      </div>
    </motion.div>
  );
}
