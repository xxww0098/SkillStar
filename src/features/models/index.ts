/**
 * Public API of the models feature.
 *
 * Cross-feature consumers (pages, layout, settings, mcp) must import from
 * here instead of reaching into internal paths. `ModelsHub` is intentionally
 * NOT re-exported: pages/Models.tsx imports it directly so the hub stays in
 * its own lazy-loaded chunk.
 *
 * Purely presentational components with no models business logic
 * (`DrawerShell`, `ProviderBrandIcon`) live in `src/components/shared/`
 * instead — import them from there, not from this barrel.
 */
export { useAppAiProvider, type AppAiAppId } from "./api/appAi";
export { buildModelCatalog, CLAUDE_MODEL_META_KEYS, getMetaString } from "./lib/providerPatch";
export {
  isNativeOfficialProvider,
  matrixProviders,
  CLAUDE_OFFICIAL_ID,
  CODEX_OFFICIAL_ID,
} from "./lib/officialProviders";
export { useModelFetch } from "./api/modelCatalog";
export { AgentToolIcon } from "./components/shared/AgentToolIcon";
export { useProvidersFlat } from "./hooks/useProvidersFlat";
