import { Check, Pencil, Search } from "lucide-react";
import { Popover } from "radix-ui";
import { useEffect, useMemo, useRef, useState } from "react";
import { useTranslation } from "react-i18next";
import { useQueryClient } from "@tanstack/react-query";
import { toast } from "sonner";
import type { ModelChoiceDto } from "@/types/generated/ModelChoiceDto";
import { tauriInvoke } from "@/lib/ipc";
import { cn } from "@/lib/utils";
import { modelsKeys } from "../../api/keys";

/** Display name, or the upstream id when the name is missing or secret-shaped. */
function choiceText(choice: ModelChoiceDto): string {
  const label = choice.label?.trim() ?? "";
  if (!label || label.includes("\n") || label.includes("\r") || label.includes("://") || label.includes("sk-")) {
    return choice.id;
  }
  return label;
}

/** One agent row the picker is choosing for. */
export interface PickerAgent {
  id: string;
  name: string;
  /** The label the board already read back for this agent. Empty when unset. */
  modelLabel: string;
}

interface ModelPickerPopoverProps {
  agent: PickerAgent;
  choices: ModelChoiceDto[];
  loading: boolean;
  onClose: () => void;
  /** Reload the choice list after a rename changed one label. */
  onRenamed: () => void;
}

/**
 * The magpie-style model picker as a popover anchored to the agent row: no
 * overlay, no centered modal — type to filter, arrows to walk the list, Enter
 * to save and the popover is gone. The rename strip stays because this is
 * also where a catalog model gets its display name.
 */
