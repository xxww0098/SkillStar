import ReactMarkdown from "react-markdown";
import rehypeRaw from "rehype-raw";
import rehypeSanitize, { defaultSchema } from "rehype-sanitize";
import remarkGfm from "remark-gfm";
import { markdownComponents } from "../../lib/markdown";

// This module bundles the ENTIRE heavy markdown stack (react-markdown +
// remark/rehype plugins + the sanitize schema). It is loaded lazily by
// `Markdown.tsx` so none of it is pulled into a page's chunk until markdown is
// actually rendered (e.g. opening a skill detail) — keeping page loads light.

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

/** The heavy markdown render itself — see module note above. */
export default function MarkdownRenderer({ children }: { children: string }) {
  return (
    <ReactMarkdown remarkPlugins={REMARK_PLUGINS} rehypePlugins={REHYPE_PLUGINS} components={markdownComponents}>
      {children}
    </ReactMarkdown>
  );
}
