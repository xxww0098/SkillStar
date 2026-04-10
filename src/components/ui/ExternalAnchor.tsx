import type { AnchorHTMLAttributes, MouseEvent } from "react";
import { handleExternalAnchorClick } from "../../lib/externalOpen";

type ExternalAnchorProps = Omit<AnchorHTMLAttributes<HTMLAnchorElement>, "href"> & {
  href: string;
};

export function ExternalAnchor({ href, onClick, rel: _rel, target: _target, ...props }: ExternalAnchorProps) {
  const handleClick = (event: MouseEvent<HTMLAnchorElement>) => {
    onClick?.(event);
    if (event.defaultPrevented) return;
    handleExternalAnchorClick(event, href);
  };

  return <a {...props} href={href} rel="noopener noreferrer" onClick={handleClick} />;
}
