import { Download, Loader2, Plus, Star, X } from "lucide-react";
import { useCallback, useState } from "react";
import { useTranslation } from "react-i18next";
import { Button } from "../../../../../components/ui/button";
import { Input } from "../../../../../components/ui/input";
import { cn } from "../../../../../lib/utils";
import type { ProviderForm } from "../../../hooks/useProviderForm";
import { formatModelMetadata } from "../../../lib/modelFormat";
import { fieldLabelClass } from "../../providerForm/ProviderConfigPrimitives";

/** 模型页签：拉取模型、默认模型、模型列表管理。 */
export function ModelsTab({ form }: { form: ProviderForm }) {
  const { values, setField } = form;
  const { t } = useTranslation();
  const [newModel, setNewModel] = useState("");

  const canFetch = Boolean(values.modelsUrl.trim() && values.apiKey.trim());

  const addModel = useCallback(() => {
    const id = newModel.trim();
    if (!id) return;
    if (!values.models.includes(id)) {
      setField("models", [...values.models, id]);
    }
    setNewModel("");
  }, [newModel, values.models, setField]);

  const removeModel = useCallback(
    (id: string) => {
      setField(
        "models",
        values.models.filter((m) => m !== id),
      );
      if (values.defaultModel === id) setField("defaultModel", "");
    },
    [values.models, values.defaultModel, setField],
  );

  return (
    <div className="grid gap-4">
      <div className="flex items-center justify-between gap-2">
        <div>
          <h3 className="text-sm font-semibold text-foreground">{t("models.modelsTab.title")}</h3>
          <p className="text-[11px] text-muted-foreground">
            {values.models.length > 0
              ? t("models.modelsTab.modelCount", { count: values.models.length })
              : t("models.modelsTab.empty")}
          </p>
        </div>
        <span title={canFetch ? undefined : t("models.modelsTab.fetchRequirement")}>
          <Button
            type="button"
            variant="outline"
            size="sm"
            className="h-8 gap-1.5 text-xs"
            onClick={() => void form.handleFetchModels()}
            disabled={form.isFetchingModels || !canFetch}
          >
            {form.isFetchingModels ? (
              <Loader2 className="h-3.5 w-3.5 animate-spin" />
            ) : (
              <Download className="h-3.5 w-3.5" />
            )}
            {t("models.modelsTab.fetchModels")}
          </Button>
        </span>
      </div>

      <div className="space-y-1">
        <span className={fieldLabelClass}>{t("models.modelsTab.defaultModelLabel")}</span>
        <Input
          value={values.defaultModel}
          onChange={(e) => setField("defaultModel", e.target.value)}
          placeholder="deepseek-chat"
          list="models-tab-options"
        />
        <datalist id="models-tab-options">
          {form.codexModelOptions.map((m) => (
            <option key={m} value={m} />
          ))}
        </datalist>
      </div>

      {values.models.length > 0 && (
        <ul className="grid gap-1.5">
          {values.models.map((id) => {
            const meta = values.modelCatalog.find((entry) => entry.id === id);
            const isDefault = values.defaultModel === id;
            return (
              <li
                key={id}
                className={cn(
                  "group flex items-center gap-2 rounded-lg border px-2.5 py-1.5",
                  isDefault ? "border-primary/35 bg-primary/10" : "border-border/45 bg-background/35",
                )}
              >
                <div className="min-w-0 flex-1">
                  <p className="truncate font-mono text-[11px] text-foreground">{meta?.display_name || id}</p>
                  <p className="truncate text-[10px] text-muted-foreground">{meta ? formatModelMetadata(meta) : id}</p>
                </div>
                <button
                  type="button"
                  onClick={() => setField("defaultModel", id)}
                  title={isDefault ? t("models.modelsTab.currentDefault") : t("models.modelsTab.setDefault")}
                  className={cn(
                    "rounded-md p-1 transition",
                    isDefault ? "text-primary" : "text-muted-foreground/50 hover:text-foreground",
                  )}
                >
                  <Star className={cn("h-3.5 w-3.5", isDefault && "fill-current")} />
                </button>
                <button
                  type="button"
                  onClick={() => removeModel(id)}
                  title={t("models.modelsTab.remove")}
                  className="rounded-md p-1 text-muted-foreground/50 transition hover:text-destructive"
                >
                  <X className="h-3.5 w-3.5" />
                </button>
              </li>
            );
          })}
        </ul>
      )}

      <div className="flex items-center gap-2">
        <Input
          value={newModel}
          onChange={(e) => setNewModel(e.target.value)}
          onKeyDown={(e) => {
            if (e.key === "Enter") {
              e.preventDefault();
              addModel();
            }
          }}
          placeholder={t("models.modelsTab.addPlaceholder")}
          className="h-8 text-xs"
        />
        <Button type="button" variant="ghost" size="sm" className="h-8 gap-1 text-xs" onClick={addModel}>
          <Plus className="h-3.5 w-3.5" />
          {t("models.modelsTab.add")}
        </Button>
      </div>
    </div>
  );
}
