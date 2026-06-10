/**
 * Public API of the models feature.
 *
 * Cross-feature consumers (pages, layout, settings, mcp) must import from
 * here instead of reaching into internal paths. `ModelsHub` is intentionally
 * NOT re-exported: pages/Models.tsx imports it directly so the hub stays in
 * its own lazy-loaded chunk.
 */
export { useAppAiProvider, type AppAiAppId } from "./api/appAi";
export { useModelFetch } from "./api/modelCatalog";
export { AgentToolIcon } from "./components/shared/AgentToolIcon";
export { DrawerShell, type DrawerShellProps } from "./components/shared/DrawerShell";
export { ProviderBrandIcon } from "./components/shared/ProviderBrandIcon";
export { useProvidersFlat } from "./hooks/useProvidersFlat";
