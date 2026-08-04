import { Check, Search, Settings2, Unplug } from "lucide-react";
import { Popover } from "radix-ui";
import { useMemo, useState, type ReactNode } from "react";
import { Button } from "../../../../../../../components/ui/button";
import { cn } from "../../../../../../../lib/utils";
import type { ProviderEntryFlat } from "../../../../../../../types";
import type { AgentDescriptor } from "../../../../../lib/agentRegistry";
import { activeEntry } from "../../../../../lib/toolBinding";
import type { PrototypeHubData } from "../../types";
import { isCompatible, providerModels, RichMatrixShell } from "./RichMatrixShell";

/**
 * B2c — Searchable bind panel
 * Wider cell with status line + latency stub. Opens a command-style panel
 * (search models, bind/switch, agent settings) — better when catalogs are long.
 */
export function VariantB2c({ data }: { data: PrototypeHubData }) {
  return (
    <RichMatrixShell
      data={data}
      subtitle="B2c · Search panel — command-style model picker for long catalogs."
      editorStyle="split"
      providerCol="full"
      legend={
        <p className="text-[11px] text-muted-foreground">
          Bound cells show status + mock latency. Open panel to search / switch model.
        </p>
      }
      renderCell={({ provider, agent }) => <SearchPanelCell data={data} provider={provider} agent={agent} />}
    />
  );
}

function SearchPanelCell({
  data,
  provider,
  agent,
}: {
  data: PrototypeHubData;
  provider: ProviderEntryFlat;
  agent: AgentDescriptor;
}) {
  const [open, setOpen] = useState(false);
  const [query, setQuery] = useState("");
  const entry = activeEntry(data.toolActivations[agent.toolId]);
  const isActive = entry?.provider_id === provider.id;
  const models = providerModels(provider);
  const filtered = useMemo(() => {
    const q = query.trim().toLowerCase();
    if (!q) return models;
    return models.filter((m) => m.toLowerCase().includes(q));
  }, [models, query]);

  // Deterministic fake latency for prototype density judgment only.
  const latencyMs = isActive ? 40 + ((provider.id.length * 17 + agent.toolId.length * 11) % 180) : null;

  if (!isCompatible(provider, agent)) {
    return (
      <div className="mx-auto flex h-16 w-full max-w-[168px] items-center justify-center text-[10px] text-muted-foreground/30">
        n/a
      </div>
    );
  }

  if (!isActive) {
    return (
      <Popover.Root
        open={open}
        onOpenChange={(next) => {
          setOpen(next);
          if (!next) setQuery("");
        }}
      >
        <Popover.Trigger asChild>
          <button
            type="button"
            className="mx-auto flex h-16 w-full max-w-[168px] flex-col items-center justify-center gap-0.5 rounded-xl border border-dashed border-border/55 text-[11px] text-muted-foreground hover:border-primary/40 hover:bg-primary/[0.04] hover:text-foreground"
          >
            <span>Bind…</span>
            <span className="text-[9px] text-muted-foreground/70">pick model</span>
          </button>
        </Popover.Trigger>
        <BindPanel
          title={`Bind ${agent.displayName}`}
          query={query}
          setQuery={setQuery}
          models={filtered}
          current={null}
          onPick={(m) => {
            data.stub("activate", { toolId: agent.toolId, providerId: provider.id, model: m });
            setOpen(false);
            setQuery("");
          }}
          footer={null}
        />
      </Popover.Root>
    );
  }

  return (
    <Popover.Root
      open={open}
      onOpenChange={(next) => {
        setOpen(next);
        if (!next) setQuery("");
      }}
    >
      <Popover.Trigger asChild>
        <button
          type="button"
          className="mx-auto flex h-16 w-full max-w-[168px] flex-col items-start justify-center gap-0.5 rounded-xl border border-emerald-500/35 bg-emerald-500/[0.08] px-2.5 text-left hover:bg-emerald-500/[0.12]"
        >
          <span className="flex w-full items-center gap-1 text-[10px] font-semibold text-emerald-400">
            <Check className="h-3 w-3" />
            Live
            {latencyMs != null ? (
              <span className="ml-auto font-mono text-[9px] font-normal text-emerald-400/80">{latencyMs}ms</span>
            ) : null}
          </span>
          <span className="w-full truncate font-mono text-[10px] text-foreground">
            {entry?.model || provider.default_model || "model"}
          </span>
          <span className="text-[9px] text-muted-foreground">search to switch</span>
        </button>
      </Popover.Trigger>
      <BindPanel
        title={`${agent.displayName} · ${provider.name}`}
        query={query}
        setQuery={setQuery}
        models={filtered}
        current={entry?.model || provider.default_model || null}
        onPick={(m) => {
          data.stub("pickModel", { toolId: agent.toolId, model: m });
          setOpen(false);
          setQuery("");
        }}
        footer={
          <div className="flex gap-1">
            <Button
              size="xs"
              variant="ghost"
              className="flex-1"
              onClick={() => {
                setOpen(false);
                data.setOverlay({ type: "agent-settings", toolId: agent.toolId });
              }}
            >
              <Settings2 className="mr-1 h-3 w-3" />
              Agent
            </Button>
            <Button
              size="xs"
              variant="ghost"
              className="flex-1 text-destructive"
              onClick={() => {
                data.stub("deactivate", { toolId: agent.toolId });
                setOpen(false);
              }}
            >
              <Unplug className="mr-1 h-3 w-3" />
              Unbind
            </Button>
          </div>
        }
      />
    </Popover.Root>
  );
}

