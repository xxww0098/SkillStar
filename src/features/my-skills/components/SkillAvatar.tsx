import { memo, useMemo, useState } from "react";
import { remoteAvatarUrl } from "../../../components/shared/PublisherAvatar";
import { cn } from "../../../lib/utils";
import type { Skill } from "../../../types";
import { getSkillVisualIdentity } from "../lib/skillAvatarHelper";

export interface SkillAvatarProps {
  skill: Skill;
  size?: "sm" | "md" | "lg";
  className?: string;
}

/**
 * Extracts GitHub owner/organization from a Skill object.
 */
export function extractSkillOwner(skill: Skill): string | null {
  if (skill.skill_type === "local") return null;

  if (skill.source && skill.source !== "remote" && skill.source !== "local") {
    const parts = skill.source.split("/");
    if (parts[0]) return parts[0].trim();
  }

  if (skill.author && skill.author.trim()) {
    return skill.author.trim().replace(/^@/, "");
  }

  if (skill.git_url) {
    try {
      const url = new URL(skill.git_url);
      if (url.hostname.includes("github.com")) {
        const pathParts = url.pathname.replace(/^\//, "").split("/");
        if (pathParts[0]) return pathParts[0].trim();
      }
    } catch {
      // Ignore git_url parse errors
    }
  }

  return null;
}

export const SkillAvatar = memo(function SkillAvatar({ skill, size = "md", className }: SkillAvatarProps) {
  const visual = getSkillVisualIdentity(skill.name, skill.category, skill.topics, skill.source);
  const SkillIcon = visual.icon;
  const owner = useMemo(() => extractSkillOwner(skill), [skill]);

  // Which candidate we are on, kept separate from whether it loaded: deriving the
  // src from a "loaded" state made a successful local hit immediately re-point at
  // the remote URL and overwrite itself.
  const [stage, setStage] = useState<"local" | "remote" | "none">(owner ? "local" : "none");
  const [loaded, setLoaded] = useState(false);

  const sizeClasses = {
    sm: "w-7 h-7 rounded-lg text-xs",
    md: "w-8 h-8 rounded-xl text-sm",
    lg: "w-10 h-10 rounded-2xl text-base",
  };

  const iconSizes = {
    sm: "w-3.5 h-3.5",
    md: "w-4 h-4",
    lg: "w-5 h-5",
  };

  const currentSrc = useMemo(() => {
    if (!owner || stage === "none") return null;
    return stage === "local" ? `/publishers/${owner}.png` : remoteAvatarUrl(owner);
  }, [owner, stage]);

  return (
    <div
      className={cn(
        "relative shrink-0 flex items-center justify-center border shadow-xs overflow-hidden transition-transform duration-200 group-hover:scale-105",
        sizeClasses[size],
        visual.badgeBorder,
        loaded ? "bg-muted/40" : cn("bg-gradient-to-br", visual.gradient, visual.badgeText),
        className,
      )}
    >
      {/* Dynamic Fallback / Placeholder Icon */}
      {!loaded && <SkillIcon className={cn(iconSizes[size], "transition-transform")} />}

      {/* GitHub Repo Owner / Org Avatar Image */}
      {currentSrc && (
        <img
          src={currentSrc}
          alt={owner || skill.name}
          loading="lazy"
          decoding="async"
          onLoad={() => setLoaded(true)}
          onError={() => setStage(stage === "local" ? "remote" : "none")}
          className={cn(
            "absolute inset-0 w-full h-full object-cover transition-opacity duration-200",
            loaded ? "opacity-100" : "opacity-0 pointer-events-none",
          )}
        />
      )}
    </div>
  );
});