export function ModelPickerPopover({ agent, choices, loading, onClose, onRenamed }: ModelPickerPopoverProps) {
  const { t } = useTranslation();
  const queryClient = useQueryClient();
  const [filter, setFilter] = useState("");
  const [activeIndex, setActiveIndex] = useState(0);
  const [saving, setSaving] = useState(false);
  const [nameId, setNameId] = useState("");
  const [displayName, setDisplayName] = useState("");
  const [nameError, setNameError] = useState("");
  const searchRef = useRef<HTMLInputElement>(null);
  const listRef = useRef<HTMLUListElement>(null);

  const items = useMemo(() => {
    const query = filter.trim().toLowerCase();
    const withText = choices.map((choice) => ({ choice, text: choiceText(choice) }));
    if (!query) return withText;
    return withText.filter(
      (item) => item.text.toLowerCase().includes(query) || item.choice.id.toLowerCase().includes(query),
    );
  }, [choices, filter]);

  useEffect(() => {
    setActiveIndex(0);
  }, [filter]);

  useEffect(() => {
    const button = listRef.current?.children[activeIndex]?.querySelector("button");
    button?.scrollIntoView?.({ block: "nearest" });
  }, [activeIndex]);

  const currentText = agent.modelLabel.trim();

  const save = async (modelRef: string) => {
    if (saving) return;
    setSaving(true);
    try {
      await tauriInvoke("save_agent_model", { agentId: agent.id, modelRef });
      await queryClient.invalidateQueries({ queryKey: modelsKeys.board() });
      toast.success(t("models.picker.saved", { model: choiceOf(items, modelRef) }));
      onClose();
    } catch (caught) {
      toast.error(t("models.picker.saveFailed"), {
        description: caught instanceof Error ? caught.message : String(caught),
      });
    } finally {
      setSaving(false);
    }
  };

  return (
    <Popover.Content
      aria-label={agent.name}
      side="right"
      align="start"
      sideOffset={8}
      collisionPadding={8}
      onOpenAutoFocus={(event) => {
        event.preventDefault();
        searchRef.current?.focus();
      }}
      onCloseAutoFocus={(event) => event.preventDefault()}
      className="z-50 flex w-80 flex-col rounded-xl border border-border bg-card p-2 shadow-2xl outline-none animate-in fade-in-0 zoom-in-95 data-[state=closed]:animate-out data-[state=closed]:fade-out-0 data-[state=closed]:zoom-out-95"
    >
      <div className="relative shrink-0">
        <Search
          aria-hidden
          className="pointer-events-none absolute top-1/2 left-2 h-3.5 w-3.5 -translate-y-1/2 text-muted-foreground"
        />
        <input
          ref={searchRef}
          type="text"
          value={filter}
          aria-label={t("models.picker.searchLabel")}
          placeholder={t("models.picker.searchPlaceholder")}
          onChange={(event) => setFilter(event.target.value)}
          onKeyDown={(event) => {
            if (event.key === "ArrowDown") {
              event.preventDefault();
              setActiveIndex((index) => (items.length === 0 ? 0 : (index + 1) % items.length));
            } else if (event.key === "ArrowUp") {
              event.preventDefault();
              setActiveIndex((index) => (items.length === 0 ? 0 : (index - 1 + items.length) % items.length));
            } else if (event.key === "Enter") {
              event.preventDefault();
              const item = items[activeIndex];
              if (item) void save(item.choice.id);
            }
          }}
          className="h-8 w-full rounded-lg border border-border/70 bg-background/60 pr-2.5 pl-7.5 text-[13px] text-foreground placeholder:text-muted-foreground focus-visible:border-primary/50 focus-visible:outline-none focus-visible:ring-2 focus-visible:ring-primary/30"
        />
      </div>
      <ul ref={listRef} aria-label="model choices" className="mt-1.5 max-h-64 min-h-16 overflow-auto">
        {items.length === 0 ? (
          <li className="px-2 py-5 text-center text-xs text-muted-foreground">
            {loading ? t("models.picker.loading") : t("models.picker.noMatches")}
          </li>
        ) : (
          items.map((item, index) => {
            const isCurrent = currentText !== "" && item.text === currentText;
            return (
              <li key={item.choice.id}>
                <div
                  className={cn(
                    "group flex items-center rounded-lg",
                    index === activeIndex && "bg-muted/50",
                    saving && "opacity-60",
                  )}
                >
                  <button
                    type="button"
                    disabled={saving}
                    onClick={() => void save(item.choice.id)}
                    className="flex min-w-0 flex-1 items-center gap-2 rounded-lg px-2 py-1 text-left text-xs text-foreground"
                  >
                    {isCurrent ? (
                      <Check aria-hidden className="h-3.5 w-3.5 shrink-0 text-primary" />
                    ) : (
                      <span aria-hidden className="h-3.5 w-3.5 shrink-0 rounded-full border border-border/70" />
                    )}
                    <span className="truncate font-mono">{item.text}</span>
                  </button>
                  <button
                    type="button"
                    aria-label={t("models.picker.rename", { id: item.choice.id })}
                    title={t("models.picker.rename", { id: item.choice.id })}
                    onClick={() => {
                      setNameId(item.choice.id);
                      setDisplayName("");
                      setNameError("");
                    }}
                    className="mr-0.5 flex h-6 w-6 shrink-0 items-center justify-center rounded-md text-muted-foreground opacity-0 transition group-hover:opacity-100 hover:bg-muted/60 hover:text-foreground focus-visible:opacity-100"
                  >
                    <Pencil className="h-3 w-3" />
                  </button>
                </div>
              </li>
            );
          })
        )}
      </ul>
      <form
        aria-label="model display name"
        className="mt-1.5 flex shrink-0 flex-wrap gap-1.5 rounded-lg bg-muted/30 p-1.5"
        onSubmit={(event) => {
          event.preventDefault();
          void tauriInvoke("save_model_name", { id: nameId, name: displayName })
            .then(async () => {
              setNameError("");
              setDisplayName("");
              await queryClient.invalidateQueries({ queryKey: modelsKeys.board() });
              onRenamed();
            })
            .catch((caught: unknown) => {
              setNameError(caught instanceof Error ? caught.message : "");
            });
        }}
      >
        <input
          aria-label="model id"
          placeholder="provider/model"
          value={nameId}
          onChange={(event) => setNameId(event.target.value)}
          className="h-7 min-w-0 flex-[1.1] rounded-md border border-border/60 bg-background px-2 font-mono text-xs"
        />
        <input
          aria-label="display name"
          placeholder={t("models.picker.displayName")}
          value={displayName}
          onChange={(event) => setDisplayName(event.target.value)}
          className="h-7 min-w-0 flex-1 rounded-md border border-border/60 bg-background px-2 text-xs"
        />
        <button
          type="submit"
          className="h-7 rounded-md border border-border/60 bg-background px-2 text-xs text-foreground transition hover:bg-muted/50"
        >
          {t("models.picker.saveName")}
        </button>
        {nameError ? (
          <p role="alert" className="basis-full text-xs text-destructive">
            {nameError}
          </p>
        ) : null}
      </form>
    </Popover.Content>
  );
}

/** The saved display text for one model ref, for the success toast. */
function choiceOf(items: { choice: ModelChoiceDto; text: string }[], modelRef: string): string {
  return items.find((item) => item.choice.id === modelRef)?.text ?? modelRef;
}
