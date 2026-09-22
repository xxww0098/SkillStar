//! Zed quota, native-app RSA login, and token / keychain import.
//!
//! Quota is only `GET https://cloud.zed.dev/client/users/me`.
//! `Authorization` is `{user_id} {access_token}`, not `Bearer`.
//! `/frontend/billing/*` is not called: those routes 401 a valid desktop
//! credential and would be stored as reauth. There is no refresh token.

mod import;
mod login;
mod quota;

use crate::UsageResult;
use crate::subscription::{Subscription, SubscriptionUsage};

pub(crate) const CATALOG_ID: &str = "zed";
pub(super) const CLOUD_BASE: &str = "https://cloud.zed.dev";
pub(super) const USER_PATH: &str = "/client/users/me";
pub(super) const SIGNIN_URL: &str = "https://zed.dev/native_app_signin";
pub(super) const KEYCHAIN_SERVER: &str = "https://zed.dev";
pub(super) const PLACEHOLDER_NAME: &str = "Zed";

pub(crate) use import::{import_from_local, import_from_token, oauth_row_from_imported};
pub(crate) use login::start_login;

pub(crate) async fn fetch(subscription: &mut Subscription) -> UsageResult<SubscriptionUsage> {
    quota::fetch_quota(subscription).await
}
