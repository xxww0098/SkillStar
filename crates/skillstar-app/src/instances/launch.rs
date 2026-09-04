//! Spawn the macOS app with the instance profile. Creating a directory is not enough.

use super::apps::{DesktopAppId, open_argv};
use super::error::InstanceError;
use std::path::Path;
use std::process::{Command, Stdio};

pub fn start_macos_app(
    app: DesktopAppId,
    user_data_dir: &Path,
    extra_args: &[String],
) -> Result<(), InstanceError> {
    if !cfg!(target_os = "macos") {
        return Err(InstanceError::Platform);
    }
    std::fs::create_dir_all(user_data_dir)?;
    let mut argv = open_argv(app, user_data_dir);
    argv.extend(extra_args.iter().cloned());
    let program = argv
        .first()
        .cloned()
        .ok_or_else(|| InstanceError::Other("内部错误：空启动参数".to_string()))?;
    Command::new(program)
        .args(&argv[1..])
        .stdin(Stdio::null())
        .stdout(Stdio::null())
        .stderr(Stdio::null())
        .spawn()
        .map_err(|e| InstanceError::Other(format!("无法启动 {}: {e}", app.display_name())))?;
    Ok(())
}
