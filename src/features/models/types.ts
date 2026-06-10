/** Domain types shared across the models feature. */

/**
 * Auto-save lifecycle state surfaced by the provider editor drawer.
 *
 * - `idle`: form mirrors persisted state; nothing in flight.
 * - `dirty`: user has typed but the debounce hasn't fired yet.
 * - `saving`: a save is in flight.
 * - `saved`: most recent save succeeded.
 * - `error`: most recent save (or validation) failed.
 */
export type ProviderSaveState = "idle" | "dirty" | "saving" | "saved" | "error";

/** Result of one autosave attempt. `validation` means nothing was sent. */
export type SaveAttemptResult = "saved" | "validation" | "error";

/** Tabs of the provider editor drawer. */
export type ProviderEditorTab = "connection" | "models" | "advanced" | "diagnostics";
