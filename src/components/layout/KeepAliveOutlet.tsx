import { useEffect, useState, type ReactNode } from "react";
import { PageActivityContext } from "../../lib/pageActivity";
import { cn } from "../../lib/utils";

/**
 * Keep the last few visited pages mounted and hidden.
 *
 * Skills-mode list pages throw away search, scroll and the already-loaded
 * chunk every time the sidebar switches. Remounting a 21k-row MCP catalog or
 * a populated SkillGrid reads as "the app is slow". An LRU of hidden pages
 * makes back-navigation instant without keeping every route in memory.
 *
 * Hidden pages retain state and query observers. The activity context lets
 * portalled modals deactivate too, beyond the DOM display:none boundary.
 * Do not put drill-in pages (publisher detail) here — their identity is the
 * selected record, not the route name.
 */
export function KeepAliveOutlet({
  active,
  keep,
  limit = 3,
  render,
}: {
  active: string;
  keep: readonly string[];
  limit?: number;
  render: (id: string) => ReactNode;
}) {
  const [cached, setCached] = useState<string[]>(() => (keep.includes(active) ? [active] : []));

  useEffect(() => {
    if (!keep.includes(active)) return;
    setCached((prev) => {
      const next = [active, ...prev.filter((id) => id !== active && keep.includes(id))];
      return next.slice(0, limit);
    });
  }, [active, keep, limit]);

  const keepActive = keep.includes(active);

  return (
    <>
      {cached.map((id) => {
        const shown = id === active;
        return (
          <PageActivityContext.Provider key={id} value={shown}>
            <div
              hidden={!shown}
              aria-hidden={!shown}
              className={cn("min-h-0 min-w-0 flex-1 flex-col overflow-hidden", shown ? "flex" : "hidden")}
            >
              {render(id)}
            </div>
          </PageActivityContext.Provider>
        );
      })}
      {keepActive ? null : render(active)}
    </>
  );
}
