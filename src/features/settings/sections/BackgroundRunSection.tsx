import { EyeOff } from "lucide-react";
import { useTranslation } from "react-i18next";
import { SettingsSectionHeader } from "../../../components/ui/SettingsSectionHeader";
import { Switch } from "../../../components/ui/switch";

const STORAGE_KEY = "skillstar:background-run";
const CHANGE_EVENT = "skillstar:background-run-changed";

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

  if (typeof window !== "undefined") {
    window.dispatchEvent(new CustomEvent<boolean>(CHANGE_EVENT, { detail: enabled }));
  }
}

export function onBackgroundRunChanged(listener: (enabled: boolean) => void): () => void {
  const handleChange = (event: Event) => {
    listener((event as CustomEvent<boolean>).detail);
  };

  window.addEventListener(CHANGE_EVENT, handleChange);
  return () => {
    window.removeEventListener(CHANGE_EVENT, handleChange);
  };
}

interface BackgroundRunSectionProps {
  enabled: boolean;
  onToggle: (enabled: boolean) => void;
}

export function BackgroundRunSection({ enabled, onToggle }: BackgroundRunSectionProps) {
  const { t } = useTranslation();

  return (
    <section>
      <SettingsSectionHeader icon={<EyeOff className="h-4 w-4" />} title={t("settings.backgroundRun")} />

      <div className="rounded-xl border border-border bg-card px-4 py-4">
        <div className="flex items-center justify-between gap-4">
          <p className="max-w-[520px] text-xs leading-relaxed text-muted-foreground">
            {t("settings.backgroundRunHint")}
          </p>

          <Switch checked={enabled} onCheckedChange={onToggle} aria-label={t("settings.backgroundRun")} />
        </div>
      </div>
    </section>
  );
}
