//! The typed schema of `model_gateway.json`: one document, one owner.
//!
//! [`ModelGatewayDoc`] carries the known top-level keys and the fields
//! shared by `providers` and `groups` rows as typed data. Every key this
//! crate does not interpret — `redact_*`, `vision`, the future `families`
//! section until it is promoted, handwritten keys, row-level `note`,
//! `classifier`, `rules` — rides in `rest`/`extra` and is written back
//! verbatim. There is no `deny_unknown_fields` and nothing is dropped.
//! The top-level `prices` map is typed as [`PriceRow`]s (slice 08): same
//! rules at row level — zero unit prices serialize as key-absent, unknown
//! row keys ride `extra`, and a row whose shape the typed view cannot
//! carry fails the whole open like any other known field.
//!
//! Known fields serialize only when they differ from their default, so
//! `open()` followed by `save()` is an identity on every file whose rows
//! are JSON objects: a provider row never gains `"members": []`, a row
//! without `routing` never gains `null`, an empty document writes as `{}`.
//! Nothing loaded is dropped and nothing absent is fabricated; removing a
//! key ("Smart/Auto means the key is gone") stays a lens-setter concern —
//! `None`/empty is exactly key-absent, which is the handle a setter uses.
//!
//! Open points. With the reader and writer reroutes landed (spec slices
//! 04-05), [`ModelGatewayDoc::open`] and [`ModelGatewayDoc::open_lenient`]
//! are the only places that open `model_gateway.json`: every read goes
//! through `open_lenient` and one of the lens read functions, every write
//! through `open`, a lens write and [`ModelGatewayDoc::save`]. The
//! exemption list is empty
//! and stays empty: no module gets a permanent private door into this
//! file, and no top-level key is exempt from round-trip preservation.
//!
//! Not transactional (decision D7): open, mutate, save is an unsynchronized
//! read-modify-write cycle. Two processes can interleave and lose a write;
//! that was true before this module existed and stays true after it.

use std::collections::BTreeMap;
use std::fs;
use std::io;
use std::path::PathBuf;

use serde::{Deserialize, Serialize};
use serde_json::{Map, Value};
use skillstar_core::infra::fs_ops::atomic_write;

// Slice 03 shipped the container before any reader or writer was rerouted
// onto it. Slice 04 rerouted the readers and slice 05 the writers: the
// accessors below, `open_lenient` and `save` have live callers in this
// crate, so they carry no `#[allow(dead_code)]` here — but the
// `#[path]`-mounted test target (tests/store_doc.rs) compiles this file
// without most of those callers, so the accessors keep their allow for that
// compilation alone (`save` is called by that target directly and needs no
// allow).

/// Why a strict open or a save failed. The file is never modified on any
/// of these paths.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub(crate) enum DocStoreError {
    /// The file exists but its bytes could not be read.
    Read,
    /// The bytes are not a document this schema can hold: unparseable JSON,
    /// a top-level value that is not an object, or a known field whose
    /// shape the typed view cannot carry.
    Parse,
    /// The serialized document could not be replaced on disk.
    Write,
}

impl std::fmt::Display for DocStoreError {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.write_str(match self {
            Self::Read => "doc_read",
            Self::Parse => "doc_parse",
            Self::Write => "doc_write",
        })
    }
}

/// The whole `model_gateway.json` document, typed on the outside.
///
/// `model_efforts` and `visible` have no code writer today; they are
/// carried for the read view only and this container exposes no setter
/// for them.
#[derive(Clone, Debug, Default, PartialEq, Serialize, Deserialize)]
pub(crate) struct ModelGatewayDoc {
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    providers: Vec<OwnerRow>,
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    groups: Vec<OwnerRow>,
    #[serde(default, skip_serializing_if = "BTreeMap::is_empty")]
    model_names: BTreeMap<String, String>,
    #[serde(default, skip_serializing_if = "BTreeMap::is_empty")]
    model_efforts: BTreeMap<String, Vec<String>>,
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    profiles: Vec<ProfileRow>,
    /// User price overrides (slice 08), keyed by exact `"<catalog>/<model>"`
    /// or the `"<catalog>"` wildcard.
    #[serde(default, skip_serializing_if = "BTreeMap::is_empty")]
    prices: BTreeMap<String, PriceRow>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    listen: Option<String>,
    #[serde(default, skip_serializing_if = "BTreeMap::is_empty")]
    visible: BTreeMap<String, Vec<String>>,
    /// Top-level keys this crate does not interpret, written back verbatim.
    #[serde(flatten)]
    rest: BTreeMap<String, Value>,
}

