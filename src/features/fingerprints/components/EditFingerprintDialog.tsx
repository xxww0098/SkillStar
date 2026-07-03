import { AnimatePresence, motion } from "framer-motion";
import { Edit3, Globe2, Loader2, Network, X } from "lucide-react";
import { useEffect, useMemo, useState } from "react";
import { useTranslation } from "react-i18next";
import { toast } from "sonner";
import { Button } from "@/components/ui/button";
import { Input } from "@/components/ui/input";
import { cn } from "@/lib/utils";
import type {
  FingerprintRow,
  HttpProfile,
  NetworkProfile,
  TlsProfile,
  TlsProfileKind,
  UpdateFingerprintInput,
} from "../types";

interface EditFingerprintDialogProps {
  open: boolean;
  row: FingerprintRow | null;
  onClose: () => void;
  onSubmit: (input: UpdateFingerprintInput) => Promise<void>;
}

/** All TLS kinds the picker exposes. `default` keeps reqwest's stock rustls.
 *  Labels are plain (non-translated) brand names — only "default" needs a
 *  translated label, resolved inline where `TLS_KINDS` is consumed. */
const TLS_KINDS: { value: TlsProfileKind; label: string; defaultMajor: number | null }[] = [
  { value: "default", label: "default", defaultMajor: null },
  { value: "chrome", label: "Chrome", defaultMajor: 147 },
  { value: "safari", label: "Safari", defaultMajor: 26 },
  { value: "edge", label: "Edge", defaultMajor: 147 },
  { value: "firefox", label: "Firefox", defaultMajor: 136 },
  { value: "opera", label: "Opera", defaultMajor: 130 },
  { value: "ok_http", label: "OkHttp", defaultMajor: 13 },
];

/**
 * Multi-section editor for a stored fingerprint. Surfaces only the fields
 * a user actually needs to tweak; deep internals (HTTP/2 SETTINGS, extra
 * headers map) are intentionally read-only or left for a future "advanced"
 * pane to keep the UI legible.
 */
