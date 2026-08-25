import { Laptop, Server, UsersRound } from "lucide-react";
import { useTranslation } from "react-i18next";
import { cn } from "../../../lib/utils";

export type MySkillsScope = "local" | "remote" | "shared";

interface Props {
  scope: MySkillsScope;
  onScopeChange: (scope: MySkillsScope) => void;
  className?: string;
}

export function MySkillsScopeSwitch({ scope, onScopeChange, className }: Props) {
  const { t } = useTranslation();
  return (
    <div
      className={cn(
        "inline-flex items-center rounded-lg border border-border/80 bg-background/50 p-0.5 text-xs font-medium shadow-2xs",
        className,
      )}
      role="tablist"
      aria-label={t("mySkills.scopeLabel")}
    >
      <button
        type="button"
        role="tab"
        aria-selected={scope === "local"}
        title={t("mySkills.scopeLocal")}
        aria-label={t("mySkills.scopeLocal")}
        className={cn(
          "inline-flex items-center justify-center rounded-md px-2 py-1 transition-all duration-150 cursor-pointer focus-ring select-none",
          scope === "local"
            ? "bg-primary/20 text-primary font-semibold shadow-xs ring-1 ring-primary/30 dark:bg-primary/25"
            : "text-muted-foreground hover:text-foreground font-medium",
        )}
        onClick={() => onScopeChange("local")}
      >
        <Laptop className="size-3.5" strokeWidth={scope === "local" ? 2.4 : 2} />
      </button>
      <button
        type="button"
        role="tab"
        aria-selected={scope === "shared"}
        title={t("mySkills.scopeShared", { defaultValue: "Shared" })}
        aria-label={t("mySkills.scopeShared", { defaultValue: "Shared" })}
        className={cn(
          "inline-flex items-center justify-center rounded-md px-2 py-1 transition-all duration-150 cursor-pointer focus-ring select-none",
          scope === "shared"
            ? "bg-primary/20 text-primary font-semibold shadow-xs ring-1 ring-primary/30 dark:bg-primary/25"
            : "text-muted-foreground hover:text-foreground font-medium",
        )}
        onClick={() => onScopeChange("shared")}
      >
        <UsersRound className="size-3.5" strokeWidth={scope === "shared" ? 2.4 : 2} />
      </button>
      <button
        type="button"
        role="tab"
        aria-selected={scope === "remote"}
        title={t("mySkills.scopeRemote")}
        aria-label={t("mySkills.scopeRemote")}
        className={cn(
          "inline-flex items-center justify-center rounded-md px-2 py-1 transition-all duration-150 cursor-pointer focus-ring select-none",
          scope === "remote"
            ? "bg-primary/20 text-primary font-semibold shadow-xs ring-1 ring-primary/30 dark:bg-primary/25"
            : "text-muted-foreground hover:text-foreground font-medium",
        )}
        onClick={() => onScopeChange("remote")}
      >
        <Server className="size-3.5" strokeWidth={scope === "remote" ? 2.4 : 2} />
      </button>
    </div>
  );
}
