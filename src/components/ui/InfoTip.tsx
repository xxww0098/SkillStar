import { AnimatePresence, motion } from "framer-motion";
import { HelpCircle } from "lucide-react";
import { useState } from "react";
import { cn } from "../../lib/utils";

interface InfoTipProps {
  content: string;
  className?: string;
  iconClassName?: string;
}

export function InfoTip({ content, className, iconClassName }: InfoTipProps) {
  const [isHovered, setIsHovered] = useState(false);

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
    <div
      className={cn("relative inline-flex items-center justify-center select-none", className)}
      onMouseEnter={() => setIsHovered(true)}
      onMouseLeave={() => setIsHovered(false)}
      onFocus={() => setIsHovered(true)}
      onBlur={() => setIsHovered(false)}
    >
      <HelpCircle
        className={cn(
          "w-3.5 h-3.5 text-muted-foreground/60 hover:text-foreground/90 transition-colors duration-200 cursor-help outline-none",
          iconClassName,
        )}
        tabIndex={0}
        aria-label="配置帮助信息"
      />

      <AnimatePresence>
        {isHovered && (
          <motion.div
            initial={{ opacity: 0, y: 6, scale: 0.96 }}
            animate={{ opacity: 1, y: 0, scale: 1 }}
            exit={{ opacity: 0, y: 6, scale: 0.96 }}
            transition={{ duration: 0.12, ease: "easeOut" }}
            className="absolute bottom-full left-1/2 -translate-x-1/2 mb-2 z-50 pointer-events-none w-64"
          >
            <div className="backdrop-blur-md bg-background/95 border border-border/45 rounded-xl p-3 text-xs text-foreground shadow-2xl text-left">
              {parseContent(content)}
            </div>
            {/* Small tooltip indicator tip */}
            <div className="absolute top-full left-1/2 -translate-x-1/2 -mt-1 border-4 border-transparent border-t-background/95" />
          </motion.div>
        )}
      </AnimatePresence>
    </div>
  );
}
