import { MessageSquareText } from "lucide-react";
import { useTranslation } from "react-i18next";

/**
 * Short Text Translation status card.
 * Now simplified — no more MyMemory priority selection.
 * Short text always uses the AI provider configured in Settings > AI Provider.
 */
export function ShortTextServiceSection() {
  const { t } = useTranslation();

  return (
    <section>
      <div className="rounded-xl border border-border bg-card flex items-center gap-4 px-4 py-3">
        <div className="flex items-center gap-3 min-w-0">
          <div className="w-8 h-8 rounded-lg bg-indigo-500/10 flex items-center justify-center shrink-0 border border-indigo-500/20">
            <MessageSquareText className="w-4 h-4 text-indigo-500" />
          </div>
          <div className="min-w-0">
            <div className="text-sm font-medium text-foreground whitespace-nowrap truncate">
              {t("settings.shortTextServiceTitle", { defaultValue: "Short Text Translation" })}
            </div>
            <div className="text-[10px] text-muted-foreground mt-0.5 whitespace-nowrap truncate">
              {t("settings.shortTextServiceHint", {
                defaultValue: "Uses AI provider for skill descriptions. DeepL/DeepLX used as fallback.",
              })}
            </div>
          </div>
        </div>
      </div>
    </section>
  );
}
