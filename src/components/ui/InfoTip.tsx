import { HelpCircle } from "lucide-react";
import { Tooltip } from "radix-ui";
import { useTranslation } from "react-i18next";
import { cn } from "../../lib/utils";

interface InfoTipProps {
  content: string;
  className?: string;
  iconClassName?: string;
}

export function InfoTip({ content, className, iconClassName }: InfoTipProps) {
  const { t } = useTranslation();

  const parseContent = (text: string) => {
    const lines = text.split("\n");
    return lines.map((line, index) => {
      if (!line.trim()) {
        return <div key={index} className="h-1.5" />;
      }

      const colonIndex = line.indexOf(":");
      if (colonIndex > 0) {
        const label = line.slice(0, colonIndex);
        const value = line.slice(colonIndex + 1);

        // Ensure we don't accidentally treat URLs as label:value
        if (label.length < 30 && !label.includes("http") && !label.includes("https")) {
          return (
            <div key={index} className="text-muted-foreground leading-relaxed text-[11px]">
              <span className="font-semibold text-foreground">{label}:</span>
              {value}
            </div>
          );
        }
      }

      return (
        <div key={index} className="text-muted-foreground leading-relaxed text-[11px]">
          {line}
        </div>
      );
    });
  };

  return (
    <Tooltip.Provider delayDuration={220} skipDelayDuration={80}>
      <Tooltip.Root>
        <Tooltip.Trigger asChild>
          <button
            type="button"
            aria-label={t("common.helpInfo")}
            className={cn(
              "inline-flex items-center justify-center rounded-full text-muted-foreground/60 transition-colors duration-200 hover:text-foreground/90 focus-visible:outline-none focus-visible:ring-2 focus-visible:ring-primary/35",
              className,
            )}
          >
            <HelpCircle className={cn("h-3.5 w-3.5 cursor-help", iconClassName)} />
          </button>
        </Tooltip.Trigger>
        <Tooltip.Portal>
          <Tooltip.Content
            side="top"
            sideOffset={8}
            collisionPadding={12}
            className="z-[120] w-64 rounded-xl border border-border/55 bg-background/95 p-3 text-left text-xs text-foreground shadow-2xl backdrop-blur-xl data-[state=delayed-open]:animate-in data-[state=closed]:animate-out data-[state=delayed-open]:fade-in data-[state=closed]:fade-out"
          >
            {parseContent(content)}
            <Tooltip.Arrow className="fill-background/95" width={10} height={5} />
          </Tooltip.Content>
        </Tooltip.Portal>
      </Tooltip.Root>
    </Tooltip.Provider>
  );
}
