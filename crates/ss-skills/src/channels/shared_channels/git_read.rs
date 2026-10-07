use super::{SharedChannelError, SharedChannelErrorCode};

pub(super) fn git_read_error(error: anyhow::Error, action: &str) -> SharedChannelError {
    use crate::git::transport::GitTransportErrorCode as GitCode;

    let code = error
        .chain()
        .find_map(|cause| cause.downcast_ref::<crate::git::transport::GitTransportError>())
        .map(|transport| match transport.code {
            GitCode::Network => SharedChannelErrorCode::Network,
            GitCode::NotAuthenticated | GitCode::TokenExpired | GitCode::CredentialUnavailable => {
                SharedChannelErrorCode::NotAuthenticated
            }
            GitCode::Unauthorized | GitCode::AppNotInstalled => {
                SharedChannelErrorCode::AppRepositoryAccessRequired
            }
            GitCode::Cancelled => SharedChannelErrorCode::Cancelled,
            GitCode::UnsafeRemote => SharedChannelErrorCode::Integrity,
            GitCode::Other => SharedChannelErrorCode::Protocol,
        })
        .unwrap_or(SharedChannelErrorCode::Protocol);
    SharedChannelError::new(code, format!("{action}: {error:#}"))
}
