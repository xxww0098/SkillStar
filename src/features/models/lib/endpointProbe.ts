import type { EndpointLatencyResult } from "../../../types";

export function isAuthProbeStatus(status: number | null | undefined): boolean {
  return status === 401 || status === 403;
}

/** Endpoint responded (incl. auth errors); excludes timeouts and HTTP 404/5xx. */
export function isEndpointReachable(result: EndpointLatencyResult): boolean {
  if (result.latency_ms == null) return false;
  if (result.status == null) return true;
  return result.status < 400 || isAuthProbeStatus(result.status);
}

export function endpointProbeLabel(result: EndpointLatencyResult): string {
  if (result.error && result.latency_ms == null) {
    return result.error;
  }
  if (result.latency_ms != null) {
    if (isAuthProbeStatus(result.status)) {
      return `${result.latency_ms}ms · 鉴权失败`;
    }
    if (result.error) {
      return `${result.latency_ms}ms · ${result.error}`;
    }
    const status = result.status != null ? ` · HTTP ${result.status}` : "";
    return `${result.latency_ms}ms${status}`;
  }
  return "失败";
}

export function endpointProbeTone(result: EndpointLatencyResult): "ok" | "auth" | "error" {
  if (!isEndpointReachable(result)) return "error";
  if (isAuthProbeStatus(result.status)) return "auth";
  return "ok";
}
