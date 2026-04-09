import { memo, useCallback, useRef } from "react";

interface SplitHandleProps {
  direction: "h" | "v";
  onRatioChange: (ratio: number) => void;
}

/** Draggable split handle — uses CSS-only during drag, commits on mouseup. */
export const SplitHandle = memo(function SplitHandle({ direction, onRatioChange }: SplitHandleProps) {
  const handleRef = useRef<HTMLDivElement>(null);

  const onMouseDown = useCallback(
    (e: React.MouseEvent) => {
      e.preventDefault();
      const handle = handleRef.current;
      if (!handle) return;
      const container = handle.parentElement;
      if (!container) return;

      const onMouseMove = (ev: MouseEvent) => {
        const rect = container.getBoundingClientRect();
        let newRatio: number;
        if (direction === "h") {
          newRatio = (ev.clientX - rect.left) / rect.width;
        } else {
          newRatio = (ev.clientY - rect.top) / rect.height;
        }
        const clamped = Math.max(0.15, Math.min(0.85, newRatio));
        container.style.setProperty("--split-ratio", String(clamped));
      };

      const onMouseUp = () => {
        const raw = container.style.getPropertyValue("--split-ratio");
        const ratio = raw ? Number.parseFloat(raw) : 0.5;
        onRatioChange(ratio);
        document.removeEventListener("mousemove", onMouseMove);
        document.removeEventListener("mouseup", onMouseUp);
        document.body.style.cursor = "";
        document.body.style.userSelect = "";
      };

      document.body.style.cursor = direction === "h" ? "col-resize" : "row-resize";
      document.body.style.userSelect = "none";
      document.addEventListener("mousemove", onMouseMove);
      document.addEventListener("mouseup", onMouseUp);
    },
    [direction, onRatioChange],
  );

  const isHorizontal = direction === "h";

  return (
    // biome-ignore lint/a11y/noStaticElementInteractions: drag handle — keyboard resize not applicable
    <div
      ref={handleRef}
      onMouseDown={onMouseDown}
      className={`group shrink-0 flex items-center justify-center
        ${isHorizontal ? "w-2 cursor-col-resize" : "h-2 cursor-row-resize"}
        hover:bg-primary/10 transition-colors`}
    >
      <div
        className={`rounded-full bg-border group-hover:bg-primary/50 transition-colors
          ${isHorizontal ? "w-0.5 h-6" : "h-0.5 w-6"}`}
      />
    </div>
  );
});
