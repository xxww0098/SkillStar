import { useEffect, useState } from "react";
import { useTranslation } from "react-i18next";
import { Loader2, Plug, Wifi, WifiOff } from "lucide-react";
import { toast } from "sonner";
import { Button } from "../../../components/ui/button";
import { Input } from "../../../components/ui/input";
import { ModalHeader, ModalShell } from "../../../components/ui/ModalShell";
import { Switch } from "../../../components/ui/switch";
import type { S3Target } from "../../../lib/ipc/commands/s3";
import { useTestS3Connection } from "../api/targets";

export interface S3TargetFormValues {
  id?: string;
  displayName: string;
  endpointUrl: string;
  region: string;
  bucket: string;
  prefix: string;
  accessKeyId: string;
  secretAccessKey: string;
  forcePathStyle: boolean;
}

interface Props {
  open: boolean;
  initial?: S3Target | null;
  onOpenChange: (open: boolean) => void;
  onSubmit: (values: S3TargetFormValues) => void;
}

function emptyValues(): S3TargetFormValues {
  return {
    displayName: "",
    endpointUrl: "",
    region: "us-east-1",
    bucket: "",
    prefix: "skillstar/",
    accessKeyId: "",
    secretAccessKey: "",
    forcePathStyle: false,
  };
}

function fromTarget(t: S3Target): S3TargetFormValues {
  return {
    id: t.id,
    displayName: t.display_name,
    endpointUrl: t.endpoint_url,
    region: t.region,
    bucket: t.bucket,
    prefix: t.prefix,
    accessKeyId: t.access_key_id,
    secretAccessKey: "",
    forcePathStyle: t.force_path_style,
  };
}

export function S3TargetForm({ open, initial, onOpenChange, onSubmit }: Props) {
  const { t } = useTranslation();
  const [values, setValues] = useState<S3TargetFormValues>(emptyValues());
  const test = useTestS3Connection();
  const [testResult, setTestResult] = useState<number | "error" | null>(null);

  useEffect(() => {
    if (open) {
      setValues(initial ? fromTarget(initial) : emptyValues());
      setTestResult(null);
    }
  }, [open, initial]);

  const set = <K extends keyof S3TargetFormValues>(k: K, v: S3TargetFormValues[K]) =>
    setValues((p) => ({ ...p, [k]: v }));

  const toDef = (v: S3TargetFormValues): S3Target => ({
    id: v.id ?? "",
    display_name: v.displayName.trim() || v.bucket,
    endpoint_url: v.endpointUrl.trim(),
    region: v.region.trim() || "us-east-1",
    bucket: v.bucket.trim(),
    prefix: v.prefix.trim(),
    access_key_id: v.accessKeyId.trim(),
    force_path_style: v.forcePathStyle,
  });

  const handleTest = async () => {
    const def = toDef(values);
    if (!def.bucket || !def.access_key_id) {
      toast.error(t("s3.toast.testFailed"), { description: "bucket + access key required" });
      return;
    }
    setTestResult(null);
    try {
      const out = await test.mutateAsync({ def });
      setTestResult(out.latency_ms);
    } catch (_e) {
      setTestResult("error");
    }
  };

  const handleSubmit = (e: React.FormEvent) => {
    e.preventDefault();
    onSubmit(values);
  };

  return (
    <ModalShell open={open} onOpenChange={onOpenChange}>
      <ModalHeader title={initial ? t("s3.editTarget") : t("s3.addTarget")} onOpenChange={onOpenChange} />
      <form onSubmit={handleSubmit} className="space-y-4 p-4">
        <Field label={t("s3.form.displayName")}>
          <Input
            value={values.displayName}
            onChange={(e) => set("displayName", e.target.value)}
            placeholder="Cloudflare R2"
          />
        </Field>

        <Field label={t("s3.form.endpointUrl")} hint={t("s3.form.endpointHint")}>
          <Input
            value={values.endpointUrl}
            onChange={(e) => set("endpointUrl", e.target.value)}
            placeholder="https://<account>.r2.cloudflarestorage.com"
          />
        </Field>

        <div className="grid grid-cols-2 gap-3">
          <Field label={t("s3.form.region")}>
            <Input value={values.region} onChange={(e) => set("region", e.target.value)} placeholder="auto" />
          </Field>
          <Field label={t("s3.form.bucket")}>
            <Input value={values.bucket} onChange={(e) => set("bucket", e.target.value)} placeholder="skillstar" />
          </Field>
        </div>

        <Field label={t("s3.form.prefix")} hint={t("s3.form.prefixHint")}>
          <Input value={values.prefix} onChange={(e) => set("prefix", e.target.value)} placeholder="skillstar/" />
        </Field>

        <div className="grid grid-cols-2 gap-3">
          <Field label={t("s3.form.accessKeyId")}>
            <Input
              value={values.accessKeyId}
              onChange={(e) => set("accessKeyId", e.target.value)}
              placeholder="AKIA…"
            />
          </Field>
          <Field label={t("s3.form.secretAccessKey")} hint={initial ? t("s3.form.secretHintExisting") : undefined}>
            <Input
              type="password"
              value={values.secretAccessKey}
              onChange={(e) => set("secretAccessKey", e.target.value)}
              placeholder={initial ? "•••••• (leave blank to keep)" : "••••••••"}
            />
          </Field>
        </div>

        <div className="flex items-center justify-between rounded-lg border border-border px-3 py-2.5">
          <div className="min-w-0 pr-3">
            <div className="text-sm font-medium text-foreground">{t("s3.form.forcePathStyle")}</div>
            <div className="text-xs text-muted-foreground truncate">{t("s3.form.forcePathHint")}</div>
          </div>
          <Switch checked={values.forcePathStyle} onCheckedChange={(checked) => set("forcePathStyle", checked)} />
        </div>

        <div className="flex items-center gap-2">
          <Button type="button" variant="outline" size="sm" onClick={handleTest} disabled={test.isPending}>
            {test.isPending ? <Loader2 className="w-4 h-4 animate-spin mr-1.5" /> : <Plug className="w-4 h-4 mr-1.5" />}
            {t("s3.test")}
          </Button>
          {testResult !== null && (
            <span
              className={
                testResult === "error"
                  ? "text-xs text-red-400 flex items-center gap-1"
                  : "text-xs text-emerald-400 flex items-center gap-1"
              }
            >
              {testResult === "error" ? (
                <>
                  <WifiOff className="w-3.5 h-3.5" />
                  {t("s3.testFail")}
                </>
              ) : (
                <>
                  <Wifi className="w-3.5 h-3.5" />
                  {testResult}ms
                </>
              )}
            </span>
          )}
        </div>

        <div className="flex justify-end gap-2 pt-1">
          <Button type="button" variant="ghost" onClick={() => onOpenChange(false)}>
            {t("common.cancel")}
          </Button>
          <Button type="submit" disabled={!values.bucket || !values.accessKeyId}>
            {t("common.save")}
          </Button>
        </div>
      </form>
    </ModalShell>
  );
}

function Field({ label, hint, children }: { label: string; hint?: string; children: React.ReactNode }) {
  return (
    <div className="space-y-1.5">
      <label className="block text-xs font-medium text-foreground">
        {label}
        {hint ? <span className="ml-1.5 font-normal text-muted-foreground">{hint}</span> : null}
      </label>
      {children}
    </div>
  );
}
