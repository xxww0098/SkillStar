import { Paintbrush } from "lucide-react";
import { useTranslation } from "react-i18next";
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
      <div className="flex items-center gap-2 mb-3 px-1">
        <div className="w-7 h-7 rounded-lg bg-pink-500/10 flex items-center justify-center shrink-0 border border-pink-500/20">
          <Paintbrush className="w-4 h-4 text-pink-500" />
        </div>
        <h2 className="text-sm font-semibold text-foreground tracking-tight">{t("settings.backgroundStyle")}</h2>
      </div>

      <div className="rounded-xl border border-border bg-card px-4 py-4">
        <div className="flex flex-col gap-3">
          <span className="text-sm font-medium">{t("settings.backgroundStyle")}</span>
          <div className="flex items-center gap-1.5 p-1 bg-muted/50 rounded-lg w-fit">
            {BACKGROUND_OPTIONS.map((option) => (
              <button
                key={option.id}
                onClick={() => onBackgroundStyleChange(option.id)}
                className={cn(
                  "px-4 py-1.5 rounded-md text-xs font-medium transition cursor-pointer",
                  backgroundStyle === option.id
                    ? "bg-background text-foreground shadow-sm ring-1 ring-border/50"
                    : "text-muted-foreground hover:text-foreground hover:bg-muted",
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
