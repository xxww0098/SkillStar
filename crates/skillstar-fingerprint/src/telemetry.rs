//! IDE-telemetry identity carried by a [`DeviceFingerprint`].
//!
//! When SkillStar applies a fingerprint to a target IDE, it overwrites the
//! VS Code-fork `telemetry.*` fields in that IDE's `storage.json`. The
//! field names below match what Cursor / Windsurf / Kiro / Antigravity all
//! read on startup — applying the same telemetry across siblings makes
//! the machine "look like one device" to every IDE's analytics pipeline.

use serde::{Deserialize, Serialize};
use sha2::{Digest, Sha256};
use uuid::Uuid;

/// VS Code-style machine identity. All fields are optional; an absent
/// value means "leave whatever the IDE already has on disk".
#[derive(Debug, Clone, Default, Serialize, Deserialize, PartialEq, Eq)]
pub struct IdeTelemetry {
    /// `telemetry.machineId` — `auth0|user_<32 hex>` (used by Cursor / etc.).
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub machine_id: Option<String>,

    /// `telemetry.macMachineId` — standard UUIDv4-shape per VS Code's
    /// `vs/base/node/id.ts` (xxxxxxxx-xxxx-4xxx-yxxx-xxxxxxxxxxxx).
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub mac_machine_id: Option<String>,

    /// `telemetry.devDeviceId` — plain UUIDv4.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub dev_device_id: Option<String>,

    /// `telemetry.sqmId` — `{UPPERCASE-UUIDv4}` (braces included).
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub sqm_id: Option<String>,

    /// `storage.serviceMachineId` — used by Antigravity IDE only; UUIDv4.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub service_machine_id: Option<String>,

    /// Custom installation identifier some IDE forks (Kiro, Windsurf) read.
    /// Plain UUIDv4 — stored under `telemetry.installationId` or similar.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub installation_id: Option<String>,
}

impl IdeTelemetry {
    /// Fully-random telemetry. Use this when a *new* fingerprint should
    /// look like a brand-new device.
    pub fn generate() -> Self {
        Self {
            machine_id: Some(format!("auth0|user_{}", random_hex(32))),
            mac_machine_id: Some(new_vscode_machine_id()),
            dev_device_id: Some(Uuid::new_v4().to_string()),
            sqm_id: Some(format!("{{{}}}", Uuid::new_v4().to_string().to_uppercase())),
            service_machine_id: Some(Uuid::new_v4().to_string()),
            installation_id: Some(Uuid::new_v4().to_string()),
        }
    }

    /// Deterministic telemetry derived from a stable id (e.g. the
    /// fingerprint's own `id`). Re-applying the same fingerprint after a
    /// restart yields the same telemetry — so quota refresh, analytics,
    /// etc. see a consistent device.
    ///
    /// We do NOT use this for `mac_machine_id` directly because the
    /// VS Code shape demands a specific layout (`4xxx-yxxx`); for those
    /// fields we fall back to a random value derived from the seed.
    pub fn deterministic_from_id(seed: &str) -> Self {
        let mut hasher = Sha256::new();
        hasher.update(seed.as_bytes());
        let digest = hasher.finalize();
        let hex = hex_encode(&digest);
        // Splice the 64-char hash into UUID-shaped slots.
        let mut rng = SeedlessRng { source: hex };
        Self {
            machine_id: Some(format!("auth0|user_{}", rng.take(32).to_lowercase())),
            mac_machine_id: Some(uuid_v4_from_hex(rng.take(32).as_str())),
            dev_device_id: Some(uuid_v4_from_hex(rng.take(32).as_str())),
            sqm_id: Some(format!(
                "{{{}}}",
                uuid_v4_from_hex(rng.take(32).as_str()).to_uppercase()
            )),
            service_machine_id: Some(uuid_v4_from_hex(rng.take(32).as_str())),
            installation_id: Some(uuid_v4_from_hex(rng.take(32).as_str())),
        }
    }

    /// Merge two telemetries: any `Some` on `other` overrides `self`.
    pub fn merged_with(self, other: &Self) -> Self {
        Self {
            machine_id: other.machine_id.clone().or(self.machine_id),
            mac_machine_id: other.mac_machine_id.clone().or(self.mac_machine_id),
            dev_device_id: other.dev_device_id.clone().or(self.dev_device_id),
            sqm_id: other.sqm_id.clone().or(self.sqm_id),
            service_machine_id: other.service_machine_id.clone().or(self.service_machine_id),
            installation_id: other.installation_id.clone().or(self.installation_id),
        }
    }

