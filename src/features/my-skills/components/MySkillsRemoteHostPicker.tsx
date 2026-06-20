import { Check, ChevronDown, Download, Pencil, Plus, Server, Trash2 } from "lucide-react";
import { Popover } from "radix-ui";
import type React from "react";
import { useState } from "react";
import { useTranslation } from "react-i18next";
import { Button } from "../../../components/ui/button";
import { cn } from "../../../lib/utils";
import type { SshHost, SshHostListItem } from "../../../lib/ipc/commands/ssh";
import { remoteHostItemKey, remoteHostLabel } from "../lib/remoteHostKey";

interface Props {
  hosts: SshHostListItem[] | undefined;
  isLoading: boolean;
  selectedKey: string | null;
  onSelect: (item: SshHostListItem) => void;
  onAdd: () => void;
  onEdit: (host: SshHost) => void;
  onDelete: (id: string) => void;
  onImport: (alias: string) => void;
}

export function MySkillsRemoteHostPicker({
  hosts,
  isLoading,
  selectedKey,
  onSelect,
  onAdd,
  onEdit,
  onDelete,
  onImport,
}: Props) {
  const { t } = useTranslation();
  const [open, setOpen] = useState(false);

  const managed = hosts?.filter((h): h is SshHost & { source: "managed" } => h.source === "managed") ?? [];
  const system = hosts?.filter((h) => h.source === "system") ?? [];
  const selected = hosts?.find((h) => remoteHostItemKey(h) === selectedKey) ?? null;
  const label = selected ? remoteHostLabel(selected) : t("ssh.selectHost");

  return (
    <Popover.Root open={open} onOpenChange={setOpen}>
      <Popover.Trigger asChild>
        <button
          type="button"
          disabled={isLoading}
          className={cn(
            "flex h-8 max-w-[200px] items-center gap-1.5 rounded-lg border border-border/70 bg-background/50 px-2.5 text-xs font-medium text-foreground/90 shadow-sm backdrop-blur-md shrink-0 focus-ring",
            isLoading && "opacity-60",
          )}
          title={t("mySkills.remoteHostPicker", { defaultValue: "Remote server" })}
        >
          <Server className="size-3.5 shrink-0 text-muted-foreground" />
          <span className="truncate">{label}</span>
          <ChevronDown className="size-3.5 shrink-0 text-muted-foreground" />
        </button>
      </Popover.Trigger>
      <Popover.Portal>
        <Popover.Content
          sideOffset={6}
          align="start"
          className="z-50 w-[min(320px,calc(100vw-2rem))] max-h-[min(420px,70vh)] overflow-y-auto rounded-xl border border-border bg-card/95 p-1.5 shadow-xl backdrop-blur-xl animate-in fade-in-0 zoom-in-95"
        >
          <button
            type="button"
            onClick={() => {
              onAdd();
              setOpen(false);
            }}
            className="mb-1 flex w-full items-center gap-2 rounded-lg px-3 py-2 text-xs font-medium text-primary hover:bg-accent/50"
          >
            <Plus className="size-3.5" />
            {t("ssh.addHost")}
          </button>

          {managed.length > 0 && (
            <p className="px-2 py-1 text-[10px] font-semibold uppercase tracking-wide text-muted-foreground">
              {t("ssh.sectionManaged")}
            </p>
          )}
          {managed.map((h) => (
            <HostRow
              key={h.id}
              active={selectedKey === h.id}
              title={h.display_name}
              subtitle={`${h.username}@${h.host}:${h.port}`}
              onSelect={() => {
                onSelect(h);
                setOpen(false);
              }}
              actions={
                <>
                  <IconBtn
                    title={t("ssh.editHost")}
                    onClick={(e) => {
                      e.stopPropagation();
                      onEdit(h);
                      setOpen(false);
                    }}
                  >
                    <Pencil className="size-3.5" />
                  </IconBtn>
                  <IconBtn
                    title={t("ssh.delete")}
                    onClick={(e) => {
                      e.stopPropagation();
                      onDelete(h.id);
                    }}
                  >
                    <Trash2 className="size-3.5 text-destructive" />
                  </IconBtn>
                </>
              }
            />
          ))}

          {system.length > 0 && (
            <p className="mt-1 px-2 py-1 text-[10px] font-semibold uppercase tracking-wide text-muted-foreground">
              {t("ssh.sectionSystem")}
            </p>
          )}
          {system.map((h) => (
            <HostRow
              key={`system:${h.alias}`}
              active={selectedKey === `system:${h.alias}`}
              title={h.alias}
              subtitle={`${h.username ? `${h.username}@` : ""}${h.host}:${h.port}`}
              badge={t("ssh.badgeSystem")}
              onSelect={() => {
                onSelect(h);
                setOpen(false);
              }}
              actions={
                <IconBtn
                  title={t("ssh.import")}
                  onClick={(e) => {
                    e.stopPropagation();
                    onImport(h.alias);
                  }}
                >
                  <Download className="size-3.5" />
                </IconBtn>
              }
            />
          ))}
        </Popover.Content>
      </Popover.Portal>
    </Popover.Root>
  );
}

function HostRow({
  active,
  title,
  subtitle,
  badge,
  onSelect,
  actions,
}: {
  active: boolean;
  title: string;
  subtitle: string;
  badge?: string;
  onSelect: () => void;
  actions?: React.ReactNode;
}) {
  return (
    <div
      className={cn(
        "flex items-center gap-1 rounded-lg pr-1 transition-colors",
        active ? "bg-primary/10" : "hover:bg-accent/40",
      )}
    >
      <button type="button" onClick={onSelect} className="min-w-0 flex-1 px-2 py-2 text-left">
        <div className="flex items-center gap-2">
          <span className="truncate text-xs font-medium">{title}</span>
          {badge ? (
            <span className="shrink-0 rounded bg-muted px-1 py-0.5 text-[9px] text-muted-foreground">{badge}</span>
          ) : null}
          {active ? <Check className="ml-auto size-3.5 shrink-0 text-primary" /> : null}
        </div>
        <p className="truncate font-mono text-[10px] text-muted-foreground">{subtitle}</p>
      </button>
      {actions ? <div className="flex shrink-0 items-center gap-0.5">{actions}</div> : null}
    </div>
  );
}

function IconBtn({
  children,
  title,
  onClick,
}: {
  children: React.ReactNode;
  title: string;
  onClick: (e: React.MouseEvent) => void;
}) {
  return (
    <Button variant="ghost" size="icon-xs" title={title} onClick={onClick}>
      {children}
    </Button>
  );
}
