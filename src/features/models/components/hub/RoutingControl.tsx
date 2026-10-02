import { useQueryClient } from "@tanstack/react-query";
import { tauriInvoke } from "@/lib/ipc";
import { cn } from "@/lib/utils";
import { modelsKeys } from "../../api/keys";

const ROUTING = ["smart", "order", "rotate", "usage"] as const;
const AFFINITY = ["auto", "session", "turn", "off"] as const;

/**
 * Segmented routing and affinity inside one labelled card. The buttons send
 * those eight words and nothing else. A caption that looks like a URL or a
 * key is left blank.
 */
export function RoutingControl({
  owner,
  id,
  title,
  routing,
  affinity,
}: {
  owner: "provider" | "group";
  id: string;
  /** Display name when one is known; falls back to the id. */
  title?: string;
  routing: string;
  affinity: string;
}) {
  const queryClient = useQueryClient();
  const label = plainId(id);
  const heading = plainId(title ?? "") || label;

  function save(nextRouting: string, nextAffinity: string) {
    void tauriInvoke("save_routing", {
      owner,
      id,
      routing: nextRouting,
      affinity: nextAffinity,
    }).then(
      () => {
        void queryClient.invalidateQueries({ queryKey: [...modelsKeys.all, "routing-page"] });
      },
      () => undefined,
    );
  }

  return (
    <div
      role="group"
      aria-label={`routing ${owner} ${label}`.trim()}
      className="space-y-2 rounded-xl border border-border/60 bg-muted/20 px-3 py-2.5"
    >
      {heading ? <div className="truncate text-xs font-medium text-foreground">{heading}</div> : null}
      <div className="flex flex-wrap items-center gap-1.5">
        <span className="w-10 shrink-0 text-[11px] text-muted-foreground">路由</span>
        <div role="group" aria-label="Routing" className="flex flex-wrap rounded-lg bg-muted/60 p-0.5">
          {ROUTING.map((mode) => (
            <button
              key={mode}
              type="button"
              aria-pressed={routing === mode}
              onClick={() => save(mode, affinity)}
              className={cn(
                "rounded-md px-2 py-0.5 font-mono text-[11px] transition",
                routing === mode
                  ? "bg-background font-medium text-foreground shadow-sm ring-1 ring-border/60"
                  : "text-muted-foreground hover:text-foreground",
              )}
            >
              {mode}
            </button>
          ))}
        </div>
      </div>
      <div className="flex flex-wrap items-center gap-1.5">
        <span className="w-10 shrink-0 text-[11px] text-muted-foreground">亲和</span>
        <div role="group" aria-label="Affinity" className="flex flex-wrap rounded-lg bg-muted/60 p-0.5">
          {AFFINITY.map((mode) => (
            <button
              key={mode}
              type="button"
              aria-pressed={affinity === mode}
              onClick={() => save(routing, mode)}
              className={cn(
                "rounded-md px-2 py-0.5 font-mono text-[11px] transition",
                affinity === mode
                  ? "bg-background font-medium text-foreground shadow-sm ring-1 ring-border/60"
                  : "text-muted-foreground hover:text-foreground",
              )}
            >
              {mode}
            </button>
          ))}
        </div>
      </div>
    </div>
  );
}

function plainId(value: string): string {
  if (value.includes("://") || value.includes("sk-") || value.includes("api.openai.com")) return "";
  return value;
}
