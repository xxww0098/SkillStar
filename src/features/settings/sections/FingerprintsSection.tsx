import { ChevronDown, ChevronRight, Fingerprint } from "lucide-react";
import { useState } from "react";
import { useTranslation } from "react-i18next";
import { FingerprintsPanel } from "@/features/fingerprints";

/**
 * Settings → 设备指纹 section. Collapsible to match the rest of the
 * Settings page; defaults to closed since most users won't touch this.
 */
export function FingerprintsSection() {
  const { t } = useTranslation();
  const [expanded, setExpanded] = useState(false);

  return (
    <section>
      <div className="flex items-center gap-2 mb-3 px-1">
        <div className="w-7 h-7 rounded-lg bg-violet-500/10 flex items-center justify-center shrink-0 border border-violet-500/20">
          <Fingerprint className="w-4 h-4 text-violet-400" />
        </div>
        <h2 className="text-sm font-semibold text-foreground tracking-tight">
          {t("settings.fingerprintsTitle", { defaultValue: "设备指纹" })}
        </h2>
      </div>

      <div className="rounded-xl border border-border bg-card">
        <button
          type="button"
          className="flex w-full items-center justify-between gap-3 px-4 py-3 text-left cursor-pointer select-none"
          onClick={() => setExpanded((v) => !v)}
          aria-expanded={expanded}
        >
          <div>
            <p className="text-xs font-medium text-foreground">
              {t("settings.fingerprintsProfile", { defaultValue: "管理浏览器画像" })}
            </p>
            <p className="text-[11px] text-muted-foreground">
              {t("settings.fingerprintsHint", { defaultValue: "绑定到 Usage 订阅以伪装 TLS / HTTP 头" })}
            </p>
          </div>
          {expanded ? (
            <ChevronDown className="w-4 h-4 text-muted-foreground" />
          ) : (
            <ChevronRight className="w-4 h-4 text-muted-foreground" />
          )}
        </button>
        {expanded && (
          <div className="border-t border-border/50 px-4 py-4">
            <FingerprintsPanel showHeader={false} />
          </div>
        )}
      </div>
    </section>
  );
}
