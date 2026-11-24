//! Windows Credential Manager (`advapi32`) 封装，用于 Antigravity 2.x 的 `gemini:antigravity`。
//!
//! 严禁新增 Cargo 依赖：直接通过 `#[link(name = "advapi32")]` 绑定 `CredReadW` /
//! `CredWriteW` / `CredDeleteW` / `CredFree`。读写抽象成 [WinCredStore]，macOS /
//! Linux 下也可注入假实现完整单测读写、合并与失败回滚分支。

use crate::error::{AccountError, AccountResult};

use super::antigravity_credential;

/// WinCred 的 TargetName。
pub(crate) const WINCRED_TARGET: &str = "gemini:antigravity";
/// WinCred 的 UserName。
pub(crate) const WINCRED_USERNAME: &str = "antigravity";
/// Win32 `CRED_TYPE_GENERIC`。
#[cfg(any(windows, test))]
pub(crate) const CRED_TYPE_GENERIC: u32 = 1;
/// Win32 `CRED_PERSIST_LOCAL_MACHINE`。
#[cfg(any(windows, test))]
pub(crate) const CRED_PERSIST_LOCAL_MACHINE: u32 = 2;

/// 最小 Windows Credential Manager 存储接口，便于跨平台注入假实现测试。
pub(crate) trait WinCredStore {
    fn read(&self, target: &str) -> AccountResult<Option<Vec<u8>>>;
    fn write(&self, target: &str, username: &str, blob: &[u8]) -> AccountResult<()>;
    fn delete(&self, target: &str) -> AccountResult<()>;
}

/// 生产用的 WinCred 存储：Windows 下走 `advapi32` FFI。
pub(crate) struct SystemWinCred;

/// 解析 WinCred 读出的原始字节：同时兼容裸 UTF-8 JSON 与 `go-keyring-base64:` 包装。
pub(crate) fn decode_wincred_payload(bytes: &[u8]) -> AccountResult<Option<String>> {
    let text = std::str::from_utf8(bytes).map_err(|_| {
        AccountError::Client("Antigravity Windows WinCred 凭据不是有效 UTF-8".to_string())
    })?;
    antigravity_credential::decode_payload_text(text)
}

#[cfg(windows)]
mod ffi {
    use std::ffi::c_void;

    #[repr(C)]
    pub(super) struct FileTime {
        pub(super) dw_low_date_time: u32,
        pub(super) dw_high_date_time: u32,
    }

    #[repr(C)]
    pub(super) struct CredentialW {
        pub(super) flags: u32,
        pub(super) r#type: u32,
        pub(super) target_name: *mut u16,
        pub(super) comment: *mut u16,
        pub(super) last_written: FileTime,
        pub(super) credential_blob_size: u32,
        pub(super) credential_blob: *mut u8,
        pub(super) persist: u32,
        pub(super) attribute_count: u32,
        pub(super) attributes: *mut c_void,
        pub(super) target_alias: *mut u16,
        pub(super) user_name: *mut u16,
    }

    #[link(name = "advapi32")]
    unsafe extern "system" {
        pub(super) fn CredReadW(
            target_name: *const u16,
            r#type: u32,
            flags: u32,
            credential: *mut *mut CredentialW,
        ) -> i32;
        pub(super) fn CredWriteW(credential: *const CredentialW, flags: u32) -> i32;
        pub(super) fn CredDeleteW(target_name: *const u16, r#type: u32, flags: u32) -> i32;
        pub(super) fn CredFree(buffer: *mut c_void);
    }
}

#[cfg(windows)]
mod platform {
    use std::ptr;

    use super::ffi::{CredDeleteW, CredFree, CredReadW, CredWriteW, CredentialW, FileTime};
    use super::{CRED_PERSIST_LOCAL_MACHINE, CRED_TYPE_GENERIC};
    use crate::error::{AccountError, AccountResult};

    const ERROR_NOT_FOUND: i32 = 1168;

    struct CredGuard(*mut CredentialW);

    impl Drop for CredGuard {
        fn drop(&mut self) {
            if !self.0.is_null() {
                // SAFETY: `self.0` came from a successful `CredReadW` call and has not been freed yet.
                unsafe { CredFree(self.0.cast()) };
            }
        }
    }

    fn wide_nul(value: &str) -> Vec<u16> {
        value.encode_utf16().chain(std::iter::once(0)).collect()
    }

