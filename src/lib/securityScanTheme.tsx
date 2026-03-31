import { File, FileCode, FileJson, FileText } from "lucide-react";

import type { RiskLevel } from "../types";

type IconType = typeof File;

export interface FileTheme {
  icon: IconType;
  tint: string;
  tintText: string;
  tintSoft: string;
  chip: string;
}

export interface RiskTone {
  dot: string;
  text: string;
  glow: string;
  pill: string;
}

export function getFileTheme(fileName: string | null): FileTheme {
  const ext = fileName?.split(".").pop()?.toLowerCase();

  switch (ext) {
    case "ts":
    case "tsx":
    case "js":
    case "jsx":
      return {
        icon: FileCode,
        tint: "rgba(34,197,94,0.32)",
        tintText: "text-emerald-700 dark:text-emerald-300",
        tintSoft: "text-emerald-700/75 dark:text-emerald-200/70",
        chip: "bg-emerald-500/12 border-emerald-400/25",
      };
    case "md":
    case "markdown":
      return {
        icon: FileText,
        tint: "rgba(var(--color-primary-rgb),0.3)",
        tintText: "text-accent-foreground",
        tintSoft: "text-accent-foreground/80",
        chip: "bg-accent/45 border-primary/25",
      };
    case "json":
    case "yaml":
    case "yml":
    case "toml":
      return {
        icon: FileJson,
        tint: "rgba(var(--color-primary-rgb),0.3)",
        tintText: "text-accent-foreground",
        tintSoft: "text-accent-foreground/80",
        chip: "bg-accent/45 border-primary/25",
      };
    default:
      return {
        icon: File,
        tint: "rgba(var(--color-success-rgb),0.24)",
        tintText: "text-success",
        tintSoft: "text-success/70",
        chip: "bg-success/10 border-success/20",
      };
  }
}

export function getRiskTone(riskLevel?: RiskLevel): RiskTone {
  switch (riskLevel) {
    case "Critical":
      return {
        dot: "bg-rose-500",
        text: "text-rose-300",
        glow: "shadow-[0_0_12px_rgba(244,63,94,0.45)]",
        pill: "bg-rose-500/12 border-rose-400/25",
      };
    case "High":
      return {
        dot: "bg-orange-500",
        text: "text-orange-300",
        glow: "shadow-[0_0_12px_rgba(249,115,22,0.45)]",
        pill: "bg-orange-500/12 border-orange-400/25",
      };
    case "Medium":
      return {
        dot: "bg-amber-400",
        text: "text-amber-200",
        glow: "shadow-[0_0_12px_rgba(251,191,36,0.42)]",
        pill: "bg-amber-500/12 border-amber-300/25",
      };
    case "Low":
      return {
        dot: "bg-lime-400",
        text: "text-lime-200",
        glow: "shadow-[0_0_10px_rgba(163,230,53,0.35)]",
        pill: "bg-lime-500/10 border-lime-300/20",
      };
    default:
      return {
        dot: "bg-success",
        text: "text-success/80",
        glow: "shadow-[0_0_10px_rgba(var(--color-success-rgb),0.35)]",
        pill: "bg-success/10 border-success/20",
      };
  }
}
