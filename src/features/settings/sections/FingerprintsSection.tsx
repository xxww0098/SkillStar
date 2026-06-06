import { ChevronDown, ChevronRight, Fingerprint } from "lucide-react";
import { useState } from "react";
import { FingerprintsPanel } from "@/features/fingerprints";

/**
 * Settings → 设备指纹 section. Collapsible to match the rest of the
 * Settings page; defaults to closed since most users won't touch this.
 */
export function FingerprintsSection() {
  const [expanded, setExpanded] = useState(false);
  return (
    <div className="rounded-2xl border border-zinc-200/60 bg-white/70 backdrop-blur-sm">
      <button
        type="button"
        className="flex w-full items-center justify-between gap-3 px-4 py-3 text-left"
        onClick={() => setExpanded((v) => !v)}
        aria-expanded={expanded}
      >
        <div className="flex items-center gap-2">
          <Fingerprint className="h-4 w-4 text-violet-500" />
          <div>
            <div className="text-sm font-semibold">设备指纹</div>
            <div className="text-[11px] text-muted-foreground">
              管理浏览器画像，绑定到 Usage 订阅以伪装 TLS / HTTP 头
            </div>
          </div>
        </div>
        {expanded ? (
          <ChevronDown className="h-4 w-4 text-zinc-400" />
        ) : (
          <ChevronRight className="h-4 w-4 text-zinc-400" />
        )}
      </button>
      {expanded && (
        <div className="border-t border-zinc-200/60 px-4 py-4">
          <FingerprintsPanel />
        </div>
      )}
    </div>
  );
}
