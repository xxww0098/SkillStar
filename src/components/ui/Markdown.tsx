import { lazy, memo, type ReactNode, Suspense } from "react";
import { cn } from "../../lib/utils";

// The full markdown stack (react-markdown + remark/rehype plugins + sanitize
// schema) lives in `MarkdownRenderer` and is loaded lazily, so pages that merely
// import `Markdown` don't pull ~300kB of plugin code into their chunk — it only
// loads when markdown is actually rendered (e.g. opening a skill detail).
const MarkdownRenderer = lazy(() => import("./MarkdownRenderer"));

interface MarkdownProps {
  children: string;
  /** Additional class names for the wrapper div */
  className?: string;
  /** Fallback shown while the markdown chunk loads (default: plain text) */
  fallback?: ReactNode;
  /** When true, render as plain text to avoid expensive re-parsing during streaming */
  streaming?: boolean;
}

/**
 * Shared markdown renderer.
 *
 * Encapsulates:
 * - Lazy-loaded markdown stack (downloaded only when first rendered)
 * - `remark-gfm` for tables, strikethrough, task lists, autolinks
 * - `rehype-sanitize` (SVG-aware schema) + `rehype-raw`
 * - `.markdown-content` + Tailwind `prose` styling
 * - `<Suspense>` with a plain-text fallback
 */
export const Markdown = memo(
  function Markdown({ children, className, fallback, streaming }: MarkdownProps) {
    return (
      <div className={cn("markdown-content prose prose-sm dark:prose-invert max-w-none", className)}>
        {streaming ? (
          <p className="text-body whitespace-pre-wrap">{children}</p>
        ) : (
          <Suspense fallback={fallback ?? <p className="text-body whitespace-pre-wrap">{children}</p>}>
            <MarkdownRenderer>{children}</MarkdownRenderer>
          </Suspense>
        )}
      </div>
    );
  },
  (prev, next) =>
    prev.children === next.children && prev.streaming === next.streaming && prev.className === next.className,
);
