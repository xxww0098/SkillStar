import { useState } from "react";
import { motion } from "framer-motion";
import { Building2, Folder, Package, ChevronRight } from "lucide-react";
import { Card } from "../ui/card";
import { Badge } from "../ui/badge";
import { Button } from "../ui/button";
import { cn } from "../../lib/utils";
import type { OfficialPublisher } from "../../types";

// ── Avatar caching ─────────────────────────────────────────────────────

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
    if (!entry) return "local";
    if (Date.now() - entry.at > AVATAR_CACHE_MAX_AGE_MS) return "local";
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
    // Ignore storage failures.
  }
}

export function publisherAvatarUrl(name: string): string {
  return `https://avatars.githubusercontent.com/${encodeURIComponent(name)}?size=120`;
}

// ── Publisher Avatar ───────────────────────────────────────────────────

export function PublisherAvatar({
  name,
  size = "md",
}: {
  name: string;
  size?: "sm" | "md" | "lg";
}) {
  const [avatarSource, setAvatarSource] = useState<AvatarSource>(() =>
    readAvatarSource(name)
  );

  const showFallbackIcon = avatarSource === "none";
  const avatarSrc =
    avatarSource === "remote"
      ? publisherAvatarUrl(name)
      : `/publishers/${name}.png`;

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
        "bg-gradient-to-br from-primary/15 to-primary/5 border border-primary/10 flex items-center justify-center shrink-0 overflow-hidden"
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
        <Building2 className={cn(iconSizes[size], "text-primary/70")} />
      )}
    </div>
  );
}

// ── Publisher Card ──────────────────────────────────────────────────────

const itemVariants = {
  hidden: { opacity: 0, y: 8 },
  show: { opacity: 1, y: 0, transition: { duration: 0.2 } },
};

function PublisherCard({
  publisher,
  onClick,
}: {
  publisher: OfficialPublisher;
  onClick?: () => void;
}) {
  return (
    <motion.div variants={itemVariants}>
      <Card
        className={cn(
          "group transition-all cursor-pointer p-0 border border-border/80",
          "shadow-sm hover:shadow-md hover:border-primary/20",
          "hover:-translate-y-[1px]"
        )}
        onClick={onClick}
      >
        <div className="flex items-center gap-3.5 p-4">
          {/* Avatar */}
          <PublisherAvatar name={publisher.name} size="md" />

          {/* Info */}
          <div className="min-w-0 flex-1">
            <div className="flex items-center gap-2">
              <span className="text-sm font-semibold text-foreground truncate group-hover:text-primary transition-colors">
                {publisher.name}
              </span>
              <Badge
                variant="outline"
                className="text-[10px] px-1.5 py-0 h-4 font-normal text-muted-foreground bg-muted border-transparent shrink-0"
              >
                Official
              </Badge>
            </div>
            <div className="flex items-center gap-3 mt-0.5">
              <span className="text-xs text-muted-foreground flex items-center gap-1">
                <Folder className="w-3 h-3" />
                {publisher.repo_count} repos
              </span>
              <span className="text-xs text-muted-foreground flex items-center gap-1">
                <Package className="w-3 h-3" />
                {publisher.skill_count} skills
              </span>
            </div>
          </div>

          {/* Navigate arrow */}
          <ChevronRight className="w-4 h-4 text-muted-foreground/50 group-hover:text-primary/70 transition-all group-hover:translate-x-0.5 shrink-0" />
        </div>
      </Card>
    </motion.div>
  );
}

// ── Official Publishers Grid ───────────────────────────────────────────

interface OfficialPublishersProps {
  publishers: OfficialPublisher[];
  onPublisherClick?: (publisher: OfficialPublisher) => void;
}

const containerVariants = {
  hidden: {},
  show: {
    transition: { staggerChildren: 0.03 },
  },
};

export function OfficialPublishers({
  publishers,
  onPublisherClick,
}: OfficialPublishersProps) {
  const [showAll, setShowAll] = useState(false);
  const visiblePublishers = showAll ? publishers : publishers.slice(0, 12);

  if (publishers.length === 0) {
    return (
      <div className="flex items-center justify-center py-20 text-muted-foreground text-sm">
        Loading official publishers...
      </div>
    );
  }

  return (
    <div className="space-y-4">
      {/* Header */}
      <div className="flex items-center justify-between">
        <div>
          <h2 className="text-heading-sm">Official Publishers</h2>
          <p className="text-caption mt-0.5">
            Skills from the companies that build the technology — the makers
            teaching you how to use their products.
          </p>
        </div>
        <Badge variant="outline" className="shrink-0">
          {publishers.length} publishers
        </Badge>
      </div>

      {/* Grid */}
      <motion.div
        variants={containerVariants}
        initial="hidden"
        animate="show"
        className="grid grid-cols-1 md:grid-cols-2 xl:grid-cols-3 gap-3"
      >
        {visiblePublishers.map((pub_) => (
          <PublisherCard
            key={pub_.name}
            publisher={pub_}
            onClick={() => onPublisherClick?.(pub_)}
          />
        ))}
      </motion.div>

      {/* Show more / less */}
      {publishers.length > 12 && (
        <div className="flex justify-center">
          <Button
            variant="outline"
            size="sm"
            onClick={() => setShowAll(!showAll)}
            className="text-xs"
          >
            {showAll
              ? "Show less"
              : `Show all ${publishers.length} publishers`}
          </Button>
        </div>
      )}
    </div>
  );
}
