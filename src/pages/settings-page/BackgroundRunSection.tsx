import { EyeOff } from "lucide-react";
import { useTranslation } from "react-i18next";

const STORAGE_KEY = "skillstar:background-run";

export function readBackgroundRun(): boolean {
  try {
    return localStorage.getItem(STORAGE_KEY) === "true";
  } catch {
    return false;
  }
}

export function writeBackgroundRun(enabled: boolean): void {
  try {
    localStorage.setItem(STORAGE_KEY, String(enabled));
  } catch {
    // ignore
  }
}

interface BackgroundRunSectionProps {
  enabled: boolean;
  onToggle: (enabled: boolean) => void;
}

export function BackgroundRunSection({
  enabled,
  onToggle,
}: BackgroundRunSectionProps) {
  const { t } = useTranslation();

  return (
    <section>
      <div className="flex items-center gap-2 mb-3 px-1">
        <div className="w-7 h-7 rounded-lg bg-indigo-500/10 flex items-center justify-center shrink-0 border border-indigo-500/20">
          <EyeOff className="w-4 h-4 text-indigo-400" />
        </div>
        <h2 className="text-sm font-semibold text-foreground tracking-tight">
          {t("settings.backgroundRun", { defaultValue: "后台运行" })}
        </h2>
      </div>

      <div className="rounded-xl border border-border bg-card px-4 py-4">
        <div className="flex items-center justify-between gap-4">
          <p className="text-xs text-muted-foreground leading-relaxed max-w-[520px]">
            {t("settings.backgroundRunHint", {
              defaultValue:
                "开启后，关闭窗口时隐藏至后台继续检查技能更新。",
            })}
          </p>

          <button
            role="switch"
            aria-checked={enabled}
            onClick={() => onToggle(!enabled)}
            className={`
              relative inline-flex h-6 w-11 shrink-0 cursor-pointer rounded-full
              border-2 border-transparent transition-colors duration-200 ease-in-out
              focus-visible:outline-none focus-visible:ring-2 focus-visible:ring-ring focus-visible:ring-offset-2 focus-visible:ring-offset-background
              ${enabled ? "bg-primary" : "bg-muted"}
            `}
          >
            <span
              className={`
                pointer-events-none inline-block h-5 w-5 rounded-full bg-background shadow-lg ring-0
                transition-transform duration-200 ease-in-out
                ${enabled ? "translate-x-5" : "translate-x-0"}
              `}
            />
          </button>
        </div>
      </div>
    </section>
  );
}
