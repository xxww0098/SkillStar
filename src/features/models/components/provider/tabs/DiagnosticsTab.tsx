import type { ProviderEntryFlat } from "../../../../../types";
import type { ProviderForm } from "../../../hooks/useProviderForm";
import { ConnectionStatusPanel } from "../../diagnostics/ConnectionStatusPanel";
import { EndpointSpeedPanel } from "../../diagnostics/EndpointSpeedPanel";

/** Provider connection tests, balance, and endpoint latency diagnostics. */
export function DiagnosticsTab({ form, provider }: { form: ProviderForm; provider: ProviderEntryFlat }) {
  const { values } = form;
  return (
    <div className="grid gap-3.5">
      <ConnectionStatusPanel
        providerId={provider.id}
        presetId={provider.preset_id}
        apiKey={values.apiKey}
        baseUrlOpenai={values.baseUrlOpenai}
        baseUrlAnthropic={values.baseUrlAnthropic}
      />

      <EndpointSpeedPanel
        urls={form.speedTestUrls}
        providerId={provider.id}
        onApplyFastest={form.handleApplyFastestEndpoint}
      />
    </div>
  );
}
