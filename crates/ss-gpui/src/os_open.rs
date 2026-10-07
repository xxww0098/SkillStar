//! Hand a URL or a local folder to the platform's own handler.
//!
//! Both helpers shell out: a URL opens in the default browser, a folder in
//! the file manager (Finder / Explorer / xdg-open). They sit at the crate
//! root beside `notify` because they carry no page semantics, and several
//! capabilities call them — a folder button belongs to Settings, the reveal
//! command to My Skills, the install links to About.

use std::path::Path;

/// Open `target` — a URL or an absolute path — with the OS default handler.
pub(crate) fn open_external(target: &str) {
    #[cfg(target_os = "macos")]
    let _ = std::process::Command::new("/usr/bin/open")
        .arg(target)
        .spawn();
    #[cfg(target_os = "windows")]
    let _ = std::process::Command::new("cmd")
        .args(["/C", "start", "", target])
        .spawn();
    #[cfg(target_os = "linux")]
    let _ = std::process::Command::new("xdg-open").arg(target).spawn();
}

/// Reveal `path` in the platform file manager. Windows needs its own
/// command because `start` would open the folder's default handler instead
/// of the folder itself.
pub(crate) fn open_folder(path: impl AsRef<Path>) {
    #[cfg(target_os = "windows")]
    let _ = std::process::Command::new("explorer")
        .arg(path.as_ref())
        .spawn();
    #[cfg(not(target_os = "windows"))]
    open_external(&path.as_ref().to_string_lossy());
}
