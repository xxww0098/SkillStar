import { useTranslation } from "react-i18next";
import { Download, KeyRound, Pencil, Plus, Server, Trash2 } from "lucide-react";
import { Button } from "../../../components/ui/button";
import { EmptyState } from "../../../components/ui/EmptyState";
import { Skeleton } from "../../../components/ui/Skeleton";
import type { SshHost, SshHostListItem } from "../../../lib/ipc/commands/ssh";

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

export function SshHostsList({ hosts, isLoading, selectedKey, onSelect, onAdd, onEdit, onDelete, onImport }: Props) {
  const { t } = useTranslation();

  const managed = hosts?.filter((h): h is SshHost & { source: "managed" } => h.source === "managed") ?? [];
  const system = hosts?.filter((h) => h.source === "system") ?? [];

  return (
    <aside className="flex w-[260px] shrink-0 flex-col gap-2 overflow-y-auto border-r border-border/40 p-3">
      <Button onClick={onAdd} className="justify-start">
        <Plus className="size-4" />
        {t("ssh.addHost")}
      </Button>

      {isLoading ? (
        <div className="flex flex-col gap-2">
          <Skeleton className="h-16 w-full" />
          <Skeleton className="h-16 w-full" />
        </div>
      ) : (
        <>
          {managed.length > 0 && <SectionLabel>{t("ssh.sectionManaged")}</SectionLabel>}
          {managed.map((h) => (
            <HostCard
              key={h.id}
              active={selectedKey === h.id}
              onClick={() => onSelect(h)}
              title={h.display_name}
              subtitle={`${h.username}@${h.host}:${h.port}`}
              keyIcon={h.auth_method.kind === "key"}
              actions={
                <>
                  <Button
                    variant="ghost"
                    size="icon-xs"
                    onClick={(e) => {
                      e.stopPropagation();
                      onEdit(h);
                    }}
                    title={t("ssh.editHost")}
                  >
                    <Pencil className="size-3" />
                  </Button>
                  <Button
                    variant="ghost"
                    size="icon-xs"
                    onClick={(e) => {
                      e.stopPropagation();
                      onDelete(h.id);
                    }}
                    title={t("ssh.delete")}
                  >
                    <Trash2 className="size-3 text-destructive" />
                  </Button>
                </>
              }
            />
          ))}

          {system.length > 0 && <SectionLabel>{t("ssh.sectionSystem")}</SectionLabel>}
          {system.map((h) => (
            <HostCard
              key={`system:${h.alias}`}
              active={selectedKey === `system:${h.alias}`}
              onClick={() => onSelect(h)}
              title={h.alias}
              subtitle={`${h.username ? `${h.username}@` : ""}${h.host}:${h.port}`}
              keyIcon={!!h.identity_file}
              badge={t("ssh.badgeSystem")}
              actions={
                <Button
                  variant="ghost"
                  size="icon-xs"
                  onClick={(e) => {
                    e.stopPropagation();
                    onImport(h.alias);
                  }}
                  title={t("ssh.import")}
                >
                  <Download className="size-3" />
                </Button>
              }
            />
          ))}

          {managed.length === 0 && system.length === 0 && (
            <EmptyState
              icon={<Server className="size-5" />}
              title={t("ssh.noHosts")}
              description={t("ssh.noHostsHint")}
              size="sm"
            />
          )}
        </>
      )}
    </aside>
  );
}

function SectionLabel({ children }: { children: React.ReactNode }) {
  return (
    <div className="px-1 pt-2 text-[10px] font-medium uppercase tracking-wider text-muted-foreground">{children}</div>
  );
}

function HostCard({
  title,
  subtitle,
  keyIcon,
  badge,
  active,
  onClick,
  actions,
}: {
  title: string;
  subtitle: string;
  keyIcon: boolean;
  badge?: string;
  active: boolean;
  onClick: () => void;
  actions: React.ReactNode;
}) {
  return (
    <div
      role="button"
      tabIndex={0}
      onClick={onClick}
      onKeyDown={(e) => e.key === "Enter" && onClick()}
      className={`group flex cursor-pointer flex-col gap-1 rounded-xl border p-3 transition ${
        active
          ? "border-primary/60 bg-primary/10"
          : "border-border/40 bg-card/30 hover:border-border/70 hover:bg-accent/5"
      }`}
    >
      <div className="flex items-center gap-2">
        <Server className="size-4 shrink-0 text-muted-foreground" />
        <span className="min-w-0 flex-1 truncate text-sm font-medium">{title}</span>
        {badge ? (
          <span className="shrink-0 rounded bg-accent/20 px-1.5 py-0.5 text-[9px] text-muted-foreground">{badge}</span>
        ) : null}
        {keyIcon ? <KeyRound className="size-3.5 text-muted-foreground" /> : null}
      </div>
      <div className="flex items-center justify-between gap-2">
        <div className="truncate font-mono text-[11px] text-muted-foreground">{subtitle}</div>
        <div className="flex shrink-0 gap-1 opacity-0 transition group-hover:opacity-100">{actions}</div>
      </div>
    </div>
  );
}
