import { Loader2 } from "lucide-react";
import { Button } from "@/components/ui/button";

interface AutoImportBannerProps {
  catalogId: string;
  providerName?: string;
  scanningLocal: boolean;
  submitting: boolean;
  onImportLocal: () => void;
  onAutoScanAll: () => void;
}

/** Smart auto-import banner shown in create mode for locally-importable providers. */
export function AutoImportBanner({
  catalogId,
  providerName,
  scanningLocal,
  submitting,
  onImportLocal,
  onAutoScanAll,
}: AutoImportBannerProps) {
  return (
    <div className="relative overflow-hidden rounded-2xl border border-primary/20 bg-primary/5 p-4 text-center">
      <h4 className="mb-1.5 flex items-center justify-center gap-1.5 text-xs font-bold uppercase tracking-wider text-primary">
        ⚡ 智能自动导入 (Smart Auto-Import)
      </h4>
      <p className="mx-auto mb-3.5 max-w-xs text-[11px] leading-normal text-muted-foreground sm:max-w-sm">
        {catalogId
          ? `自动扫描并导入本地的 ${providerName ?? catalogId} 账号凭证，无需手动填写。`
          : "一键自动扫描并导入本地的 Codex / Antigravity / Qoder 账号凭证，无需手动填写。"}
      </p>
      <Button
        type="button"
        size="sm"
        variant="outline"
        onClick={catalogId ? onImportLocal : onAutoScanAll}
        disabled={scanningLocal || submitting}
        className="w-full border-primary/25 bg-primary/10 px-6 font-semibold text-primary hover:bg-primary/15 sm:w-auto"
      >
        {scanningLocal ? (
          <>
            <Loader2 className="mr-2 h-3.5 w-3.5 animate-spin" />
            正在扫描本地环境…
          </>
        ) : catalogId ? (
          "🚀 自动扫描并导入"
        ) : (
          "🚀 一键自动扫描并导入"
        )}
      </Button>
    </div>
  );
}
