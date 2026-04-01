import { Toaster as Sonner } from "sonner";

export function Toaster() {
  return (
    <Sonner
      position="bottom-right"
      toastOptions={{
        style: {
          background: "var(--color-card)",
          backdropFilter: "blur(20px)",
          color: "var(--color-foreground)",
          border: "1px solid var(--color-border)",
          borderRadius: "var(--radius-xl)",
          fontSize: "0.875rem",
          padding: "12px 16px",
          boxShadow: "0 8px 32px var(--color-shadow)",
        },
        classNames: {
          toast: "shadow-lg",
          title: "font-medium",
          description: "text-muted-foreground",
          error: "!border-destructive/30 !bg-destructive/10",
          success: "!border-success/30 !bg-success/10",
          warning: "!border-warning/30 !bg-warning/10",
        },
      }}
    />
  );
}
