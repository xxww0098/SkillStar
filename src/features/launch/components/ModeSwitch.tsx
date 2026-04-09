import { useTranslation } from "react-i18next";
import type { LaunchMode } from "../hooks/useLaunchConfig";

interface ModeSwitchProps {
  mode: LaunchMode;
  onModeChange: (mode: LaunchMode) => void;
  disabled?: boolean;
}

export function ModeSwitch({ mode, onModeChange, disabled }: ModeSwitchProps) {
  const { t } = useTranslation();

  return (
    <div className="flex items-center rounded-lg border border-border p-0.5 bg-muted/30">
      <button
        type="button"
        disabled={disabled}
        className={`px-3 py-1 rounded-md text-xs font-medium transition-all duration-200 ${
          mode === "single" ? "bg-card shadow-sm text-foreground" : "text-muted-foreground hover:text-foreground"
        } ${disabled ? "opacity-50 cursor-not-allowed" : "cursor-pointer"}`}
        onClick={() => onModeChange("single")}
      >
        {t("launch.modeSingle", "单终端")}
      </button>
      <button
        type="button"
        disabled={disabled}
        className={`px-3 py-1 rounded-md text-xs font-medium transition-all duration-200 ${
          mode === "multi" ? "bg-card shadow-sm text-foreground" : "text-muted-foreground hover:text-foreground"
        } ${disabled ? "opacity-50 cursor-not-allowed" : "cursor-pointer"}`}
        onClick={() => onModeChange("multi")}
      >
        {t("launch.modeMulti", "多面板")}
      </button>
    </div>
  );
}
