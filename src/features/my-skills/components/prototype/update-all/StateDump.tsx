type StateDumpProps = {
  state: Record<string, unknown>;
};

/** Surfaces full relevant prototype state after every action / variant switch. */
export function StateDump({ state }: StateDumpProps) {
  return (
    <pre className="mt-3 max-h-36 overflow-auto rounded-lg border border-dashed border-amber-500/40 bg-amber-500/[0.06] p-3 font-mono text-[10px] leading-relaxed text-amber-900/80 dark:text-amber-200/90">
      {JSON.stringify(state, null, 2)}
    </pre>
  );
}
