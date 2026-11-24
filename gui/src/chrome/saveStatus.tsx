import { Text, View, signal, type JSX } from "nsh";
import { paletteToken } from "../services/themeMode";
import { t } from "../services/translation";

// Row-level save state for optimistic edits. The retry closure stays in a
// map so the signal only carries the phase the row paints.

const SAVED_MS = 1600;

export interface SaveMark {
  phase: "saving" | "saved" | "failed";
  reason?: string;
}

const marks = signal<Record<string, SaveMark>>({});
const retries = new Map<string, () => void>();
const timers = new Map<string, ReturnType<typeof setTimeout>>();

function clearTimer(id: string): void {
  const pending = timers.get(id);
  if (pending === undefined) return;
  clearTimeout(pending);
  timers.delete(id);
}

function write(id: string, mark: SaveMark | null): void {
  const next = { ...marks() };
  if (mark) next[id] = mark;
  else delete next[id];
  marks.set(next);
}

export function beginSave(id: string): void {
  clearTimer(id);
  retries.delete(id);
  write(id, { phase: "saving" });
}

export function finishSave(id: string): void {
  clearTimer(id);
  retries.delete(id);
  write(id, { phase: "saved" });
  timers.set(
    id,
    setTimeout(() => {
      timers.delete(id);
      if (marks()[id]?.phase === "saved") write(id, null);
    }, SAVED_MS),
  );
}

export function failSave(id: string, reason: string, retry?: () => void): void {
  clearTimer(id);
  if (retry) retries.set(id, retry);
  else retries.delete(id);
  write(id, { phase: "failed", reason });
}

export function saveMark(id: string): SaveMark | undefined {
  return marks()[id];
}

export function retrySave(id: string): void {
  retries.get(id)?.();
}

export function hasRetry(id: string): boolean {
  return retries.has(id);
}

/** 12px line under the control that changed. Reads `saveMark` in JSX. */
export function SaveStatus(props: { id: string }): JSX.Element {
  return (
    <If when={saveMark(props.id) !== undefined}>
      <View
        style={{
          display: "flex",
          flexDirection: "row",
          alignItems: "center",
          gap: 8,
          flexWrap: "wrap",
        }}
      >
        <Text
          style={{
            fontSize: 12,
            color:
              saveMark(props.id)?.phase === "failed"
                ? paletteToken("danger_fg")
                : paletteToken("fg_muted"),
          }}
        >
          {saveLine(props.id)}
        </Text>
        <If when={saveMark(props.id)?.phase === "failed" && hasRetry(props.id)}>
          <View
            testID={`save-retry-${props.id}`}
            onPress={() => retrySave(props.id)}
            style={{ cursor: "pointer" }}
          >
            <Text style={{ fontSize: 12, color: paletteToken("accent_fg") }}>{t("common.retry")}</Text>
          </View>
        </If>
      </View>
    </If>
  );
}

function saveLine(id: string): string {
  const mark = saveMark(id);
  if (!mark || mark.phase === "saving") return t("wait.saving");
  if (mark.phase === "saved") return t("wait.saved");
  return mark.reason || t("wait.couldntSave");
}
