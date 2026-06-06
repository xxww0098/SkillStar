import type { TFunction } from "i18next";

const NETWORK_ERROR_NEEDLES = [
  "error sending request",
  "operation timed out",
  "timed out",
  "connection refused",
  "connection reset",
  "dns",
  "failed to lookup address",
  "tcp connect error",
  "network is unreachable",
];

export function formatUsageErrorForDisplay(error: string | null | undefined, t: TFunction): string | null {
  const message = error?.trim();
  if (!message) return null;

  if (looksLikeNetworkTransportError(message)) {
    return t("usage.refreshNetworkTransportError");
  }

  return truncateUsageError(message);
}

export function truncateUsageError(message: string): string {
  return message.length > 180 ? `${message.slice(0, 177)}...` : message;
}

function looksLikeNetworkTransportError(message: string): boolean {
  const lower = message.toLowerCase();
  return NETWORK_ERROR_NEEDLES.some((needle) => lower.includes(needle));
}