function BindPanel({
  title,
  query,
  setQuery,
  models,
  current,
  onPick,
  footer,
}: {
  title: string;
  query: string;
  setQuery: (q: string) => void;
  models: string[];
  current: string | null;
  onPick: (m: string) => void;
  footer: ReactNode;
}) {
  return (
    <Popover.Portal>
      <Popover.Content
        align="center"
        sideOffset={6}
        className="z-[120] w-64 overflow-hidden rounded-xl border border-border/60 bg-card shadow-xl"
        onOpenAutoFocus={(e) => {
          // Focus search field.
          const root = e.currentTarget as HTMLElement;
          const input = root.querySelector("input");
          if (input) {
            e.preventDefault();
            input.focus();
          }
        }}
      >
        <div className="border-b border-border/40 px-3 py-2">
          <p className="truncate text-[11px] font-semibold">{title}</p>
          <div className="relative mt-1.5">
            <Search className="pointer-events-none absolute left-2 top-1/2 h-3 w-3 -translate-y-1/2 text-muted-foreground" />
            <input
              value={query}
              onChange={(e) => setQuery(e.target.value)}
              placeholder="Filter models…"
              className="h-8 w-full rounded-lg border border-border/50 bg-background pl-7 pr-2 text-[11px] outline-none focus:ring-1 focus:ring-primary/40"
            />
          </div>
        </div>
        <div className="max-h-48 space-y-0.5 overflow-y-auto p-1.5">
          {models.length === 0 ? (
            <p className="px-2 py-3 text-center text-[11px] text-muted-foreground">No matches</p>
          ) : (
            models.map((m) => (
              <button
                key={m}
                type="button"
                onClick={() => onPick(m)}
                className={cn(
                  "flex w-full items-center gap-2 rounded-lg px-2 py-1.5 font-mono text-[11px]",
                  current === m
                    ? "bg-primary/10 text-foreground"
                    : "text-muted-foreground hover:bg-muted/40 hover:text-foreground",
                )}
              >
                {current === m ? <Check className="h-3 w-3 shrink-0 text-primary" /> : <span className="w-3" />}
                <span className="truncate">{m}</span>
              </button>
            ))
          )}
        </div>
        {footer ? <div className="border-t border-border/40 p-1.5">{footer}</div> : null}
      </Popover.Content>
    </Popover.Portal>
  );
}
