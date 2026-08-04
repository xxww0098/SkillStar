import { ChevronLeft, ChevronRight } from "lucide-react";
import { useEffect } from "react";
import { cn } from "../../lib/utils";

export type PrototypeVariantMeta = {
  key: string;
  name: string;
};

type PrototypeSwitcherProps = {
  variants: PrototypeVariantMeta[];
  current: string;
  /** Updates the URL search param so the choice is shareable / reload-stable. */
  onChange: (key: string) => void;
  className?: string;
};

/**
 * PROTOTYPE ONLY — floating bottom bar for flipping UI variants.
 * Hidden in production builds by the caller (gate on NODE_ENV).
 */
export function PrototypeSwitcher({ variants, current, onChange, className }: PrototypeSwitcherProps) {
  const index = Math.max(
    0,
    variants.findIndex((v) => v.key === current),
  );
  const active = variants[index] ?? variants[0];

  const cycle = (delta: number) => {
    if (variants.length === 0) return;
    const next = (index + delta + variants.length) % variants.length;
    onChange(variants[next].key);
  };

  useEffect(() => {
    const onKey = (event: KeyboardEvent) => {
      const target = event.target as HTMLElement | null;
      if (
        target &&
        (target.tagName === "INPUT" ||
          target.tagName === "TEXTAREA" ||
          target.tagName === "SELECT" ||
          target.isContentEditable)
      ) {
        return;
      }
      if (event.key === "ArrowLeft") {
        event.preventDefault();
        cycle(-1);
      } else if (event.key === "ArrowRight") {
        event.preventDefault();
        cycle(1);
      }
    };
    window.addEventListener("keydown", onKey);
    return () => window.removeEventListener("keydown", onKey);
    // eslint-disable-next-line react-hooks/exhaustive-deps
  }, [index, variants]);

  if (!active) return null;

  return (
    <div
      className={cn("pointer-events-none fixed inset-x-0 bottom-5 z-[200] flex justify-center px-4", className)}
      data-prototype-switcher
    >
      <div className="pointer-events-auto flex items-center gap-1 rounded-full border-2 border-amber-400 bg-zinc-950 px-2 py-1.5 text-amber-50 shadow-[0_12px_40px_-8px_rgba(0,0,0,0.65)]">
        <button
          type="button"
          aria-label="Previous prototype variant"
          onClick={() => cycle(-1)}
          className="rounded-full p-1.5 hover:bg-white/10"
        >
          <ChevronLeft className="h-4 w-4" />
        </button>
        <div className="min-w-[200px] px-2 text-center font-mono text-[11px] font-semibold tracking-wide">
          <span className="text-amber-300">{active.key}</span>
          <span className="mx-1.5 text-zinc-500">—</span>
          <span>{active.name}</span>
        </div>
        <button
          type="button"
          aria-label="Next prototype variant"
          onClick={() => cycle(1)}
          className="rounded-full p-1.5 hover:bg-white/10"
        >
          <ChevronRight className="h-4 w-4" />
        </button>
      </div>
    </div>
  );
}

/** Read / write `?variant=` on the current location (works with hash routing). */
export function usePrototypeVariantParam(defaultKey = "A") {
  const read = () => {
    if (typeof window === "undefined") return defaultKey;
    return new URLSearchParams(window.location.search).get("variant") ?? defaultKey;
  };

  const set = (key: string) => {
    const url = new URL(window.location.href);
    url.searchParams.set("variant", key);
    window.history.replaceState(null, "", `${url.pathname}${url.search}${url.hash}`);
    window.dispatchEvent(new Event("prototype-variant-change"));
  };

  return { read, set };
}
