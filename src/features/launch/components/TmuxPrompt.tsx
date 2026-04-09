import { AlertTriangle, Copy } from "lucide-react";
import { useTranslation } from "react-i18next";
import { toast } from "sonner";

export function TmuxPrompt() {
  const { t } = useTranslation();

  const handleCopy = () => {
    navigator.clipboard.writeText("brew install tmux");
    toast.success(t("launch.tmuxCopied", "已复制到剪贴板"));
  };

  return (
    <div className="rounded-xl border border-amber-500/20 bg-amber-500/5 px-4 py-3">
      <div className="flex items-start gap-3">
        <div className="mt-0.5 flex h-7 w-7 shrink-0 items-center justify-center rounded-lg border border-amber-500/20 bg-amber-500/10">
          <AlertTriangle className="w-4 h-4 text-amber-400" />
        </div>
        <div className="min-w-0 flex-1">
          <div className="text-sm font-medium text-foreground">{t("launch.tmuxRequired", "需要安装 tmux")}</div>
          <p className="text-xs text-muted-foreground mt-1">
            {t("launch.tmuxDesc", "多面板模式需要 tmux。安装后即可使用多终端布局。")}
          </p>
          <div className="mt-2 flex items-center gap-2">
            <code className="text-xs bg-muted/50 rounded px-2 py-1 font-mono text-foreground/80">
              brew install tmux
            </code>
            <button
              type="button"
              onClick={handleCopy}
              className="flex items-center gap-1 px-2 py-1 text-xs text-muted-foreground hover:text-foreground rounded-md hover:bg-muted/50 transition-colors cursor-pointer"
            >
              <Copy className="w-3 h-3" />
              {t("launch.copy", "复制")}
            </button>
          </div>
        </div>
      </div>
    </div>
  );
}
