import { Languages } from "lucide-react";
import { useTranslation } from "react-i18next";
import { SettingsSectionHeader } from "../../../components/ui/SettingsSectionHeader";
import { supportedLanguages } from "../../../i18n";
import { cn } from "../../../lib/utils";

interface LanguageSectionProps {
  currentLang: string;
  onLanguageChange: (lang: string) => void;
}

export function LanguageSection({ currentLang, onLanguageChange }: LanguageSectionProps) {
  const { t } = useTranslation();

  return (
    <section>
      <SettingsSectionHeader icon={<Languages className="h-4 w-4" />} title={t("settings.language")} />

      <div className="rounded-xl border border-border bg-card px-4 py-4">
        <div className="flex flex-col gap-3">
          <span className="text-sm font-medium">{t("settings.language")}</span>
          <div
            role="radiogroup"
            aria-label={t("settings.language")}
            className="flex w-fit flex-wrap items-center gap-1.5 rounded-lg bg-muted/50 p-1"
          >
            {supportedLanguages.map((lang) => (
              <button
                key={lang.code}
                type="button"
                role="radio"
                aria-checked={currentLang === lang.code || currentLang.startsWith(lang.code)}
                onClick={() => onLanguageChange(lang.code)}
                className={cn(
                  "h-8 cursor-pointer rounded-md px-4 text-xs font-medium transition focus-ring",
                  currentLang === lang.code || currentLang.startsWith(lang.code)
                    ? "bg-background text-foreground shadow-sm ring-1 ring-border/50"
                    : "text-muted-foreground hover:bg-muted hover:text-foreground",
                )}
              >
                {lang.label}
              </button>
            ))}
          </div>
        </div>
      </div>
    </section>
  );
}
