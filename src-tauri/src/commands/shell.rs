use crate::core::infra::error::AppError;

#[tauri::command]
pub async fn write_text_file(path: String, content: String) -> Result<(), AppError> {
    Ok(std::fs::write(&path, &content)?)
}

#[tauri::command]
pub async fn read_text_file(path: String) -> Result<String, AppError> {
    Ok(std::fs::read_to_string(&path)?)
}

#[tauri::command]
pub async fn open_external_url(url: String) -> Result<(), AppError> {
    let trimmed = url.trim();
    let lower = trimmed.to_ascii_lowercase();

    if !(lower.starts_with("http://") || lower.starts_with("https://")) {
        return Err(AppError::Other(
            "Only http(s) URLs are supported".to_string(),
        ));
    }

    #[cfg(target_os = "windows")]
    {
        if std::process::Command::new("rundll32")
            .args(["url.dll,FileProtocolHandler", trimmed])
            .spawn()
            .is_err()
        {
            std::process::Command::new("explorer")
                .arg(trimmed)
                .spawn()
                .map_err(|e| AppError::Other(format!("Failed to open URL on Windows: {e}")))?;
        }
        return Ok(());
    }

    #[cfg(target_os = "macos")]
    {
        std::process::Command::new("open")
            .arg(trimmed)
            .spawn()
            .map_err(|e| AppError::Other(format!("Failed to open URL on macOS: {e}")))?;
        return Ok(());
    }

    #[cfg(target_os = "linux")]
    {
        if std::process::Command::new("xdg-open")
            .arg(trimmed)
            .spawn()
            .is_err()
        {
            std::process::Command::new("gio")
                .args(["open", trimmed])
                .spawn()
                .map_err(|e| AppError::Other(format!("Failed to open URL on Linux: {e}")))?;
        }
        return Ok(());
    }

    #[allow(unreachable_code)]
    Ok(())
}

#[tauri::command]
pub async fn open_folder(path: String) -> Result<(), AppError> {
    #[cfg(target_os = "macos")]
    std::process::Command::new("open").arg(&path).spawn()?;

    #[cfg(target_os = "windows")]
    std::process::Command::new("explorer").arg(&path).spawn()?;

    #[cfg(target_os = "linux")]
    std::process::Command::new("xdg-open").arg(&path).spawn()?;

    Ok(())
}
