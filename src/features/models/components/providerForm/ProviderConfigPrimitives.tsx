import type { ReactNode } from "react";
import { RadioGroup } from "radix-ui";
import { InfoTip } from "../../../../components/ui/InfoTip";
import { cn } from "../../../../lib/utils";

/**
 * Models owns a small, feature-local form language. Keeping the classes here
 * prevents the provider drawer, creation flow and Agent settings from slowly
 * drifting into three visually different products.
 */
export const modelControlSurfaceClass =
  "border border-input-border bg-input text-foreground shadow-sm backdrop-blur-sm transition duration-200 enabled:hover:border-primary/30 focus-visible:border-primary/60 focus-visible:outline-none focus-visible:ring-2 focus-visible:ring-primary/25 disabled:cursor-not-allowed disabled:bg-muted/30 disabled:opacity-60 aria-invalid:border-destructive/60 aria-invalid:ring-2 aria-invalid:ring-destructive/20";

export const modelInputClass = cn(modelControlSurfaceClass, "h-10 rounded-[10px] px-3 text-[13px]");

export const modelCompactInputClass = cn(modelControlSurfaceClass, "h-9 rounded-[9px] px-3 text-xs");

export const modelTextareaClass = cn(
  modelControlSurfaceClass,
  "min-h-24 w-full resize-y rounded-[10px] px-3 py-2.5 text-[13px] leading-5",
);

export const modelPopoverTriggerClass = cn(
  modelControlSurfaceClass,
  "flex h-10 w-full items-center gap-2 rounded-[10px] px-3 text-left text-[13px]",
);

export const modelCompactPopoverTriggerClass = cn(
  modelControlSurfaceClass,
  "flex h-9 w-full items-center gap-2 rounded-[9px] px-2.5 text-left text-xs",
);

export const modelPopoverContentClass =
  "z-[95] rounded-xl border border-border/65 bg-card/95 p-1.5 shadow-[0_24px_64px_-28px_var(--color-shadow)] backdrop-blur-2xl";

export const modelOptionClass =
  "flex w-full cursor-pointer items-center gap-2 rounded-lg px-2.5 py-2 text-left text-xs outline-none transition duration-150 hover:bg-primary/10 focus-visible:bg-primary/10 focus-visible:outline-none";

export const providerCardClass =
  "rounded-2xl border border-border/55 bg-background/30 shadow-[0_12px_32px_-28px_var(--color-shadow)] backdrop-blur-sm";

export const fieldLabelClass = "text-[11px] font-semibold tracking-[0.01em] text-foreground/80";
export const fieldHintClass = "text-[11px] leading-5 text-muted-foreground/80";

interface ModelFormFieldProps {
  id?: string;
  label: ReactNode;
  hint?: ReactNode;
  info?: string;
  error?: ReactNode;
  required?: boolean;
  action?: ReactNode;
  className?: string;
  children: ReactNode;
}

export function ModelFormField({
  id,
  label,
  hint,
  info,
  error,
  required,
  action,
  className,
  children,
}: ModelFormFieldProps) {
  return (
    <div className={cn("grid gap-1.5", className)}>
      <div className="flex min-h-4 items-center justify-between gap-3">
        <div className="flex min-w-0 items-center gap-1.5">
          <label htmlFor={id} className={fieldLabelClass}>
            {label}
            {required ? <span className="ml-0.5 text-destructive">*</span> : null}
          </label>
          {info ? <InfoTip content={info} /> : null}
        </div>
        {action ? <div className="shrink-0">{action}</div> : null}
      </div>
      {children}
      {hint ? (
        <p id={id ? `${id}-hint` : undefined} className={fieldHintClass}>
          {hint}
        </p>
      ) : null}
      {error ? (
        <p id={id ? `${id}-error` : undefined} className="text-[11px] leading-5 text-destructive" role="alert">
          {error}
        </p>
      ) : null}
    </div>
  );
}

interface ModelFormSectionProps {
  title: ReactNode;
  description?: ReactNode;
  icon?: ReactNode;
  action?: ReactNode;
  className?: string;
  contentClassName?: string;
  children: ReactNode;
}

export function ModelFormSection({
  title,
  description,
  icon,
  action,
  className,
  contentClassName,
  children,
}: ModelFormSectionProps) {
  return (
    <section className={cn(providerCardClass, "p-4", className)}>
      <header className="mb-3.5 flex items-start gap-2.5">
        {icon ? (
          <span className="flex h-8 w-8 shrink-0 items-center justify-center rounded-[10px] border border-border/55 bg-background/70 text-primary">
            {icon}
          </span>
        ) : null}
        <div className="min-w-0 flex-1">
          <h3 className="text-[13px] font-semibold leading-5 text-foreground">{title}</h3>
          {description ? <p className="mt-0.5 text-[11px] leading-4 text-muted-foreground">{description}</p> : null}
        </div>
        {action ? <div className="shrink-0">{action}</div> : null}
      </header>
      <div className={cn("grid gap-3.5", contentClassName)}>{children}</div>
    </section>
  );
}

interface ModelSegmentedControlProps<T extends string> {
  value: T;
  options: { value: T; label: ReactNode }[];
  onChange: (value: T) => void;
  ariaLabel: string;
  disabled?: boolean;
  className?: string;
}

export function ModelSegmentedControl<T extends string>({
  value,
  options,
  onChange,
  ariaLabel,
  disabled,
  className,
}: ModelSegmentedControlProps<T>) {
  return (
    <RadioGroup.Root
      value={value}
      onValueChange={(next) => onChange(next as T)}
      disabled={disabled}
      aria-label={ariaLabel}
      className={cn("flex min-h-10 gap-1 rounded-[10px] border border-border/55 bg-muted/35 p-1", className)}
    >
      {options.map((option) => {
        const selected = value === option.value;
        return (
          <RadioGroup.Item
            key={option.value}
            value={option.value}
            className={cn(
              "min-w-0 flex-1 rounded-[7px] px-2.5 py-1.5 text-[11px] font-semibold transition duration-200 focus-visible:outline-none focus-visible:ring-2 focus-visible:ring-primary/35",
              selected
                ? "bg-background text-foreground shadow-sm ring-1 ring-border/60"
                : "text-muted-foreground hover:bg-background/40 hover:text-foreground",
              disabled && "cursor-not-allowed opacity-50",
            )}
          >
            {option.label}
          </RadioGroup.Item>
        );
      })}
    </RadioGroup.Root>
  );
}
