export type LatencyColor = "green" | "yellow" | "red" | "gray";

export function getLatencyColor(latencyMs: number | null | undefined): LatencyColor {
  if (latencyMs == null) return "gray"; // untested
  if (latencyMs < 500) return "green";
  if (latencyMs <= 2000) return "yellow";
  return "red"; // > 2000ms or error
}
