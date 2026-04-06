import { motion } from "framer-motion";
import {
  Bot,
  Check,
  CheckSquare,
  Layers,
  Link2,
  Loader2,
  MinusSquare,
  RefreshCw,
  Rocket,
  Share2,
  Trash2,
  X,
} from "lucide-react";
import { useState } from "react";
import { useTranslation } from "react-i18next";
import { AgentIcon } from "../../../components/ui/AgentIcon";
import { agentIconCls, cn } from "../../../lib/utils";
import type { AgentProfile } from "../../../types";

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
  /** Batch-unlink selected skills from all agents */
  onBatchUnlinkAll?: () => void;
  /** Batch AI Translation & Summary */
  onBatchAiProcess?: () => Promise<void>;
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
  onBatchUnlinkAll,
  onBatchAiProcess,
}: SkillSelectionBarProps) {
  const { t } = useTranslation();
  const [linkMenuOpen, setLinkMenuOpen] = useState(false);
  const [isUpdating, setIsUpdating] = useState(false);
  const [updateSuccess, setUpdateSuccess] = useState(false);
  const enabledProfiles = agentProfiles?.filter((p) => p.enabled) ?? [];
  const allSelected = totalCount !== undefined && selectedCount >= totalCount;

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
    "group inline-flex items-center gap-1.5 px-3 h-7 rounded-lg text-[12px] font-medium text-muted-foreground hover:text-foreground bg-muted/30 hover:bg-muted/60 active:bg-muted border border-border/40 hover:border-border/60 hover:shadow-sm shadow-sm shadow-black/5 ring-1 ring-white/5 transition-all duration-200 cursor-pointer select-none disabled:opacity-40 disabled:pointer-events-none whitespace-nowrap";

  const ghostBtnPaper = "";

  return (
    <motion.div
      initial={{ height: 0, opacity: 0 }}
      animate={{ height: "auto", opacity: 1 }}
      exit={{ height: 0, opacity: 0 }}
      transition={{ duration: 0.2, ease: "easeOut" }}
      className={`relative z-40 ${linkMenuOpen ? "overflow-visible" : "overflow-hidden"}`}
    >
      {/* Glass bar */}
      <div
        className="flex items-center gap-1 px-4 py-1.5 border-b border-border-subtle bg-sidebar
          backdrop-blur-xl"
      >
        {/* ─── Zone 1: Selection state ─────────────────────────── */}
        <div className="flex items-center shrink-0 bg-muted/40 p-[3px] rounded-xl border border-border/40 shadow-inner">
          {/* Counter pill */}
          <span className="flex items-center justify-center h-7 px-3 rounded-[9px] bg-background text-primary text-[12px] font-bold tabular-nums tracking-tight whitespace-nowrap shadow-sm ring-1 ring-border-subtle/50">
            {t("selectionBar.selected", { count: selectedCount })}
          </span>

          {/* Select-all / deselect-all toggle */}
          {onSelectAll && totalCount !== undefined && (
            <button
              onClick={allSelected ? onClear : onSelectAll}
              className="group flex items-center gap-1.5 h-7 px-3 ml-[1px] rounded-[9px] text-[12px] font-medium text-muted-foreground hover:text-foreground hover:bg-background/80 hover:shadow-sm transition-all duration-200 cursor-pointer select-none whitespace-nowrap"
            >
              <div className="relative w-3.5 h-3.5 shrink-0 flex items-center justify-center">
                <CheckSquare
                  className={`absolute w-3.5 h-3.5 transition-all duration-300 ease-out ${allSelected ? "opacity-0 scale-50 rotate-90" : "opacity-100 scale-100 rotate-0 group-hover:text-primary/80"}`}
                />
                <MinusSquare
                  className={`absolute w-3.5 h-3.5 transition-all duration-300 ease-out ${allSelected ? "opacity-100 scale-100 rotate-0 text-foreground" : "opacity-0 scale-50 -rotate-90"}`}
                />
              </div>
              <span className="relative">{allSelected ? t("common.deselectAll") : t("common.selectAll")}</span>
            </button>
          )}
        </div>

        {/* Divider */}
        <div className="w-px h-3.5 bg-border-subtle mx-1 shrink-0" />

        {/* ─── Zone 2: Actions ─────────────────────────────────── */}
        <div className="flex items-center gap-1 flex-1 min-w-0">
          {/* Pack as deck */}
          {onPackSkills && (
            <button onClick={onPackSkills} disabled={disabled} className={`${ghostBtn} ${ghostBtnPaper}`}>
              <Layers className="w-3.5 h-3.5 shrink-0 group-hover:scale-110 transition-transform duration-200" />
              {t("selectionBar.packSkills")}
            </button>
          )}

          {/* Link to agent */}
          {enabledProfiles.length > 0 && onBatchLink && (
            <div className="relative">
              <button
                onClick={() => setLinkMenuOpen(!linkMenuOpen)}
                className={cn(ghostBtn, ghostBtnPaper, linkMenuOpen && "bg-muted text-foreground")}
              >
                <Link2 className="w-3.5 h-3.5 shrink-0" />
                {t("selectionBar.linkToAgent")}
              </button>
              {linkMenuOpen && (
                <>
                  <div className="fixed inset-0 z-40" onClick={() => setLinkMenuOpen(false)} />
                  <motion.div
                    initial={{ opacity: 0, scale: 0.95, y: -4 }}
                    animate={{ opacity: 1, scale: 1, y: 0 }}
                    className="absolute left-0 top-full mt-1.5 w-[220px] rounded-xl border border-border/80 bg-card/95 backdrop-blur-xl shadow-xl z-50 overflow-hidden flex flex-col"
                  >
                    <div className="px-3 py-2 text-xs font-medium text-muted-foreground border-b border-border/40 bg-muted/20">
                      {t("selectionBar.linkToAgent", { defaultValue: "Link to Agent" })}
                    </div>

                    <div className="p-2 grid grid-cols-4 gap-1">
                      {enabledProfiles.map((profile) => (
                        <button
                          key={profile.id}
                          onClick={() => {
                            onBatchLink(profile.id);
                            setLinkMenuOpen(false);
                          }}
                          className="flex flex-col items-center justify-center gap-1.5 p-2 rounded-lg hover:bg-muted transition-colors cursor-pointer group focus-ring"
                          title={profile.display_name}
                        >
                          <AgentIcon
                            profile={profile}
                            className={cn(
                              agentIconCls(profile.icon, "w-6 h-6"),
                              "transition-[filter,transform] duration-300 drop-shadow-sm",
                              "grayscale opacity-70 group-hover:grayscale-0 group-hover:opacity-100 group-hover:scale-110",
                            )}
                          />
                          <span className="text-[10px] whitespace-nowrap overflow-hidden text-ellipsis w-full text-center text-muted-foreground group-hover:text-foreground">
                            {profile.display_name}
                          </span>
                        </button>
                      ))}
                    </div>

                    {onBatchUnlinkAll && (
                      <div className="p-1.5 border-t border-border/40 bg-muted/10">
                        <button
                          onClick={() => {
                            onBatchUnlinkAll();
                            setLinkMenuOpen(false);
                          }}
                          className="w-full flex items-center justify-center gap-1.5 px-3 py-2 rounded-md text-xs font-medium text-warning hover:text-warning hover:bg-warning/10 transition-colors cursor-pointer"
                        >
                          <Link2 className="w-3.5 h-3.5 relative">
                            {/* Simple slash overlay for unlink icon */}
                            <svg
                              className="absolute inset-0 text-current"
                              viewBox="0 0 24 24"
                              fill="none"
                              stroke="currentColor"
                              strokeWidth="2.5"
                              strokeLinecap="round"
                            >
                              <line x1="4" y1="20" x2="20" y2="4" />
                            </svg>
                          </Link2>
                          {t("selectionBar.unlinkAll", { defaultValue: "Unlink all agents" })}
                        </button>
                      </div>
                    )}
                  </motion.div>
                </>
              )}
            </div>
          )}

          {/* ★ Primary CTA: Deploy */}
          <button
            onClick={onDeploy}
            disabled={disabled}
            className="group inline-flex items-center gap-1.5 px-3.5 h-7 rounded-lg text-[12px] font-semibold text-white cursor-pointer select-none
              bg-gradient-to-b from-primary/90 to-primary shadow-[0_1px_3px_rgba(59,130,246,0.3),_inset_0_1px_1px_rgba(255,255,255,0.2)] 
              hover:shadow-[0_2px_6px_rgba(59,130,246,0.4),_inset_0_1px_1px_rgba(255,255,255,0.3)] hover:ring-1 hover:ring-primary/50
              hover:brightness-110 active:brightness-95 active:translate-y-px transition-all duration-200
              disabled:opacity-40 disabled:pointer-events-none whitespace-nowrap"
          >
            <Rocket className="w-3.5 h-3.5 shrink-0 group-hover:-translate-y-px transition-transform duration-200" />
            {t("selectionBar.deployToProject")}
          </button>

          {/* Save as group */}
          {onSaveGroup && (
            <button onClick={onSaveGroup} disabled={disabled} className={`${ghostBtn} ${ghostBtnPaper}`}>
              <Layers className="w-3.5 h-3.5 shrink-0 group-hover:scale-110 transition-transform duration-200" />
              {t("selectionBar.saveAsGroup")}
            </button>
          )}

          {/* Share */}
          {onShare && (
            <button
              onClick={onShare}
              disabled={disabled || isUpdating}
              className="group inline-flex items-center gap-1.5 px-3 h-7 rounded-lg text-[12px] font-medium text-violet-600 dark:text-violet-300 bg-violet-500/10 hover:bg-violet-500/20 active:bg-violet-500/25 border border-violet-500/20 hover:border-violet-500/30 transition-all duration-200 cursor-pointer select-none disabled:opacity-40 disabled:pointer-events-none whitespace-nowrap"
            >
              <Share2 className="w-3.5 h-3.5 shrink-0 text-violet-500 dark:text-violet-400 group-hover:-translate-y-px group-hover:scale-105 transition-all duration-200" />
              {t("selectionBar.share")}
            </button>
          )}

          {/* AI Batch Process */}
          {onBatchAiProcess && (
            <button
              onClick={onBatchAiProcess}
              disabled={disabled || isUpdating}
              className="group inline-flex items-center gap-1.5 px-3 h-7 rounded-lg text-[12px] font-medium text-emerald-600 dark:text-emerald-300 bg-emerald-500/10 hover:bg-emerald-500/20 active:bg-emerald-500/25 border border-emerald-500/20 hover:border-emerald-500/30 transition-all duration-200 cursor-pointer select-none disabled:opacity-40 disabled:pointer-events-none whitespace-nowrap"
            >
              <Bot className="w-3.5 h-3.5 shrink-0 text-emerald-500 dark:text-emerald-400 group-hover:rotate-12 group-hover:scale-105 transition-all duration-300" />
              {t("selectionBar.batchAiProcess", { defaultValue: "Automate Translation" })}
            </button>
          )}

          {/* Update selected */}
          <button
            onClick={handleUpdate}
            disabled={disabled || isUpdating || updateSuccess}
            className={`${ghostBtn} ${ghostBtnPaper} ${
              updateSuccess ? "!text-success !border-success/20 !bg-success/10" : ""
            }`}
          >
            {isUpdating ? (
              <Loader2 className="w-3.5 h-3.5 shrink-0 animate-spin text-primary" />
            ) : updateSuccess ? (
              <Check className="w-3.5 h-3.5 shrink-0 scale-110" />
            ) : (
              <RefreshCw className="w-3.5 h-3.5 shrink-0 group-hover:rotate-180 transition-transform duration-300 ease-out" />
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
            className="group inline-flex items-center gap-1.5 px-3 h-7 rounded-lg text-[12px] font-medium text-destructive/80 hover:text-destructive bg-destructive/5 hover:bg-destructive/10 active:bg-destructive/20 border border-destructive/20 hover:border-destructive/30 shadow-sm shadow-black/5 ring-1 ring-white/5 transition-all duration-200 cursor-pointer select-none disabled:opacity-40 disabled:pointer-events-none whitespace-nowrap"
          >
            <Trash2 className="w-3.5 h-3.5 shrink-0 group-hover:scale-110 group-hover:-rotate-6 transition-transform duration-200" />
            {t("selectionBar.uninstall")}
          </button>

          {/* Dismiss selection */}
          <button
            onClick={onClear}
            className="group flex-shrink-0 inline-flex items-center justify-center w-7 h-7 rounded-full text-muted-foreground/60 hover:text-foreground hover:bg-muted/40 active:bg-muted/60 transition-all duration-200 cursor-pointer bg-transparent border border-transparent hover:border-border/50 hover:shadow-sm"
            aria-label={t("selectionBar.clear")}
          >
            <X className="w-3.5 h-3.5 group-hover:rotate-90 transition-transform duration-200" />
          </button>
        </div>
      </div>
    </motion.div>
  );
}
