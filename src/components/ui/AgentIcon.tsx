import { useState, useEffect } from "react";
import type { AgentProfile } from "../../types";
import { AntigravityIcon } from "./icons/AntigravityIcon";

interface AgentIconProps {
  profile: AgentProfile;
  className?: string;
  alt?: string;
}

/**
 * Cache fetched SVG markup so each icon is only fetched once per session.
 * This survives component unmount/remount.
 */
const svgCache = new Map<string, string>();

/**
 * Render an agent icon.
 *
 * SVG icons are fetched and inlined so that CSS filters like `grayscale`
 * work reliably across platforms (Windows WebView2 and macOS WebKit both
 * have rendering bugs when CSS filters are applied to `<img src="*.svg">`).
 *
 * PNG / data-URI icons continue to use `<img>` which handles filters fine.
 */
export function AgentIcon({ profile, className, alt }: AgentIconProps) {
  if (profile.id === "antigravity") {
    return <AntigravityIcon className={className} />;
  }

  const isSvg =
    profile.icon.endsWith(".svg") && !profile.icon.startsWith("data:image");

  if (!isSvg) {
    const imgSrc = profile.icon.startsWith("data:image")
      ? profile.icon
      : `/${profile.icon}`;
    return (
      <img
        src={imgSrc}
        alt={alt ?? profile.display_name}
        className={className}
        loading="lazy"
        decoding="async"
      />
    );
  }

  // Inline SVG path for reliable CSS filter support
  return <InlineSvgIcon path={profile.icon} className={className} />;
}

// ── Private inline-SVG loader ────────────────────────────────────────

function InlineSvgIcon({
  path,
  className,
}: {
  path: string;
  className?: string;
}) {
  const [markup, setMarkup] = useState<string | null>(
    () => svgCache.get(path) ?? null,
  );

  useEffect(() => {
    if (svgCache.has(path)) {
      setMarkup(svgCache.get(path)!);
      return;
    }

    let cancelled = false;
    const src = path.startsWith("/") ? path : `/${path}`;
    fetch(src)
      .then((res) => (res.ok ? res.text() : Promise.reject(res.statusText)))
      .then((text) => {
        svgCache.set(path, text);
        if (!cancelled) setMarkup(text);
      })
      .catch((err) => {
        console.warn(`[AgentIcon] failed to inline SVG "${path}":`, err);
      });

    return () => {
      cancelled = true;
    };
  }, [path]);

  if (!markup) {
    // Fallback to <img> while loading (usually instant from cache or local files)
    return (
      <img
        src={path.startsWith("/") ? path : `/${path}`}
        alt=""
        className={className}
        loading="lazy"
        decoding="async"
      />
    );
  }

  return (
    <span
      dangerouslySetInnerHTML={{ __html: markup }}
      className={`inline-flex items-center justify-center shrink-0 [&>svg]:w-[inherit] [&>svg]:h-[inherit] [&>svg]:block ${className ?? ""}`}
    />
  );
}
