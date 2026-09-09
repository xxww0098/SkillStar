import { Loader2, Plus } from "lucide-react";
import { Tabs } from "radix-ui";
import { useEffect, useRef, useState } from "react";
import { useTranslation } from "react-i18next";
import { Button } from "@/components/ui/button";
import type { ModelsHubData } from "../../../hooks/useModelsData";
import { canConfigureClaudeClient, CLAUDE_CLIENTS } from "../../../lib/claudeClients";
import { findOfficialProvider, isNativeOfficialProvider } from "../../../lib/officialProviders";
import { ClaudeMappingPanel } from "./ClaudeMappingPanel";

export function ClaudeWorkbench({ data }: { data: ModelsHubData }) {
  const { t } = useTranslation();
  const [busy, setBusy] = useState(false);
  const [roleBusy, setRoleBusy] = useState(false);
  const locked = busy || roleBusy;
  const { setInteractionLocked } = data;
  useEffect(() => {
    setInteractionLocked(locked);
    return () => setInteractionLocked(false);
  }, [locked, setInteractionLocked]);
  const inFlight = useRef(false);
  // Keep failures across source/client changes: an optimistic cache is not proof of a write.
  const [error, setError] = useState<string | null>(null);
  const official = findOfficialProvider(data.providers, "claude-code");
  const provider = data.providers.find((p) => p.id === data.sourceId);
  const native = data.sourceId === official?.id || Boolean(provider && isNativeOfficialProvider(provider));
  const target = native ? official : provider;
  const compatible = native || Boolean(target?.base_url_anthropic.trim());
  const applied = Boolean(target && data.currentEntry?.provider_id === target.id && !busy && !error);
  const currentProvider = data.providers.find((p) => p.id === data.currentEntry?.provider_id);
  const writable = canConfigureClaudeClient(data.clientId);

  async function apply() {
    if (inFlight.current || roleBusy || !writable || !target || !compatible) return;
    inFlight.current = true;
    setBusy(true);
    try {
      const result = await data.activateTool(target.id, data.clientId, native ? "" : target.default_model);
      if (!result.success) throw new Error(result.error || t("models.claudeWorkbench.applyFailed"));
      setError(null);
    } catch (cause) {
      setError(cause instanceof Error ? cause.message : String(cause));
    } finally {
      inFlight.current = false;
      setBusy(false);
    }
  }

  return (
    <div className="flex min-h-0 flex-1 flex-col overflow-hidden">
      <div data-tauri-drag-region className="h-4 shrink-0" aria-hidden />
      <main className="ss-page-scroll">
        <div className="mx-auto w-full max-w-5xl space-y-7 px-4 py-6 text-sm leading-relaxed sm:px-8">
          <header className="space-y-2">
            <h1 className="text-2xl font-semibold tracking-tight">{t("models.claudeWorkbench.title")}</h1>
            <p className="max-w-prose text-muted-foreground">{t("models.claudeWorkbench.description")}</p>
          </header>
          <Tabs.Root
            value={data.clientId}
            onValueChange={(id) => {
              if (!locked) data.selectClient(id as typeof data.clientId);
            }}
          >
            <Tabs.List
              aria-label={t("models.claudeWorkbench.client")}
              className="flex flex-wrap gap-1 border-b border-border"
            >
              {CLAUDE_CLIENTS.map((client) => (
                <Tabs.Trigger
                  key={client.id}
                  value={client.id}
                  disabled={locked}
                  className="border-b-2 border-transparent px-4 py-3 font-medium text-muted-foreground outline-none hover:text-foreground focus-visible:ring-2 focus-visible:ring-ring disabled:opacity-50 data-[state=active]:border-primary data-[state=active]:text-foreground"
                >
                  {client.label}
                </Tabs.Trigger>
              ))}
            </Tabs.List>
            <Tabs.Content value="claude-desktop" className="space-y-3 py-7 outline-none">
              <h2 className="text-lg font-semibold">{t("models.claudeWorkbench.desktopTitle")}</h2>
              <p className="max-w-prose text-muted-foreground">{t("models.claudeWorkbench.desktopDescription")}</p>
              <p className="max-w-prose text-muted-foreground">{t("models.claudeWorkbench.desktopLegacy")}</p>
            </Tabs.Content>
            <Tabs.Content value="claude-code" className="space-y-7 py-6 outline-none">
              <section
                className="space-y-2 border-b border-border pb-5"
                aria-label={t("models.claudeWorkbench.currentSource")}
              >
                <h2 className="font-semibold">{t("models.claudeWorkbench.currentSource")}</h2>
                <p>
                  {busy || error
                    ? t("models.claudeWorkbench.unconfirmed")
                    : currentProvider
                      ? isNativeOfficialProvider(currentProvider)
                        ? t("models.claudeWorkbench.nativeSource")
                        : currentProvider.name
                      : t("models.claudeWorkbench.unmanaged")}
                </p>
                <p className="text-muted-foreground">{t("models.claudeWorkbench.sourceScope")}</p>
              </section>
              <fieldset disabled={locked || !writable} className="space-y-5">
                <legend className="mb-3 font-semibold">{t("models.claudeWorkbench.connectionMode")}</legend>
                <div className="flex flex-wrap gap-x-6 gap-y-3">
                  <label className="flex cursor-pointer items-center gap-2">
                    <input
                      type="radio"
                      name="claude-source"
                      checked={native}
                      onChange={() => data.selectSource(official?.id ?? "")}
                    />
                    {t("models.claudeWorkbench.native")}
                  </label>
                  <label className="flex cursor-pointer items-center gap-2">
                    <input
                      type="radio"
                      name="claude-source"
                      checked={!native}
                      onChange={() => data.selectSource(data.thirdPartyProviders[0]?.id ?? "")}
                    />
                    {t("models.claudeWorkbench.api")}
                  </label>
                </div>
                {native ? (
                  <p className="max-w-prose text-muted-foreground">{t("models.claudeWorkbench.nativeDescription")}</p>
                ) : (
                  <div className="space-y-4">
                    <div className="flex flex-wrap items-end gap-3">
                      <label className="min-w-0 flex-1 space-y-2">
                        <span className="block">{t("models.claudeWorkbench.provider")}</span>
                        <select
                          value={data.sourceId}
                          onChange={(event) => data.selectSource(event.target.value)}
                          className="h-10 w-full min-w-0 rounded-lg border border-input bg-background px-3 outline-none focus-visible:ring-2 focus-visible:ring-ring"
                        >
                          <option value="">{t("models.claudeWorkbench.chooseProvider")}</option>
                          {data.thirdPartyProviders.map((p) => (
                            <option key={p.id} value={p.id}>
                              {p.name}
                              {p.base_url_anthropic.trim() ? "" : " — " + t("models.claudeWorkbench.incompatible")}
                            </option>
                          ))}
                        </select>
                      </label>
                      {target && (
                        <Button
                          variant="outline"
                          onClick={() => data.setOverlay({ type: "edit", providerId: target.id })}
                        >
                          {t("models.claudeWorkbench.edit")}
                        </Button>
                      )}
                      <Button variant="outline" onClick={() => data.setOverlay({ type: "create" })}>
                        <Plus aria-hidden />
                        {t("models.claudeWorkbench.add")}
                      </Button>
                    </div>
                    {target ? (
                      <dl className="grid grid-cols-1 gap-x-5 gap-y-2 sm:grid-cols-[auto_1fr]">
                        <dt className="text-muted-foreground">{t("models.claudeWorkbench.endpoint")}</dt>
                        <dd className="break-all">
                          {target.base_url_anthropic || t("models.claudeWorkbench.incompatible")}
                        </dd>
                        <dt className="text-muted-foreground">{t("models.claudeWorkbench.credential")}</dt>
                        <dd>
                          {target.api_key ? "•••••••• · " : ""}
                          {t("models.claudeWorkbench.credentialDescription")}
                        </dd>
                      </dl>
                    ) : (
                      <p className="text-muted-foreground">{t("models.claudeWorkbench.noProvider")}</p>
                    )}
                    {target && !compatible && (
                      <p className="text-muted-foreground">{t("models.claudeWorkbench.incompatibleDescription")}</p>
                    )}
                  </div>
                )}
              </fieldset>
              <div className="space-y-3">
                {error && (
                  <p role="alert" className="break-words text-destructive">
                    {t("models.claudeWorkbench.applyFailed")} {error}
                  </p>
                )}
                <div className="flex flex-wrap items-center gap-3">
                  <Button disabled={locked || !writable || !target || !compatible} onClick={() => void apply()}>
                    {busy && <Loader2 className="animate-spin" aria-hidden />}
                    {t(
                      busy
                        ? "models.claudeWorkbench.applying"
                        : error
                          ? "models.claudeWorkbench.retryApply"
                          : "models.claudeWorkbench.apply",
                    )}
                  </Button>
                  <p role="status" className="text-muted-foreground">
                    {t(applied ? "models.claudeWorkbench.applied" : "models.claudeWorkbench.notApplied")}
                  </p>
                </div>
              </div>
              {writable && applied && !native && compatible && target && data.binding && (
                <ClaudeMappingPanel
                  key={target.id}
                  provider={target}
                  toolId={data.clientId}
                  binding={data.binding}
                  onBusyChange={setRoleBusy}
                />
              )}
              <section className="space-y-3 border-t border-border pt-6">
                <div className="flex flex-wrap items-center justify-between gap-3">
                  <h2 className="font-semibold">{t("models.claudeWorkbench.savedProviders")}</h2>
                  <Button variant="ghost" disabled={locked} onClick={() => data.setOverlay({ type: "create" })}>
                    <Plus aria-hidden />
                    {t("models.claudeWorkbench.add")}
                  </Button>
                </div>
                {data.thirdPartyProviders.length === 0 ? (
                  <p className="text-muted-foreground">{t("models.claudeWorkbench.noProvider")}</p>
                ) : (
                  <ul className="divide-y divide-border">
                    {data.thirdPartyProviders.map((p) => (
                      <li key={p.id} className="flex flex-wrap items-center justify-between gap-3 py-3">
                        <div className="min-w-0">
                          <p className="break-words font-medium">{p.name}</p>
                          <p className="break-all text-[13px] text-muted-foreground">
                            {p.base_url_anthropic || t("models.claudeWorkbench.incompatible")}
                          </p>
                        </div>
                        <Button
                          variant="ghost"
                          disabled={locked}
                          aria-label={t("models.claudeWorkbench.editNamed", { name: p.name })}
                          onClick={() => data.setOverlay({ type: "edit", providerId: p.id })}
                        >
                          {t("models.claudeWorkbench.edit")}
                        </Button>
                      </li>
                    ))}
                  </ul>
                )}
              </section>
            </Tabs.Content>
          </Tabs.Root>
        </div>
      </main>
    </div>
  );
}
