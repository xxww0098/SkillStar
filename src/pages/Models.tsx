import { ModelsHub } from "../features/models/components/hub/ModelsHub";
import type { ModelsNavBridge } from "../features/models/lib/navBridge";

/**
 * Single Models page — the production hub is the Provider × Agent matrix.
 *
 * Nav bridge is supplied by App (outside the lazy chunk) so the hub never
 * calls `useNavigation` itself.
 */
export function Models(props: ModelsNavBridge) {
  return <ModelsHub {...props} />;
}
