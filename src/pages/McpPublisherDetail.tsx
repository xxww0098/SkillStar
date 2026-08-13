import { motion } from "framer-motion";
import { ArrowLeft, Boxes, ExternalLink } from "lucide-react";
import { useTranslation } from "react-i18next";
import { PageToolbar } from "../components/layout/PageToolbar";
import { Badge } from "../components/ui/badge";
import { Button } from "../components/ui/button";
import { ExternalAnchor } from "../components/ui/ExternalAnchor";
import { McpMarketPage } from "../features/mcp/components/McpMarketPage";
import { PUBLISHER_BRAND_ICON, hasPublisherBrandIcon } from "../features/mcp/components/McpPublishers";
import { PublisherAvatar } from "../components/shared/PublisherAvatar";
import type { McpPublisherSummary } from "../types";

interface McpPublisherDetailProps {
  publisher: McpPublisherSummary;
  onBack: () => void;
}

/**
 * One publisher's slice of the MCP catalog.
 *
 * The page is now a thin hero over the same paginated browse the MCP page uses,
 * scoped with `publisherId`. It used to load the publisher's whole bucket
 * unpaginated and filter it in memory over three fields — which for the
 * `github` bucket means the entire remote registry — while the snapshot's FTS
 * index, its filters and its sort orders all went unused, and the refresh
 * button was wired to an empty function (audit D.2, D.3-1/2/3).
 *
 * The hero is a fixed header rather than part of the scroll, because the
 * scroller now belongs to `McpMarketPage`. The old back-to-top button went with
 * it: a page is at most one `limit` of cards, and a button bound to a container
 * that no longer scrolls is worse than no button.
 */
export function McpPublisherDetail({ publisher, onBack }: McpPublisherDetailProps) {
  const { t } = useTranslation();
  const hasBrandIcon = hasPublisherBrandIcon(publisher.id);

  return (
    <div className="relative flex min-w-0 flex-1 overflow-hidden">
      <div className="flex min-w-0 flex-1 flex-col overflow-hidden">
        <PageToolbar
          title={
            <div className="flex min-w-0 items-center gap-2">
              <Button
                variant="ghost"
                size="sm"
                onClick={onBack}
                className="-ml-2 gap-1.5 text-muted-foreground hover:text-foreground"
              >
                <ArrowLeft className="h-4 w-4" />
                {t("publisherDetail.back")}
              </Button>
              <div className="mx-1 h-5 w-px bg-border" />
              <span className="truncate whitespace-nowrap text-sm font-semibold">{publisher.name}</span>
            </div>
          }
        />

        <motion.div
          initial={{ opacity: 0 }}
          animate={{ opacity: 1 }}
          transition={{ duration: 0.2 }}
          className="flex min-h-0 flex-1 flex-col overflow-hidden"
        >
          <div className="shrink-0 border-b border-border bg-gradient-to-b from-primary/5 to-transparent px-6 pb-5 pt-6">
            <div className="flex max-w-4xl items-start gap-5">
              {hasBrandIcon ? (
                <div className="flex h-14 w-14 shrink-0 items-center justify-center rounded-2xl border border-primary/10 bg-gradient-to-br from-primary/15 to-primary/5">
                  {PUBLISHER_BRAND_ICON[publisher.id]}
                </div>
              ) : (
                <PublisherAvatar name={publisher.id} size="lg" />
              )}
              <div className="min-w-0 flex-1">
                <div className="mb-1 flex items-center gap-2.5">
                  <h2 className="truncate text-heading-lg">{publisher.name}</h2>
                  <Badge
                    variant="outline"
                    className="h-5 shrink-0 border-primary/20 bg-primary/8 px-2 py-0.5 text-micro font-medium text-primary"
                  >
                    {t("publisherDetail.official")}
                  </Badge>
                </div>

                <div className="mt-2 flex flex-wrap items-center gap-4">
                  <span className="flex items-center gap-1.5 text-sm text-muted-foreground">
                    <Boxes className="h-3.5 w-3.5" />
                    {t("publisherDetail.mcpServers", {
                      count: publisher.serverCount,
                      defaultValue: "{{count}} servers",
                    })}
                  </span>
                  <ExternalAnchor
                    href={publisher.url}
                    className="ml-auto flex items-center gap-1.5 text-sm text-primary/70 transition-colors hover:text-primary"
                  >
                    <ExternalLink className="h-3.5 w-3.5" />
                    {t("publisherDetail.viewOnSkillsSh", { defaultValue: "Open" })}
                  </ExternalAnchor>
                </div>
              </div>
            </div>
          </div>

          <McpMarketPage key={publisher.id} publisherId={publisher.id} />
        </motion.div>
      </div>
    </div>
  );
}
