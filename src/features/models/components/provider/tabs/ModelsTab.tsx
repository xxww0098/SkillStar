import { Boxes, CheckCircle2, Download, Loader2, Plus, Search, Star, TriangleAlert, X } from "lucide-react";
import { useCallback, useEffect, useMemo, useState } from "react";
import { useTranslation } from "react-i18next";
import { Button } from "../../../../../components/ui/button";
import { Input } from "../../../../../components/ui/input";
import { cn } from "../../../../../lib/utils";
import type { ProviderForm } from "../../../hooks/useProviderForm";
import { formatModelMetadata } from "../../../lib/modelFormat";
import { ModelFormField, ModelFormSection, modelCompactInputClass } from "../../providerForm/ProviderConfigPrimitives";
import { EditableModelCombobox } from "../../shared/EditableModelCombobox";

/** 模型页签：拉取模型、默认模型、模型列表管理。 */
export function ModelsTab({ form }: { form: ProviderForm }) {
  const { values, setField } = form;
  const { t } = useTranslation();
  const [newModel, setNewModel] = useState("");
  const [query, setQuery] = useState("");

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

  const filteredModels = useMemo(() => {
    if (values.models.length <= 6) return values.models;
    const normalized = query.trim().toLowerCase();
    if (!normalized) return values.models;
    return values.models.filter((id) => {
      const meta = values.modelCatalog.find((entry) => entry.id === id);
      return id.toLowerCase().includes(normalized) || meta?.display_name?.toLowerCase().includes(normalized);
    });
  }, [query, values.models, values.modelCatalog]);

  useEffect(() => {
    if (values.models.length <= 6 && query) setQuery("");
  }, [query, values.models.length]);

  return (
    <div className="grid gap-3.5">
      <ModelFormSection
        title={t("models.modelsTab.title")}
        description={
          values.models.length > 0
            ? t("models.modelsTab.modelCount", { count: values.models.length })
            : t("models.modelsTab.empty")
        }
        icon={<Boxes className="h-4 w-4" />}
        action={
          <span title={canFetch ? undefined : t("models.modelsTab.fetchRequirement")}>
            <Button
              type="button"
              variant="outline"
              size="sm"
              className="gap-1.5 text-xs"
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
        }
      >
        <ModelFormField
          id="provider-default-model"
          label={t("models.modelsTab.defaultModelLabel")}
          info={t("models.modelsTab.defaultModelHint")}
        >
          <EditableModelCombobox
            id="provider-default-model"
            ariaLabel={t("models.modelsTab.defaultModelLabel")}
            options={form.codexModelOptions}
            value={values.defaultModel}
            onChange={(model) => setField("defaultModel", model)}
            placeholder={t("models.modelsTab.defaultModelPlaceholder")}
          />
        </ModelFormField>

        {form.fetchError ? (
          <p
            className="flex items-start gap-1.5 rounded-[10px] border border-destructive/25 bg-destructive/10 px-3 py-2 text-[11px] leading-4 text-destructive"
            role="alert"
          >
            <TriangleAlert className="mt-0.5 h-3.5 w-3.5 shrink-0" />
            {form.fetchError.message}
          </p>
        ) : form.modelFetchCount !== null ? (
          <p
            className="flex items-center gap-1.5 rounded-[10px] border border-success/20 bg-success/10 px-3 py-2 text-[11px] text-success"
            role="status"
          >
            <CheckCircle2 className="h-3.5 w-3.5" />
            {t("models.modelsTab.fetchSuccess", { count: form.modelFetchCount })}
          </p>
        ) : null}

        {!canFetch ? (
          <p className="text-[11px] leading-4 text-muted-foreground">{t("models.modelsTab.fetchRequirement")}</p>
        ) : null}

        {values.models.length > 6 ? (
          <div className="border-t border-border/40 pt-3.5">
            <div className="relative">
              <Search className="pointer-events-none absolute left-3 top-1/2 h-3.5 w-3.5 -translate-y-1/2 text-muted-foreground/60" />
              <Input
                value={query}
                onChange={(event) => setQuery(event.target.value)}
                placeholder={t("models.modelsTab.searchPlaceholder")}
                className={`${modelCompactInputClass} pl-9`}
              />
            </div>
          </div>
        ) : null}

        {values.models.length > 0 && filteredModels.length === 0 ? (
          <p className="rounded-[10px] border border-dashed border-border/55 px-3 py-5 text-center text-[11px] text-muted-foreground">
            {t("models.picker.noModelMatch")}
          </p>
        ) : values.models.length > 0 ? (
          <ul className="grid max-h-72 gap-1.5 overflow-y-auto pr-1">
            {filteredModels.map((id) => {
              const meta = values.modelCatalog.find((entry) => entry.id === id);
              const isDefault = values.defaultModel === id;
              return (
                <li
                  key={id}
                  className={cn(
                    "group flex min-h-11 items-center gap-2 rounded-[10px] border px-3 py-2 transition",
                    isDefault
                      ? "border-primary/30 bg-primary/[0.08]"
                      : "border-border/45 bg-background/35 hover:border-border/70",
                  )}
                >
                  <div className="min-w-0 flex-1">
                    <p className="truncate font-mono text-[11px] text-foreground">{meta?.display_name || id}</p>
                    <p className="truncate text-[10px] text-muted-foreground">
                      {meta ? formatModelMetadata(meta, t) : id}
                    </p>
                  </div>
                  <Button
                    type="button"
                    variant="ghost"
                    size="icon-xs"
                    onClick={() => setField("defaultModel", id)}
                    title={isDefault ? t("models.modelsTab.currentDefault") : t("models.modelsTab.setDefault")}
                    aria-label={isDefault ? t("models.modelsTab.currentDefault") : t("models.modelsTab.setDefault")}
                    className={cn(isDefault ? "text-primary" : "text-muted-foreground/50 hover:text-foreground")}
                  >
                    <Star className={cn("h-3.5 w-3.5", isDefault && "fill-current")} />
                  </Button>
                  <Button
                    type="button"
                    variant="ghost"
                    size="icon-xs"
                    onClick={() => removeModel(id)}
                    title={t("models.modelsTab.remove")}
                    aria-label={t("models.modelsTab.remove")}
                    className="text-muted-foreground/50 hover:bg-destructive/10 hover:text-destructive"
                  >
                    <X className="h-3.5 w-3.5" />
                  </Button>
                </li>
              );
            })}
          </ul>
        ) : null}

        <div className="flex items-center gap-2 border-t border-border/40 pt-3.5">
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
            className={modelCompactInputClass}
          />
          <Button
            type="button"
            variant="outline"
            size="sm"
            className="h-9 gap-1 text-xs"
            onClick={addModel}
            disabled={!newModel.trim()}
          >
            <Plus className="h-3.5 w-3.5" />
            {t("models.modelsTab.add")}
          </Button>
        </div>
      </ModelFormSection>
    </div>
  );
}
