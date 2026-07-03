import type { TFunction } from "i18next";
import type { TlsProfile } from "./types";

/** Short human label for a TLS profile, e.g. "Chrome 147". */
export function tlsLabel(tls: TlsProfile, t: TFunction): string {
  const major = tls.major ?? "?";
  switch (tls.kind) {
    case "default":
      return t("fingerprints.tls.default");
    case "chrome":
      return t("fingerprints.tls.chrome", { major });
    case "safari":
      return t("fingerprints.tls.safari", { major });
    case "edge":
      return t("fingerprints.tls.edge", { major });
    case "firefox":
      return t("fingerprints.tls.firefox", { major });
    case "opera":
      return t("fingerprints.tls.opera", { major });
    case "ok_http":
      return t("fingerprints.tls.okHttp", { major });
    default:
      return tls.kind;
  }
}
