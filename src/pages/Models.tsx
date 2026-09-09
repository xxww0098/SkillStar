import { ModelsHub } from "../features/models/components/hub/ModelsHub";
import type { ModelsNavBridge } from "../features/models/lib/navBridge";

/**
 * Claude Code connection and model configuration workbench.
 *
 * Nav bridge is supplied by App (outside the lazy chunk) so the hub never
 * calls `useNavigation` itself.
 */
export function Models(props: ModelsNavBridge) {
  return <ModelsHub {...props} />;
}