    /// True when no field is set — common for the "original" baseline.
    pub fn is_empty(&self) -> bool {
        self.machine_id.is_none()
            && self.mac_machine_id.is_none()
            && self.dev_device_id.is_none()
            && self.sqm_id.is_none()
            && self.service_machine_id.is_none()
            && self.installation_id.is_none()
    }
}

// ── helpers ──────────────────────────────────────────────────────────

fn random_hex(length: usize) -> String {
    // 4 hex chars per random u16 → request `length/4` words, then trim/extend.
    let mut out = String::with_capacity(length);
    while out.len() < length {
        out.push_str(&format!("{:04x}", rand::random::<u16>()));
    }
    out.truncate(length);
    out
}

fn new_vscode_machine_id() -> String {
    let mut id = String::with_capacity(36);
    for ch in "xxxxxxxx-xxxx-4xxx-yxxx-xxxxxxxxxxxx".chars() {
        match ch {
            '-' | '4' => id.push(ch),
            'x' => {
                let nibble = rand::random::<u8>() & 0x0F;
                id.push_str(&format!("{:x}", nibble));
            }
            'y' => {
                let nibble = 8 + (rand::random::<u8>() & 0x03);
                id.push_str(&format!("{:x}", nibble));
            }
            _ => unreachable!(),
        }
    }
    id
}

/// Re-shape an arbitrary 32-hex-char input into a UUIDv4 string layout.
fn uuid_v4_from_hex(hex32: &str) -> String {
    // Pad / truncate to 32 hex chars.
    let h: String = hex32
        .chars()
        .filter(|c| c.is_ascii_hexdigit())
        .take(32)
        .collect();
    let mut chars: Vec<char> = h.chars().collect();
    while chars.len() < 32 {
        chars.push('0');
    }
    // Force version=4 (13th nibble) and variant=8/9/a/b (17th nibble).
    chars[12] = '4';
    chars[16] = match chars[16] {
        '0' | '1' | '2' | '3' => '8',
        '4' | '5' | '6' | '7' => '9',
        '8' | '9' => 'a',
        _ => 'b',
    };
    format!(
        "{}-{}-{}-{}-{}",
        &chars[0..8].iter().collect::<String>(),
        &chars[8..12].iter().collect::<String>(),
        &chars[12..16].iter().collect::<String>(),
        &chars[16..20].iter().collect::<String>(),
        &chars[20..32].iter().collect::<String>(),
    )
}

fn hex_encode(bytes: &[u8]) -> String {
    let mut s = String::with_capacity(bytes.len() * 2);
    for b in bytes {
        s.push_str(&format!("{:02x}", b));
    }
    s
}

/// Cursor over a pre-computed hex string — yields successive chunks without
/// needing real randomness. Used by [`IdeTelemetry::deterministic_from_id`].
struct SeedlessRng {
    source: String,
}

impl SeedlessRng {
    fn take(&mut self, n: usize) -> String {
        // If we exhaust the source, re-hash to extend it.
        while self.source.len() < n {
            let mut h = Sha256::new();
            h.update(self.source.as_bytes());
            self.source.push_str(&hex_encode(&h.finalize()));
        }
        let out: String = self.source.chars().take(n).collect();
        self.source.drain(..n);
        out
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn generate_produces_uuid_shaped_fields() {
        let t = IdeTelemetry::generate();
        assert!(t.machine_id.unwrap().starts_with("auth0|user_"));
        let mac = t.mac_machine_id.unwrap();
        assert_eq!(mac.len(), 36, "macMachineId must be UUID-shaped");
        assert!(mac.chars().nth(14).unwrap() == '4', "version nibble = 4");
    }

    #[test]
    fn deterministic_is_stable() {
        let a = IdeTelemetry::deterministic_from_id("fp-123");
        let b = IdeTelemetry::deterministic_from_id("fp-123");
        assert_eq!(a, b);
        let c = IdeTelemetry::deterministic_from_id("fp-other");
        assert_ne!(a, c);
    }

    #[test]
    fn merge_other_overrides() {
        let a = IdeTelemetry {
            machine_id: Some("a".into()),
            mac_machine_id: Some("a-mac".into()),
            ..Default::default()
        };
        let b = IdeTelemetry {
            machine_id: Some("b".into()),
            ..Default::default()
        };
        let merged = a.merged_with(&b);
        assert_eq!(merged.machine_id.as_deref(), Some("b"));
        assert_eq!(merged.mac_machine_id.as_deref(), Some("a-mac"));
    }
}
