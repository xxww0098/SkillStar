import { invoke } from "@tauri-apps/api/core";
import { Loader2, Rocket } from "lucide-react";
import { useState } from "react";
import { useTranslation } from "react-i18next";
import { toast } from "sonner";
import type { LaunchConfig } from "../hooks/useLaunchConfig";

interface DeployButtonProps {
  config: LaunchConfig;
  projectPath: string;
  disabled?: boolean;
}

interface DeployResult {
  success: boolean;
  message: string;
  script_path: string | null;
}

export function DeployButton({ config, projectPath, disabled }: DeployButtonProps) {
  const { t } = useTranslation();
  const [deploying, setDeploying] = useState(false);

  const handleDeploy = async () => {
    if (deploying || disabled) return;
    setDeploying(true);

    try {
      const result = await invoke<DeployResult>("deploy_launch", {
        config: { ...config, updatedAt: Date.now() },
        projectPath,
      });

      if (result.success) {
        toast.success(result.message);
      } else {
        toast.error(result.message);
      }
    } catch (e) {
      toast.error(String(e));
    } finally {
      setDeploying(false);
    }
  };

  return (
    <button
      type="button"
      onClick={handleDeploy}
      disabled={disabled || deploying}
      className={`flex items-center gap-2 px-4 py-2 rounded-lg text-sm font-medium transition-all duration-200 ${
        disabled || deploying
          ? "bg-muted text-muted-foreground cursor-not-allowed opacity-60"
          : "bg-primary text-primary-foreground hover:bg-primary/90 shadow-sm hover:shadow-md cursor-pointer active:scale-[0.98]"
      }`}
    >
      {deploying ? <Loader2 className="w-4 h-4 animate-spin" /> : <Rocket className="w-4 h-4" />}
      {t("launch.deploy", "部署")}
    </button>
  );
}

