import { motion } from "framer-motion";
import { cn } from "../../../lib/utils";
import { AgentIcon } from "./shared/ProviderIcon";

export type ModelAppId = "claude" | "codex" | "opencode" | "gemini";

interface AppCapsuleItem {
  id: ModelAppId;
  label: string;
  color: string;
}

const APPS: AppCapsuleItem[] = [
  { id: "claude", label: "Claude", color: "#D97757" },
  { id: "codex", label: "Codex", color: "#00A67E" },
  { id: "opencode", label: "OpenCode", color: "#6366F1" },
  { id: "gemini", label: "Gemini", color: "#3B82F6" },
];

interface AppCapsuleSwitcherProps {
  value: ModelAppId;
  onChange: (id: ModelAppId) => void;
}

export function AppCapsuleSwitcher({ value, onChange }: AppCapsuleSwitcherProps) {
  return (
    <div className="relative inline-flex items-center rounded-full p-1 bg-muted/60 border border-border backdrop-blur-sm">
      {APPS.map((app) => {
        const isActive = value === app.id;
        return (
          <button
            key={app.id}
            type="button"
            onClick={() => onChange(app.id)}
            className={cn(
              "relative z-10 px-5 py-2 rounded-full text-sm font-medium transition-colors duration-200 whitespace-nowrap",
              isActive ? "text-white" : "text-muted-foreground hover:text-foreground",
            )}
          >
            {isActive && (
              <motion.div
                layoutId="capsule-highlight"
                className="absolute inset-0 rounded-full shadow-sm"
                style={{ backgroundColor: app.color }}
                transition={{
                  type: "spring",
                  stiffness: 500,
                  damping: 35,
                }}
              />
            )}
            <span className="relative z-10 flex items-center gap-2">
              <AgentIcon
                appId={app.id}
                color={isActive ? "#ffffff" : app.color}
                size="w-4 h-4"
                className={cn(
                  "transition-all duration-200",
                  isActive ? "opacity-100 scale-100" : "opacity-50 scale-90",
                )}
              />
              {app.label}
            </span>
          </button>
        );
      })}
    </div>
  );
}
