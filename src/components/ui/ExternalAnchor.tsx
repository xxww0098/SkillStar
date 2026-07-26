import type { AnchorHTMLAttributes, MouseEvent } from "react";
import { handleExternalAnchorClick } from "../../lib/externalOpen";

type ExternalAnchorProps = Omit<AnchorHTMLAttributes<HTMLAnchorElement>, "href"> & {
  href: string;
};

/**
 * Anchor that always opens http(s) URLs in the **system browser** via
 * `open_external_url`, never inside the Tauri webview.
 *
 * `target` / `rel` from callers are ignored so we control the open path;
 * we still set `rel` + `target="_blank"` on the DOM as progressive
 * enhancement for middle-click / no-JS edge cases.
 */
export function ExternalAnchor({ href, onClick, rel: _rel, target: _target, ...props }: ExternalAnchorProps) {
  const handleClick = (event: MouseEvent<HTMLAnchorElement>) => {
    onClick?.(event);
    if (event.defaultPrevented) return;
    handleExternalAnchorClick(event, href);
  };

  return (
    <a
      {...props}
      href={href}
      target="_blank"
      rel="noopener noreferrer"
      onClick={handleClick}
      onAuxClick={handleClick}
    />
  );
}