/// The row shape `providers` and `groups` share.
///
/// Unknown keys inside a row (`note`, `auto`, `classifier`, `rules`, future
/// family extensions) ride in `extra` and are written back as they were.
/// Row lookup is first-match-by-id for callers; the container itself keeps
/// duplicate-id rows and rows without an `id` exactly as they are, in file
/// order, without deduplicating or dropping.
#[derive(Clone, Debug, Default, PartialEq, Serialize, Deserialize)]
pub(crate) struct OwnerRow {
    #[serde(default, skip_serializing_if = "String::is_empty")]
    pub id: String,
    /// Only meaningful on groups; providers keep this empty.
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub members: Vec<String>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub routing: Option<String>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub affinity: Option<String>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub family: Option<String>,
    #[serde(flatten)]
    extra: Map<String, Value>,
}

/// One `prices` row: per-million-token unit prices under one override key.
///
/// The map key is the id — an exact `"<catalog>/<model>"` or a `"<catalog>"`
/// wildcard — so the row itself carries only prices. A unit the row leaves
/// out bills zero; an explicit `0` normalizes to key-absent on save while
/// the row stays a valid all-zero price. Unknown row keys (`note`, future
/// scope extensions) ride `extra` and are written back as they were.
#[derive(Clone, Debug, Default, PartialEq, Serialize, Deserialize)]
pub(crate) struct PriceRow {
    #[serde(default, skip_serializing_if = "price_unit_is_zero")]
    pub input: f64,
    #[serde(default, skip_serializing_if = "price_unit_is_zero")]
    pub output: f64,
    #[serde(default, skip_serializing_if = "price_unit_is_zero")]
    pub cache_read: f64,
    #[serde(default, skip_serializing_if = "price_unit_is_zero")]
    pub cache_write: f64,
    /// Reserved for a future multi-currency extension. `None` (key absent)
    /// means USD; no consumer reads it yet.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub currency: Option<String>,
    #[serde(flatten)]
    extra: Map<String, Value>,
}

/// One `profiles` row: a name plus agent selections. Unknown row keys ride
/// in `extra`; whether a save keeps them is the profile writer's rule (it
/// drops them today), not the container's.
#[allow(dead_code)]
#[derive(Clone, Debug, Default, PartialEq, Serialize, Deserialize)]
pub(crate) struct ProfileRow {
    #[serde(default, skip_serializing_if = "String::is_empty")]
    pub name: String,
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub agents: Vec<ProfileAgentRow>,
    #[serde(flatten)]
    extra: Map<String, Value>,
}

/// One agent selection inside a profile row.
#[allow(dead_code)]
#[derive(Clone, Debug, Default, PartialEq, Serialize, Deserialize)]
pub(crate) struct ProfileAgentRow {
    #[serde(default, skip_serializing_if = "String::is_empty")]
    pub id: String,
    #[serde(default, skip_serializing_if = "String::is_empty")]
    pub model_ref: String,
    #[serde(flatten)]
    extra: Map<String, Value>,
}

impl ProfileRow {
    /// A row carrying a name and agent selections; the profile writer's
    /// starting point. Its rebuild never carries `extra`, which is the
    /// documented unknown-field drop.
    #[allow(dead_code)]
    pub(crate) fn new(name: String, agents: Vec<ProfileAgentRow>) -> Self {
        Self {
            name,
            agents,
            ..Self::default()
        }
    }
}

impl ProfileAgentRow {
    /// One selection carrying only `id` and `model_ref`.
    #[allow(dead_code)]
    pub(crate) fn new(id: String, model_ref: String) -> Self {
        Self {
            id,
            model_ref,
            ..Self::default()
        }
    }
}

impl ModelGatewayDoc {
    /// The document as it stands, for the write path.
    ///
    /// A missing file is an empty document and is not created. A file that
    /// exists but cannot be held by the schema is an error and its bytes
    /// are left alone.
    pub(crate) fn open() -> Result<Self, DocStoreError> {
        let bytes = match fs::read(gateway_path()) {
            Ok(bytes) => bytes,
            Err(error) if error.kind() == io::ErrorKind::NotFound => {
                return Ok(Self::default());
            }
            Err(_) => return Err(DocStoreError::Read),
        };
        serde_json::from_slice(&bytes).map_err(|_| DocStoreError::Parse)
    }

    /// The document for the read path: a broken file reads as defaults,
    /// matching the legacy `read_doc` semantics.
    pub(crate) fn open_lenient() -> Self {
        Self::open().unwrap_or_default()
    }

    /// Replace the whole file at once: pretty JSON, atomic write.
    pub(crate) fn save(&self) -> Result<(), DocStoreError> {
        let bytes = serde_json::to_vec_pretty(self).map_err(|_| DocStoreError::Write)?;
        atomic_write(&gateway_path(), &bytes).map_err(|_| DocStoreError::Write)
    }

