import { Search } from "lucide-react";
import React from "react";
import { cn } from "../../lib/utils";
import { Input } from "./input";

export interface SearchInputProps extends React.InputHTMLAttributes<HTMLInputElement> {
  containerClassName?: string;
  iconClassName?: string;
  /** Optional element rendered at the right edge inside the input container */
  suffix?: React.ReactNode;
}

export const SearchInput = React.forwardRef<HTMLInputElement, SearchInputProps>(
  ({ className, containerClassName, iconClassName, suffix, ...props }, ref) => {
    return (
      <div className={cn("relative flex-1", containerClassName)}>
        <Search
          className={cn(
            "pointer-events-none absolute left-3 top-1/2 -translate-y-1/2 h-3.5 w-3.5 text-muted-foreground/80",
            iconClassName,
          )}
        />
        <Input ref={ref} className={cn("pl-8", suffix ? "pr-9" : undefined, className)} {...props} />
        {suffix && <div className="absolute right-1 top-1/2 -translate-y-1/2 flex items-center">{suffix}</div>}
      </div>
    );
  },
);
SearchInput.displayName = "SearchInput";
