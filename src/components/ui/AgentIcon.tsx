import type { AgentProfile } from "../../types";
import { cn } from "../../lib/utils";
import { getAgentIcon } from "./icons/agentIcons";
import { LobeIcon } from "./icons/LobeIcon";

/** Minimal shape required to render an agent icon. */
type AgentIconProfile = Pick<AgentProfile, "id" | "icon" | "display_name">;

interface AgentIconProps {
  profile: AgentIconProfile;
  className?: string;
  alt?: string;
}

/**
 * Render an agent icon.
 */
export function AgentIcon({ profile, className, alt }: AgentIconProps) {
  if (!profile.icon.startsWith("data:image")) {
    const BrandIcon = getAgentIcon(profile.id);
    return <LobeIcon icon={BrandIcon} size="100%" className={cn("pointer-events-none", className)} />;
  }

  return (
    <img src={profile.icon} alt={alt ?? profile.display_name} className={className} loading="lazy" decoding="async" />
  );
}
