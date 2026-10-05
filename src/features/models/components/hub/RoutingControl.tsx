import { useQueryClient } from "@tanstack/react-query";
import { useTranslation } from "react-i18next";
import { tauriInvoke } from "@/lib/ipc";
import { modelsKeys } from "../../api/keys";
import { SegmentedControl } from "./SegmentedControl";

const ROUTING = ["smart", "order", "rotate", "usage"] as const;
const AFFINITY = ["auto", "session", "turn", "off"] as const;

/**
 * One owner's routing and affinity as a flat row: the name on the left,
 * two labelled segmented groups on the right. The buttons send those eight
 * words and nothing else. A caption that looks like a URL or a key is left
 * blank.
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
  const { t } = useTranslation();
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
      className="flex flex-wrap items-center gap-x-5 gap-y-2 rounded-lg px-2 py-2 transition-colors hover:bg-muted/30"
    >
      {heading ? (
        <div className="min-w-24 flex-1 basis-28 truncate text-xs font-medium text-foreground">{heading}</div>
      ) : null}
      <div className="flex items-center gap-2">
        <span className="w-8 shrink-0 text-[11px] text-muted-foreground">{t("models.gateway.routingLabel")}</span>
        <SegmentedControl
          ariaLabel="Routing"
          mono
          options={ROUTING.map((mode) => ({ id: mode, label: mode }))}
          value={routing as (typeof ROUTING)[number]}
          onSelect={(mode) => save(mode, affinity)}
        />
      </div>
      <div className="flex items-center gap-2">
        <span className="w-8 shrink-0 text-[11px] text-muted-foreground">{t("models.gateway.affinityLabel")}</span>
        <SegmentedControl
          ariaLabel="Affinity"
          mono
          options={AFFINITY.map((mode) => ({ id: mode, label: mode }))}
          value={affinity as (typeof AFFINITY)[number]}
          onSelect={(mode) => save(routing, mode)}
        />
      </div>
    </div>
  );
}

function plainId(value: string): string {
  if (value.includes("://") || value.includes("sk-") || value.includes("api.openai.com")) return "";
  return value;
}
