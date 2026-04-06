import { AnimatePresence, motion, useReducedMotion } from "framer-motion";
import { useEffect, useState } from "react";

const SPLASH_DURATION_MS = 1200;
const SPLASH_KEY = "skillstar-has-launched";

/**
 * First-launch splash screen with the SkillStar logo.
 * Shows a polished entrance animation on first app load,
 * then a faster version on subsequent cold starts.
 */
export function SplashScreen({ children }: { children: React.ReactNode }) {
  const prefersReducedMotion = useReducedMotion();
  const [showSplash, setShowSplash] = useState(() => {
    // Skip splash entirely if reduced motion is on
    if (prefersReducedMotion) return false;
    return true;
  });
  const [isFirstLaunch] = useState(() => {
    try {
      return !localStorage.getItem(SPLASH_KEY);
    } catch {
      return true;
    }
  });

  useEffect(() => {
    if (!showSplash) return;

    // First launch gets a longer animation; repeat visits get a quick fade
    const duration = isFirstLaunch ? SPLASH_DURATION_MS : 600;

    const timer = setTimeout(() => {
      setShowSplash(false);
      try {
        localStorage.setItem(SPLASH_KEY, "1");
      } catch {
        // Storage full — harmless
      }
    }, duration);

    return () => clearTimeout(timer);
  }, [showSplash, isFirstLaunch]);

  return (
    <>
      <AnimatePresence>
        {showSplash && (
          <motion.div
            key="splash"
            initial={{ opacity: 1 }}
            exit={{ opacity: 0 }}
            transition={{ duration: 0.35, ease: [0.22, 1, 0.36, 1] }}
            className="fixed inset-0 z-[9999] flex flex-col items-center justify-center bg-background"
            style={{
              backgroundImage:
                "radial-gradient(ellipse at 20% 0%, rgba(59, 130, 246, 0.08) 0%, transparent 50%), radial-gradient(ellipse at 80% 100%, rgba(139, 92, 246, 0.06) 0%, transparent 50%)",
            }}
          >
            {/* Logo */}
            <motion.div
              initial={{ scale: 0.5, opacity: 0, rotate: -90 }}
              animate={{ scale: 1, opacity: 1, rotate: 0 }}
              transition={{
                duration: isFirstLaunch ? 0.7 : 0.35,
                ease: [0.16, 1, 0.3, 1],
              }}
              className="w-16 h-16 rounded-2xl overflow-hidden bg-white shadow-lg"
            >
              <img src="/skillstar-icon.svg" alt="SkillStar" className="w-full h-full" />
            </motion.div>

            {/* App name — only on first launch */}
            {isFirstLaunch && (
              <motion.div
                initial={{ opacity: 0, y: 12 }}
                animate={{ opacity: 1, y: 0 }}
                transition={{ delay: 0.3, duration: 0.4, ease: [0.22, 1, 0.36, 1] }}
                className="mt-5 flex flex-col items-center gap-1"
              >
                <span className="text-xl font-bold tracking-tight text-foreground">SkillStar</span>
                <span className="text-xs text-muted-foreground tracking-widest uppercase">Agent Skill Manager</span>
              </motion.div>
            )}

            {/* Loading dots — subtle progress indication */}
            <motion.div
              initial={{ opacity: 0 }}
              animate={{ opacity: 1 }}
              transition={{ delay: isFirstLaunch ? 0.5 : 0.2, duration: 0.3 }}
              className="mt-8 flex items-center gap-1.5"
            >
              {[0, 1, 2].map((i) => (
                <motion.div
                  key={i}
                  className="w-1.5 h-1.5 rounded-full bg-primary/40"
                  animate={{ opacity: [0.3, 1, 0.3] }}
                  transition={{
                    duration: 1,
                    repeat: Infinity,
                    delay: i * 0.15,
                    ease: "easeInOut",
                  }}
                />
              ))}
            </motion.div>
          </motion.div>
        )}
      </AnimatePresence>

      {/* App content renders underneath and becomes visible as splash exits */}
      <motion.div
        initial={{ opacity: showSplash ? 0 : 1 }}
        animate={{ opacity: 1 }}
        transition={{
          delay: showSplash ? 0 : 0,
          duration: showSplash ? 0.3 : 0,
        }}
        className="contents"
      >
        {children}
      </motion.div>
    </>
  );
}
