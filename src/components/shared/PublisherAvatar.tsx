import { Building2 } from "lucide-react";
import { useState } from "react";
import { cn } from "../../lib/utils";

const AVATAR_CACHE_KEY = "publisher-avatar-source-v1";
const AVATAR_CACHE_MAX_AGE_MS = 1000 * 60 * 60 * 24 * 30;

type AvatarSource = "local" | "remote" | "none";

interface AvatarCacheEntry {
  source: AvatarSource;
  at: number;
}

function readAvatarSource(name: string): AvatarSource {
  try {
    const raw = localStorage.getItem(AVATAR_CACHE_KEY);
    if (!raw) return "local";
    const parsed = JSON.parse(raw) as Record<string, AvatarCacheEntry>;
    const entry = parsed[name];
    if (!entry || Date.now() - entry.at > AVATAR_CACHE_MAX_AGE_MS) return "local";
    return entry.source;
  } catch {
    return "local";
  }
}

function writeAvatarSource(name: string, source: AvatarSource) {
  try {
    const raw = localStorage.getItem(AVATAR_CACHE_KEY);
    const parsed: Record<string, AvatarCacheEntry> = raw ? JSON.parse(raw) : {};
    parsed[name] = { source, at: Date.now() };
    localStorage.setItem(AVATAR_CACHE_KEY, JSON.stringify(parsed));
  } catch {
    // Storage is only an optimisation; rendering must still work without it.
  }
}

function remoteAvatarUrl(name: string): string {
  return `https://avatars.githubusercontent.com/${encodeURIComponent(name)}?size=120`;
}

export interface PublisherAvatarProps {
  name: string;
  size?: "sm" | "md" | "lg";
}

/**
 * Publisher identity image with a local asset → GitHub avatar → icon fallback.
 * The fallback choice is cached, while callers only provide identity and size.
 */
export function PublisherAvatar({ name, size = "md" }: PublisherAvatarProps) {
  const [avatarSource, setAvatarSource] = useState<AvatarSource>(() => readAvatarSource(name));

  const showFallbackIcon = avatarSource === "none";
  const avatarSrc = avatarSource === "remote" ? remoteAvatarUrl(name) : `/publishers/${name}.png`;

  const sizeClasses = {
    sm: "w-8 h-8 rounded-lg",
    md: "w-10 h-10 rounded-xl",
    lg: "w-14 h-14 rounded-2xl",
  };

  const iconSizes = {
    sm: "w-4 h-4",
    md: "w-5 h-5",
    lg: "w-7 h-7",
  };

  return (
    <div
      className={cn(
        sizeClasses[size],
        "bg-gradient-to-br from-primary/15 to-primary/5 border border-primary/10 flex items-center justify-center shrink-0 overflow-hidden",
      )}
    >
      {!showFallbackIcon ? (
        <img
          src={avatarSrc}
          alt={name}
          className="w-full h-full object-cover"
          loading="lazy"
          decoding="async"
          onLoad={() => writeAvatarSource(name, avatarSource)}
          onError={() => {
            if (avatarSource === "local") {
              setAvatarSource("remote");
            } else {
              setAvatarSource("none");
              writeAvatarSource(name, "none");
            }
          }}
        />
      ) : (
        <Building2 aria-label={`${name} publisher`} className={cn(iconSizes[size], "text-primary/70")} />
      )}
    </div>
  );
}
