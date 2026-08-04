import { ModelsHub } from "../features/models/components/hub/ModelsHub";
import { ModelsHubPrototype } from "../features/models/components/hub/prototype/ModelsHubPrototype";
import type { ModelsNavBridge } from "../features/models/components/hub/prototype/modelsNavBridge";

/**
 * Single Models page — production hub is the D1 Provider × Agent matrix.
 *
 * DEV only: `?variant=D2` / `?variant=D3` still open alternate IA prototypes.
 *
 * Nav bridge is supplied by App (outside the lazy chunk) so the hub never
 * calls `useNavigation` itself.
 */
function isAlternateIaPrototype(): boolean {
  if (import.meta.env.PROD) return false;
  if (typeof window === "undefined") return false;
  const variant = new URLSearchParams(window.location.search).get("variant");
  return variant === "D2" || variant === "D3";
}

export function Models(props: ModelsNavBridge) {
  if (isAlternateIaPrototype()) {
    return <ModelsHubPrototype {...props} />;
  }
  return <ModelsHub {...props} />;
}
