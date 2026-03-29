import { Children, type ReactNode } from "react";
import type { Components } from "react-markdown";

function stringifyChildren(children: ReactNode): string {
  return Children.toArray(children)
    .map((child) => (typeof child === "string" || typeof child === "number" ? String(child) : ""))
    .join("");
}

function stripWrappedBackticks(value: string): string {
  if (value.includes("\n")) {
    return value;
  }

  let next = value;
  while (next.length >= 2 && next.startsWith("`") && next.endsWith("`")) {
    next = next.slice(1, -1);
  }
  return next;
}

export const markdownComponents: Components = {
  code({ node: _node, children, className, ...props }) {
    const raw = stringifyChildren(children).replace(/\n$/, "");
    const normalized = stripWrappedBackticks(raw);
    return (
      <code className={className} {...props}>
        {normalized}
      </code>
    );
  },
};