export function EditFingerprintDialog({ open, row, onClose, onSubmit }: EditFingerprintDialogProps) {
  const { t } = useTranslation();
  // -- form state ---------------------------------------------------
  const [name, setName] = useState("");
  const [tlsKind, setTlsKind] = useState<TlsProfileKind>("default");
  const [tlsMajor, setTlsMajor] = useState<string>("");

  const [userAgent, setUserAgent] = useState("");
  const [acceptLanguage, setAcceptLanguage] = useState("");
  const [acceptEncoding, setAcceptEncoding] = useState("");
  const [secChUa, setSecChUa] = useState("");
  const [secChUaPlatform, setSecChUaPlatform] = useState("");
  const [secChUaMobile, setSecChUaMobile] = useState(false);

  const [proxyUrl, setProxyUrl] = useState("");
  const [dohUrl, setDohUrl] = useState("");
  const [egressCountry, setEgressCountry] = useState("");

  const [busy, setBusy] = useState(false);

  // -- hydrate from `row` whenever the dialog opens with a new row --
  useEffect(() => {
    if (!open || !row) return;
    setName(row.name);
    setTlsKind(row.tls.kind);
    setTlsMajor(row.tls.major != null ? String(row.tls.major) : "");
    setUserAgent(row.http.user_agent);
    setAcceptLanguage(row.http.accept_language);
    setAcceptEncoding(row.http.accept_encoding);
    setSecChUa(row.http.sec_ch_ua ?? "");
    setSecChUaPlatform(row.http.sec_ch_ua_platform ?? "");
    setSecChUaMobile(row.http.sec_ch_ua_mobile);
    setProxyUrl(row.network.proxy_url ?? "");
    setDohUrl(row.network.doh_url ?? "");
    setEgressCountry(row.network.egress_country ?? "");
    setBusy(false);
  }, [open, row]);

  // When the user switches TLS kind, suggest a sensible major version.
  useEffect(() => {
    const def = TLS_KINDS.find((k) => k.value === tlsKind);
    if (!def) return;
    if (def.defaultMajor == null) {
      setTlsMajor("");
    } else if (!tlsMajor || Number.isNaN(Number(tlsMajor))) {
      setTlsMajor(String(def.defaultMajor));
    }
    // We intentionally don't depend on tlsMajor — only fire on kind change.
    // eslint-disable-next-line react-hooks/exhaustive-deps
  }, [tlsKind]);

  const tlsDisabled = tlsKind === "default";

  // Currently-edited TLS profile snapshot (used by submit).
  const editedTls: TlsProfile = useMemo(() => {
    if (tlsKind === "default") return { kind: "default" };
    const major = Number.parseInt(tlsMajor, 10);
    return Number.isFinite(major) && major > 0 ? { kind: tlsKind, major } : { kind: tlsKind };
  }, [tlsKind, tlsMajor]);

  if (!open || !row) return null;

  const submit = async () => {
    if (!name.trim()) {
      toast.error(t("fingerprints.editDialog.toastNameRequired"));
      return;
    }
    setBusy(true);

    // Build a fresh HttpProfile mirroring the row's existing extra_headers
    // (the editor doesn't touch them today).
    const http: HttpProfile = {
      user_agent: userAgent.trim() || row.http.user_agent,
      accept_language: acceptLanguage.trim() || row.http.accept_language,
      accept_encoding: acceptEncoding.trim() || row.http.accept_encoding,
      sec_ch_ua: secChUa.trim() ? secChUa.trim() : undefined,
      sec_ch_ua_platform: secChUaPlatform.trim() ? secChUaPlatform.trim() : undefined,
      sec_ch_ua_mobile: secChUaMobile,
      extra_headers: row.http.extra_headers,
    };

    const network: NetworkProfile = {
      proxy_url: proxyUrl.trim() ? proxyUrl.trim() : undefined,
      doh_url: dohUrl.trim() ? dohUrl.trim() : undefined,
      egress_country: egressCountry.trim() ? egressCountry.trim().toUpperCase() : undefined,
    };

    const input: UpdateFingerprintInput = {
      name: name.trim(),
      tls: editedTls,
      http,
      network,
    };

    try {
      await onSubmit(input);
      toast.success(t("fingerprints.editDialog.toastSaved"), { description: name.trim() });
      onClose();
    } catch (e) {
      toast.error(t("fingerprints.editDialog.toastSaveFailed"), {
        description: e instanceof Error ? e.message : String(e),
      });
    } finally {
      setBusy(false);
    }
  };

  return (
    <AnimatePresence>
      {open && (
        <motion.div
          className="fixed inset-0 z-50 flex items-center justify-center bg-black/40 backdrop-blur-sm px-4"
          initial={{ opacity: 0 }}
          animate={{ opacity: 1 }}
          exit={{ opacity: 0 }}
          onClick={onClose}
        >
          <motion.div
            className="relative w-full max-w-2xl rounded-3xl border border-zinc-200/60 bg-white p-6 shadow-2xl"
            initial={{ scale: 0.94, opacity: 0 }}
            animate={{ scale: 1, opacity: 1 }}
            exit={{ scale: 0.96, opacity: 0 }}
            transition={{ duration: 0.18 }}
            onClick={(e) => e.stopPropagation()}
          >
            <button
              type="button"
              className="absolute right-4 top-4 inline-flex h-8 w-8 items-center justify-center rounded-md text-muted-foreground hover:bg-zinc-100 hover:text-foreground"
              onClick={onClose}
              aria-label="close"
            >
              <X className="h-4 w-4" />
            </button>

            <div className="mb-5 flex items-center gap-2.5">
              <div className="inline-flex h-9 w-9 items-center justify-center rounded-xl bg-violet-50 text-violet-600">
                <Edit3 className="h-4 w-4" />
              </div>
              <div>
                <h2 className="text-base font-semibold">{t("fingerprints.editDialog.title")}</h2>
                <p className="text-xs text-muted-foreground">{t("fingerprints.editDialog.subtitle")}</p>
              </div>
            </div>

            <div className="max-h-[60vh] space-y-5 overflow-y-auto pr-1">
              {/* ── basic ────────────────────────────────────────────── */}
              <Section title={t("fingerprints.editDialog.sectionBasic")}>
                <Field label={t("fingerprints.editDialog.nameLabel")}>
                  <Input
                    value={name}
                    onChange={(e) => setName(e.target.value)}
                    placeholder={t("fingerprints.editDialog.namePlaceholder")}
                    disabled={busy}
                  />
                </Field>
              </Section>

              {/* ── TLS ──────────────────────────────────────────── */}
              <Section title={t("fingerprints.editDialog.sectionTls")} icon={<Globe2 className="h-3.5 w-3.5" />}>
                <div className="grid grid-cols-3 gap-2">
                  <Field label={t("fingerprints.editDialog.familyLabel")} className="col-span-2">
                    <div className="flex flex-wrap gap-1.5">
                      {TLS_KINDS.map((k) => (
                        <button
                          key={k.value}
                          type="button"
                          onClick={() => setTlsKind(k.value)}
                          disabled={busy}
                          className={cn(
                            "rounded-full border px-2.5 py-1 text-[11px] transition-all",
                            tlsKind === k.value
                              ? "border-violet-400 bg-violet-50 text-violet-800 ring-2 ring-violet-200"
                              : "border-zinc-200 bg-white text-zinc-700 hover:border-zinc-300",
                          )}
                        >
                          {k.value === "default" ? t("fingerprints.tls.default") : k.label}
                        </button>
                      ))}
                    </div>
                  </Field>
                  <Field label={t("fingerprints.editDialog.majorVersionLabel")}>
                    <Input
                      value={tlsMajor}
                      onChange={(e) => setTlsMajor(e.target.value.replace(/\D/g, ""))}
                      placeholder={
                        tlsDisabled
                          ? t("fingerprints.editDialog.majorVersionPlaceholderDisabled")
                          : t("fingerprints.editDialog.majorVersionPlaceholder")
                      }
                      disabled={busy || tlsDisabled}
                      inputMode="numeric"
                    />
                  </Field>
                </div>
                <p className="mt-2 text-[11px] text-muted-foreground">{t("fingerprints.editDialog.tlsHint")}</p>
              </Section>

              {/* ── HTTP headers ────────────────────────────────────── */}
              <Section title={t("fingerprints.editDialog.sectionHttpHeaders")}>
                <Field label={t("fingerprints.editDialog.userAgentLabel")}>
                  <textarea
                    value={userAgent}
                    onChange={(e) => setUserAgent(e.target.value)}
                    disabled={busy}
                    rows={2}
                    className="w-full resize-none rounded-lg border border-zinc-200 bg-white px-2.5 py-1.5 font-mono text-[11px] leading-snug focus:border-violet-300 focus:outline-none focus:ring-2 focus:ring-violet-100"
                  />
                </Field>
                <div className="grid grid-cols-2 gap-2">
                  <Field label={t("fingerprints.editDialog.acceptLanguageLabel")}>
                    <Input
                      value={acceptLanguage}
                      onChange={(e) => setAcceptLanguage(e.target.value)}
                      placeholder="zh-CN,zh;q=0.9,en;q=0.8"
                      disabled={busy}
                    />
                  </Field>
                  <Field label={t("fingerprints.editDialog.acceptEncodingLabel")}>
                    <Input
                      value={acceptEncoding}
                      onChange={(e) => setAcceptEncoding(e.target.value)}
                      placeholder="gzip, deflate, br, zstd"
                      disabled={busy}
                    />
                  </Field>
                </div>
                <div className="grid grid-cols-2 gap-2">
                  <Field label={t("fingerprints.editDialog.secChUaLabel")}>
                    <Input
                      value={secChUa}
                      onChange={(e) => setSecChUa(e.target.value)}
                      placeholder={'"Chromium";v="147", ...'}
                      disabled={busy}
                    />
                  </Field>
                  <Field label={t("fingerprints.editDialog.secChUaPlatformLabel")}>
                    <Input
                      value={secChUaPlatform}
                      onChange={(e) => setSecChUaPlatform(e.target.value)}
                      placeholder={'"macOS" / "Windows"'}
                      disabled={busy}
                    />
                  </Field>
                </div>
                <label className="mt-1 inline-flex items-center gap-1.5 text-[11px] text-zinc-700">
                  <input
                    type="checkbox"
                    checked={secChUaMobile}
                    onChange={(e) => setSecChUaMobile(e.target.checked)}
                    disabled={busy}
                  />
                  {t("fingerprints.editDialog.secChUaMobileLabel")}
                </label>
              </Section>

              {/* ── network ───────────────────────────────────────── */}
              <Section title={t("fingerprints.editDialog.sectionNetwork")} icon={<Network className="h-3.5 w-3.5" />}>
                <Field label={t("fingerprints.editDialog.proxyLabel")}>
                  <Input
                    value={proxyUrl}
                    onChange={(e) => setProxyUrl(e.target.value)}
                    placeholder="socks5h://127.0.0.1:1080 or http://user:pass@host:port"
                    disabled={busy}
                  />
                </Field>
                <div className="grid grid-cols-2 gap-2">
                  <Field label={t("fingerprints.editDialog.dohLabel")}>
                    <Input
                      value={dohUrl}
                      onChange={(e) => setDohUrl(e.target.value)}
                      placeholder="https://1.1.1.1/dns-query"
                      disabled={busy}
                    />
                  </Field>
                  <Field label={t("fingerprints.editDialog.egressCountryLabel")}>
                    <Input
                      value={egressCountry}
                      onChange={(e) => setEgressCountry(e.target.value.toUpperCase())}
                      placeholder="US / CN / JP"
                      maxLength={2}
                      disabled={busy}
                    />
                  </Field>
                </div>
                <p className="mt-2 text-[11px] text-muted-foreground">{t("fingerprints.editDialog.networkHint")}</p>
              </Section>
            </div>

            <div className="mt-5 flex justify-end gap-2">
              <Button variant="outline" onClick={onClose} disabled={busy}>
                {t("fingerprints.editDialog.cancel")}
              </Button>
              <Button onClick={submit} disabled={busy}>
                {busy ? (
                  <>
                    <Loader2 className="mr-2 h-3.5 w-3.5 animate-spin" />
                    {t("fingerprints.editDialog.saving")}
                  </>
                ) : (
                  t("fingerprints.editDialog.save")
                )}
              </Button>
            </div>
          </motion.div>
        </motion.div>
      )}
    </AnimatePresence>
  );
}

// ── small layout helpers ───────────────────────────────────────────

function Section({ title, icon, children }: { title: string; icon?: React.ReactNode; children: React.ReactNode }) {
  return (
    <div className="rounded-2xl border border-zinc-200/60 bg-zinc-50/40 p-3">
      <div className="mb-2 flex items-center gap-1.5 text-[11px] font-semibold uppercase tracking-wide text-zinc-600">
        {icon}
        {title}
      </div>
      <div className="space-y-2">{children}</div>
    </div>
  );
}

function Field({ label, className, children }: { label: string; className?: string; children: React.ReactNode }) {
  return (
    <div className={cn("space-y-1", className)}>
      <label className="block text-[11px] font-medium text-zinc-700">{label}</label>
      {children}
    </div>
  );
}
