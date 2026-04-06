import { AlertTriangle, Check, Download, GitBranch, KeyRound, Package } from "lucide-react";
import { useTranslation } from "react-i18next";
import { Button } from "../../../../components/ui/button";
import type { ShareCodeData } from "../../../../lib/shareCode";

export interface ShareCodePreviewPhaseProps {
  data: ShareCodeData | null;
  error: string;
  password: string;
  existingNames?: string[];
  onPasswordChange: (v: string) => void;
  onRetryWithPassword: (pw: string) => void;
  onInstall: () => void;
  onBack: () => void;
}

export function ShareCodePreviewPhase({
  data,
  error,
  password,
  existingNames = [],
  onPasswordChange,
  onRetryWithPassword,
  onInstall,
  onBack,
}: ShareCodePreviewPhaseProps) {
  const { t } = useTranslation();

  // Error / needs password state
  if (!data) {
    return (
      <div className="px-6 py-6 space-y-4">
        <div className="flex flex-col items-center gap-3 py-2">
          <div className="w-14 h-14 rounded-2xl bg-amber-500/10 flex items-center justify-center">
            <KeyRound className="w-7 h-7 text-amber-500/80" />
          </div>
          <p className="text-sm text-destructive text-center">{error}</p>
        </div>

        {error.includes("password") && (
          <div className="space-y-3">
            <input
              type="password"
              value={password}
              onChange={(e) => onPasswordChange(e.target.value)}
              placeholder={t("importShareCodeModal.passwordPlaceholder")}
              className="w-full h-11 rounded-2xl border border-input bg-background/80 px-4 text-sm shadow-inner placeholder:text-muted-foreground focus-visible:outline-none focus-visible:ring-1 focus-visible:ring-ring"
              onKeyDown={(e) => {
                if (e.key === "Enter" && password.trim()) onRetryWithPassword(password);
              }}
            />
            <Button
              className="w-full h-11 rounded-2xl"
              onClick={() => onRetryWithPassword(password)}
              disabled={!password.trim()}
            >
              {t("shareCodeImport.retryWithPassword")}
            </Button>
          </div>
        )}

        <Button variant="ghost" size="sm" onClick={onBack} className="w-full">
          {t("common.back")}
        </Button>
      </div>
    );
  }

  // Success: show skill preview
  const existingSet = new Set(existingNames.map((name) => name.trim().toLowerCase()));
  const installableCount = data.s.filter((skill) => !existingSet.has(skill.n.trim().toLowerCase())).length;
  const hasEmbedded = data.s.some((s) => s.c);
  const hasPrivate = data.s.some((s) => s.p);

  return (
    <div className="flex flex-col">
      {/* Header info */}
      <div className="px-6 pt-5 pb-3 space-y-3">
        <div className="flex items-center gap-3">
          <div className="w-10 h-10 rounded-xl bg-gradient-to-br from-primary/15 to-accent/15 flex items-center justify-center text-lg">
            {data.i || "\u2B50"}
          </div>
          <div className="flex-1 min-w-0">
            <h3 className="text-sm font-semibold truncate">{data.n}</h3>
            <p className="text-xs text-muted-foreground truncate">{data.d}</p>
          </div>
          <span className="text-micro bg-muted px-1.5 py-0.5 rounded-md text-muted-foreground/80">
            {data.s.length} skills
          </span>
        </div>

        {hasEmbedded && (
          <div className="flex items-start gap-2 text-xs text-primary bg-primary/5 border border-primary/20 rounded-lg px-2.5 py-2">
            <Package className="w-3.5 h-3.5 shrink-0 mt-0.5" />
            <span>{t("importShareCodeModal.embeddedDesc")}</span>
          </div>
        )}

        {hasPrivate && (
          <div className="flex items-start gap-2 text-xs text-amber-600 bg-amber-500/5 border border-amber-500/20 rounded-lg px-2.5 py-2">
            <AlertTriangle className="w-3.5 h-3.5 shrink-0 mt-0.5" />
            <span>{t("importShareCodeModal.privateRepoDesc")}</span>
          </div>
        )}

        {existingNames.length > 0 && (
          <div className="flex items-start gap-2 text-xs text-emerald-600 bg-emerald-500/5 border border-emerald-500/20 rounded-lg px-2.5 py-2">
            <Check className="w-3.5 h-3.5 shrink-0 mt-0.5" />
            <span>{t("shareCodeImport.alreadyDetected", { count: existingNames.length })}</span>
          </div>
        )}
      </div>

      {/* Skill list */}
      <div className="px-6 pb-2 max-h-[35vh] overflow-y-auto">
        <div className="space-y-0.5">
          {data.s.map((skill) => {
            const isExisting = existingSet.has(skill.n.trim().toLowerCase());
            return (
              <div
                key={skill.n}
                className="flex items-center gap-3 px-3 py-2 rounded-xl hover:bg-muted transition-colors"
              >
                <div
                  className={`w-4 h-4 rounded border-[1.5px] flex items-center justify-center ${
                    isExisting ? "bg-emerald-500/20 border-emerald-500/40" : "bg-primary border-primary"
                  }`}
                >
                  <Check className="w-2.5 h-2.5 text-white" strokeWidth={3} />
                </div>
                <div className="flex-1 min-w-0">
                  <span className="text-caption font-medium">{skill.n}</span>
                  {isExisting && (
                    <span className="ml-1.5 text-micro px-1.5 py-0.5 rounded-full bg-emerald-500/10 text-emerald-600 font-medium">
                      {t("githubImportModal.installed")}
                    </span>
                  )}
                  {skill.c && (
                    <span className="ml-1.5 text-micro px-1.5 py-0.5 rounded-full bg-indigo-500/10 text-indigo-400 font-medium">
                      embedded
                    </span>
                  )}
                  {skill.p && (
                    <span className="ml-1.5 text-micro px-1.5 py-0.5 rounded-full bg-amber-500/10 text-amber-500 font-medium">
                      private
                    </span>
                  )}
                </div>
                {skill.u && <GitBranch className="w-3 h-3 text-muted-foreground" />}
              </div>
            );
          })}
        </div>
      </div>

      {/* Actions */}
      <div className="px-6 py-3.5 border-t border-border/60 flex items-center justify-between">
        <Button variant="ghost" size="sm" onClick={onBack}>
          {t("common.back")}
        </Button>
        <Button size="sm" onClick={onInstall} className="px-5">
          <Download className="w-3.5 h-3.5 mr-1.5" />
          {t("shareCodeImport.installSkills", { count: installableCount })}
        </Button>
      </div>
    </div>
  );
}
