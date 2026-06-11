import { ExternalLink, FolderOpen, Loader2, RefreshCw, Save, ShieldCheck, Wand2 } from "lucide-react";
import { useMemo } from "react";
import { useTranslation } from "react-i18next";
import { Button } from "../../../../components/ui/button";
import { ExternalAnchor } from "../../../../components/ui/ExternalAnchor";
import { tauriInvoke } from "../../../../lib/ipc";
import { cn } from "../../../../lib/utils";
import { ModalHeader, ModalShell } from "../../../../components/ui/ModalShell";
import { useToolConfigFiles } from "../../api/configFiles";
import { AgentToolIcon } from "../shared/AgentToolIcon";

/**
 * Claude Desktop MCP config dialog (formerly a drawer mode).
 *
 * Claude Desktop only exposes `mcpServers` in its config file — there is no
 * provider/key/base-URL to bind, so this is an on-disk JSON editor with an
 * orientation card, not a provider form.
 */
export function ClaudeDesktopConfigDialog({ open, onClose }: { open: boolean; onClose: () => void }) {
  const { t } = useTranslation();
  return (
    <ModalShell
      open={open}
      onClose={onClose}
      ariaLabel={t("models.desktop.dialogAriaLabel")}
      panelClassName="max-w-[640px]"
      surfaceClassName="flex max-h-[85vh] flex-col"
    >
      <ModalHeader
        icon={<AgentToolIcon toolId="claude-desktop" size="sm" />}
        title={t("models.desktop.dialogTitle")}
        onClose={onClose}
      />
      <div className="ss-page-scroll min-h-0 flex-1 overflow-y-auto px-6 py-4">
        <ClaudeDesktopConfigBody />
      </div>
    </ModalShell>
  );
}

