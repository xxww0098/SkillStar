//! IDE adapters write their own live store, read it back, then pin.
//! This registry is independent from desktop-instance launch support.

use crate::UsageResult;
use crate::subscription::Subscription;

use super::{CliAccountState, SwitchOutcome};

pub(super) trait IdeCredentialAdapter: Send + Sync {
    fn catalog_id(&self) -> &'static str;

    /// The live store can be addressed. A missing login is still available:
    /// reconcile reports [`CliAccountState::Missing`] rather than omitting the
    /// catalog. Tests address stores through `SKILLSTAR_TOOL_SYNC_HOME`.
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
    &super::windsurf::Adapter,
    &super::zcode::Adapter,
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
