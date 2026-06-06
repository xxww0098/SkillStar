import { Fingerprint } from "lucide-react";
import { useEffect, useState } from "react";
import { cn } from "@/lib/utils";
import { fingerprintsApi } from "../api";
import type { FingerprintRow } from "../types";
import { tlsLabel } from "../utils";

interface FingerprintPickerProps {
  /** Current binding (undefined = no binding = "default" pill). */
  value?: string | null;
  /** Called with `null` for "no binding", or a fingerprint id. */
  onChange: (id: string | null) => void;
  disabled?: boolean;
  /** Pass `false` to hide the section title (e.g. inside a section). */
  showLabel?: boolean;
}

/**
 * Compact picker used inside SubscriptionEditDialog. Renders a horizontal
 * pill row — "default" + one pill per fingerprint — letting the user bind
 * a subscription to a stored fingerprint with one click.
 *
 * Lazily fetches the fingerprint list on mount; doesn't share state with
 * `useFingerprints` so opening the dialog never blocks on the Settings
 * panel being mounted.
 */
export function FingerprintPicker({ value, onChange, disabled, showLabel = true }: FingerprintPickerProps) {
  const [items, setItems] = useState<FingerprintRow[]>([]);
  const [loading, setLoading] = useState(true);
  const [error, setError] = useState<string | null>(null);

  useEffect(() => {
    let cancelled = false;
    (async () => {
      try {
        const dto = await fingerprintsApi.list();
        if (!cancelled) setItems(dto.items);
      } catch (e) {
        if (!cancelled) setError(e instanceof Error ? e.message : String(e));
      } finally {
        if (!cancelled) setLoading(false);
      }
    })();
    return () => {
      cancelled = true;
    };
  }, []);

  if (loading) {
    return <div className="text-xs text-muted-foreground">加载指纹…</div>;
  }
  if (error) {
    return <div className="text-xs text-red-600">指纹加载失败：{error}</div>;
  }

  // Hide picker entirely if only "original" exists — keeps simple UIs simple.
  if (items.length <= 1) {
    return <div className="text-[11px] text-muted-foreground">没有自定义指纹。先在 设置 → 设备指纹 创建一个。</div>;
  }

  return (
    <div className="flex flex-col gap-1.5">
      {showLabel && (
        <div className="flex items-center gap-1.5 text-xs font-medium text-zinc-700">
          <Fingerprint className="h-3.5 w-3.5 text-violet-500" />
          指纹绑定
        </div>
      )}
      <div className="flex flex-wrap gap-1.5">
        <Pill label="默认" active={!value} disabled={disabled} onClick={() => onChange(null)} />
        {items
          .filter((x) => !x.isOriginal)
          .map((row) => (
            <Pill
              key={row.id}
              label={row.name}
              sublabel={tlsLabel(row.tls)}
              active={value === row.id}
              disabled={disabled}
              onClick={() => onChange(row.id)}
            />
          ))}
      </div>
    </div>
  );
}

interface PillProps {
  label: string;
  sublabel?: string;
  active?: boolean;
  disabled?: boolean;
  onClick: () => void;
}

function Pill({ label, sublabel, active, disabled, onClick }: PillProps) {
  return (
    <button
      type="button"
      onClick={onClick}
      disabled={disabled}
      className={cn(
        "inline-flex flex-col items-start rounded-full border px-2.5 py-1 text-[11px] leading-tight transition-all",
        active
          ? "border-violet-400 bg-violet-50 text-violet-800 ring-2 ring-violet-200"
          : "border-zinc-200 bg-white text-zinc-700 hover:border-zinc-300",
        disabled && "opacity-50 cursor-not-allowed",
      )}
    >
      <span className="font-medium">{label}</span>
      {sublabel && <span className="text-[10px] text-muted-foreground">{sublabel}</span>}
    </button>
  );
}
