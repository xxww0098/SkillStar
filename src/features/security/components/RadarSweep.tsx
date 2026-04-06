import { AnimatePresence, motion } from "framer-motion";
import { useEffect, useRef } from "react";

interface RadarSweepProps {
  active: boolean;
  activeSkills?: string[];
  currentStage?: string | null;
  syncPulseKey?: number;
  scanned: number;
  total: number;
  progressPercent: number;
}

// Deterministic blip positions — golden angle distribution (avoids re-render jitter from Math.random)
function getBlipPosition(index: number) {
  const cx = 140;
  const cy = 140;
  const goldenAngle = 2.399963;
  const angle = index * goldenAngle;
  const radius = 50 + ((index * 37 + 13) % 60);
  return {
    x: cx + Math.cos(angle) * radius,
    y: cy + Math.sin(angle) * radius,
  };
}

// Extracted static styles to avoid object recreation on every render
const containerStyle = { width: 280, height: 280 } as const;
const sweepArmStyle = {
  position: "absolute" as const,
  top: "calc(50% - 1px)",
  left: "50%",
  width: "50%",
  height: "2px",
  transformOrigin: "left center",
  background: "linear-gradient(90deg, rgba(var(--color-success-rgb), 0.8) 0%, rgba(var(--color-success-rgb), 0) 100%)",
};
const sweepTrailStyle = {
  position: "absolute" as const,
  top: 0,
  left: 0,
  right: 0,
  bottom: 0,
  borderRadius: "50%",
  background:
    "conic-gradient(from 0deg, transparent 0deg, rgba(var(--color-success-rgb), 0) 30deg, rgba(var(--color-success-rgb), 0.15) 90deg, transparent 90deg)",
};

export function RadarSweep({
  active,
  activeSkills = [],
  currentStage,
  syncPulseKey = 0,
  scanned,
  total,
  progressPercent,
}: RadarSweepProps) {
  const canvasRef = useRef<HTMLCanvasElement>(null);

  // Draw radar grid rings on canvas — read CSS variable once
  useEffect(() => {
    const canvas = canvasRef.current;
    if (!canvas) return;
    const ctx = canvas.getContext("2d");
    if (!ctx) return;

    const root = getComputedStyle(document.documentElement);
    const successRgb = root.getPropertyValue("--color-success-rgb").trim() || "16, 185, 129";

    const canvasWidth = canvas.width;
    const canvasHeight = canvas.height;
    const cx = canvasWidth / 2;
    const cy = canvasHeight / 2;
    const maxR = Math.min(cx, cy) - 4;

    ctx.clearRect(0, 0, canvasWidth, canvasHeight);

    // Grid rings
    for (let ringIndex = 1; ringIndex <= 4; ringIndex++) {
      const ringRadius = (maxR / 4) * ringIndex;
      ctx.beginPath();
      ctx.arc(cx, cy, ringRadius, 0, Math.PI * 2);
      ctx.strokeStyle = `rgba(${successRgb}, ${0.08 + ringIndex * 0.03})`;
      ctx.lineWidth = 1;
      ctx.stroke();
    }

    // Cross lines
    ctx.strokeStyle = `rgba(${successRgb}, 0.12)`;
    ctx.lineWidth = 1;
    ctx.beginPath();
    ctx.moveTo(cx, cy - maxR);
    ctx.lineTo(cx, cy + maxR);
    ctx.moveTo(cx - maxR, cy);
    ctx.lineTo(cx + maxR, cy);
    ctx.stroke();

    // Diagonal lines
    ctx.strokeStyle = `rgba(${successRgb}, 0.06)`;
    ctx.beginPath();
    const diagonalOffset = maxR * 0.707;
    ctx.moveTo(cx - diagonalOffset, cy - diagonalOffset);
    ctx.lineTo(cx + diagonalOffset, cy + diagonalOffset);
    ctx.moveTo(cx + diagonalOffset, cy - diagonalOffset);
    ctx.lineTo(cx - diagonalOffset, cy + diagonalOffset);
    ctx.stroke();
  }, []);

  const progress = progressPercent;

  return (
    <div className="relative flex items-center justify-center" style={containerStyle}>
      {/* Radar background */}
      <canvas ref={canvasRef} width={280} height={280} className="absolute inset-0" />

      {/* Outer glow ring */}
      <div
        className="absolute inset-0 rounded-full transition-[box-shadow] duration-[600ms]"
        style={{
          border: "1px solid rgba(var(--color-success-rgb), 0.2)",
          boxShadow: active
            ? "0 0 30px rgba(var(--color-success-rgb), 0.15), inset 0 0 30px rgba(var(--color-success-rgb), 0.05)"
            : "none",
        }}
      />

      <AnimatePresence>
        {active && (
          <motion.div
            key={syncPulseKey}
            className="absolute inset-[14%] rounded-full border border-success/40"
            initial={{ opacity: 0.4, scale: 0.75 }}
            animate={{ opacity: 0, scale: 1.05 }}
            exit={{ opacity: 0 }}
            transition={{ duration: 0.7, ease: "easeOut" }}
          />
        )}
      </AnimatePresence>

      {/* Sweep arm — continuous CSS spin animation */}
      {active && (
        <div
          className="absolute inset-0"
          style={{
            transformOrigin: "center center",
            animation: "spin 3s linear infinite",
          }}
        >
          <div style={sweepArmStyle} />
          <div style={sweepTrailStyle} />
        </div>
      )}

      {/* Center info */}
      <div className="relative z-10 flex flex-col items-center text-center">
        {active ? (
          <>
            <div className="text-3xl font-bold text-success tabular-nums">{progress}%</div>
            <div className="text-xs text-muted-foreground mt-1 tabular-nums">
              {scanned}/{total}
            </div>
            {/* Active skills — show all concurrent scans (up to 4) */}
            {activeSkills.length > 0 && (
              <div className="mt-2.5 max-w-[170px] flex flex-col items-center gap-1">
                {activeSkills.map((skill) => (
                  <div key={skill} className="flex items-center gap-1.5 w-full">
                    <span className="w-1.5 h-1.5 rounded-full bg-success shrink-0 animate-pulse" />
                    <span className="text-[11px] text-success truncate">{skill}</span>
                  </div>
                ))}
                {currentStage && (
                  <span className="block mt-1 text-[10px] tracking-wide uppercase text-success/80">
                    {currentStage === "collect"
                      ? "Preparing..."
                      : currentStage === "static"
                        ? "Static Match..."
                        : currentStage === "triage"
                          ? "Triage..."
                          : currentStage === "ai" || currentStage === "ai-analyze"
                            ? "AI Analysis..."
                            : currentStage === "aggregator" || currentStage === "aggregate"
                              ? "AI Consensus..."
                              : "Scanning..."}
                  </span>
                )}
              </div>
            )}
          </>
        ) : (
          <div className="text-muted-foreground/70 text-xs font-medium tracking-widest uppercase">Ready</div>
        )}
      </div>

      {/* Deterministic blip dots for completed scans */}
      {active &&
        Array.from({ length: Math.min(scanned, 8) }).map((_, i) => {
          const pos = getBlipPosition(i);
          return (
            <motion.div
              key={`blip-${i}`}
              className="absolute w-1.5 h-1.5 rounded-full bg-success"
              style={{ left: pos.x, top: pos.y }}
              initial={{ scale: 0, opacity: 0 }}
              animate={{ scale: 1, opacity: 0.6 }}
              transition={{ delay: i * 0.15, duration: 0.3 }}
            />
          );
        })}
    </div>
  );
}
