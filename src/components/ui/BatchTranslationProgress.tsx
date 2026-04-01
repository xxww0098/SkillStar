import { useEffect, useState, useRef } from "react";
import { listen } from "@tauri-apps/api/event";
import { invoke } from "@tauri-apps/api/core";
import { motion, AnimatePresence } from "framer-motion";
import { Check, Languages } from "lucide-react";

interface BatchProgress {
  completed: number;
  total: number;
  currentName: string;
}

/**
 * Floating circular progress indicator for batch AI translation.
 * Renders fixed in the bottom-right corner above the sonner toast area.
 *
 * On mount, checks for a previously interrupted batch and auto-resumes it.
 */
export function BatchTranslationProgress() {
  const [progress, setProgress] = useState<BatchProgress | null>(null);
  const [done, setDone] = useState(false);
  const hideTimerRef = useRef<ReturnType<typeof setTimeout> | null>(null);
  const resumedRef = useRef(false);

  useEffect(() => {
    let unlisten: (() => void) | null = null;

    listen<BatchProgress>("ai://batch-progress", (event) => {
      const { completed, total, currentName } = event.payload;

      if (hideTimerRef.current) {
        clearTimeout(hideTimerRef.current);
        hideTimerRef.current = null;
      }

      if (completed >= total && total > 0) {
        // Done — show completion state briefly
        setProgress({ completed: total, total, currentName: "" });
        setDone(true);
        hideTimerRef.current = setTimeout(() => {
          setProgress(null);
          setDone(false);
        }, 3000);
      } else {
        setDone(false);
        setProgress({ completed, total, currentName });
      }
    }).then((fn) => {
      unlisten = fn;
    });

    // Auto-resume: check for interrupted batch on mount
    if (!resumedRef.current) {
      resumedRef.current = true;
      invoke<string[]>("check_pending_batch_translate")
        .then((pendingNames) => {
          if (pendingNames && pendingNames.length > 0) {
            console.log(
              `[batch-translate] Resuming ${pendingNames.length} pending skills`
            );
            invoke("ai_batch_process_skills", {
              skillNames: pendingNames,
            }).catch((err) => {
              console.error("[batch-translate] Resume failed:", err);
            });
          }
        })
        .catch(() => {
          // No pending batch — nothing to do
        });
    }

    return () => {
      unlisten?.();
      if (hideTimerRef.current) clearTimeout(hideTimerRef.current);
    };
  }, []);

  const pct = progress
    ? progress.total > 0
      ? progress.completed / progress.total
      : 0
    : 0;

  // SVG circle params
  const size = 52;
  const strokeWidth = 3.5;
  const radius = (size - strokeWidth) / 2;
  const circumference = 2 * Math.PI * radius;
  const dashOffset = circumference * (1 - pct);

  return (
    <AnimatePresence>
      {progress && (
        <motion.div
          initial={{ opacity: 0, scale: 0.6, y: 20 }}
          animate={{ opacity: 1, scale: 1, y: 0 }}
          exit={{ opacity: 0, scale: 0.6, y: 20 }}
          transition={{ type: "spring", stiffness: 400, damping: 25 }}
          className="fixed bottom-6 right-6 z-[9999] group cursor-default"
        >
          {/* Tooltip on hover */}
          <div className="absolute bottom-full right-0 mb-2 pointer-events-none opacity-0 group-hover:opacity-100 transition-opacity duration-200">
            <div className="bg-card/95 backdrop-blur-xl border border-border rounded-lg px-3 py-1.5 shadow-xl whitespace-nowrap text-xs text-foreground">
              {done
                ? `翻译完成 (${progress.total})`
                : progress.currentName
                  ? `${progress.currentName}`
                  : "准备中..."}
            </div>
          </div>

          {/* Circular progress */}
          <div
            className={`relative flex items-center justify-center rounded-full shadow-2xl transition-colors duration-500 ${
              done
                ? "bg-success/15 border-2 border-success/30"
                : "bg-card/90 backdrop-blur-xl border-2 border-border/60"
            }`}
            style={{ width: size + 8, height: size + 8 }}
          >
            {/* SVG ring */}
            <svg width={size} height={size} className="absolute -rotate-90">
              {/* Background track */}
              <circle
                cx={size / 2}
                cy={size / 2}
                r={radius}
                fill="none"
                stroke="currentColor"
                strokeWidth={strokeWidth}
                className="text-border/30"
              />
              {/* Progress arc */}
              <circle
                cx={size / 2}
                cy={size / 2}
                r={radius}
                fill="none"
                stroke="currentColor"
                strokeWidth={strokeWidth}
                strokeLinecap="round"
                strokeDasharray={circumference}
                strokeDashoffset={dashOffset}
                className={`transition-all duration-700 ease-out ${
                  done ? "text-success" : "text-primary"
                }`}
              />
            </svg>

            {/* Center content */}
            <div className="relative z-10 flex flex-col items-center justify-center">
              <AnimatePresence mode="wait">
                {done ? (
                  <motion.div
                    key="done"
                    initial={{ scale: 0, rotate: -90 }}
                    animate={{ scale: 1, rotate: 0 }}
                    transition={{ type: "spring", stiffness: 500, damping: 20 }}
                  >
                    <Check
                      className="w-5 h-5 text-success"
                      strokeWidth={3}
                    />
                  </motion.div>
                ) : (
                  <motion.div
                    key="progress"
                    initial={{ opacity: 0 }}
                    animate={{ opacity: 1 }}
                    exit={{ opacity: 0 }}
                    className="flex flex-col items-center"
                  >
                    <Languages className="w-3.5 h-3.5 text-primary/70 mb-0.5" />
                    <span className="text-[10px] font-bold tabular-nums text-foreground leading-none">
                      {progress.completed}/{progress.total}
                    </span>
                  </motion.div>
                )}
              </AnimatePresence>
            </div>

            {/* Subtle pulse when active */}
            {!done && (
              <motion.div
                className="absolute inset-0 rounded-full border-2 border-primary/20"
                animate={{ scale: [1, 1.15, 1], opacity: [0.5, 0, 0.5] }}
                transition={{
                  duration: 2,
                  repeat: Infinity,
                  ease: "easeInOut",
                }}
              />
            )}
          </div>
        </motion.div>
      )}
    </AnimatePresence>
  );
}
