import { useState, type CSSProperties } from "react";
import { motion } from "framer-motion";
import { Building2, Folder, Package, ChevronRight } from "lucide-react";
import { useTranslation } from "react-i18next";
import { CardTemplate } from "../../../components/ui/card-template";
import { Badge } from "../../../components/ui/badge";
import { Button } from "../../../components/ui/button";
import { cn } from "../../../lib/utils";
import type { OfficialPublisher, ViewMode } from "../../../types";

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
  const { t } = useTranslation();

  return (
    <motion.div variants={itemVariants}>
      <CardTemplate
        className={cn(
          "group transition cursor-pointer border border-border/80",
          "shadow-sm hover:shadow-md hover:border-primary/20",
          "hover:-translate-y-[1px]"
        )}
        onClick={onClick}
        role="button"
        tabIndex={0}
        onKeyDown={(e) => { if (e.key === "Enter" || e.key === " ") { e.preventDefault(); onClick?.(); }}}
        bodyClassName="p-0"
        body={
          <div className="ss-card-body flex items-center gap-3.5">
            <PublisherAvatar name={publisher.name} size="md" />

            <div className="min-w-0 flex-1">
              <div className="flex items-center gap-2">
                <span className="ss-card-title text-foreground truncate group-hover:text-primary transition-colors">
                  {publisher.name}
                </span>
                <Badge
                  variant="outline"
                  className="text-micro px-1.5 py-0 h-4 font-normal text-muted-foreground bg-muted border-transparent shrink-0"
                >
                  {t("marketplace.officialBadge")}
                </Badge>
              </div>
              <div className="flex items-center gap-3 mt-0.5">
                <span className="ss-card-meta flex items-center gap-1">
                  <Folder className="w-3 h-3" />
                  {t("marketplace.repoCount", { count: publisher.repo_count })}
                </span>
                <span className="ss-card-meta flex items-center gap-1">
                  <Package className="w-3 h-3" />
                  {t("marketplace.skillCount", { count: publisher.skill_count })}
                </span>
              </div>
            </div>

            <ChevronRight className="w-4 h-4 text-muted-foreground/50 group-hover:text-primary/70 transition group-hover:translate-x-0.5 shrink-0" />
          </div>
        }
      >
      </CardTemplate>
    </motion.div>
  );
}

// ── Official Publishers Grid ───────────────────────────────────────────

interface OfficialPublishersProps {
  publishers: OfficialPublisher[];
  viewMode?: ViewMode;
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
  viewMode = "grid",
  onPublisherClick,
}: OfficialPublishersProps) {
  const { t } = useTranslation();
  const [showAll, setShowAll] = useState(false);
  const visiblePublishers = showAll ? publishers : publishers.slice(0, 12);

  if (publishers.length === 0) {
    return (
      <div className="flex items-center justify-center py-20 text-muted-foreground text-sm">
        {t("marketplace.loadingOfficialPublishers")}
      </div>
    );
  }

  return (
    <div className="space-y-4">
      {/* Header */}
      <div className="flex items-center justify-between">
        <div>
          <h2 className="text-heading-sm">{t("marketplace.officialPublishersTitle")}</h2>
          <p className="text-caption mt-0.5">
            {t("marketplace.officialPublishersSubtitle")}
          </p>
        </div>
        <Badge variant="outline" className="shrink-0">
          {t("marketplace.publishersCount", { count: publishers.length })}
        </Badge>
      </div>

      {/* Grid */}
      <motion.div
        variants={containerVariants}
        initial="hidden"
        animate="show"
        className={cn(viewMode === "grid" ? "ss-cards-grid" : "ss-cards-list")}
        style={viewMode === "grid" ? { "--ss-card-min": "280px" } as CSSProperties : undefined}
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
              ? t("marketplace.showLess")
              : t("marketplace.showAllPublishers", { count: publishers.length })}
          </Button>
        </div>
      )}
    </div>
  );
}