function ClaudeDesktopConfigBody() {
  const { t } = useTranslation();
  const editor = useToolConfigFiles("claude-desktop");
  const activeFile = editor.files.find((f) => f.file_id === editor.activeFileId);

  const mcpServers = useMemo(() => {
    if (!editor.content) return null;
    try {
      const parsed = JSON.parse(editor.content) as { mcpServers?: Record<string, unknown> };
      return parsed.mcpServers ?? null;
    } catch {
      return null;
    }
  }, [editor.content]);

  const serverCount = mcpServers ? Object.keys(mcpServers).length : 0;
  const parseError = !editor.loading && editor.content && mcpServers === null;

  const handleAddTemplate = () => {
    const next: Record<string, unknown> = mcpServers ? { ...mcpServers } : {};
    next[`example-${serverCount + 1}`] = {
      command: "npx",
      args: ["-y", "@modelcontextprotocol/server-filesystem", "/Users/you/Documents"],
    };
    const merged: Record<string, unknown> = {};
    if (editor.content) {
      try {
        Object.assign(merged, JSON.parse(editor.content));
      } catch {
        /* ignore */
      }
    }
    merged.mcpServers = next;
    editor.setContent(`${JSON.stringify(merged, null, 2)}\n`);
  };

  return (
    <div className="space-y-3">
      {/* Orientation card */}
      <section className="rounded-xl border border-primary/15 bg-primary/[0.04] p-4">
        <h4 className="flex items-center gap-2 text-sm font-semibold text-foreground">
          <ShieldCheck className="h-4 w-4 text-primary" />
          {t("models.desktop.aboutTitle")}
        </h4>
        <ul className="mt-2 space-y-1.5 text-[11px] leading-relaxed text-muted-foreground/95">
          <li>
            <strong className="text-foreground/90">{t("models.desktop.aboutConfigurable")}</strong>
            <code className="mx-1 rounded bg-muted/50 px-1 py-0.5 font-mono text-[10px]">mcpServers</code>
            {t("models.desktop.aboutMcpDesc")}
          </li>
          <li>
            <strong className="text-foreground/90">{t("models.desktop.aboutNotConfigurable")}</strong>{" "}
            {t("models.desktop.aboutNotDesc")}
          </li>
          <li>
            {t("models.desktop.aboutCustomBefore")}
            <code className="mx-1 rounded bg-muted/50 px-1 py-0.5 font-mono text-[10px]">ANTHROPIC_BASE_URL</code>
            {t("models.desktop.aboutCustomAfter")}
          </li>
        </ul>
        <div className="mt-3 flex flex-wrap gap-1.5">
          <ExternalAnchor
            href="https://modelcontextprotocol.io/quickstart/user"
            className="inline-flex items-center gap-1 rounded-md border border-border/60 px-2 py-1 text-[11px] font-medium text-foreground/80 hover:border-primary/40 hover:bg-card-hover"
          >
            {t("models.desktop.mcpQuickstart")} <ExternalLink className="h-3 w-3" />
          </ExternalAnchor>
          <ExternalAnchor
            href="https://github.com/modelcontextprotocol/servers"
            className="inline-flex items-center gap-1 rounded-md border border-border/60 px-2 py-1 text-[11px] font-medium text-foreground/80 hover:border-primary/40 hover:bg-card-hover"
          >
            {t("models.desktop.mcpDirectory")} <ExternalLink className="h-3 w-3" />
          </ExternalAnchor>
        </div>
      </section>

      {/* MCP servers summary */}
      <section className="rounded-xl border border-border/55 bg-card/55 p-4">
        <div className="flex items-center justify-between gap-2">
          <h4 className="text-sm font-semibold text-foreground">{t("models.desktop.mcpServers")}</h4>
          <span className="rounded-full bg-muted/50 px-2 py-0.5 text-[10px] font-medium text-muted-foreground">
            {editor.loading ? (
              <Loader2 className="inline h-3 w-3 animate-spin" />
            ) : (
              t("models.desktop.serversConfigured", { count: serverCount })
            )}
          </span>
        </div>

        {mcpServers && serverCount > 0 ? (
          <ul className="mt-3 space-y-1">
            {Object.entries(mcpServers).map(([name, def]) => {
              const command =
                typeof def === "object" && def !== null && "command" in def
                  ? String((def as { command: unknown }).command ?? "")
                  : "";
              return (
                <li key={name} className="flex items-baseline gap-2 truncate text-[11px]">
                  <span className="font-mono font-semibold text-foreground">{name}</span>
                  {command && <span className="truncate text-muted-foreground">— {command}</span>}
                </li>
              );
            })}
          </ul>
        ) : (
          <p className="mt-3 text-[11px] text-muted-foreground/90">{t("models.desktop.noServersHint")}</p>
        )}

        <div className="mt-3 flex flex-wrap gap-1.5">
          <Button type="button" size="sm" variant="outline" onClick={handleAddTemplate} disabled={editor.loading}>
            {t("models.desktop.insertExample")}
          </Button>
        </div>
      </section>

      {/* JSON editor */}
      <section className="rounded-xl border border-border/55 bg-card/55 p-4">
        <div className="mb-2 flex items-center justify-between">
          <h4 className="text-sm font-semibold text-foreground">claude_desktop_config.json</h4>
          {activeFile ? (
            <p className="truncate font-mono text-[10px] text-muted-foreground" title={activeFile.path}>
              {activeFile.path}
            </p>
          ) : null}
        </div>

        {editor.loading ? (
          <div className="flex h-48 items-center justify-center rounded-lg border border-border/50 bg-background/40">
            <Loader2 className="h-5 w-5 animate-spin text-primary" />
          </div>
        ) : (
          <textarea
            value={editor.content}
            onChange={(e) => editor.setContent(e.target.value)}
            spellCheck={false}
            className={cn(
              "min-h-[240px] w-full resize-y rounded-lg border border-border/55 bg-background/50 p-2.5",
              "font-mono text-[11px] leading-5 text-foreground",
              "focus-visible:outline-none focus-visible:ring-2 focus-visible:ring-primary/35",
            )}
            aria-label={t("models.desktop.editorAria")}
          />
        )}

        {parseError ? <p className="mt-1 text-[11px] text-destructive">{t("models.desktop.parseError")}</p> : null}

        <div className="mt-2.5 flex flex-wrap items-center gap-1.5">
          <Button
            type="button"
            size="sm"
            variant="default"
            onClick={() => void editor.save()}
            disabled={editor.saving || editor.loading}
          >
            {editor.saving ? <Loader2 className="h-3 w-3 animate-spin" /> : <Save className="h-3 w-3" />}
            {t("models.desktop.save")}
          </Button>
          <Button
            type="button"
            size="sm"
            variant="outline"
            onClick={() => void editor.formatContent()}
            disabled={editor.loading}
          >
            <Wand2 className="mr-1 h-3 w-3" />
            {t("models.desktop.format")}
          </Button>
          <Button
            type="button"
            size="sm"
            variant="outline"
            onClick={() => void editor.reload()}
            disabled={editor.loading}
          >
            <RefreshCw className="mr-1 h-3 w-3" />
            {t("models.desktop.reload")}
          </Button>
          {activeFile ? (
            <Button
              type="button"
              size="sm"
              variant="ghost"
              className="ml-auto h-7 text-[11px]"
              onClick={() => {
                const dir = activeFile.path.replace(/[/\\][^/\\]+$/, "");
                void tauriInvoke("open_folder", { path: dir });
              }}
            >
              <FolderOpen className="mr-1 h-3 w-3" />
              {t("models.desktop.openFolder")}
            </Button>
          ) : null}
        </div>

        {editor.dirty ? <p className="mt-1 text-[10px] text-amber-500">{t("models.desktop.unsaved")}</p> : null}
      </section>
    </div>
  );
}
