/**
 * Phase 5 — frontend module for the fingerprint store backed by the
 * Phase 4 Tauri commands. Public exports:
 *
 *  - {@link FingerprintsPanel}    — drop-in Settings section
 *  - {@link FingerprintPicker}    — pill row for Subscription dialogs
 *  - {@link useFingerprints}       — shared store/CRUD hook
 *  - {@link fingerprintsApi}      — raw typed invoke wrappers
 */
export { FingerprintsPanel } from "./components/FingerprintsPanel";
export { FingerprintPicker } from "./components/FingerprintPicker";
export { CreateFromPresetDialog } from "./components/CreateFromPresetDialog";
export { EditFingerprintDialog } from "./components/EditFingerprintDialog";
export { FingerprintCard } from "./components/FingerprintCard";
export { IdeProjectorsPanel } from "./components/IdeProjectorsPanel";
export { useFingerprints } from "./hooks/useFingerprints";
export { fingerprintsApi } from "./api";
export { tlsLabel } from "./utils";
export type {
  DeviceFingerprint,
  FingerprintListDto,
  FingerprintRow,
  FingerprintSource,
  FingerprintSourceKind,
  Http2Profile,
  HttpProfile,
  IdeTelemetry,
  NetworkProfile,
  PresetId,
  PresetTemplate,
  SupportedIde,
  TlsProfile,
  TlsProfileKind,
  UpdateFingerprintInput,
} from "./types";
