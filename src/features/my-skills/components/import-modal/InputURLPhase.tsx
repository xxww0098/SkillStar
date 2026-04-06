import { Clock, Download, Package, Search, Share2 } from "lucide-react";
import { useTranslation } from "react-i18next";
import { Button } from "../../../../components/ui/button";
import { SearchInput } from "../../../../components/ui/SearchInput";
import { Switch } from "../../../../components/ui/switch";
import type { RepoHistoryEntry } from "../../../../types";

export interface InputURLPhaseProps {
  urlInput: string;
  setUrlInput: (v: string) => void;
  onScan: () => void;
  fullDepthEnabled: boolean;
  onToggleFullDepth: (enabled: boolean) => void;
  history: RepoHistoryEntry[];
  onSelectHistory: (entry: RepoHistoryEntry) => void;
  onPickLocalFile?: () => void;
  shareCodeDetected?: boolean;
}

export function InputURLPhase({
  urlInput,
  setUrlInput,
  onScan,
  fullDepthEnabled,
  onToggleFullDepth,
  history,
  onSelectHistory,
  onPickLocalFile,
  shareCodeDetected,
}: InputURLPhaseProps) {
  const { t } = useTranslation();

  return (
    <div className="px-6 py-6 space-y-5">
      {/* Illustration */}
      <div className="flex flex-col items-center gap-3 py-2">
        <div className="w-14 h-14 rounded-2xl bg-gradient-to-br from-primary/15 to-accent/15 flex items-center justify-center">
          <Download className="w-7 h-7 text-primary/80" />
        </div>
        <p className="text-sm text-muted-foreground text-center">{t("shareCodeImport.smartInputHint")}</p>
      </div>

      {/* Clipboard detected banner */}
      {shareCodeDetected && (
        <div className="flex items-center gap-2 bg-primary/5 border border-primary/20 rounded-xl px-3 py-2 text-xs text-primary">
          <Share2 className="w-3.5 h-3.5" />
          {t("shareCodeImport.detected")}
        </div>
      )}

      {/* URL input */}
      <div className="flex items-center gap-2.5">
        <SearchInput
          value={urlInput}
          onChange={(e) => setUrlInput(e.target.value)}
          placeholder={t("githubImportModal.placeholder")}
          className="h-11 rounded-2xl border-border/80 bg-background/80 shadow-inner pl-9"
          iconClassName="left-3 h-4 w-4"
          onKeyDown={(e) => {
            if (e.key === "Enter" && urlInput.trim()) onScan();
          }}
        />
        <Button
          size="sm"
          onClick={onScan}
          disabled={!urlInput.trim()}
          className="h-11 min-w-[108px] rounded-2xl border border-primary/40 bg-primary text-primary-foreground px-4 shadow-[0_10px_24px_-12px_rgba(var(--color-primary-rgb),0.85)] hover:bg-primary-hover"
        >
          <Search className="w-3.5 h-3.5 mr-1.5" />
          {t("githubImportModal.scan")}
        </Button>
      </div>

      <div className="flex items-start justify-between gap-3 rounded-xl border border-border/60 bg-muted/25 px-3 py-2.5">
        <div className="space-y-0.5">
          <p className="text-xs font-medium">{t("githubImportModal.fullDepthLabel")}</p>
          <p className="text-micro text-muted-foreground">{t("githubImportModal.fullDepthHint")}</p>
        </div>
        <Switch
          checked={fullDepthEnabled}
          onCheckedChange={(checked) => onToggleFullDepth(Boolean(checked))}
          aria-label={t("githubImportModal.fullDepthLabel")}
        />
      </div>

      {onPickLocalFile && (
        <div className="space-y-4 pt-1">
          <div className="flex items-center gap-4">
            <div className="flex-1 h-px bg-border/60"></div>
            <span className="text-micro text-muted-foreground font-medium uppercase tracking-wider">
              {t("common.or")}
            </span>
            <div className="flex-1 h-px bg-border/60"></div>
          </div>
          <Button
            variant="outline"
            className="w-full h-11 rounded-2xl border-dashed border-border hover:border-primary/40 hover:bg-primary/5 transition text-muted-foreground hover:text-foreground cursor-pointer shadow-sm"
            onClick={onPickLocalFile}
          >
            <Package className="w-4 h-4 mr-2" />
            {t("importBundleModal.pickFile", { defaultValue: "Import from Local File (.ags / .agd)" })}
          </Button>
        </div>
      )}

      {/* History */}
      {history.length > 0 && (
        <div className="space-y-2">
          <p className="text-xs text-muted-foreground font-medium uppercase tracking-wider">
            {t("githubImportModal.recentRepos")}
          </p>
          <div className="max-h-36 overflow-y-auto rounded-lg space-y-0.5">
            {history.map((entry) => (
              <button
                key={entry.source}
                onClick={() => onSelectHistory(entry)}
                className="w-full flex items-center gap-2.5 px-3 py-2 rounded-lg hover:bg-muted transition-colors text-left cursor-pointer group"
              >
                <Clock className="w-3.5 h-3.5 text-muted-foreground shrink-0 group-hover:text-foreground transition-colors" />
                <span className="text-sm truncate">{entry.source}</span>
              </button>
            ))}
          </div>
        </div>
      )}
    </div>
  );
}
