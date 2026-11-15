//! Receipt commit shared by channel upgrade and historical rollback.
//!
//! Release selection, per-skill results, and pins stay with the caller.
//! Network validation stays outside this module. A failed restore is part of
//! the same error as the step that failed.

use super::{ChannelSkillUpdateReceipt, ChannelSubscriptionUpdater, SharedChannelError};

pub(super) struct VerifiedReceipts {
    pub kept: Vec<ChannelSkillUpdateReceipt>,
    /// Verify failed and this receipt was restored.
    pub reverted: Vec<(ChannelSkillUpdateReceipt, SharedChannelError)>,
}

/// Verify each applied receipt.
///
/// A failed verify restores that receipt and records it so the batch can
/// continue. If that restore fails, every receipt in the batch is restored
/// and the error includes those compensation failures.
pub(super) async fn verify_applied_receipts<I: ChannelSubscriptionUpdater + ?Sized>(
    installer: &I,
    receipts: &[ChannelSkillUpdateReceipt],
) -> Result<VerifiedReceipts, SharedChannelError> {
    let mut kept = Vec::new();
    let mut reverted = Vec::new();
    for receipt in receipts {
        match installer.verify(receipt).await {
            Ok(()) => kept.push(receipt.clone()),
            Err(error) => {
                if let Err(rollback) = installer.rollback(receipt).await {
                    let mut failures = rollback_messages(installer, receipts).await;
                    failures.push(format!("{}: {}", receipt.previous.id, rollback.message));
                    return Err(attach_batch_rollback(error, failures));
                }
                reverted.push((receipt.clone(), error));
            }
        }
    }
    Ok(VerifiedReceipts { kept, reverted })
}

/// The subscription write already ran. Success releases retained copies.
/// Failure restores every receipt still in the batch.
pub(super) async fn commit_verified_receipts<I: ChannelSubscriptionUpdater + ?Sized>(
    installer: &I,
    receipts: &[ChannelSkillUpdateReceipt],
    commit: Result<(), SharedChannelError>,
) -> Result<(), SharedChannelError> {
    if let Err(error) = commit {
        return Err(restore_batch(installer, receipts, error).await);
    }
    for receipt in receipts {
        installer.finalize(receipt).await;
    }
    Ok(())
}

pub(super) async fn verify_historical_receipt<I: ChannelSubscriptionUpdater + ?Sized>(
    installer: &I,
    receipt: &ChannelSkillUpdateReceipt,
) -> Result<(), SharedChannelError> {
    if let Err(error) = installer.verify(receipt).await {
        return Err(restore_historical(installer, receipt, error).await);
    }
    Ok(())
}

/// `commit` is the caller's locked re-verify plus subscription write.
pub(super) async fn commit_historical_receipt<I: ChannelSubscriptionUpdater + ?Sized>(
    installer: &I,
    receipt: &ChannelSkillUpdateReceipt,
    commit: Result<(), SharedChannelError>,
) -> Result<(), SharedChannelError> {
    if let Err(error) = commit {
        return Err(restore_historical(installer, receipt, error).await);
    }
    installer.finalize(receipt).await;
    Ok(())
}

pub(super) async fn restore_batch<I: ChannelSubscriptionUpdater + ?Sized>(
    installer: &I,
    receipts: &[ChannelSkillUpdateReceipt],
    error: SharedChannelError,
) -> SharedChannelError {
    attach_batch_rollback(error, rollback_messages(installer, receipts).await)
}

pub(super) async fn restore_historical<I: ChannelSubscriptionUpdater + ?Sized>(
    installer: &I,
    receipt: &ChannelSkillUpdateReceipt,
    error: SharedChannelError,
) -> SharedChannelError {
    match installer.rollback(receipt).await {
        Ok(()) => error,
        Err(rollback) => SharedChannelError::new(
            error.code,
            format!(
                "{}; the staged historical version could not be rolled back: {}",
                error.message, rollback.message
            ),
        ),
    }
}

async fn rollback_messages<I: ChannelSubscriptionUpdater + ?Sized>(
    installer: &I,
    receipts: &[ChannelSkillUpdateReceipt],
) -> Vec<String> {
    let mut failures = Vec::new();
    for receipt in receipts.iter().rev() {
        if let Err(rollback) = installer.rollback(receipt).await {
            failures.push(format!("{}: {}", receipt.previous.id, rollback.message));
        }
    }
    failures
}

fn attach_batch_rollback(
    original: SharedChannelError,
    failures: Vec<String>,
) -> SharedChannelError {
    if failures.is_empty() {
        original
    } else {
        SharedChannelError::new(
            original.code,
            format!(
                "{}; updated Skills also could not be rolled back: {}",
                original.message,
                failures.join(", ")
            ),
        )
    }
}
