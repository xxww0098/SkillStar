import { Plus } from "lucide-react";
import { useTranslation } from "react-i18next";
import { Button } from "@/components/ui/button";

interface UsageHomeEmptyProps {
  onBrowseProviders: () => void;
}

/** Shown on the usage home when there are no bound subscriptions yet. */
export function UsageHomeEmpty({ onBrowseProviders }: UsageHomeEmptyProps) {
  const { t } = useTranslation();

  return (
    <div className="flex flex-1 flex-col items-center justify-center px-6 py-16 text-center">
      <div className="mb-4 flex h-12 w-12 items-center justify-center rounded-2xl bg-primary/10 text-primary">
        <Plus className="h-6 w-6" aria-hidden />
      </div>
      <p className="text-sm font-semibold text-foreground">{t("usage.homeEmptyTitle")}</p>
      <p className="mt-2 max-w-sm text-xs leading-relaxed text-muted-foreground">{t("usage.homeEmptyHint")}</p>
      <Button type="button" size="sm" variant="outline" className="mt-5" onClick={onBrowseProviders}>
        {t("usage.homeEmptyAction")}
      </Button>
    </div>
  );
}
