import type { ProviderEntryFlat } from "../../../../../types";
import type { ProviderForm } from "../../../hooks/useProviderForm";
import { ConnectionStatusPanel } from "../../diagnostics/ConnectionStatusPanel";
import { EndpointSpeedPanel } from "../../diagnostics/EndpointSpeedPanel";

/** 诊断页签：连接测试、余额、端点测速。磁盘配置文件在各 Agent 的接入设置里。 */
export function DiagnosticsTab({ form, provider }: { form: ProviderForm; provider: ProviderEntryFlat }) {
  const { values } = form;
  return (
    <div className="grid gap-5">
      <ConnectionStatusPanel
        providerId={provider.id}
        presetId={provider.preset_id}
        apiKey={values.apiKey}
        baseUrlOpenai={values.baseUrlOpenai}
        baseUrlAnthropic={values.baseUrlAnthropic}
      />

      <section className="border-t border-border/40 pt-4">
        <EndpointSpeedPanel
          urls={form.speedTestUrls}
          apiKey={values.apiKey}
          onApplyFastest={form.handleApplyFastestEndpoint}
        />
      </section>
    </div>
  );
}
