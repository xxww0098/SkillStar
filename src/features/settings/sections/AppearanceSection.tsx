import { Paintbrush } from "lucide-react";
import { useTranslation } from "react-i18next";
import { SettingsSectionHeader } from "../../../components/ui/SettingsSectionHeader";
import type { BackgroundStyle } from "../../../lib/backgroundStyle";
import { cn } from "../../../lib/utils";

interface AppearanceSectionProps {
  backgroundStyle: BackgroundStyle;
  onBackgroundStyleChange: (style: BackgroundStyle) => void;
}

const BACKGROUND_OPTIONS: { id: BackgroundStyle; labelKey: string }[] = [
  { id: "paper", labelKey: "settings.backgroundPaper" },
  { id: "current", labelKey: "settings.backgroundCurrent" },
];

export function AppearanceSection({ backgroundStyle, onBackgroundStyleChange }: AppearanceSectionProps) {
  const { t } = useTranslation();

  return (
    <section>
      <SettingsSectionHeader icon={<Paintbrush className="h-4 w-4" />} title={t("settings.backgroundStyle")} />

      <div className="rounded-xl border border-border bg-card px-4 py-4">
        <div className="flex flex-col gap-3">
          <span className="text-sm font-medium">{t("settings.backgroundStyle")}</span>
          <div
            role="radiogroup"
            aria-label={t("settings.backgroundStyle")}
            className="flex w-fit items-center gap-1.5 rounded-lg bg-muted/50 p-1"
          >
            {BACKGROUND_OPTIONS.map((option) => (
              <button
                key={option.id}
                type="button"
                role="radio"
                aria-checked={backgroundStyle === option.id}
                onClick={() => onBackgroundStyleChange(option.id)}
                className={cn(
                  "h-8 cursor-pointer rounded-md px-4 text-xs font-medium transition focus-ring",
                  backgroundStyle === option.id
                    ? "bg-background text-foreground shadow-sm ring-1 ring-border/50"
                    : "text-muted-foreground hover:bg-muted hover:text-foreground",
                )}
              >
                {t(option.labelKey)}
              </button>
            ))}
          </div>
        </div>
      </div>
    </section>
  );
}
