import type { TlsProfile } from "./types";

/** Short human label for a TLS profile, e.g. "Chrome 147". */
export function tlsLabel(tls: TlsProfile): string {
  switch (tls.kind) {
    case "default":
      return "默认 (rustls)";
    case "chrome":
      return `Chrome ${tls.major ?? "?"}`;
    case "safari":
      return `Safari ${tls.major ?? "?"}`;
    case "edge":
      return `Edge ${tls.major ?? "?"}`;
    case "firefox":
      return `Firefox ${tls.major ?? "?"}`;
    case "opera":
      return `Opera ${tls.major ?? "?"}`;
    case "ok_http":
      return `OkHttp ${tls.major ?? "?"}`;
    default:
      return tls.kind;
  }
}
