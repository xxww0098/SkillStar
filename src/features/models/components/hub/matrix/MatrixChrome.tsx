import type { ReactNode } from "react";
import { isNativeOfficialProvider } from "../../../lib/officialProviders";
import type { ModelsHubData } from "./types";
import { AgentColumnCarousel } from "./AgentColumnCarousel";

type MatrixChromeProps = {
  data: ModelsHubData;
  title: string;
  subtitle: string;
  /** Extra controls under the header (search, filters, legend). */
  toolbar?: ReactNode;
  children: ReactNode;
};

/** Shared B-family shell: header + SVG agent carousel + scroll frame. */
export function MatrixChrome({ data, title, subtitle, toolbar, children }: MatrixChromeProps) {
  return (
    <div className="flex h-full min-h-0 flex-col overflow-hidden">
      <div data-tauri-drag-region className="h-4 shrink-0" aria-hidden />
      <main className="ss-page-scroll">
        <div className="mx-auto w-full max-w-6xl space-y-4 px-5 py-6">
          <header>
            <h1 className="text-2xl font-bold tracking-tight">{title}</h1>
            <p className="mt-1 text-sm text-muted-foreground">{subtitle}</p>
          </header>

          <AgentColumnCarousel data={data} />

          {toolbar}

          {children}
        </div>
      </main>
    </div>
  );
}

export function providerReady(provider: {
  id?: string;
  preset_id?: string;
  api_key?: string;
  base_url_openai?: string;
  base_url_anthropic?: string;
}) {
  if (isNativeOfficialProvider(provider)) {
    return { label: "official", tone: "ok" as const };
  }
  const key = Boolean(provider.api_key?.trim());
  const url = Boolean(provider.base_url_openai?.trim() || provider.base_url_anthropic?.trim());
  if (key && url) return { label: "ready", tone: "ok" as const };
  if (!key) return { label: "no key", tone: "warn" as const };
  return { label: "no url", tone: "warn" as const };
}