    /// The `listen` field as stored; `None` is a missing or absent field.
    #[allow(dead_code)]
    pub(crate) fn listen(&self) -> Option<&str> {
        self.listen.as_deref()
    }

    /// Set or clear `listen`; `None` is key-absent, which is how
    /// `save_listen("loopback")` removes the field.
    #[allow(dead_code)]
    pub(crate) fn set_listen(&mut self, listen: Option<String>) {
        self.listen = listen;
    }

    /// The `model_names` map, read view.
    #[allow(dead_code)]
    pub(crate) fn model_names(&self) -> &BTreeMap<String, String> {
        &self.model_names
    }

    /// The `model_names` map for the write lens; inserting here is adding
    /// the key, removing here is deleting it.
    #[allow(dead_code)]
    pub(crate) fn model_names_mut(&mut self) -> &mut BTreeMap<String, String> {
        &mut self.model_names
    }

    /// The `providers` rows for the write lens. Row lookup and creation is
    /// the lens's rule (first-match-by-id, new rows appended).
    #[allow(dead_code)]
    pub(crate) fn providers_mut(&mut self) -> &mut Vec<OwnerRow> {
        &mut self.providers
    }

    /// The `groups` rows for the write lens, same rule as
    /// [`Self::providers_mut`].
    #[allow(dead_code)]
    pub(crate) fn groups_mut(&mut self) -> &mut Vec<OwnerRow> {
        &mut self.groups
    }

    /// The `profiles` rows, in file order, read view.
    #[allow(dead_code)]
    pub(crate) fn profiles(&self) -> &[ProfileRow] {
        &self.profiles
    }

    /// Replace the whole `profiles` array; the profile writer rebuilds it
    /// from name and agents only, which is how unknown row fields die
    /// (the behavior-lock test pins that drop).
    #[allow(dead_code)]
    pub(crate) fn set_profiles(&mut self, profiles: Vec<ProfileRow>) {
        self.profiles = profiles;
    }

    /// The `providers` rows, in file order. Row lookup is the caller's rule.
    #[allow(dead_code)]
    pub(crate) fn providers(&self) -> &[OwnerRow] {
        &self.providers
    }

    /// The `groups` rows, in file order. Row lookup is the caller's rule.
    #[allow(dead_code)]
    pub(crate) fn groups(&self) -> &[OwnerRow] {
        &self.groups
    }

    /// The `prices` rows, read view only (no writer today). Lookup — exact
    /// `catalog/model` key first, then the catalog wildcard — is the
    /// caller's rule.
    #[allow(dead_code)]
    pub(crate) fn prices(&self) -> &BTreeMap<String, PriceRow> {
        &self.prices
    }

    /// The `model_efforts` subset map, read view only (no writer today).
    #[allow(dead_code)]
    pub(crate) fn model_efforts(&self) -> &BTreeMap<String, Vec<String>> {
        &self.model_efforts
    }

    /// The agent-to-names `visible` map, read view only (no writer today).
    #[allow(dead_code)]
    pub(crate) fn visible(&self) -> &BTreeMap<String, Vec<String>> {
        &self.visible
    }

    /// Top-level keys this crate does not interpret (`redact_*`, `vision`,
    /// handwritten keys), as stored.
    #[allow(dead_code)]
    pub(crate) fn rest(&self) -> &BTreeMap<String, Value> {
        &self.rest
    }
}

impl OwnerRow {
    /// A row carrying only an id; the write lenses' starting point for a
    /// row the file does not have yet.
    #[allow(dead_code)]
    pub(crate) fn from_id(id: &str) -> Self {
        Self {
            id: id.to_string(),
            ..Self::default()
        }
    }

    /// One unknown key inside this row (`note`, `classifier`, `rules`, …),
    /// exactly as stored. Known-row-schema modules do not go through here.
    #[allow(dead_code)]
    pub(crate) fn extra(&self, key: &str) -> Option<&Value> {
        self.extra.get(key)
    }

    /// Remove one unknown key from this row; the group writer drops a
    /// stale `auto` marker when it saves a real row over a derived one.
    #[allow(dead_code)]
    pub(crate) fn remove_extra(&mut self, key: &str) {
        self.extra.remove(key);
    }
}

/// Whether a unit price serializes as key-absent: zero is the typed
/// default, and defaults are never fabricated on save.
fn price_unit_is_zero(unit: &f64) -> bool {
    *unit == 0.0
}

fn gateway_path() -> PathBuf {
    skillstar_core::infra::paths::config_dir().join("model_gateway.json")
}
