import { ModelsHub } from "../features/models/components/hub/ModelsHub";

/**
 * Single Models page. The hub merges what used to be four separate sub-pages
 * (Agent connections / providers / health / tool configs) into one Agent-centric
 * workbench with a right-side drawer for create/edit.
 */
export function Models() {
  return <ModelsHub />;
}
