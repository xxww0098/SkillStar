//! Trae product identity shared by the four catalog entries.
//!
//! Directory names match cockpit `TraePlatformKind::app_support_dir_name`
//! (which returns `display_name()`). Region hosts are the static origins from
//! cockpit `candidate_api_origins` plus the account-API origins that list does
//! not already include (`TRAE_ACCOUNT_API_ORIGIN_*` in `trae_oauth.rs`).

use crate::{UsageError, UsageResult};

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum TraePlatformKind {
    Trae,
    TraeSolo,
    TraeCn,
    TraeSoloCn,
}

impl TraePlatformKind {
    pub const ALL: [Self; 4] = [Self::Trae, Self::TraeSolo, Self::TraeCn, Self::TraeSoloCn];

    /// Cockpit `parse`: empty defaults to Trae, `-` is treated as `_`.
    pub fn parse(raw: Option<&str>) -> UsageResult<Self> {
        let normalized = raw
            .map(str::trim)
            .filter(|value| !value.is_empty())
            .unwrap_or("trae")
            .to_ascii_lowercase()
            .replace('-', "_");
        match normalized.as_str() {
            "trae" => Ok(Self::Trae),
            "trae_solo" => Ok(Self::TraeSolo),
            "trae_cn" => Ok(Self::TraeCn),
            "trae_solo_cn" => Ok(Self::TraeSoloCn),
            other => Err(UsageError::Other(format!("不支持的 Trae 平台: {other}"))),
        }
    }

    /// SkillStar catalog id (kebab-case). Does not default an empty string to Trae.
    pub fn from_catalog_id(catalog_id: &str) -> Option<Self> {
        let trimmed = catalog_id.trim();
        if trimmed.is_empty() {
            return None;
        }
        Self::parse(Some(trimmed)).ok()
    }

    pub const fn catalog_id(self) -> &'static str {
        match self {
            Self::Trae => "trae",
            Self::TraeSolo => "trae-solo",
            Self::TraeCn => "trae-cn",
            Self::TraeSoloCn => "trae-solo-cn",
        }
    }

    pub const fn provider_key(self) -> &'static str {
        match self {
            Self::Trae => "trae",
            Self::TraeSolo => "trae_solo",
            Self::TraeCn => "trae_cn",
            Self::TraeSoloCn => "trae_solo_cn",
        }
    }

    pub const fn display_name(self) -> &'static str {
        match self {
            Self::Trae => "Trae",
            Self::TraeSolo => "TRAE SOLO",
            Self::TraeCn => "Trae CN",
            Self::TraeSoloCn => "TRAE SOLO CN",
        }
    }

    pub const fn is_cn(self) -> bool {
        matches!(self, Self::TraeCn | Self::TraeSoloCn)
    }

    pub const fn is_solo(self) -> bool {
        matches!(self, Self::TraeSolo | Self::TraeSoloCn)
    }

    /// Same string as cockpit `app_support_dir_name`.
    pub const fn app_support_dir_name(self) -> &'static str {
        self.display_name()
    }

    /// Cockpit `macos_app_name` (bundle name, every OS — path lookup does not use it).
    pub const fn app_name(self) -> &'static str {
        match self {
            Self::Trae => "Trae.app",
            Self::TraeSolo => "TRAE SOLO.app",
            Self::TraeCn => "Trae CN.app",
            Self::TraeSoloCn => "TRAE SOLO CN.app",
        }
    }

    pub const fn region_hosts(self) -> &'static [&'static str] {
        if self.is_cn() {
            CN_REGION_HOSTS
        } else {
            GLOBAL_REGION_HOSTS
        }
    }

    /// Cockpit `auth_client_id`. Solo products share one id; CN does not change it.
    pub const fn auth_client_id(self) -> &'static str {
        if self.is_solo() {
            SOLO_AUTH_CLIENT_ID
        } else {
            TRAE_AUTH_CLIENT_ID
        }
    }

    pub const fn auth_domain(self) -> &'static str {
        if self.is_cn() {
            CN_AUTH_DOMAIN
        } else {
            GLOBAL_AUTH_DOMAIN
        }
    }

    /// Single account-API origin when a row has no stored `loginHost`.
    ///
    /// ExchangeToken spends a one-time refresh token, so this is not
    /// [`Self::region_hosts`] and must not be fanned out.
    pub const fn default_login_host(self) -> &'static str {
        if self.is_cn() {
            CN_DEFAULT_LOGIN_HOST
        } else {
            GLOBAL_DEFAULT_LOGIN_HOST
        }
    }

    /// Product site for the renew button. Not a login URL.
    pub const fn product_home(self) -> &'static str {
        if self.is_cn() {
            "https://www.trae.cn"
        } else {
            "https://www.trae.ai"
        }
    }

    pub const fn pay_status_paths(self) -> &'static [&'static str] {
        if self.is_cn() {
            CN_PAY_STATUS_PATHS
        } else {
            GLOBAL_PAY_STATUS_PATHS
        }
    }

    pub const fn ent_usage_paths(self) -> &'static [&'static str] {
        if self.is_cn() {
            CN_ENT_USAGE_PATHS
        } else {
            GLOBAL_ENT_USAGE_PATHS
        }
    }

    /// CN-only fallback when `ide_user_ent_usage` has no pack list. Empty elsewhere.
    pub const fn current_entitlement_paths(self) -> &'static [&'static str] {
        if self.is_cn() {
            CN_CURRENT_ENTITLEMENT_PATHS
        } else {
            NO_PATHS
        }
    }
}

