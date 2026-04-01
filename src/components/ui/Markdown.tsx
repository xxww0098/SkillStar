import { lazy, Suspense, type ReactNode } from "react";
import remarkGfm from "remark-gfm";
import { markdownComponents } from "../../lib/markdown";
import { cn } from "../../lib/utils";

const ReactMarkdown = lazy(() => import("react-markdown"));

/** Stable reference — avoids re-creating the array on every render */
const REMARK_PLUGINS = [remarkGfm];

interface MarkdownProps {
  children: string;
  /** Additional class names for the wrapper div */
  className?: string;
  /** Fallback shown while react-markdown chunk loads (default: plain text) */
  fallback?: ReactNode;
  /** When true, render as plain text to avoid expensive re-parsing during streaming */
  streaming?: boolean;
}

/**
 * Shared markdown renderer.
 *
 * Encapsulates:
 * - Lazy-loaded `react-markdown` (only downloaded when first used)
 * - `remark-gfm` for tables, strikethrough, task lists, autolinks
 * - `markdownComponents` for inline code normalization
 * - `.markdown-content` + Tailwind `prose` styling
 * - `<Suspense>` with a plain-text fallback
 */
export function Markdown({ children, className, fallback, streaming }: MarkdownProps) {
  return (
    <div className={cn("markdown-content prose prose-sm dark:prose-invert max-w-none", className)}>
      {streaming ? (
        <p className="text-body whitespace-pre-wrap">{children}</p>
      ) : (
        <Suspense
          fallback={
            fallback ?? (
              <p className="text-body whitespace-pre-wrap">{children}</p>
            )
          }
        >
          <ReactMarkdown
            remarkPlugins={REMARK_PLUGINS}
            components={markdownComponents}
          >
            {children}
          </ReactMarkdown>
        </Suspense>
      )}
    </div>
  );
}
