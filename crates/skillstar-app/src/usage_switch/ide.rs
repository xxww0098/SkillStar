//! IDE credential adapters.
//!
//! Antigravity, Cursor, Windsurf, Kiro, Qoder, CodeBuddy, Trae, and Zed do not
//! fit the CLI symlink model. Each adapter writes its own live store, reads it
//! back, then pins. The registry is the only switch path that knows those
//! catalogs. Zed's store is the macOS keychain and is unavailable off macOS
//! or while tool-sync is sandboxed.

use skillstar_usage::UsageResult;
use skillstar_usage::subscription::Subscription;

use super::{CliAccountState, SwitchOutcome};

pub(super) trait IdeCredentialAdapter: Send + Sync {
    fn catalog_id(&self) -> &'static str;

    /// The live store can be addressed. A missing login is still available:
    /// reconcile reports [`CliAccountState::Missing`] rather than omitting the
    /// catalog. Zed returns false off macOS and while
    /// `SKILLSTAR_TOOL_SYNC_HOME` is set.
    fn available(&self) -> bool;

    fn activate(&self, sub_id: &str) -> UsageResult<(Subscription, SwitchOutcome)>;

    fn sync(&self, sub: &Subscription) -> UsageResult<SwitchOutcome>;

    fn reconcile(&self) -> UsageResult<Option<CliAccountState>>;

    fn adopt_before_refresh(&self, sub: &mut Subscription) -> UsageResult<()>;

    fn forget(&self, sub_id: &str) -> UsageResult<()>;
}

const IDE_ADAPTERS: &[&'static dyn IdeCredentialAdapter] = &[
    &super::antigravity::Adapter,
    &super::cursor::Adapter,
    &super::kiro::Adapter,
    &super::qoder::Adapter,
    &super::windsurf::Adapter,
    &super::codebuddy::GLOBAL_ADAPTER,
    &super::codebuddy::CN_ADAPTER,
    &super::trae::TRAE,
    &super::trae::SOLO,
    &super::trae::CN,
    &super::trae::SOLO_CN,
    &super::zed::Adapter,
];

pub(super) fn adapters() -> &'static [&'static dyn IdeCredentialAdapter] {
    IDE_ADAPTERS
}

pub(super) fn ide_adapter_for(catalog_id: &str) -> Option<&'static dyn IdeCredentialAdapter> {
    IDE_ADAPTERS
        .iter()
        .copied()
        .find(|adapter| adapter.catalog_id() == catalog_id)
}
