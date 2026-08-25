import type { LucideIcon } from "lucide-react";
import {
  BookOpen,
  Code2,
  FileCode2,
  GitBranch,
  GitPullRequest,
  Languages,
  MonitorPlay,
  ShieldCheck,
  Sparkles,
  Terminal,
  Video,
} from "lucide-react";

export interface SkillVisualIdentity {
  icon: LucideIcon;
  gradient: string;
  badgeBg: string;
  badgeBorder: string;
  badgeText: string;
  accentColor: string;
  initials: string;
}

/**
 * Derives a consistent visual identity (icon, gradient, color theme) for a skill
 * based on its name, description, tags, and source.
 */
export function getSkillVisualIdentity(
  name: string,
  category?: string,
  topics?: string[],
  source?: string | null,
): SkillVisualIdentity {
  const lowerName = name.toLowerCase();
  const lowerDesc = (category || "").toLowerCase() + (topics || []).join(" ").toLowerCase();
  const lowerSource = (source || "").toLowerCase();

  // Extract initials (up to 2 chars)
  const cleanParts = name
    .replace(/^dsh-/, "")
    .split(/[-_.]+/)
    .filter(Boolean);
  let initials = "SK";
  if (cleanParts.length >= 2) {
    initials = (cleanParts[0][0] + cleanParts[1][0]).toUpperCase();
  } else if (cleanParts.length === 1 && cleanParts[0].length >= 2) {
    initials = cleanParts[0].slice(0, 2).toUpperCase();
  }

  // 1. Rust / Systems / Low level
  if (lowerName.includes("rust") || lowerDesc.includes("rust") || lowerName.includes("cargo")) {
    return {
      icon: Code2,
      gradient: "from-orange-500/20 via-amber-500/10 to-red-500/5",
      badgeBg: "bg-orange-500/10 paper:bg-orange-50",
      badgeBorder: "border-orange-500/30 paper:border-orange-200",
      badgeText: "text-orange-400 paper:text-orange-700",
      accentColor: "#f97316",
      initials: "RS",
    };
  }

  // 2. Git / PR / Merge / Stack
  if (
    lowerName.includes("git") ||
    lowerName.includes("pr") ||
    lowerName.includes("merge") ||
    lowerName.includes("stack") ||
    lowerDesc.includes("pull request")
  ) {
    return {
      icon: lowerName.includes("pr") || lowerName.includes("merge") ? GitPullRequest : GitBranch,
      gradient: "from-purple-500/20 via-pink-500/10 to-indigo-500/5",
      badgeBg: "bg-purple-500/10 paper:bg-purple-50",
      badgeBorder: "border-purple-500/30 paper:border-purple-200",
      badgeText: "text-purple-400 paper:text-purple-700",
      accentColor: "#a855f7",
      initials: initials || "PR",
    };
  }

  // 3. Browser / Video / GIF / UI demo
  if (
    lowerName.includes("browser") ||
    lowerName.includes("gif") ||
    lowerName.includes("video") ||
    lowerName.includes("record") ||
    lowerName.includes("screen")
  ) {
    return {
      icon: lowerName.includes("gif") || lowerName.includes("record") ? MonitorPlay : Video,
      gradient: "from-cyan-500/20 via-blue-500/10 to-teal-500/5",
      badgeBg: "bg-cyan-500/10 paper:bg-cyan-50",
      badgeBorder: "border-cyan-500/30 paper:border-cyan-200",
      badgeText: "text-cyan-400 paper:text-cyan-700",
      accentColor: "#06b6d4",
      initials: initials || "UI",
    };
  }

  // 4. Docs / Prose / Translate / Notes / Archive
  if (
    lowerName.includes("doc") ||
    lowerName.includes("prose") ||
    lowerName.includes("translate") ||
    lowerName.includes("note") ||
    lowerName.includes("archive")
  ) {
    return {
      icon: lowerName.includes("translate") ? Languages : BookOpen,
      gradient: "from-emerald-500/20 via-teal-500/10 to-cyan-500/5",
      badgeBg: "bg-emerald-500/10 paper:bg-emerald-50",
      badgeBorder: "border-emerald-500/30 paper:border-emerald-200",
      badgeText: "text-emerald-400 paper:text-emerald-700",
      accentColor: "#10b981",
      initials: initials || "DC",
    };
  }

  // 5. AI / DeepSeek / Reasoning / CoT / Prompts
  if (
    lowerName.includes("cot") ||
    lowerName.includes("deepseek") ||
    lowerSource.includes("deepseek") ||
    lowerName.includes("ai") ||
    lowerName.includes("reason")
  ) {
    return {
      icon: Sparkles,
      gradient: "from-blue-500/20 via-indigo-500/10 to-violet-500/5",
      badgeBg: "bg-blue-500/10 paper:bg-blue-50",
      badgeBorder: "border-blue-500/30 paper:border-blue-200",
      badgeText: "text-blue-400 paper:text-blue-700",
      accentColor: "#3b82f6",
      initials: initials || "AI",
    };
  }

  // 6. Test / Review / Check / Simplification
  if (
    lowerName.includes("test") ||
    lowerName.includes("check") ||
    lowerName.includes("review") ||
    lowerName.includes("lint") ||
    lowerName.includes("simplif")
  ) {
    return {
      icon: ShieldCheck,
      gradient: "from-amber-500/20 via-yellow-500/10 to-orange-500/5",
      badgeBg: "bg-amber-500/10 paper:bg-amber-50",
      badgeBorder: "border-amber-500/30 paper:border-amber-200",
      badgeText: "text-amber-400 paper:text-amber-700",
      accentColor: "#f59e0b",
      initials: initials || "CK",
    };
  }

  // 7. Code / Refactor / Syntax / AST
  if (lowerName.includes("code") || lowerName.includes("script") || lowerName.includes("tool")) {
    return {
      icon: FileCode2,
      gradient: "from-sky-500/20 via-blue-500/10 to-indigo-500/5",
      badgeBg: "bg-sky-500/10 paper:bg-sky-50",
      badgeBorder: "border-sky-500/30 paper:border-sky-200",
      badgeText: "text-sky-400 paper:text-sky-700",
      accentColor: "#0284c7",
      initials: initials || "CD",
    };
  }

  // Default / Generic Skill
  return {
    icon: Terminal,
    gradient: "from-primary/20 via-primary/10 to-transparent",
    badgeBg: "bg-primary/10 paper:bg-blue-50",
    badgeBorder: "border-primary/30 paper:border-blue-200",
    badgeText: "text-primary-hover paper:text-primary",
    accentColor: "#3b82f6",
    initials,
  };
}
