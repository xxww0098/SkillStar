import { X } from "lucide-react";
import { useEffect, useRef } from "react";
import { useTranslation } from "react-i18next";
import { Button } from "@/components/ui/button";
import type { DesktopAppId } from "../../types";
import { AppInstancesPanel } from "./AppInstancesPanel";

export function AppInstancesOverlay({ appId, onClose }: { appId: DesktopAppId; onClose: () => void }) {
  const { t } = useTranslation();
  const panelRef = useRef<HTMLDivElement>(null);

  useEffect(() => {
    panelRef.current?.querySelector<HTMLButtonElement>("[data-instances-close]")?.focus();
  }, []);

  useEffect(() => {
    const onKey = (event: KeyboardEvent) => {
      if (event.key === "Escape") onClose();
    };
    document.addEventListener("keydown", onKey);
    return () => document.removeEventListener("keydown", onKey);
  }, [onClose]);

  return (
    <div className="absolute inset-0 z-30 flex flex-col bg-white/97 p-3 backdrop-blur-sm">
      <div ref={panelRef} className="flex min-h-0 flex-1 flex-col">
        <div className="mb-2 flex items-center justify-between gap-2">
          <h3 className="text-sm font-semibold text-foreground">{t("usage.instances")}</h3>
          <Button
            type="button"
            size="icon-sm"
            variant="ghost"
            data-instances-close
            title={t("common.close")}
            aria-label={t("common.close")}
            onClick={onClose}
          >
            <X className="h-3.5 w-3.5" aria-hidden />
          </Button>
        </div>
        <div className="min-h-0 flex-1 overflow-y-auto">
          <AppInstancesPanel appId={appId} compact />
        </div>
      </div>
    </div>
  );
}