const GLOBAL_REGION_HOSTS: &[&str] = &[
    "https://api.marscode.com",
    "https://api.trae.ai",
    "https://www.trae.ai",
    "https://www.marscode.com",
    "https://grow-normal.trae.ai",
    "https://growsg-normal.trae.ai",
    "https://grow-normal.traeapi.us",
];

const CN_REGION_HOSTS: &[&str] = &[
    "https://api.trae.cn",
    "https://api.trae.com.cn",
    "https://www.trae.cn",
];

const TRAE_AUTH_CLIENT_ID: &str = "ono9krqynydwx5";
const SOLO_AUTH_CLIENT_ID: &str = "en1oxy7wnw8j9n";
const GLOBAL_AUTH_DOMAIN: &str = "www.trae.ai";
const CN_AUTH_DOMAIN: &str = "www.trae.cn";
const GLOBAL_DEFAULT_LOGIN_HOST: &str = "https://grow-normal.trae.ai";
const CN_DEFAULT_LOGIN_HOST: &str = "https://api.trae.cn";
const NO_PATHS: &[&str] = &[];
const GLOBAL_PAY_STATUS_PATHS: &[&str] = &["/trae/api/v1/pay/ide_user_pay_status"];
const CN_PAY_STATUS_PATHS: &[&str] = &[
    "/trae/api/v2/pay/ide_user_pay_status",
    "/trae/api/v1/pay/ide_user_pay_status",
];
const GLOBAL_ENT_USAGE_PATHS: &[&str] = &["/trae/api/v1/pay/ide_user_ent_usage"];
const CN_ENT_USAGE_PATHS: &[&str] = &[
    "/trae/api/v2/pay/ide_user_ent_usage",
    "/trae/api/v1/pay/ide_user_ent_usage",
];
const CN_CURRENT_ENTITLEMENT_PATHS: &[&str] = &["/trae/api/v2/pay/user_current_entitlement_list"];

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn platform_table_matches_cockpit_product_paths() {
        let rows = [
            (
                TraePlatformKind::Trae,
                "trae",
                "trae",
                "Trae",
                "Trae.app",
                false,
                false,
            ),
            (
                TraePlatformKind::TraeSolo,
                "trae-solo",
                "trae_solo",
                "TRAE SOLO",
                "TRAE SOLO.app",
                false,
                true,
            ),
            (
                TraePlatformKind::TraeCn,
                "trae-cn",
                "trae_cn",
                "Trae CN",
                "Trae CN.app",
                true,
                false,
            ),
            (
                TraePlatformKind::TraeSoloCn,
                "trae-solo-cn",
                "trae_solo_cn",
                "TRAE SOLO CN",
                "TRAE SOLO CN.app",
                true,
                true,
            ),
        ];
        assert_eq!(TraePlatformKind::ALL.len(), rows.len());
        for (kind, catalog_id, provider_key, display, app_name, cn, solo) in rows {
            assert_eq!(kind.catalog_id(), catalog_id);
            assert_eq!(kind.provider_key(), provider_key);
            assert_eq!(kind.display_name(), display);
            assert_eq!(kind.app_support_dir_name(), display);
            assert_eq!(kind.app_name(), app_name);
            assert_eq!(kind.is_cn(), cn);
            assert_eq!(kind.is_solo(), solo);
            assert_eq!(TraePlatformKind::from_catalog_id(catalog_id), Some(kind));
            assert_eq!(
                TraePlatformKind::parse(Some(provider_key)).expect("provider key"),
                kind
            );
            assert_eq!(
                kind.region_hosts(),
                if cn {
                    CN_REGION_HOSTS
                } else {
                    GLOBAL_REGION_HOSTS
                }
            );
        }

        assert_eq!(
            GLOBAL_REGION_HOSTS,
            [
                "https://api.marscode.com",
                "https://api.trae.ai",
                "https://www.trae.ai",
                "https://www.marscode.com",
                "https://grow-normal.trae.ai",
                "https://growsg-normal.trae.ai",
                "https://grow-normal.traeapi.us",
            ]
        );
        assert_eq!(
            CN_REGION_HOSTS,
            [
                "https://api.trae.cn",
                "https://api.trae.com.cn",
                "https://www.trae.cn",
            ]
        );
        assert!(
            CN_REGION_HOSTS
                .iter()
                .all(|host| !host.contains("trae.ai") && !host.contains("marscode.com"))
        );
        for kind in TraePlatformKind::ALL {
            assert!(
                kind.region_hosts().contains(&kind.default_login_host()),
                "{}",
                kind.catalog_id()
            );
            assert!(
                kind.region_hosts().contains(&kind.product_home()),
                "{}",
                kind.catalog_id()
            );
            assert!(kind.auth_domain().starts_with("www."));
            assert!(!kind.pay_status_paths().is_empty());
            assert!(!kind.ent_usage_paths().is_empty());
        }
        assert_eq!(TraePlatformKind::Trae.auth_client_id(), TRAE_AUTH_CLIENT_ID);
        assert_eq!(
            TraePlatformKind::TraeCn.auth_client_id(),
            TRAE_AUTH_CLIENT_ID
        );
        assert_eq!(
            TraePlatformKind::TraeSolo.auth_client_id(),
            SOLO_AUTH_CLIENT_ID
        );
        assert_eq!(
            TraePlatformKind::TraeSoloCn.auth_client_id(),
            SOLO_AUTH_CLIENT_ID
        );
        assert_eq!(TraePlatformKind::Trae.auth_domain(), "www.trae.ai");
        assert_eq!(TraePlatformKind::TraeCn.auth_domain(), "www.trae.cn");
        assert_eq!(
            TraePlatformKind::Trae.default_login_host(),
            "https://grow-normal.trae.ai"
        );
        assert_eq!(
            TraePlatformKind::TraeCn.default_login_host(),
            "https://api.trae.cn"
        );
        assert_eq!(
            TraePlatformKind::Trae.pay_status_paths(),
            &["/trae/api/v1/pay/ide_user_pay_status"]
        );
        assert_eq!(
            TraePlatformKind::TraeCn.pay_status_paths()[0],
            "/trae/api/v2/pay/ide_user_pay_status"
        );
        assert!(
            TraePlatformKind::Trae
                .current_entitlement_paths()
                .is_empty()
        );
        assert_eq!(
            TraePlatformKind::TraeCn.current_entitlement_paths(),
            &["/trae/api/v2/pay/user_current_entitlement_list"]
        );
    }

    #[test]
    fn parse_accepts_kebab_case_and_defaults_empty_to_trae() {
        assert_eq!(
            TraePlatformKind::parse(Some("Trae-Solo")).expect("kebab"),
            TraePlatformKind::TraeSolo
        );
        assert_eq!(
            TraePlatformKind::parse(None).expect("default"),
            TraePlatformKind::Trae
        );
        assert_eq!(
            TraePlatformKind::parse(Some("  ")).expect("blank"),
            TraePlatformKind::Trae
        );
        let error = TraePlatformKind::parse(Some("windsurf")).expect_err("unknown");
        assert!(
            error.to_string().contains("不支持的 Trae 平台: windsurf"),
            "{error}"
        );
        assert_eq!(TraePlatformKind::from_catalog_id(""), None);
        assert_eq!(TraePlatformKind::from_catalog_id("windsurf"), None);
    }
}
