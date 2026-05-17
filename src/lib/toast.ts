import { createElement } from "react";
import { toast as sonner } from "sonner";
import { SuccessCheckmark } from "../components/ui/SuccessCheckmark";

export const toast = {
  success: (message: string) =>
    sonner.success(message, {
      icon: createElement(SuccessCheckmark, { size: 18, className: "text-success" }),
    }),
  error: (message: string) => sonner.error(message, { duration: 6000 }),
  info: (message: string) => sonner(message),
  warning: (message: string) => sonner.warning(message, { duration: 4000 }),
  /** Show or update a progress toast by ID (stays until dismissed). */
  loading: (message: string, id: string) => sonner.loading(message, { id, duration: Infinity }),
  /** Dismiss a toast by ID. */
  dismiss: (id: string) => sonner.dismiss(id),
};
