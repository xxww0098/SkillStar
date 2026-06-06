import { SlidersHorizontal } from "lucide-react";
import { memo, useState } from "react";
import { Input } from "../../../../components/ui/input";
import { Switch } from "../../../../components/ui/switch";
import { ConfigCollapseSection, fieldLabelClass } from "./ProviderConfigPrimitives";
import type { ProviderFormState } from "./useProviderFormState";

export interface ProviderAdvancedOptionsProps {
  form: ProviderFormState;
}

function ProviderAdvancedOptionsInner({ form }: ProviderAdvancedOptionsProps) {
  const [expanded, setExpanded] = useState(false);

  return (
    <ConfigCollapseSection
      id="provider-runtime"
      icon={SlidersHorizontal}
      title="运行时参数"
      summary={form.advancedSummary}
      expanded={expanded}
      onToggle={() => setExpanded((p) => !p)}
    >
      <div className="grid gap-3 sm:grid-cols-2 lg:grid-cols-4">
        <label className="space-y-1">
          <span className={fieldLabelClass}>上下文</span>
          <Input
            type="number"
            value={form.contextLength}
            onChange={(e) => form.setContextLength(Number(e.target.value))}
            min={1024}
          />
        </label>
        <label className="space-y-1">
          <span className={fieldLabelClass}>最大 Tokens</span>
          <Input
            type="number"
            value={form.maxTokens}
            onChange={(e) => form.setMaxTokens(Number(e.target.value))}
            min={1}
          />
        </label>
        <label className="space-y-1">
          <span className={fieldLabelClass}>超时 (秒)</span>
          <Input
            type="number"
            value={form.timeout}
            onChange={(e) => form.setTimeout_(Number(e.target.value))}
            min={1}
          />
        </label>
        <label className="space-y-1">
          <span className={fieldLabelClass}>重试</span>
          <Input
            type="number"
            value={form.retryCount}
            onChange={(e) => form.setRetryCount(Number(e.target.value))}
            min={0}
          />
        </label>
      </div>
      <div className="flex items-center justify-between rounded-lg border border-border/45 bg-background/35 px-3 py-2">
        <span className="text-xs font-medium text-foreground">流式输出</span>
        <Switch checked={form.streaming} onCheckedChange={form.setStreaming} />
      </div>
    </ConfigCollapseSection>
  );
}

export const ProviderAdvancedOptions = memo(ProviderAdvancedOptionsInner);
