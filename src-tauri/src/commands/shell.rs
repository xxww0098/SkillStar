use skillstar_core::infra::error::AppError;

/// Validate that `url` is a safe http(s) URL for external-launcher handoff.
///
/// Returns the trimmed input for shell handoff. Rejects non-http(s) schemes,
/// URLs without a host, embedded userinfo, and the `"` character (which would
/// break cmd quoting on Windows). `&`/`|` are legal URL query characters and
/// are NOT rejected here — Windows callers must quote the URL argument when
/// handing off through `cmd` so they cannot act as command separators.
fn validate_external_url(url: &str) -> Result<&str, &'static str> {
    let trimmed = url.trim();
    if trimmed.contains('"') {
        return Err("URL must not contain double quotes");
    }
    let parsed = url::Url::parse(trimmed).map_err(|_| "URL is not valid")?;
    match parsed.scheme() {
        "http" | "https" => {}
        _ => return Err("Only http(s) URLs are supported"),
    }
    if !parsed.host_str().is_some_and(|host| !host.is_empty()) {
        return Err("URL must have a host");
    }
    if !parsed.username().is_empty() || parsed.password().is_some() {
        return Err("URL must not contain credentials");
    }
    Ok(trimmed)
}

/// Open an http(s) URL in the system default browser.
///
/// Uses absolute launcher paths where possible so Dock/Finder-launched apps
/// (thin `PATH`) still resolve `open` / friends. This powers Usage card
/// "open console" / ExternalAnchor links — failures must surface as `Err`,
/// never silent no-ops.
#[tauri::command]
pub async fn open_external_url(url: String) -> Result<(), AppError> {
    let validated = validate_external_url(&url).map_err(|msg| AppError::Other(msg.to_string()))?;
    open_with_system_launcher(validated)
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
        // `cmd /C start "" "<url>"` is the most reliable Windows hand-off to
        // the default browser; rundll32/explorer are fallbacks. The URL
        // argument MUST be quoted: cmd treats `&`/`|` as command separators
        // outside quotes. validate_external_url already rejected embedded `"`
        // characters, so quoting is unambiguous here.
        let quoted = format!("\"{url}\"");
        if std::process::Command::new("cmd")
            .args(["/C", "start", ""])
            .arg(&quoted)
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

#[cfg(test)]
mod tests {
    use super::validate_external_url;

    #[test]
    fn accepts_http_urls() {
        assert!(validate_external_url("https://github.com/xxww0098/SkillStar").is_ok());
        assert!(validate_external_url("http://example.com/path?a=1&b=2").is_ok());
        assert!(validate_external_url("  https://trimmed.example/  ").is_ok());
        // Scheme comparison is case-insensitive (url::Url normalizes it).
        assert!(validate_external_url("HTTP://EXAMPLE.COM/").is_ok());
    }

    #[test]
    fn rejects_non_http_schemes() {
        assert!(validate_external_url("javascript:alert(1)").is_err());
        assert!(validate_external_url("file:///etc/passwd").is_err());
        assert!(validate_external_url("data:text/html,<script>alert(1)</script>").is_err());
        assert!(validate_external_url("skillstar://models/cloud-sync").is_err());
    }

    #[test]
    fn rejects_quotes_credentials_and_hostless_urls() {
        // `"` would break cmd quoting and must be rejected outright.
        assert!(validate_external_url("https://example.com/x\"/y").is_err());
        assert!(validate_external_url("https://user:pass@example.com/").is_err());
        assert!(validate_external_url("https://user@example.com/").is_err());
        // `https://` parses with no host; `https:///path` is a valid URL whose
        // authority happens to be "path" (no injection surface), so it stays
        // accepted.
        assert!(validate_external_url("https://").is_err());
        assert!(validate_external_url("https:///path").is_ok());
    }

    #[test]
    fn allows_query_ampersands_for_windows_quoting_path() {
        // `&`/`|` are legal URL query characters; the Windows launcher quotes
        // the argument so cmd cannot interpret them as separators.
        let url = "https://example.com/?a=1&calc";
        let validated = validate_external_url(url).expect("valid http URL with query");
        assert_eq!(validated, url);
    }
}
