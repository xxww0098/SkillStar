import { useEffect, useState } from "react";
import { useTranslation } from "react-i18next";
import { FolderOpen, Plug, ShieldAlert, ShieldCheck } from "lucide-react";
import { toast } from "sonner";
import { Button } from "../../../components/ui/button";
import { Input } from "../../../components/ui/input";
import { ModalHeader, ModalShell } from "../../../components/ui/ModalShell";
import type { AuthMethod, SshHost } from "../../../lib/ipc/commands/ssh";
import { useAcceptHostKey, useTestConnection } from "../api/remote";

export interface SshHostFormValues {
  id?: string;
  displayName: string;
  host: string;
  port: number;
  username: string;
  authMethod: AuthMethod;
  keyPath: string;
  credential: string;
  defaultRemoteDir: string;
}

interface Props {
  open: boolean;
  initial?: SshHost | null;
  onOpenChange: (open: boolean) => void;
  onSubmit: (values: SshHostFormValues) => void;
}

function emptyValues(): SshHostFormValues {
  return {
    displayName: "",
    host: "",
    port: 22,
    username: "root",
    authMethod: { kind: "key", key_path: "~/.ssh/id_ed25519" },
    keyPath: "~/.ssh/id_ed25519",
    credential: "",
    defaultRemoteDir: "~/.claude/skills",
  };
}

function fromHost(h: SshHost): SshHostFormValues {
  return {
    id: h.id,
    displayName: h.display_name,
    host: h.host,
    port: h.port,
    username: h.username,
    authMethod: h.auth_method,
    keyPath: h.auth_method.kind === "key" ? h.auth_method.key_path : "",
    credential: "",
    defaultRemoteDir: h.default_remote_dir,
  };
}

