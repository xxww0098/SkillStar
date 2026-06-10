import type { ProviderEntryFlat } from "../../../../../types";
import type { ProviderForm } from "../../../hooks/useProviderForm";
import { ConnectionStatusPanel } from "../../diagnostics/ConnectionStatusPanel";
import { EndpointSpeedPanel } from "../../diagnostics/EndpointSpeedPanel";
import { ToolJsonConfigPanel } from "../../ToolJsonConfigPanel";

/** 诊断页签：连接测试、余额、端点测速、磁盘配置文件。 */
export function DiagnosticsTab({
  form,
  provider,
  isToolActive,
}: {
  form: ProviderForm;
  provider: ProviderEntryFlat;
  isToolActive: (toolId: string) => boolean;
}) {
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

      <section className="grid gap-2 border-t border-border/40 pt-4">
        <h3 className="text-sm font-semibold text-foreground">磁盘配置文件</h3>
        <ToolJsonConfigPanel providerId={provider.id} isToolActive={isToolActive} embedded />
      </section>
    </div>
  );
}
