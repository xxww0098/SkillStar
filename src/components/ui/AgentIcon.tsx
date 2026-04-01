import type { AgentProfile } from "../../types";
import { AntigravityIcon } from "./icons/AntigravityIcon";

interface AgentIconProps {
  profile: AgentProfile;
  className?: string;
  alt?: string;
}

export function AgentIcon({ profile, className, alt }: AgentIconProps) {
  if (profile.id === "antigravity") {
    // Inject the inline SVG for antigravity to bypass macOS WebKit <img src="svg"> missing-filter bugs
    return <AntigravityIcon className={className} />;
  }
  const imgSrc = profile.icon.startsWith("data:image") ? profile.icon : `/${profile.icon}`;
  return <img src={imgSrc} alt={alt ?? profile.display_name} className={className} loading="lazy" decoding="async" />;
}
