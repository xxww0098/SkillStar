import { Check, ChevronDown, Settings2, Unplug } from "lucide-react";
import { Popover } from "radix-ui";
import { useState } from "react";
import { Button } from "../../../../../../../components/ui/button";
import { cn } from "../../../../../../../lib/utils";
import type { ProviderEntryFlat } from "../../../../../../../types";
import type { AgentDescriptor } from "../../../../../lib/agentRegistry";
import { activeEntry } from "../../../../../lib/toolBinding";
import type { PrototypeHubData } from "../../types";
import { isCompatible, providerModels, RichMatrixShell } from "./RichMatrixShell";

/**
 * B2a — Popover cell (B2 baseline)
 * Bound cell shows model; click opens popover for model list + unbind.
 */
export function VariantB2a({ data }: { data: PrototypeHubData }) {
  return (
    <RichMatrixShell
      data={data}
      subtitle="B2a · Popover — click bound cell for model list / unbind / agent."
      editorStyle="sections"
      providerCol="full"
      legend={
        <div className="flex flex-wrap gap-3 text-[11px] text-muted-foreground">
          <span>
            <span className="rounded-md border border-emerald-500/35 bg-emerald-500/10 px-1.5 py-0.5 font-mono text-[10px] text-emerald-400">
              model
            </span>{" "}
            → popover
          </span>
          <span>
            <span className="rounded-md border border-dashed border-border/60 px-1.5 py-0.5">Bind</span> → one click
          </span>
        </div>
      }
      renderCell={({ provider, agent }) => <PopoverCell data={data} provider={provider} agent={agent} />}
    />
  );
}

function PopoverCell({
  data,
  provider,
  agent,
}: {
  data: PrototypeHubData;
  provider: ProviderEntryFlat;
  agent: AgentDescriptor;
}) {
  const [open, setOpen] = useState(false);
  const entry = activeEntry(data.toolActivations[agent.toolId]);
  const isActive = entry?.provider_id === provider.id;
  const models = providerModels(provider);

  if (!isCompatible(provider, agent)) {
    return <Idle label="n/a" muted />;
  }
  if (!isActive) {
    return (
      <button
        type="button"
        onClick={() => data.stub("activate", { toolId: agent.toolId, providerId: provider.id })}
        className="mx-auto flex h-14 w-full max-w-[148px] items-center justify-center rounded-xl border border-dashed border-border/55 text-[11px] text-muted-foreground hover:border-primary/40 hover:bg-primary/[0.04] hover:text-foreground"
      >
        Bind
      </button>
    );
  }

  return (
    <Popover.Root open={open} onOpenChange={setOpen}>
      <Popover.Trigger asChild>
        <button
          type="button"
          className="mx-auto flex h-14 w-full max-w-[148px] flex-col items-start justify-center gap-0.5 rounded-xl border border-emerald-500/35 bg-emerald-500/[0.08] px-2.5 text-left hover:bg-emerald-500/[0.12]"
        >
          <span className="flex w-full items-center gap-1 text-[10px] font-semibold text-emerald-400">
            <Check className="h-3 w-3 shrink-0" />
            Bound
            <ChevronDown className="ml-auto h-3 w-3 opacity-60" />
          </span>
          <span className="w-full truncate font-mono text-[10px] text-foreground">
            {entry?.model || provider.default_model || "model"}
          </span>
        </button>
      </Popover.Trigger>
      <Popover.Portal>
        <Popover.Content
          align="center"
          sideOffset={6}
          className="z-[120] w-56 rounded-xl border border-border/60 bg-card p-2 shadow-xl"
        >
          <p className="px-1.5 pb-1.5 text-[10px] font-semibold uppercase tracking-wider text-muted-foreground">
            Model
          </p>
          <div className="max-h-40 space-y-0.5 overflow-y-auto">
            {models.map((m) => (
              <button
                key={m}
                type="button"
                onClick={() => {
                  data.stub("pickModel", { toolId: agent.toolId, model: m });
                  setOpen(false);
                }}
                className={cn(
                  "flex w-full items-center rounded-lg px-2 py-1.5 font-mono text-[11px]",
                  (entry?.model || provider.default_model) === m
                    ? "bg-primary/10 text-foreground"
                    : "text-muted-foreground hover:bg-muted/40 hover:text-foreground",
                )}
              >
                {m}
              </button>
            ))}
          </div>
          <div className="mt-2 flex gap-1 border-t border-border/40 pt-2">
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
        </Popover.Content>
      </Popover.Portal>
    </Popover.Root>
  );
}

function Idle({ label, muted }: { label: string; muted?: boolean }) {
  return (
    <div
      className={cn(
        "mx-auto flex h-14 w-full max-w-[148px] items-center justify-center rounded-xl text-[10px]",
        muted ? "text-muted-foreground/30" : "text-muted-foreground",
      )}
    >
      {label}
    </div>
  );
}
