import { useTranslation } from "react-i18next";
import { cn } from "../../../../../lib/utils";
import { activeEntry } from "../../../lib/toolBinding";
import { AgentToolIcon } from "../../shared/AgentToolIcon";
import { isToolOnOfficial } from "../../../lib/officialProviders";
import type { ModelsHubData } from "./types";
import { ClaudeSurfaceIcon } from "../../shared/ClaudeSurfaceIcon";
import { MATRIX_COLUMNS, type MatrixColumn, type MatrixColumnId } from "./matrixColumns";

/**
 * PROTOTYPE — SVG icon rail.
 * Claude CLI / Desktop use cc-switch badge logic (Terminal / Monitor corner);
 * others use AgentToolIcon. Click toggles selected ↔ unselected.
 */
export function AgentColumnCarousel({ data }: { data: ModelsHubData }) {
  const { t } = useTranslation();
  return (
    <div className="flex flex-wrap items-center gap-2 overflow-visible">
      {MATRIX_COLUMNS.map((column) => {
        const selected = data.visibleColumnIds?.includes(column.columnId) ?? true;
        const entry = activeEntry(data.toolActivations[column.bindToolId]);
        const provider = entry ? (data.providers.find((p) => p.id === entry.provider_id) ?? null) : null;
        const onOfficial = isToolOnOfficial(data.providers, data.toolActivations, column.bindToolId);
        const title = [
          column.displayName,
          selected ? t("models.matrix.columnShown") : t("models.matrix.columnHidden"),
          onOfficial
            ? t("models.matrix.officialNativeLogin")
            : provider
              ? `${provider.name}${entry?.model ? ` · ${entry.model}` : ""}`
              : "idle",
        ].join(" · ");

        return (
          <CarouselIcon
            key={column.columnId}
            column={column}
            selected={selected}
            title={title}
            onToggle={() => data.toggleVisibleColumn(column.columnId)}
          />
        );
      })}
    </div>
  );
}

function CarouselIcon({
  column,
  selected,
  title,
  onToggle,
}: {
  column: MatrixColumn;
  selected: boolean;
  title: string;
  onToggle: () => void;
}) {
  return (
    <button
      type="button"
      onClick={onToggle}
      aria-label={title}
      aria-pressed={selected}
      title={title}
      className={cn(
        "relative flex h-10 w-10 shrink-0 cursor-pointer items-center justify-center overflow-visible rounded-xl border transition-[background-color,border-color,box-shadow,transform,opacity] duration-200",
        "active:scale-[0.96] focus-visible:outline-none focus-visible:ring-2 focus-visible:ring-primary/45 focus-visible:ring-offset-1 focus-visible:ring-offset-background",
        selected
          ? "border-primary/50 bg-primary/12 shadow-[0_0_0_1px_rgba(var(--color-primary-rgb),0.22),0_0_14px_rgba(var(--color-primary-rgb),0.28)]"
          : "border-transparent bg-transparent opacity-55 hover:opacity-85 hover:bg-muted/50",
      )}
    >
      {column.claudeSurface ? (
        <ClaudeSurfaceIcon surface={column.claudeSurface} size={22} muted={!selected} />
      ) : (
        <AgentToolIcon
          toolId={column.bindToolId}
          size="md"
          className={cn(
            "transition-[filter,opacity]",
            !selected && "grayscale",
            "[&>span]:border-0 [&>span]:bg-transparent",
          )}
        />
      )}
    </button>
  );
}

/** Columns currently selected in the carousel, in canonical order. */
export function visibleColumns(data: ModelsHubData): MatrixColumn[] {
  const ids = data.visibleColumnIds ?? MATRIX_COLUMNS.map((c) => c.columnId);
  const set = new Set<MatrixColumnId>(ids);
  return MATRIX_COLUMNS.filter((c) => set.has(c.columnId));
}
