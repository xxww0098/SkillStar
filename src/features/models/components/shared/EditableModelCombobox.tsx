import { Check, ChevronsUpDown } from "lucide-react";
import { Popover } from "radix-ui";
import { useEffect, useId, useMemo, useRef, useState } from "react";
import { Input } from "../../../../components/ui/input";
import { cn } from "../../../../lib/utils";
import { modelInputClass, modelOptionClass, modelPopoverContentClass } from "../providerForm/ProviderConfigPrimitives";

interface EditableModelComboboxProps {
  id?: string;
  value: string;
  options: string[];
  onChange: (value: string) => void;
  placeholder?: string;
  ariaLabel: string;
  disabled?: boolean;
}

/** Themed, editable model id input with suggestions; custom values remain valid. */
export function EditableModelCombobox({
  id,
  value,
  options,
  onChange,
  placeholder,
  ariaLabel,
  disabled,
}: EditableModelComboboxProps) {
  const listboxId = useId();
  const inputRef = useRef<HTMLInputElement>(null);
  const optionRefs = useRef<(HTMLButtonElement | null)[]>([]);
  const [open, setOpen] = useState(false);
  const [showAllSuggestions, setShowAllSuggestions] = useState(false);
  const [activeOption, setActiveOption] = useState<string | null>(null);
  const filtered = useMemo(() => {
    const query = showAllSuggestions ? "" : value.trim().toLowerCase();
    const candidates = query ? options.filter((option) => option.toLowerCase().includes(query)) : options;
    return candidates.slice(0, 80);
  }, [options, showAllSuggestions, value]);

  const hasSuggestions = filtered.length > 0;
  const activeIndex = activeOption ? filtered.indexOf(activeOption) : -1;
  const activeOptionId = activeIndex >= 0 ? `${listboxId}-option-${activeIndex}` : undefined;

  useEffect(() => {
    if (!open || activeIndex < 0) return;
    optionRefs.current[activeIndex]?.scrollIntoView?.({ block: "nearest" });
  }, [activeIndex, open]);

  const selectOption = (option: string) => {
    onChange(option);
    setOpen(false);
    setShowAllSuggestions(false);
    setActiveOption(null);
    inputRef.current?.focus();
  };

  return (
    <Popover.Root
      open={open && hasSuggestions}
      onOpenChange={(next) => {
        setOpen(next);
        if (!next) {
          setActiveOption(null);
          setShowAllSuggestions(false);
        }
      }}
    >
      <Popover.Anchor asChild>
        <div className="relative">
          <Input
            ref={inputRef}
            id={id}
            value={value}
            onChange={(event) => {
              onChange(event.target.value);
              setOpen(true);
              setShowAllSuggestions(false);
              setActiveOption(null);
            }}
            onFocus={() => setOpen(true)}
            onKeyDown={(event) => {
              if (event.key === "ArrowDown" || event.key === "ArrowUp") {
                if (!hasSuggestions) return;
                event.preventDefault();
                setOpen(true);
                const currentIndex = activeOption ? filtered.indexOf(activeOption) : -1;
                const nextIndex =
                  event.key === "ArrowDown"
                    ? currentIndex < filtered.length - 1
                      ? currentIndex + 1
                      : 0
                    : currentIndex > 0
                      ? currentIndex - 1
                      : filtered.length - 1;
                setActiveOption(filtered[nextIndex]);
                return;
              }
              if (event.key === "Enter" && open && activeIndex >= 0) {
                event.preventDefault();
                selectOption(filtered[activeIndex]);
                return;
              }
              if (event.key === "Escape" && open) {
                event.preventDefault();
                setOpen(false);
                setShowAllSuggestions(false);
                setActiveOption(null);
              }
            }}
            placeholder={placeholder}
            disabled={disabled}
            autoComplete="off"
            role="combobox"
            aria-label={ariaLabel}
            aria-autocomplete="list"
            aria-expanded={open && hasSuggestions}
            aria-controls={listboxId}
            aria-activedescendant={activeOptionId}
            className={`${modelInputClass} pr-9 font-mono text-xs`}
          />
          <button
            type="button"
            aria-label={ariaLabel}
            aria-haspopup="listbox"
            aria-expanded={open && hasSuggestions}
            aria-controls={listboxId}
            disabled={disabled || options.length === 0}
            onMouseDown={(event) => event.preventDefault()}
            onClick={() => {
              const next = !(open && hasSuggestions);
              setOpen(next);
              setShowAllSuggestions(next);
              setActiveOption(next ? (filtered.find((option) => option === value) ?? filtered[0] ?? null) : null);
              inputRef.current?.focus();
            }}
            className="absolute right-2.5 top-1/2 -translate-y-1/2 rounded-md p-1 text-muted-foreground transition hover:bg-muted/50 hover:text-foreground disabled:pointer-events-none disabled:opacity-40"
          >
            <ChevronsUpDown className="h-3.5 w-3.5" />
          </button>
        </div>
      </Popover.Anchor>

      <Popover.Portal>
        <Popover.Content
          align="start"
          sideOffset={6}
          onOpenAutoFocus={(event) => event.preventDefault()}
          onCloseAutoFocus={(event) => event.preventDefault()}
          className={cn(modelPopoverContentClass, "w-[var(--radix-popover-anchor-width)] min-w-[280px]")}
        >
          <div id={listboxId} role="listbox" aria-label={ariaLabel} className="max-h-64 overflow-y-auto">
            {filtered.map((option, index) => {
              const selected = option === value;
              const active = index === activeIndex;
              return (
                <button
                  key={option}
                  id={`${listboxId}-option-${index}`}
                  ref={(node) => {
                    optionRefs.current[index] = node;
                  }}
                  type="button"
                  role="option"
                  aria-selected={selected}
                  tabIndex={-1}
                  onMouseDown={(event) => event.preventDefault()}
                  onMouseEnter={() => setActiveOption(option)}
                  onClick={() => selectOption(option)}
                  className={cn(
                    modelOptionClass,
                    active && "bg-muted/55 text-foreground",
                    selected && "bg-primary/10 text-primary",
                  )}
                >
                  <span className="min-w-0 flex-1 truncate font-mono text-[11px]">{option}</span>
                  {selected ? <Check className="h-3.5 w-3.5 shrink-0" /> : null}
                </button>
              );
            })}
          </div>
        </Popover.Content>
      </Popover.Portal>
    </Popover.Root>
  );
}
