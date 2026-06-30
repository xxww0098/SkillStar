import ZhipuColor from "@lobehub/icons/es/Zhipu/components/Color";
import { motion } from "framer-motion";
import {
  Boxes,
  ChevronRight,
  Cloud,
  MonitorSmartphone,
  Bot,
  Wrench,
  Globe,
  Search,
  Database,
  CloudLightning,
  MessageCircle,
} from "lucide-react";
import type { ReactNode } from "react";
import { useTranslation } from "react-i18next";
import { Badge } from "../../../components/ui/badge";
import { CardTemplate } from "../../../components/ui/card-template";
import { Github } from "../../../components/ui/icons/Github";
import { PublisherAvatar } from "../../marketplace/components/OfficialPublishers";
import { cn } from "../../../lib/utils";
import type { McpPublisherSummary } from "../../../types";

// ── Publisher brand mark ────────────────────────────────────────────────

// Brand glyph per publisher. Curated publishers without a registered brand
// icon (adspower) fall through to the shared PublisherAvatar (asset → remote → icon).
export const PUBLISHER_BRAND_ICON: Record<string, ReactNode> = {
  github: <Github className="h-6 w-6 text-primary/70" />,
  bigmodel: <ZhipuColor size={26} />,
  anthropic: <Bot className="h-6 w-6 text-primary/70" />,
  microsoft: <MonitorSmartphone className="h-6 w-6 text-primary/70" />,
  saas: <Cloud className="h-6 w-6 text-primary/70" />,
  "cn-ai": <Wrench className="h-6 w-6 text-primary/70" />,
  cloudflare: <CloudLightning className="h-6 w-6 text-primary/70" />,
  brave: <Search className="h-6 w-6 text-primary/70" />,
  google: <Globe className="h-6 w-6 text-primary/70" />,
  supabase: <Database className="h-6 w-6 text-primary/70" />,
  x: <MessageCircle className="h-6 w-6 text-primary/70" />,
};

/**
 * Whether a publisher has a dedicated brand glyph (vs. falling back to the
 * shared PublisherAvatar). Used by both the card grid and the detail hero.
 */
export function hasPublisherBrandIcon(id: string): boolean {
  return Object.hasOwn(PUBLISHER_BRAND_ICON, id);
}

// ── Publisher Card ──────────────────────────────────────────────────────

const itemVariants = {
  hidden: { opacity: 0, y: 8 },
  show: { opacity: 1, y: 0, transition: { duration: 0.2 } },
};

function McpPublisherCard({ publisher, onClick }: { publisher: McpPublisherSummary; onClick?: () => void }) {
  const { t } = useTranslation();

  // Brand-marked publishers render their glyph on the standard tile;
  // others (adspower) reuse the shared PublisherAvatar.
  const brandIcon = PUBLISHER_BRAND_ICON[publisher.id];

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
            {brandIcon ? (
              <div className="w-10 h-10 rounded-xl bg-gradient-to-br from-primary/15 to-primary/5 border border-primary/10 flex items-center justify-center shrink-0">
                {brandIcon}
              </div>
            ) : (
              <PublisherAvatar name={publisher.id} size="md" />
            )}

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
                  <Boxes className="w-3 h-3" />
                  {t("marketplace.mcpPublisherServerCount", { count: publisher.server_count })}
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

interface McpPublishersProps {
  publishers: McpPublisherSummary[];
  onPublisherClick?: (publisher: McpPublisherSummary) => void;
}

const containerVariants = {
  hidden: {},
  show: {
    transition: { staggerChildren: 0.03 },
  },
};

export function McpPublishers({ publishers, onPublisherClick }: McpPublishersProps) {
  const { t } = useTranslation();

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
          <h2 className="text-heading-sm">{t("marketplace.mcpPublishersTitle")}</h2>
          <p className="text-caption mt-0.5">{t("marketplace.mcpPublishersSubtitle")}</p>
        </div>
        <Badge variant="outline" className="shrink-0">
          {t("marketplace.mcpPublishersCount", { count: publishers.length })}
        </Badge>
      </div>

      {/* Grid */}
      <motion.div
        variants={containerVariants}
        initial="hidden"
        animate="show"
        className="ss-cards-grid"
        style={{ gridTemplateColumns: "repeat(auto-fill, minmax(280px, 1fr))" }}
      >
        {publishers.map((pub_) => (
          <McpPublisherCard key={pub_.id} publisher={pub_} onClick={() => onPublisherClick?.(pub_)} />
        ))}
      </motion.div>
    </div>
  );
}
