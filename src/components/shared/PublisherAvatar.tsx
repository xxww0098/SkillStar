import { Building2 } from "lucide-react";
import { useState } from "react";
import { cn } from "../../lib/utils";

const AVATAR_CACHE_KEY = "publisher-avatar-source-v2";
const AVATAR_CACHE_MAX_AGE_MS = 1000 * 60 * 60 * 24 * 30;

type AvatarSource = "local" | "remote" | "none";

interface AvatarCacheEntry {
  source: AvatarSource;
  at: number;
}

let cachedRaw: string | null = null;
let memoryCache: Record<string, AvatarCacheEntry> | null = null;

function loadCache(): Record<string, AvatarCacheEntry> {
  try {
    const raw = localStorage.getItem(AVATAR_CACHE_KEY);
    if (raw === cachedRaw && memoryCache) return memoryCache;
    cachedRaw = raw;
    memoryCache = raw ? (JSON.parse(raw) as Record<string, AvatarCacheEntry>) : {};
    return memoryCache;
  } catch {
    cachedRaw = null;
    memoryCache = {};
    return memoryCache;
  }
}

function readAvatarSource(name: string): AvatarSource {
  const entry = loadCache()[name];
  if (!entry || Date.now() - entry.at > AVATAR_CACHE_MAX_AGE_MS) return "local";
  return entry.source;
}

function writeAvatarSource(name: string, source: AvatarSource) {
  try {
    const parsed = loadCache();
    parsed[name] = { source, at: Date.now() };
    const serialized = JSON.stringify(parsed);
    localStorage.setItem(AVATAR_CACHE_KEY, serialized);
    cachedRaw = serialized;
    memoryCache = parsed;
  } catch {
    // Storage is only an optimisation; rendering must still work without it.
  }
}

/**
 * `avatars.githubusercontent.com/<login>` answers 200 with a grey Octocat
 * placeholder for logins it cannot resolve, so an unknown owner silently renders
 * as "some GitHub thing" instead of falling through. `github.com/<login>.png`
 * redirects to the real avatar and 404s when the owner does not exist.
 */
export function remoteAvatarUrl(name: string): string {
  return `https://github.com/${encodeURIComponent(name)}.png?size=120`;
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
