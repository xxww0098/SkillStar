import type * as React from "react";
import { cn } from "../../lib/utils";
import { Card, CardContent, CardFooter, CardHeader } from "./card";

interface CardTemplateProps extends React.HTMLAttributes<HTMLDivElement> {
  header?: React.ReactNode;
  body?: React.ReactNode;
  footer?: React.ReactNode;
  topLeftSlot?: React.ReactNode;
  topRightSlot?: React.ReactNode;
  bottomLeftSlot?: React.ReactNode;
  bottomRightSlot?: React.ReactNode;
  headerClassName?: string;
  bodyClassName?: string;
  footerClassName?: string;
  selected?: boolean;
}

export function CardTemplate({
  className,
  header,
  body,
  footer,
  children,
  topLeftSlot,
  topRightSlot,
  bottomLeftSlot,
  bottomRightSlot,
  headerClassName,
  bodyClassName,
  footerClassName,
  selected,
  ...props
}: CardTemplateProps) {
  return (
    <Card
      className={cn("relative h-full flex flex-col", selected && "ring-2 ring-primary/40 border-primary/30", className)}
      {...props}
    >
      {topLeftSlot && <div className="absolute top-3 left-3 z-10">{topLeftSlot}</div>}
      {topRightSlot && <div className="absolute top-3 right-3 z-10 flex items-center">{topRightSlot}</div>}
      {bottomLeftSlot && <div className="absolute bottom-3 left-3 z-10">{bottomLeftSlot}</div>}
      {bottomRightSlot && <div className="absolute bottom-3 right-3 z-10">{bottomRightSlot}</div>}

      {header ? <CardHeader className={headerClassName}>{header}</CardHeader> : null}
      {body ? <CardContent className={bodyClassName}>{body}</CardContent> : null}
      {footer ? <CardFooter className={footerClassName}>{footer}</CardFooter> : null}
      {children}
    </Card>
  );
}
