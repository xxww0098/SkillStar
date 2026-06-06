//! SkillStar Fingerprint — TLS / HTTP-protocol identity emulation.
//!
//! This crate provides a layered fingerprint abstraction so that account
//! credentials, OAuth flows, and quota fetchers can all present a coherent
//! "device identity" to upstream services.
//!
//! ## Layers
//!
//! 1. **HTTP layer** — User-Agent, Accept-Language, Sec-CH-* hints, custom headers
//! 2. **TLS layer** — ClientHello (JA3/JA4), cipher suite order, extensions order
//! 3. **HTTP/2 layer** — SETTINGS frame, priority frames, header table size
//! 4. **Network layer** — proxy, DNS-over-HTTPS, egress region claim
//!
//! Layers 2–3 are powered by [`wreq`](https://github.com/0x676e67/wreq)
//! behind the `impersonate` feature flag. Without that feature, only the
//! HTTP layer is configurable (via `reqwest`).
//!
//! ## Quick start
//!
//! Because `reqwest::Client` and `wreq::Client` have different request-builder
//! types, fetchers should match on [`FingerprintAwareClient`] and write
//! backend-specific code for the few lines that actually send the request.
//!
//! ```no_run
//! use skillstar_fingerprint::{build_client, DeviceFingerprint, FingerprintAwareClient};
//!
//! # async fn run() -> anyhow::Result<()> {
//! let fp = DeviceFingerprint::generate_chrome();
//! let client = build_client(&fp)?;
//! let body = match &client {
//!     FingerprintAwareClient::Reqwest(c) => {
//!         c.get("https://tls.peet.ws/api/all").send().await?.text().await?
//!     }
//!     #[cfg(feature = "impersonate")]
//!     FingerprintAwareClient::Wreq(c) => {
//!         c.get("https://tls.peet.ws/api/all").send().await?.text().await?
//!     }
//! };
//! println!("{body}");
//! # Ok(())
//! # }
//! ```

pub mod client;
pub mod http_profile;
pub mod ide_projector;
pub mod preset;
pub mod request;
pub mod store;
pub mod telemetry;
pub mod tls_profile;
pub mod types;

pub use client::{build_client, build_client_with_timeout, FingerprintAwareClient};
pub use http_profile::HttpProfile;
pub use ide_projector::{IdeProjector, IdeStatus, ProjectorError, SupportedIde, VsCodeForkProjector};
pub use preset::{all_presets, instantiate, PresetId, PresetTemplate};
pub use request::{Method, Req, RequestError, Resp};
pub use store::{FingerprintStore, StoreError};
pub use telemetry::IdeTelemetry;
pub use tls_profile::{Http2Profile, TlsProfile};
pub use types::{DeviceFingerprint, FingerprintSource, NetworkProfile};
