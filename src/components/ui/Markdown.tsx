import { lazy, memo, type ReactNode, Suspense } from "react";
import rehypeRaw from "rehype-raw";
import rehypeSanitize, { defaultSchema } from "rehype-sanitize";
import remarkGfm from "remark-gfm";
import { markdownComponents } from "../../lib/markdown";
import { cn } from "../../lib/utils";

const ReactMarkdown = lazy(() => import("react-markdown"));

/** Stable reference — avoids re-creating the array on every render */
const REMARK_PLUGINS = [remarkGfm];

function mergeList(base: readonly string[] | undefined, extra: readonly string[]): string[] {
  return Array.from(new Set([...(base ?? []), ...extra]));
}

const SVG_TAGS = [
  "svg",
  "g",
  "path",
  "circle",
  "rect",
  "line",
  "polyline",
  "polygon",
  "ellipse",
  "defs",
  "linearGradient",
  "radialGradient",
  "stop",
  "clipPath",
  "mask",
  "pattern",
  "symbol",
  "use",
  "image",
  "text",
  "tspan",
] as const;

const SVG_COMMON_ATTRS = [
  "className",
  "class",
  "style",
  "viewBox",
  "viewbox",
  "width",
  "height",
  "fill",
  "fill-rule",
  "stroke",
  "stroke-width",
  "stroke-linecap",
  "stroke-linejoin",
  "stroke-miterlimit",
  "stroke-dasharray",
  "stroke-dashoffset",
  "opacity",
  "transform",
  "xmlns",
  "xmlns:xlink",
  "xlink:href",
  "href",
  "x",
  "y",
  "x1",
  "x2",
  "y1",
  "y2",
  "cx",
  "cy",
  "r",
  "rx",
  "ry",
  "d",
  "points",
  "offset",
  "stop-color",
  "stop-opacity",
  "gradientUnits",
  "gradientTransform",
  "preserveAspectRatio",
  "role",
  "aria-hidden",
  "focusable",
] as const;

const SANITIZE_SCHEMA = {
  ...defaultSchema,
  tagNames: mergeList(defaultSchema.tagNames as string[] | undefined, SVG_TAGS),
  attributes: {
    ...(defaultSchema.attributes ?? {}),
    a: mergeList((defaultSchema.attributes as Record<string, string[] | undefined>)?.a, ["target", "rel"]),
    img: mergeList((defaultSchema.attributes as Record<string, string[] | undefined>)?.img, [
      "loading",
      "decoding",
      "referrerpolicy",
    ]),
    svg: mergeList((defaultSchema.attributes as Record<string, string[] | undefined>)?.svg, SVG_COMMON_ATTRS),
    g: mergeList((defaultSchema.attributes as Record<string, string[] | undefined>)?.g, SVG_COMMON_ATTRS),
    path: mergeList((defaultSchema.attributes as Record<string, string[] | undefined>)?.path, SVG_COMMON_ATTRS),
    circle: mergeList((defaultSchema.attributes as Record<string, string[] | undefined>)?.circle, SVG_COMMON_ATTRS),
    rect: mergeList((defaultSchema.attributes as Record<string, string[] | undefined>)?.rect, SVG_COMMON_ATTRS),
    line: mergeList((defaultSchema.attributes as Record<string, string[] | undefined>)?.line, SVG_COMMON_ATTRS),
    polyline: mergeList((defaultSchema.attributes as Record<string, string[] | undefined>)?.polyline, SVG_COMMON_ATTRS),
    polygon: mergeList((defaultSchema.attributes as Record<string, string[] | undefined>)?.polygon, SVG_COMMON_ATTRS),
    ellipse: mergeList((defaultSchema.attributes as Record<string, string[] | undefined>)?.ellipse, SVG_COMMON_ATTRS),
    defs: mergeList((defaultSchema.attributes as Record<string, string[] | undefined>)?.defs, SVG_COMMON_ATTRS),
    linearGradient: mergeList(
      (defaultSchema.attributes as Record<string, string[] | undefined>)?.linearGradient,
      SVG_COMMON_ATTRS,
    ),
    radialGradient: mergeList(
      (defaultSchema.attributes as Record<string, string[] | undefined>)?.radialGradient,
      SVG_COMMON_ATTRS,
    ),
    stop: mergeList((defaultSchema.attributes as Record<string, string[] | undefined>)?.stop, SVG_COMMON_ATTRS),
    clipPath: mergeList((defaultSchema.attributes as Record<string, string[] | undefined>)?.clipPath, SVG_COMMON_ATTRS),
    mask: mergeList((defaultSchema.attributes as Record<string, string[] | undefined>)?.mask, SVG_COMMON_ATTRS),
    pattern: mergeList((defaultSchema.attributes as Record<string, string[] | undefined>)?.pattern, SVG_COMMON_ATTRS),
    symbol: mergeList((defaultSchema.attributes as Record<string, string[] | undefined>)?.symbol, SVG_COMMON_ATTRS),
    use: mergeList((defaultSchema.attributes as Record<string, string[] | undefined>)?.use, SVG_COMMON_ATTRS),
    image: mergeList((defaultSchema.attributes as Record<string, string[] | undefined>)?.image, SVG_COMMON_ATTRS),
    text: mergeList((defaultSchema.attributes as Record<string, string[] | undefined>)?.text, SVG_COMMON_ATTRS),
    tspan: mergeList((defaultSchema.attributes as Record<string, string[] | undefined>)?.tspan, SVG_COMMON_ATTRS),
  },
  protocols: {
    ...(defaultSchema.protocols ?? {}),
    src: mergeList((defaultSchema.protocols as Record<string, string[] | undefined>)?.src, ["http", "https", "data"]),
    href: mergeList((defaultSchema.protocols as Record<string, string[] | undefined>)?.href, ["http", "https", "data"]),
    "xlink:href": mergeList((defaultSchema.protocols as Record<string, string[] | undefined>)?.["xlink:href"], [
      "http",
      "https",
      "data",
    ]),
  },
};

const REHYPE_PLUGINS = [
  rehypeRaw,
  [rehypeSanitize, SANITIZE_SCHEMA] as [typeof rehypeSanitize, typeof SANITIZE_SCHEMA],
];

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
export const Markdown = memo(
  function Markdown({ children, className, fallback, streaming }: MarkdownProps) {
    return (
      <div className={cn("markdown-content prose prose-sm dark:prose-invert max-w-none", className)}>
        {streaming ? (
          <p className="text-body whitespace-pre-wrap">{children}</p>
        ) : (
          <Suspense fallback={fallback ?? <p className="text-body whitespace-pre-wrap">{children}</p>}>
            <ReactMarkdown
              remarkPlugins={REMARK_PLUGINS}
              rehypePlugins={REHYPE_PLUGINS}
              components={markdownComponents}
            >
              {children}
            </ReactMarkdown>
          </Suspense>
        )}
      </div>
    );
  },
  (prev, next) =>
    prev.children === next.children && prev.streaming === next.streaming && prev.className === next.className,
);
