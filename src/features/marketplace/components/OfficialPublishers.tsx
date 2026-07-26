import { motion } from "framer-motion";
import { ChevronRight, Folder, Package } from "lucide-react";
import { useState } from "react";
import { useTranslation } from "react-i18next";
import { PublisherAvatar } from "../../../components/shared/PublisherAvatar";
import { Badge } from "../../../components/ui/badge";
import { Button } from "../../../components/ui/button";
import { CardTemplate } from "../../../components/ui/card-template";
import { cn } from "../../../lib/utils";
import type { OfficialPublisher, ViewMode } from "../../../types";

// ── Publisher Card ──────────────────────────────────────────────────────

const itemVariants = {
  hidden: { opacity: 0, y: 8 },
  show: { opacity: 1, y: 0, transition: { duration: 0.2 } },
};

function PublisherCard({ publisher, onClick }: { publisher: OfficialPublisher; onClick?: () => void }) {
  const { t } = useTranslation();

  return (
    <motion.div variants={itemVariants}>
      <CardTemplate
        className={cn(
          "group transition cursor-pointer border border-border/80",
          "shadow-sm hover:shadow-md hover:border-primary/20",
          "hover:-translate-y-[1px]",
        )}
        onClick={onClick}
        role="button"
        tabIndex={0}
        onKeyDown={(e) => {
          if (e.key === "Enter" || e.key === " ") {
            e.preventDefault();
            onClick?.();
          }
        }}
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
      ></CardTemplate>
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

export function OfficialPublishers({ publishers, viewMode = "grid", onPublisherClick }: OfficialPublishersProps) {
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
          <p className="text-caption mt-0.5">{t("marketplace.officialPublishersSubtitle")}</p>
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
        style={viewMode === "grid" ? { gridTemplateColumns: "repeat(auto-fill, minmax(280px, 1fr))" } : undefined}
      >
        {visiblePublishers.map((pub_) => (
          <PublisherCard key={pub_.name} publisher={pub_} onClick={() => onPublisherClick?.(pub_)} />
        ))}
      </motion.div>

      {/* Show more / less */}
      {publishers.length > 12 && (
        <div className="flex justify-center">
          <Button variant="outline" size="sm" onClick={() => setShowAll(!showAll)} className="text-xs">
            {showAll ? t("marketplace.showLess") : t("marketplace.showAllPublishers", { count: publishers.length })}
          </Button>
        </div>
      )}
    </div>
  );
}
