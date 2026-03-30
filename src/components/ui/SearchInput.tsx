import { Search } from "lucide-react";
import { Input } from "./input";
import { cn } from "../../lib/utils";
import React from "react";

export interface SearchInputProps extends React.InputHTMLAttributes<HTMLInputElement> {
  containerClassName?: string;
  iconClassName?: string;
}

export const SearchInput = React.forwardRef<HTMLInputElement, SearchInputProps>(
  ({ className, containerClassName, iconClassName, ...props }, ref) => {
    return (
      <div className={cn("relative flex-1", containerClassName)}>
        <Search 
          className={cn(
            "pointer-events-none absolute left-3 top-1/2 -translate-y-1/2 h-3.5 w-3.5 text-muted-foreground/80",
            iconClassName
          )} 
        />
        <Input 
          ref={ref}
          className={cn("pl-8", className)} 
          {...props} 
        />
      </div>
    );
  }
);
SearchInput.displayName = "SearchInput";
