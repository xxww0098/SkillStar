import { useCallback } from "react";
import { useNavigation } from "../hooks/useNavigation";
import { HealthDashboard } from "../features/models/components/HealthDashboard";
import { PresetSelector } from "../features/models/components/PresetSelector";
import { ProviderDetailPanel } from "../features/models/components/ProviderDetailPanel";
import { ToolConfigPanel } from "../features/models/components/ToolConfigPanel";

/** Main column has no PageToolbar on Models routes; keep a hit target for `titleBarStyle: Overlay` window dragging. */
function ModelsTopDragStrip() {
  return <div data-tauri-drag-region className="h-4 w-full shrink-0" aria-hidden />;
}

/**
 * Thin page shell for the Models > Providers page.
 * Renders the ProviderDetailPanel in the main content area.
 * The sidebar (ModelsNav) handles provider list navigation;
 * this page displays the selected provider's detail panel.
 */
export function ModelsProviders() {
  const { selectedProviderId, showPresetSelector, setShowPresetSelector, setSelectedProviderId } = useNavigation();

  const handleProviderCreated = useCallback(
    (provider: { id: string }) => {
      setShowPresetSelector(false);
      setSelectedProviderId(provider.id);
    },
    [setShowPresetSelector, setSelectedProviderId],
  );

  const handleCancelPreset = useCallback(() => {
    setShowPresetSelector(false);
  }, [setShowPresetSelector]);

  if (showPresetSelector) {
    return (
      <div className="flex-1 min-w-0 flex flex-col overflow-hidden">
        <ModelsTopDragStrip />
        <main className="ss-page-scroll">
          <div className="w-full max-w-4xl mx-auto px-6 py-6">
            <PresetSelector onProviderCreated={handleProviderCreated} onCancel={handleCancelPreset} />
          </div>
        </main>
      </div>
    );
  }

  return (
    <div className="flex-1 min-w-0 flex flex-col overflow-hidden">
      <ModelsTopDragStrip />
      <ProviderDetailPanel providerId={selectedProviderId} />
    </div>
  );
}

/**
 * Thin page shell for the Models > Health page.
 * Wraps the HealthDashboard feature component with consistent page layout.
 */
export function ModelsHealth() {
  return (
    <div className="flex-1 min-w-0 flex flex-col overflow-hidden">
      <ModelsTopDragStrip />
      <main className="ss-page-scroll">
        <div className="w-full max-w-6xl mx-auto px-6 py-6">
          <HealthDashboard />
        </div>
      </main>
    </div>
  );
}

/**
 * Thin page shell for the Models > Tool Configs page.
 * Wraps the ToolConfigPanel feature component with consistent page layout.
 */
export function ModelsToolConfigs() {
  return (
    <div className="flex-1 min-w-0 flex flex-col overflow-hidden">
      <ModelsTopDragStrip />
      <main className="ss-page-scroll">
        <div className="w-full max-w-6xl mx-auto px-6 py-6">
          <ToolConfigPanel />
        </div>
      </main>
    </div>
  );
}

/**
 * Thin page shell for the Models > Settings page.
 * Placeholder until a dedicated ModelsSettings feature component is created.
 */
export function ModelsSettings() {
  return (
    <div className="flex-1 min-w-0 flex flex-col overflow-hidden">
      <ModelsTopDragStrip />
      <main className="ss-page-scroll">
        <div className="w-full max-w-6xl mx-auto px-6 py-6">
          <div className="flex items-center justify-center min-h-[200px] text-muted-foreground">
            <p>Models Settings — coming soon</p>
          </div>
        </div>
      </main>
    </div>
  );
}
