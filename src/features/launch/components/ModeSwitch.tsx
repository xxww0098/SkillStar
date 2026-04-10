import { useTranslation } from "react-i18next";
import type { LaunchMode } from "../hooks/useLaunchConfig";

interface ModeSwitchProps {
  mode: LaunchMode;
  onModeChange: (mode: LaunchMode) => void;
  disabled?: boolean;
  disableMulti?: boolean;
}

export function ModeSwitch({ mode, onModeChange, disabled, disableMulti }: ModeSwitchProps) {
  const { t } = useTranslation();
  const singleDisabled = !!disabled;
  const multiDisabled = !!disabled || !!disableMulti;

  return (
    <div className="flex items-center rounded-lg border border-border p-0.5 bg-muted/30">
      <button
        type="button"
        disabled={singleDisabled}
        className={`px-3 py-1 rounded-md text-xs font-medium transition-all duration-200 ${
          mode === "single" ? "bg-card shadow-sm text-foreground" : "text-muted-foreground hover:text-foreground"
        } ${singleDisabled ? "opacity-50 cursor-not-allowed" : "cursor-pointer"}`}
        onClick={() => onModeChange("single")}
      >
        {t("launch.modeSingle", "Single")}
      </button>
      <button
        type="button"
        disabled={multiDisabled}
        className={`px-3 py-1 rounded-md text-xs font-medium transition-all duration-200 ${
          mode === "multi" ? "bg-card shadow-sm text-foreground" : "text-muted-foreground hover:text-foreground"
        } ${multiDisabled ? "opacity-50 cursor-not-allowed" : "cursor-pointer"}`}
        onClick={() => onModeChange("multi")}
        title={disableMulti ? t("launch.multiDisabledWindows", "Multi mode is disabled on Windows") : undefined}
      >
        {t("launch.modeMulti", "Multi")}
      </button>
    </div>
  );
}
