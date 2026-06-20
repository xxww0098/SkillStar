import { Laptop, Server } from "lucide-react";
import { useTranslation } from "react-i18next";
import { cn } from "../../../lib/utils";

export type MySkillsScope = "local" | "remote";

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
        "inline-flex items-center rounded-lg border border-border/50 bg-muted/30 p-0.5 text-xs font-medium",
        className,
      )}
      role="tablist"
      aria-label={t("mySkills.scopeLabel")}
    >
      <button
        type="button"
        role="tab"
        aria-selected={scope === "local"}
        className={cn(
          "inline-flex items-center gap-1.5 rounded-md px-2.5 py-1 transition-colors",
          scope === "local" ? "bg-background text-foreground shadow-sm" : "text-muted-foreground hover:text-foreground",
        )}
        onClick={() => onScopeChange("local")}
      >
        <Laptop className="size-3.5" />
        {t("mySkills.scopeLocal")}
      </button>
      <button
        type="button"
        role="tab"
        aria-selected={scope === "remote"}
        className={cn(
          "inline-flex items-center gap-1.5 rounded-md px-2.5 py-1 transition-colors",
          scope === "remote"
            ? "bg-background text-foreground shadow-sm"
            : "text-muted-foreground hover:text-foreground",
        )}
        onClick={() => onScopeChange("remote")}
      >
        <Server className="size-3.5" />
        {t("mySkills.scopeRemote")}
      </button>
    </div>
  );
}
