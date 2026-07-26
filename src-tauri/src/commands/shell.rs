use skillstar_core::infra::error::AppError;

#[tauri::command]
pub async fn write_text_file(path: String, content: String) -> Result<(), AppError> {
    Ok(std::fs::write(&path, &content)?)
}

#[tauri::command]
pub async fn read_text_file(path: String) -> Result<String, AppError> {
    Ok(std::fs::read_to_string(&path)?)
}

/// Open an http(s) URL in the system default browser.
///
/// Uses absolute launcher paths where possible so Dock/Finder-launched apps
/// (thin `PATH`) still resolve `open` / friends. This powers Usage card
/// "open console" / ExternalAnchor links — failures must surface as `Err`,
/// never silent no-ops.
#[tauri::command]
pub async fn open_external_url(url: String) -> Result<(), AppError> {
    let trimmed = url.trim();
    let lower = trimmed.to_ascii_lowercase();

    if !(lower.starts_with("http://") || lower.starts_with("https://")) {
        return Err(AppError::Other(
            "Only http(s) URLs are supported".to_string(),
        ));
    }

    open_with_system_launcher(trimmed)
        .map_err(|e| AppError::Other(format!("Failed to open URL: {e}")))
}

fn open_with_system_launcher(url: &str) -> Result<(), String> {
    #[cfg(target_os = "macos")]
    {
        // Prefer absolute path — GUI launches often have a minimal PATH.
        std::process::Command::new("/usr/bin/open")
            .arg(url)
            .spawn()
            .map_err(|e| e.to_string())?;
        return Ok(());
    }

    #[cfg(target_os = "windows")]
    {
        // `cmd /C start "" <url>` is the most reliable Windows hand-off to the
        // default browser; rundll32/explorer are fallbacks.
        if std::process::Command::new("cmd")
            .args(["/C", "start", "", url])
            .spawn()
            .is_ok()
        {
            return Ok(());
        }
        if std::process::Command::new("rundll32")
            .args(["url.dll,FileProtocolHandler", url])
            .spawn()
            .is_ok()
        {
            return Ok(());
        }
        std::process::Command::new("explorer")
            .arg(url)
            .spawn()
            .map_err(|e| e.to_string())?;
        return Ok(());
    }

    #[cfg(target_os = "linux")]
    {
        if std::process::Command::new("xdg-open")
            .arg(url)
            .spawn()
            .is_ok()
        {
            return Ok(());
        }
        std::process::Command::new("gio")
            .args(["open", url])
            .spawn()
            .map_err(|e| e.to_string())?;
        return Ok(());
    }

    #[allow(unreachable_code)]
    Err("unsupported platform".into())
}

#[tauri::command]
pub async fn open_folder(path: String) -> Result<(), AppError> {
    #[cfg(target_os = "macos")]
    std::process::Command::new("/usr/bin/open")
        .arg(&path)
        .spawn()?;

    #[cfg(target_os = "windows")]
    std::process::Command::new("explorer").arg(&path).spawn()?;

    #[cfg(target_os = "linux")]
    std::process::Command::new("xdg-open").arg(&path).spawn()?;

    Ok(())
}
