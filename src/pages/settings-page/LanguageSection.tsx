import { useTranslation } from "react-i18next";
import { Languages } from "lucide-react";
import { cn } from "../../lib/utils";
import { supportedLanguages } from "../../i18n";

interface LanguageSectionProps {
  currentLang: string;
  onLanguageChange: (lang: string) => void;
}

export function LanguageSection({ currentLang, onLanguageChange }: LanguageSectionProps) {
  const { t } = useTranslation();

  return (
    <section>
      <div className="flex items-center gap-2 mb-3 px-1">
        <div className="w-7 h-7 rounded-lg bg-teal-500/10 flex items-center justify-center shrink-0 border border-teal-500/20">
          <Languages className="w-4 h-4 text-teal-500" />
        </div>
        <h2 className="text-sm font-semibold text-foreground tracking-tight">{t("settings.language")}</h2>
      </div>

      <div className="rounded-xl border border-border bg-card px-4 py-4">
        <div className="flex flex-col gap-3">
          <span className="text-sm font-medium">{t("settings.language")}</span>
          <div className="flex items-center gap-1.5 flex-wrap p-1 bg-muted/50 rounded-lg w-fit">
            {supportedLanguages.map((lang) => (
              <button
                key={lang.code}
                onClick={() => onLanguageChange(lang.code)}
                className={cn(
                  "px-4 py-1.5 rounded-md text-xs font-medium transition cursor-pointer",
                  currentLang === lang.code || currentLang.startsWith(lang.code)
                    ? "bg-background text-foreground shadow-sm ring-1 ring-border/50"
                    : "text-muted-foreground hover:text-foreground hover:bg-muted"
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
