import type { ModelsDrawerRequest } from "../../../../../hooks/useNavigation";

export type { ModelsDrawerRequest };

/**
 * Navigation fields the Models hub needs from App-level NavigationProvider.
 *
 * Passed as props (not read via useNavigation inside the lazy Models chunk) so a
 * Vite/HMR duplicate of the nav context module cannot crash the hub.
 */
export type ModelsNavBridge = {
  selectedProviderId: string | null;
  setSelectedProviderId: (id: string | null) => void;
  modelsDrawerRequest: ModelsDrawerRequest | null;
  clearModelsDrawerRequest: () => void;
};
