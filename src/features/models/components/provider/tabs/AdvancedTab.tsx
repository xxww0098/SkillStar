import { Input } from "../../../../../components/ui/input";
import { Switch } from "../../../../../components/ui/switch";
import { cn } from "../../../../../lib/utils";
import type { ProviderForm } from "../../../hooks/useProviderForm";
import { ClaudeModelMapping } from "../../agents/ClaudeModelMapping";
import { CodexSettingsForm } from "../../agents/CodexSettingsForm";
import { fieldLabelClass } from "../../providerForm/ProviderConfigPrimitives";

/** 高级页签：运行参数、备注，以及（暂驻）Agent 专属模型参数。 */
export function AdvancedTab({ form }: { form: ProviderForm }) {
  const { values, setField } = form;

  return (
    <div className="grid gap-5">
      <section className="grid gap-3">
        <h3 className="text-sm font-semibold text-foreground">运行参数</h3>
        <div className="grid gap-3 sm:grid-cols-2">
          <label className="space-y-1">
            <span className={fieldLabelClass}>上下文</span>
            <Input
              type="number"
              value={values.contextLength}
              onChange={(e) => setField("contextLength", Number(e.target.value))}
              min={1024}
            />
          </label>
          <label className="space-y-1">
            <span className={fieldLabelClass}>最大 Tokens</span>
            <Input
              type="number"
              value={values.maxTokens}
              onChange={(e) => setField("maxTokens", Number(e.target.value))}
              min={1}
            />
          </label>
          <label className="space-y-1">
            <span className={fieldLabelClass}>超时 (秒)</span>
            <Input
              type="number"
              value={values.timeout}
              onChange={(e) => setField("timeout", Number(e.target.value))}
              min={1}
            />
          </label>
          <label className="space-y-1">
            <span className={fieldLabelClass}>重试</span>
            <Input
              type="number"
              value={values.retryCount}
              onChange={(e) => setField("retryCount", Number(e.target.value))}
              min={0}
            />
          </label>
        </div>
        <div className="flex items-center justify-between rounded-lg border border-border/45 bg-background/35 px-3 py-2">
          <span className="text-xs font-medium text-foreground">流式输出</span>
          <Switch checked={values.streaming} onCheckedChange={(v) => setField("streaming", v)} />
        </div>
      </section>

      <section className="grid gap-4 border-t border-border/40 pt-4">
        <ClaudeModelMapping
          values={values}
          options={form.claudeModelOptions}
          onChange={(key, value) => setField(key, value)}
        />
        <div className="border-t border-border/40 pt-4">
          <CodexSettingsForm
            wireApi={values.codexWireApi}
            authMode={values.codexAuthMode}
            onChangeWireApi={(v) => setField("codexWireApi", v)}
            onChangeAuthMode={(v) => setField("codexAuthMode", v)}
          />
        </div>
      </section>

      <section className="grid gap-1 border-t border-border/40 pt-4">
        <span className={fieldLabelClass}>备注</span>
        <textarea
          value={values.notes}
          onChange={(e) => setField("notes", e.target.value)}
          rows={3}
          className={cn(
            "flex min-h-9 w-full resize-none rounded-xl border border-input-border bg-input px-3 py-2 text-sm",
            "focus-visible:outline-none focus-visible:ring-2 focus-visible:ring-primary/40",
          )}
        />
      </section>
    </div>
  );
}
