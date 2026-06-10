import { Copy, Eye, EyeOff, Globe2, KeyRound, Plus, ShieldCheck } from "lucide-react";
import { useCallback, useState } from "react";
import { Button } from "../../../../../components/ui/button";
import { Input } from "../../../../../components/ui/input";
import type { ProviderForm } from "../../../hooks/useProviderForm";
import { fieldLabelClass } from "../../providerForm/ProviderConfigPrimitives";

/** 连接页签：名称、API Key、双端点、模型列表 URL。 */
export function ConnectionTab({ form }: { form: ProviderForm }) {
  const { values, setField } = form;
  const [showApiKey, setShowApiKey] = useState(false);
  const [showAnthropicUrl, setShowAnthropicUrl] = useState(Boolean(values.baseUrlAnthropic.trim()));

  const handleCopyApiKey = useCallback(async () => {
    if (!values.apiKey || typeof navigator === "undefined" || !navigator.clipboard) return;
    try {
      await navigator.clipboard.writeText(values.apiKey);
    } catch {
      /* clipboard unavailable in some shells */
    }
  }, [values.apiKey]);

  return (
    <div className="grid gap-4">
      <label className="space-y-1">
        <span className={fieldLabelClass}>名称</span>
        <Input value={values.name} onChange={(e) => setField("name", e.target.value)} placeholder="DeepSeek" />
      </label>

      <div className="space-y-1">
        <div className="flex items-center justify-between">
          <span className={fieldLabelClass}>API Key</span>
          <Button
            type="button"
            variant="ghost"
            size="sm"
            onClick={handleCopyApiKey}
            disabled={!values.apiKey}
            className="h-6 px-2 text-[11px] text-muted-foreground"
          >
            <Copy className="mr-1 h-3 w-3" />
            复制
          </Button>
        </div>
        <div className="relative">
          <KeyRound className="pointer-events-none absolute left-3 top-1/2 h-3.5 w-3.5 -translate-y-1/2 text-muted-foreground/70" />
          <Input
            type={showApiKey ? "text" : "password"}
            value={values.apiKey}
            onChange={(e) => setField("apiKey", e.target.value)}
            placeholder="sk-..."
            className="pl-9 pr-10"
          />
          <button
            type="button"
            onClick={() => setShowApiKey((v) => !v)}
            className="absolute right-2.5 top-1/2 -translate-y-1/2 rounded-md p-1 text-muted-foreground hover:text-foreground"
            aria-label={showApiKey ? "隐藏" : "显示"}
          >
            {showApiKey ? <EyeOff className="h-4 w-4" /> : <Eye className="h-4 w-4" />}
          </button>
        </div>
      </div>

      <div className="space-y-1">
        <span className={fieldLabelClass}>OpenAI 兼容端点</span>
        <div className="relative">
          <Globe2 className="pointer-events-none absolute left-3 top-1/2 h-3.5 w-3.5 -translate-y-1/2 text-muted-foreground/70" />
          <Input
            value={values.baseUrlOpenai}
            onChange={(e) => setField("baseUrlOpenai", e.target.value)}
            placeholder="https://api.example.com/v1"
            className="pl-9"
          />
        </div>
        <p className="text-[11px] text-muted-foreground/75">Codex / OpenCode / Gemini CLI 使用</p>
      </div>

      {showAnthropicUrl || values.baseUrlAnthropic ? (
        <div className="space-y-1">
          <span className={fieldLabelClass}>Anthropic 兼容端点</span>
          <div className="relative">
            <Globe2 className="pointer-events-none absolute left-3 top-1/2 h-3.5 w-3.5 -translate-y-1/2 text-muted-foreground/70" />
            <Input
              value={values.baseUrlAnthropic}
              onChange={(e) => setField("baseUrlAnthropic", e.target.value)}
              placeholder="https://api.example.com/anthropic"
              className="pl-9"
            />
          </div>
          <p className="text-[11px] text-muted-foreground/75">Claude Code 使用</p>
        </div>
      ) : (
        <Button
          type="button"
          variant="ghost"
          size="sm"
          className="h-8 w-fit gap-1.5 text-xs text-muted-foreground"
          onClick={() => setShowAnthropicUrl(true)}
        >
          <Plus className="h-3.5 w-3.5" />
          添加 Anthropic 端点（Claude Code）
        </Button>
      )}

      <label className="space-y-1">
        <span className={fieldLabelClass}>模型列表 URL</span>
        <Input
          value={values.modelsUrl}
          onChange={(e) => setField("modelsUrl", e.target.value)}
          placeholder="https://api.example.com/v1/models"
        />
        <p className="text-[11px] text-muted-foreground/75">「模型」页签的拉取入口使用此地址</p>
      </label>

      <p className="flex items-center gap-1.5 text-[11px] text-muted-foreground">
        <ShieldCheck className="h-3.5 w-3.5 text-primary/80" />
        凭据仅保存在本机
      </p>
    </div>
  );
}
