import { motion } from "framer-motion";
import { AlertTriangle, ListFilter } from "lucide-react";
import { useTranslation } from "react-i18next";
import { MOTION_TRANSITION } from "../../../comm/motion";
import { navigateToSettingsSection } from "../../../lib/utils";

interface SkillListBannersProps {
  brokenCount: number;
  onlyUpdatesFilter: boolean;
  /** Skills actually shown under the active filter, for the banner count. */
  filteredCount: number;
  onClearUpdatesFilter: () => void;
}

/**
 * Thin status strips between the selection bar and the grid. Both mount in
 * place (no height animation) so the grid below relayouts once, not per frame:
 * the updates-filter strip makes the toolbar chip's effect explicit, and the
 * broken-skills strip links to the storage settings that can clean them up.
 */
export function SkillListBanners({
  brokenCount,
  onlyUpdatesFilter,
  filteredCount,
  onClearUpdatesFilter,
}: SkillListBannersProps) {
  const { t } = useTranslation();

  return (
    <>
      {onlyUpdatesFilter && (
        <div className="flex items-center gap-2.5 px-6 py-2 bg-primary/8 border-b border-primary/20">
          <ListFilter className="w-3.5 h-3.5 text-primary shrink-0" />
          <span className="text-caption text-primary/90">
            {t("mySkills.updatesFilterBanner", {
              count: filteredCount,
              defaultValue: `Filtered: showing ${filteredCount} skill(s) with updates`,
            })}
          </span>
          <button
            type="button"
            onClick={onClearUpdatesFilter}
            className="text-caption text-primary hover:text-primary/80 font-medium ml-auto cursor-pointer transition-colors"
          >
            {t("mySkills.updatesFilterShowAll", { defaultValue: "Show all" })}
          </button>
        </div>
      )}

      {brokenCount > 0 && (
        <motion.div
          initial={{ opacity: 0, y: -6 }}
          animate={{ opacity: 1, y: 0 }}
          transition={MOTION_TRANSITION.fadeFast}
        >
          <div className="flex items-center gap-2.5 px-6 py-2 bg-amber-500/8 border-b border-amber-500/20">
            <AlertTriangle className="w-3.5 h-3.5 text-amber-400 shrink-0" />
            <span className="text-caption text-amber-300/90">{t("mySkills.brokenBanner", { count: brokenCount })}</span>
            <button
              type="button"
              onClick={() => {
                navigateToSettingsSection("storage");
              }}
              className="text-caption text-amber-400 hover:text-amber-300 font-medium ml-auto cursor-pointer transition-colors"
            >
              {t("mySkills.brokenBannerAction")} →
            </button>
          </div>
        </motion.div>
      )}
    </>
  );
}