export function SshHostForm({ open, initial, onOpenChange, onSubmit }: Props) {
  const { t } = useTranslation();
  const [values, setValues] = useState<SshHostFormValues>(emptyValues());
  const test = useTestConnection();
  const acceptKey = useAcceptHostKey();

  useEffect(() => {
    if (open) setValues(initial ? fromHost(initial) : emptyValues());
  }, [open, initial]);

  const set = <K extends keyof SshHostFormValues>(k: K, v: SshHostFormValues[K]) =>
    setValues((p) => ({ ...p, [k]: v }));

  const toDef = (v: SshHostFormValues): SshHost => ({
    id: v.id ?? "",
    display_name: v.displayName.trim() || v.host,
    host: v.host.trim(),
    port: Number(v.port) || 22,
    username: v.username.trim(),
    auth_method: v.authMethod.kind === "password" ? { kind: "password" } : { kind: "key", key_path: v.keyPath.trim() },
    default_remote_dir: v.defaultRemoteDir.trim(),
  });

  const handleTest = async () => {
    const def = toDef(values);
    if (!def.host || !def.username) {
      toast.error(t("ssh.toast.testFailed"), { description: "host/username required" });
      return;
    }
    const out = await test.mutateAsync(def);
    if (out.host_key_state === "unverified" || out.host_key_state === "mismatch") {
      toast.info(out.host_key_state === "unverified" ? t("ssh.hostKeyUnverified") : t("ssh.hostKeyMismatch"), {
        description: `${t("ssh.hostKeyFingerprint")}: ${out.fingerprint}`,
        duration: 12000,
        action:
          out.host_key_state === "unverified"
            ? {
                label: t("ssh.acceptHostKey"),
                onClick: async () => {
                  await acceptKey.mutateAsync({
                    id: def.id || `pending_${def.host}`,
                    host: `${def.host}:${def.port}`,
                    fingerprint: out.fingerprint ?? "",
                  });
                },
              }
            : undefined,
      });
    }
  };

  const isKey = values.authMethod.kind === "key";

  return (
    <ModalShell
      open={open}
      onClose={() => onOpenChange(false)}
      ariaLabel={initial ? t("ssh.editHost") : t("ssh.addHost")}
      panelClassName="max-w-[520px]"
    >
      <ModalHeader title={initial ? t("ssh.editHost") : t("ssh.addHost")} onClose={() => onOpenChange(false)} />
      <form
        className="flex flex-col gap-4 p-5"
        onSubmit={(e) => {
          e.preventDefault();
          onSubmit(values);
        }}
      >
        <div className="grid grid-cols-2 gap-3">
          <Field label={t("ssh.form.displayName")} className="col-span-2">
            <Input
              value={values.displayName}
              onChange={(e) => set("displayName", e.target.value)}
              placeholder="Prod GPU Box"
            />
          </Field>
          <Field label={t("ssh.form.host")}>
            <Input value={values.host} onChange={(e) => set("host", e.target.value)} placeholder="10.0.0.42" />
          </Field>
          <Field label={t("ssh.form.port")}>
            <Input type="number" value={values.port} onChange={(e) => set("port", Number(e.target.value))} />
          </Field>
          <Field label={t("ssh.form.username")}>
            <Input value={values.username} onChange={(e) => set("username", e.target.value)} placeholder="root" />
          </Field>
          <Field label={t("ssh.form.authMethod")}>
            <select
              className="h-9 rounded-lg border border-border/80 bg-background/50 px-2 text-sm"
              value={values.authMethod.kind}
              onChange={(e) =>
                set(
                  "authMethod",
                  e.target.value === "password"
                    ? { kind: "password" }
                    : { kind: "key", key_path: values.keyPath || "~/.ssh/id_ed25519" },
                )
              }
            >
              <option value="key">{t("ssh.authKind.key")}</option>
              <option value="password">{t("ssh.authKind.password")}</option>
            </select>
          </Field>
        </div>

        {isKey ? (
          <>
            <Field label={t("ssh.form.keyPath")}>
              <div className="flex gap-2">
                <Input
                  value={values.keyPath}
                  onChange={(e) => set("keyPath", e.target.value)}
                  placeholder={t("ssh.form.keyPathPlaceholder")}
                />
                <Button
                  type="button"
                  variant="outline"
                  size="icon"
                  title={t("ssh.form.browse")}
                  onClick={() => toast.info("file picker — use tauri-plugin-dialog")}
                >
                  <FolderOpen className="size-4" />
                </Button>
              </div>
            </Field>
            <Field label={t("ssh.form.passphrase")} hint="stored in system keyring">
              <Input
                type="password"
                value={values.credential}
                onChange={(e) => set("credential", e.target.value)}
                placeholder="optional"
              />
            </Field>
          </>
        ) : (
          <Field label={t("ssh.form.password")} hint="stored in system keyring">
            <Input type="password" value={values.credential} onChange={(e) => set("credential", e.target.value)} />
          </Field>
        )}

        <Field label={t("ssh.form.defaultRemoteDir")}>
          <Input
            value={values.defaultRemoteDir}
            onChange={(e) => set("defaultRemoteDir", e.target.value)}
            placeholder="~/.claude/skills"
          />
        </Field>

        {test.data ? (
          <div className="flex items-center gap-2 rounded-lg border border-border/60 bg-card/40 px-3 py-2 text-xs text-muted-foreground">
            {test.data.host_key_state === "verified" ? (
              <ShieldCheck className="size-4 text-green-500" />
            ) : (
              <ShieldAlert className="size-4 text-amber-500" />
            )}
            <span>
              {test.data.result.remote_user} · {test.data.result.system ?? "—"} ·{" "}
              {t("ssh.latency", { ms: test.data.result.latency_ms })}
            </span>
          </div>
        ) : null}

        <div className="flex justify-between gap-2 pt-1">
          <Button type="button" variant="outline" onClick={handleTest} disabled={test.isPending}>
            <Plug className="size-4" />
            {test.isPending ? t("ssh.testing") : t("ssh.test")}
          </Button>
          <div className="flex gap-2">
            <Button type="button" variant="ghost" onClick={() => onOpenChange(false)}>
              Cancel
            </Button>
            <Button type="submit" disabled={!values.host || !values.username}>
              {initial ? t("ssh.editHost") : t("ssh.addHost")}
            </Button>
          </div>
        </div>
      </form>
    </ModalShell>
  );
}

function Field({
  label,
  hint,
  className,
  children,
}: {
  label: string;
  hint?: string;
  className?: string;
  children: React.ReactNode;
}) {
  return (
    <label className={`flex flex-col gap-1.5 ${className ?? ""}`}>
      <span className="text-xs font-medium text-muted-foreground">
        {label}
        {hint ? <span className="ml-1 text-[10px] opacity-60">({hint})</span> : null}
      </span>
      {children}
    </label>
  );
}
