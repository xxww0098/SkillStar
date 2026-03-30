import { useTranslation } from "react-i18next";
import { CheckSquare, Square } from "lucide-react";
import { Button, ButtonProps } from "./button";
import { cn } from "../../lib/utils";

interface SelectAllButtonProps extends Omit<ButtonProps, 'onClick'> {
  allSelected: boolean;
  onToggle: () => void;
  showIcon?: boolean;
}

export function SelectAllButton({
  allSelected,
  onToggle,
  showIcon = false,
  className,
  variant = "ghost",
  size = "sm",
  ...props
}: SelectAllButtonProps) {
  const { t } = useTranslation();

  return (
    <Button
      variant={variant}
      size={size}
      onClick={onToggle}
      className={cn("cursor-pointer", className)}
      {...props}
    >
      {showIcon && (
        allSelected ? (
          <Square className="w-3.5 h-3.5 mr-1 shrink-0" />
        ) : (
          <CheckSquare className="w-3.5 h-3.5 mr-1 shrink-0" />
        )
      )}
      {allSelected ? t("common.deselectAll") : t("common.selectAll")}
    </Button>
  );
}
