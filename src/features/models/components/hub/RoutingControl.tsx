import { useQueryClient } from "@tanstack/react-query";
import { tauriInvoke } from "@/lib/ipc";
import { cn } from "@/lib/utils";
import { modelsKeys } from "../../api/keys";

const ROUTING = ["smart", "order", "rotate", "usage"] as const;
const AFFINITY = ["auto", "session", "turn", "off"] as const;

/**
 * Segmented routing and affinity. The buttons send those eight words and
 * nothing else. A caption that looks like a URL or a key is left blank.
 */
export function RoutingControl({
  owner,
  id,
  routing,
  affinity,
}: {
  owner: "provider" | "group";
  id: string;
  routing: string;
  affinity: string;
}) {
  const queryClient = useQueryClient();
  const label = plainId(id);

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
    <div role="group" aria-label={`routing ${owner} ${label}`.trim()} className="shrink-0 space-y-1 px-4 pt-3">
      {label ? <div className="truncate text-xs text-muted-foreground">{label}</div> : null}
      <div role="group" aria-label="Routing" className="flex flex-wrap gap-1">
        {ROUTING.map((mode) => (
          <button
            key={mode}
            type="button"
            aria-pressed={routing === mode}
            onClick={() => save(mode, affinity)}
            className={cn(
              "rounded-lg px-2 py-1 text-xs text-foreground",
              routing === mode ? "bg-primary/15 font-medium" : "hover:bg-muted/40",
            )}
          >
            {mode}
          </button>
        ))}
      </div>
      <div role="group" aria-label="Affinity" className="flex flex-wrap gap-1">
        {AFFINITY.map((mode) => (
          <button
            key={mode}
            type="button"
            aria-pressed={affinity === mode}
            onClick={() => save(routing, mode)}
            className={cn(
              "rounded-lg px-2 py-1 text-xs text-foreground",
              affinity === mode ? "bg-primary/15 font-medium" : "hover:bg-muted/40",
            )}
          >
            {mode}
          </button>
        ))}
      </div>
    </div>
  );
}

function plainId(value: string): string {
  if (value.includes("://") || value.includes("sk-") || value.includes("api.openai.com")) return "";
  return value;
}
