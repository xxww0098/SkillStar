import { useTranslation } from "react-i18next";
import { MessageSquareText } from "lucide-react";
import type { AiConfig, MymemoryUsageStats } from "../../types";

interface ShortTextServiceSectionProps {
  localAiConfig: AiConfig;
  mymemoryUsage: MymemoryUsageStats | null;
  onConfigChange: (next: AiConfig) => void;
}

export function ShortTextServiceSection({
  localAiConfig,
  mymemoryUsage,
  onConfigChange,
}: ShortTextServiceSectionProps) {
  const { t } = useTranslation();
  const formControlClass =
    "flex h-9 rounded-xl border border-input-border bg-input backdrop-blur-sm px-3 text-sm text-foreground shadow-sm transition duration-200 focus-visible:outline-none focus-visible:ring-2 focus-visible:ring-primary/40 focus-visible:border-primary/60";
  const formatter = new Intl.NumberFormat();
  const formattedDailyChars =
    typeof mymemoryUsage?.daily_chars_sent === "number"
      ? formatter.format(mymemoryUsage.daily_chars_sent)
      : null;
  const formattedTotalChars =
    typeof mymemoryUsage?.total_chars_sent === "number"
      ? formatter.format(mymemoryUsage.total_chars_sent)
      : null;

  return (
    <section>
      <div className="rounded-xl border border-border bg-card flex items-center justify-between gap-4 px-4 py-3">
        <div className="flex items-center gap-3 min-w-0">
          <div className="w-8 h-8 rounded-lg bg-indigo-500/10 flex items-center justify-center shrink-0 border border-indigo-500/20">
            <MessageSquareText className="w-4 h-4 text-indigo-500" />
          </div>
          <div className="min-w-0">
            <div className="text-sm font-medium text-foreground whitespace-nowrap truncate">
              {t("settings.shortTextServiceTitle", { defaultValue: "Short Text Translation" })}
            </div>
            <div className="text-[10px] text-muted-foreground mt-0.5 whitespace-nowrap truncate">
              {formattedDailyChars && formattedTotalChars
                ? t("settings.myMemoryUsageDailyWithTotal", {
                    daily: formattedDailyChars,
                    total: formattedTotalChars,
                  })
                : t("settings.myMemoryCharsSentUnknown")}
            </div>
          </div>
        </div>

        <select
          value={localAiConfig.short_text_priority}
          onChange={(e) =>
            onConfigChange({
              ...localAiConfig,
              short_text_priority: e.target.value as "ai_first" | "mymemory_first",
            })
          }
          className={`${formControlClass} max-w-[280px] shrink`}
        >
          <option value="mymemory_first">{t("settings.shortTextPriorityMyMemoryFirst")}</option>
          <option value="ai_first">{t("settings.shortTextPriorityAiFirst")}</option>
        </select>
      </div>
    </section>
  );
}
