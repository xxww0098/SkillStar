import { toast as sonner } from "sonner";
import { createElement } from "react";
import { SuccessCheckmark } from "../components/ui/SuccessCheckmark";

export const toast = {
  success: (message: string) =>
    sonner.success(message, {
      icon: createElement(SuccessCheckmark, { size: 18, className: "text-success" }),
    }),
  error: (message: string) => sonner.error(message),
  info: (message: string) => sonner(message),
  warning: (message: string) => sonner.warning(message),
};