    pub(super) fn read(target: &str) -> AccountResult<Option<Vec<u8>>> {
        let target_w = wide_nul(target);
        let mut raw: *mut CredentialW = ptr::null_mut();
        // SAFETY: `target_w` is NUL-terminated UTF-16 and `raw` receives the allocated `CREDENTIALW`.
        let ok = unsafe { CredReadW(target_w.as_ptr(), CRED_TYPE_GENERIC, 0, &mut raw) };
        if ok == 0 {
            let err = std::io::Error::last_os_error();
            if err.raw_os_error() == Some(ERROR_NOT_FOUND) {
                return Ok(None);
            }
            return Err(AccountError::Client(format!(
                "读取 Antigravity Windows WinCred 凭据失败: {err}"
            )));
        }
        let guard = CredGuard(raw);
        // SAFETY: `CredReadW` succeeded so `guard.0` points to a valid `CREDENTIALW`.
        let cred = unsafe { &*guard.0 };
        if cred.credential_blob.is_null() || cred.credential_blob_size == 0 {
            return Ok(Some(Vec::new()));
        }
        // SAFETY: `credential_blob` is valid for `credential_blob_size` bytes until `CredGuard` drops.
        let bytes = unsafe {
            std::slice::from_raw_parts(cred.credential_blob, cred.credential_blob_size as usize)
        };
        Ok(Some(bytes.to_vec()))
    }

    pub(super) fn write(target: &str, username: &str, blob: &[u8]) -> AccountResult<()> {
        let blob_len = u32::try_from(blob.len()).map_err(|_| {
            AccountError::Client("Antigravity Windows WinCred 载荷过长".to_string())
        })?;
        let mut target_w = wide_nul(target);
        let mut user_w = wide_nul(username);
        let cred = CredentialW {
            flags: 0,
            r#type: CRED_TYPE_GENERIC,
            target_name: target_w.as_mut_ptr(),
            comment: ptr::null_mut(),
            last_written: FileTime {
                dw_low_date_time: 0,
                dw_high_date_time: 0,
            },
            credential_blob_size: blob_len,
            credential_blob: if blob.is_empty() {
                ptr::null_mut()
            } else {
                blob.as_ptr().cast_mut()
            },
            persist: CRED_PERSIST_LOCAL_MACHINE,
            attribute_count: 0,
            attributes: ptr::null_mut(),
            target_alias: ptr::null_mut(),
            user_name: user_w.as_mut_ptr(),
        };
        // SAFETY: All pointers in `cred` stay valid for the duration of `CredWriteW`.
        let ok = unsafe { CredWriteW(&cred, 0) };
        if ok == 0 {
            let err = std::io::Error::last_os_error();
            return Err(AccountError::Client(format!(
                "写入 Antigravity Windows WinCred 失败: {err}"
            )));
        }
        Ok(())
    }

    pub(super) fn delete(target: &str) -> AccountResult<()> {
        let target_w = wide_nul(target);
        // SAFETY: `target_w` is NUL-terminated UTF-16 for `CredDeleteW`.
        let ok = unsafe { CredDeleteW(target_w.as_ptr(), CRED_TYPE_GENERIC, 0) };
        if ok == 0 {
            let err = std::io::Error::last_os_error();
            if err.raw_os_error() == Some(ERROR_NOT_FOUND) {
                return Ok(());
            }
            return Err(AccountError::Client(format!(
                "删除 Antigravity Windows WinCred 凭据失败: {err}"
            )));
        }
        Ok(())
    }
}

#[cfg(windows)]
impl WinCredStore for SystemWinCred {
    fn read(&self, target: &str) -> AccountResult<Option<Vec<u8>>> {
        platform::read(target)
    }

    fn write(&self, target: &str, username: &str, blob: &[u8]) -> AccountResult<()> {
        platform::write(target, username, blob)
    }

    fn delete(&self, target: &str) -> AccountResult<()> {
        platform::delete(target)
    }
}

#[cfg(not(windows))]
impl WinCredStore for SystemWinCred {
    fn read(&self, _target: &str) -> AccountResult<Option<Vec<u8>>> {
        Err(AccountError::Client(
            "Windows WinCred 仅在 Windows 平台上可用".to_string(),
        ))
    }

    fn write(&self, _target: &str, _username: &str, _blob: &[u8]) -> AccountResult<()> {
        Err(AccountError::Client(
            "Windows WinCred 仅在 Windows 平台上可用".to_string(),
        ))
    }

    fn delete(&self, _target: &str) -> AccountResult<()> {
        Err(AccountError::Client(
            "Windows WinCred 仅在 Windows 平台上可用".to_string(),
        ))
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn constants_match_antigravity_wincred_contract() {
        assert_eq!(WINCRED_TARGET, "gemini:antigravity");
        assert_eq!(WINCRED_USERNAME, "antigravity");
        assert_eq!(CRED_TYPE_GENERIC, 1);
        assert_eq!(CRED_PERSIST_LOCAL_MACHINE, 2);
    }

    #[test]
    fn decode_wincred_payload_accepts_plain_utf8_json_and_go_keyring_base64() {
        let json = r#"{"token":{"access_token":"at","refresh_token":"rt"}}"#;
        assert_eq!(
            decode_wincred_payload(json.as_bytes()).expect("plain"),
            Some(json.to_string())
        );

        let wrapped = antigravity_credential::keychain_value(json);
        assert_eq!(
            decode_wincred_payload(wrapped.as_bytes()).expect("wrapped"),
            Some(json.to_string())
        );

        assert_eq!(decode_wincred_payload(b"   ").expect("empty"), None);
        assert!(decode_wincred_payload(&[0xff, 0xfe]).is_err());
    }
}
