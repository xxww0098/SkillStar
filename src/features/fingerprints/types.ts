// Mirrors `crates/skillstar-fingerprint/src/types.rs` and
// `src-tauri/src/commands/fingerprints.rs`. Keep in sync.

export type TlsProfileKind = "default" | "chrome" | "safari" | "edge" | "firefox" | "opera" | "ok_http";

export interface TlsProfile {
  kind: TlsProfileKind;
  /** Major version (absent for `default`). */
  major?: number;
}

export interface HttpProfile {
  user_agent: string;
  accept_language: string;
  accept_encoding: string;
  sec_ch_ua?: string;
  sec_ch_ua_platform?: string;
  sec_ch_ua_mobile: boolean;
  extra_headers: Record<string, string>;
}

export interface Http2Profile {
  initial_window_size?: number;
  max_concurrent_streams?: number;
  header_table_size?: number;
  enable_push?: boolean;
}

export interface NetworkProfile {
  proxy_url?: string;
  doh_url?: string;
  egress_country?: string;
}

/** VS Code-style machine identity. Fields are optional; missing fields are
 *  ignored when the projector writes to storage.json. */
export interface IdeTelemetry {
  machine_id?: string;
  mac_machine_id?: string;
  dev_device_id?: string;
  sqm_id?: string;
  service_machine_id?: string;
  installation_id?: string;
}

/** Summary returned by `list_supported_ides`. */
export interface SupportedIde {
  agentId: string;
  displayName: string;
  installed: boolean;
  storagePath?: string;
  hasBaseline: boolean;
  current?: IdeTelemetry;
}

export type FingerprintSourceKind = "original" | "generated_random" | "generated_from_persona" | "imported" | "manual";

export interface FingerprintSource {
  kind: FingerprintSourceKind;
  /** Present only when `kind === "generated_from_persona"`. */
  template_id?: string;
}

export interface DeviceFingerprint {
  id: string;
  name: string;
  source: FingerprintSource;
  created_at: number;
  updated_at: number;
  http: HttpProfile;
  tls: TlsProfile;
  http2: Http2Profile;
  network: NetworkProfile;
  /** IDE telemetry identity applied by `apply_fingerprint_to_ide`.
   *  Absent (or empty) → projector leaves the IDE's existing telemetry alone. */
  telemetry?: IdeTelemetry;
}

/** Backend response: `DeviceFingerprint` flattened with two display flags. */
export interface FingerprintRow extends DeviceFingerprint {
  isActive: boolean;
  isOriginal: boolean;
}

export interface FingerprintListDto {
  items: FingerprintRow[];
  activeId: string | null;
}

export type PresetId =
  | "default"
  | "chrome-mac"
  | "chrome-windows"
  | "safari-mac"
  | "firefox-mac"
  | "edge-mac"
  | "chrome-mac-zh-cn";

export interface PresetTemplate {
  id: PresetId;
  label: string;
  description: string;
  family: string;
}

export interface UpdateFingerprintInput {
  name?: string;
  http?: HttpProfile;
  tls?: TlsProfile;
  network?: NetworkProfile;
}
