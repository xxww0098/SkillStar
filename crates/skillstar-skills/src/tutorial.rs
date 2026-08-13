//! Persistent, versioned Skill tutorial artifacts.
//!
//! ACP transport lives in the Tauri adapter. This module owns the durable
//! artifact contract: generated HTML validation, whole-Skill coverage,
//! freshness checks, safe storage keys, and atomic replacement.

use std::collections::{BTreeMap, BTreeSet};
use std::fs::{File, OpenOptions};
use std::io::Write as _;
use std::path::{Path, PathBuf};
use std::sync::{LazyLock, Mutex, MutexGuard};
use std::time::SystemTime;

use regex::Regex;
use scraper::{Html, Selector};
use serde::{Deserialize, Serialize};
use sha2::{Digest, Sha256};
use skillstar_core::infra::error::AppError;
use uuid::Uuid;

use crate::content::SkillSnapshot;

const ARTIFACT_KEY_DOMAIN: &[u8] = b"skillstar.skill-tutorial-key.v1\0";
const MAX_HTML_BYTES: usize = 2 * 1024 * 1024;
const REQUIRED_CSP: &str =
    "default-src 'none'; style-src 'unsafe-inline'; img-src data:; font-src data:";

static ARTIFACT_IO_LOCK: LazyLock<Mutex<()>> = LazyLock::new(|| Mutex::new(()));

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum TutorialState {
    Missing,
    Fresh,
    Stale,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum TutorialStaleReason {
    ContentChanged,
    GeneratorChanged,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct TutorialMetadata {
    pub skill_name: String,
    pub content_hash: String,
    pub prompt_version: String,
    pub schema_version: String,
    pub tutorial_style: String,
    pub agent_label: String,
    pub generated_at: String,
    pub file_count: usize,
    pub total_bytes: u64,
    /// Persisted so a locally modified artifact can still be coverage-checked
    /// against the exact source version that produced it.
    #[serde(default)]
    pub source_files: Vec<String>,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct TutorialArtifact {
    pub state: TutorialState,
    pub current_hash: String,
    pub html: Option<String>,
    pub metadata: Option<TutorialMetadata>,
    pub stale_reason: Option<TutorialStaleReason>,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct ValidatedTutorialHtml(String);

impl ValidatedTutorialHtml {
    pub fn as_str(&self) -> &str {
        &self.0
    }

    pub fn into_inner(self) -> String {
        self.0
    }
}

pub fn load(
    snapshot: &SkillSnapshot,
    prompt_version: &str,
    schema_version: &str,
) -> Result<TutorialArtifact, AppError> {
    let directory = artifact_directory(&snapshot.name);
    let root = directory.parent().ok_or_else(|| {
        AppError::Other(format!(
            "Tutorial artifact has no parent: {}",
            directory.display()
        ))
    })?;
    let _artifact_guard = lock_artifact_io(root)?;
    recover_missing_artifact_directory(&directory)?;
    let html_path = directory.join("tutorial.html");
    let metadata_path = directory.join("metadata.json");

    let html_exists = html_path.is_file();
    let metadata_exists = metadata_path.is_file();
    if !html_exists && !metadata_exists {
        return Ok(missing_artifact(snapshot));
    }
    if html_exists != metadata_exists {
        return Err(AppError::Other(format!(
            "Skill tutorial artifact is incomplete: {}",
            directory.display()
        )));
    }

    let metadata: TutorialMetadata =
        serde_json::from_str(&std::fs::read_to_string(&metadata_path)?).map_err(|error| {
            AppError::Other(format!(
                "Failed to read Skill tutorial metadata {}: {error}",
                metadata_path.display()
            ))
        })?;
    if metadata.skill_name != snapshot.name {
        return Err(AppError::Other(format!(
            "Skill tutorial metadata name mismatch: expected {:?}, found {:?}",
            snapshot.name, metadata.skill_name
        )));
    }

    if metadata.file_count != metadata.source_files.len() {
        return Err(AppError::Other(format!(
            "Skill tutorial metadata file count mismatch: expected {}, found {} paths",
            metadata.file_count,
            metadata.source_files.len()
        )));
    }
    let unique_source_files = metadata.source_files.iter().collect::<BTreeSet<_>>();
    if unique_source_files.len() != metadata.source_files.len() {
        return Err(AppError::Other(
            "Skill tutorial metadata contains duplicate source paths".to_string(),
        ));
    }

    let current_source_files = snapshot
        .files
        .iter()
        .map(|file| file.relative_path.clone())
        .collect::<Vec<_>>();
    let content_matches = metadata.content_hash == snapshot.content_hash;
    let validation_paths = if content_matches {
        if metadata.source_files != current_source_files
            || metadata.file_count != snapshot.files.len()
            || metadata.total_bytes != snapshot.total_bytes
        {
            return Err(AppError::Other(
                "Skill tutorial metadata does not match the current Skill snapshot".to_string(),
            ));
        }
        &current_source_files
    } else {
        &metadata.source_files
    };

    let html = std::fs::read_to_string(&html_path)?;
    validate_html(&html, validation_paths).map_err(|error| {
        AppError::Other(format!(
            "Stored Skill tutorial failed validation ({}): {error}",
            html_path.display()
        ))
    })?;

    let (state, stale_reason) = if !content_matches {
        (
            TutorialState::Stale,
            Some(TutorialStaleReason::ContentChanged),
        )
    } else if metadata.prompt_version != prompt_version || metadata.schema_version != schema_version
    {
        (
            TutorialState::Stale,
            Some(TutorialStaleReason::GeneratorChanged),
        )
    } else {
        (TutorialState::Fresh, None)
    };

    Ok(TutorialArtifact {
        state,
        current_hash: snapshot.content_hash.clone(),
        html: Some(html),
        metadata: Some(metadata),
        stale_reason,
    })
}

pub fn validate_html(
    html: &str,
    expected_paths: &[String],
) -> Result<ValidatedTutorialHtml, AppError> {
    let html = html.trim();
    if html.is_empty() {
        return Err(validation_error("HTML is empty"));
    }
    if html.len() > MAX_HTML_BYTES {
        return Err(validation_error(format!(
            "HTML exceeds the {} byte limit",
            MAX_HTML_BYTES
        )));
    }
    if html.contains('\0') {
        return Err(validation_error("HTML contains a NUL byte"));
    }

    let lower = html.to_ascii_lowercase();
    for required in [
        "<!doctype html",
        "<html",
        "</html>",
        "<head",
        "</head>",
        "<body",
        "</body>",
    ] {
        if !lower.contains(required) {
            return Err(validation_error(format!(
                "HTML is incomplete; missing {required}"
            )));
        }
    }
    if !lower.trim_start().starts_with("<!doctype html") {
        return Err(validation_error("HTML must start with <!doctype html>"));
    }
    if !lower.trim_end().ends_with("</html>") {
        return Err(validation_error("HTML must end with </html>"));
    }
    validate_document_structure(html, &lower)?;

    validate_dom(html, expected_paths)?;

    Ok(ValidatedTutorialHtml(html.to_string()))
}

fn validate_document_structure(html: &str, lower: &str) -> Result<(), AppError> {
    let prefix = Regex::new(r"(?is)\A<!doctype\s+html\s*>\s*<html\b[^>]*>\s*<head\b[^>]*>")
        .expect("static document prefix regex");
    let opening = prefix.find(html).ok_or_else(|| {
        validation_error("document must begin with doctype, html, then head elements")
    })?;
    let head_close = lower[opening.end()..]
        .find("</head>")
        .map(|offset| opening.end() + offset)
        .ok_or_else(|| validation_error("document head is not closed"))?;
    let after_head = head_close + "</head>".len();
    let body = Regex::new(r"(?is)\A\s*<body\b[^>]*>").expect("static body prefix regex");
    if !body.is_match(&html[after_head..]) {
        return Err(validation_error(
            "body must immediately follow the closed head",
        ));
    }
    let suffix = Regex::new(r"(?is)</body>\s*</html>\z").expect("static document suffix regex");
    if !suffix.is_match(html) {
        return Err(validation_error(
            "document must finish with closed body and html elements",
        ));
    }
    Ok(())
}

pub fn save(
    snapshot: &SkillSnapshot,
    prompt_version: &str,
    schema_version: &str,
    tutorial_style: &str,
    agent_label: &str,
    validated_html: ValidatedTutorialHtml,
) -> Result<TutorialArtifact, AppError> {
    let directory = artifact_directory(&snapshot.name);
    let root = directory.parent().ok_or_else(|| {
        AppError::Other(format!(
            "Tutorial artifact has no parent: {}",
            directory.display()
        ))
    })?;
    let _artifact_guard = lock_artifact_io(root)?;
    recover_missing_artifact_directory(&directory)?;
    let source_files = snapshot
        .files
        .iter()
        .map(|file| file.relative_path.clone())
        .collect::<Vec<_>>();
    let metadata = TutorialMetadata {
        skill_name: snapshot.name.clone(),
        content_hash: snapshot.content_hash.clone(),
        prompt_version: prompt_version.to_string(),
        schema_version: schema_version.to_string(),
        tutorial_style: tutorial_style.to_string(),
        agent_label: agent_label.to_string(),
        generated_at: chrono::Utc::now().to_rfc3339(),
        file_count: snapshot.files.len(),
        total_bytes: snapshot.total_bytes,
        source_files,
    };
    let metadata_json = serde_json::to_string_pretty(&metadata)?;
    let html = validated_html.into_inner();

    replace_artifact_directory(&directory, &html, &metadata_json)?;

    Ok(TutorialArtifact {
        state: TutorialState::Fresh,
        current_hash: snapshot.content_hash.clone(),
        html: Some(html),
        metadata: Some(metadata),
        stale_reason: None,
    })
}

fn validate_css(css: &str) -> Result<(), AppError> {
    if css.contains('\\') {
        return Err(validation_error(
            "CSS escape sequences are not allowed in generated styles",
        ));
    }
    let comments = Regex::new(r"(?s)/\*.*?\*/").expect("static CSS comment regex");
    let normalized = comments.replace_all(css, "");
    if Regex::new(r"(?i)@import\b")
        .expect("static CSS import regex")
        .is_match(&normalized)
    {
        return Err(validation_error("CSS @import"));
    }
    if Regex::new(r"(?i)expression\s*\(")
        .expect("static CSS expression regex")
        .is_match(&normalized)
    {
        return Err(validation_error("CSS expression"));
    }
    if Regex::new(r"(?i)(?:display\s*:\s*none|visibility\s*:\s*hidden)")
        .expect("static hidden CSS regex")
        .is_match(&normalized)
    {
        return Err(validation_error(
            "generated CSS may not hide tutorial content",
        ));
    }

    let urls = Regex::new(r"(?is)url\s*\(([^)]*)\)").expect("static CSS url regex");
    for captures in urls.captures_iter(&normalized) {
        let value = captures
            .get(1)
            .map_or("", |capture| capture.as_str())
            .trim()
            .trim_matches(['\'', '"']);
        if !value.to_ascii_lowercase().starts_with("data:") {
            return Err(validation_error(format!(
                "CSS url is not an inline data resource: {value:?}"
            )));
        }
    }
    let without_urls = urls.replace_all(&normalized, "url(data:)");
    if Regex::new(r"(?i)(?:https?|ftp|file|blob):|//")
        .expect("static CSS network scheme regex")
        .is_match(&without_urls)
    {
        return Err(validation_error("CSS contains a network resource"));
    }
    Ok(())
}

fn validate_head_meta(
    element: &scraper::ElementRef<'_>,
    kind: &str,
    expected_content: &str,
) -> Result<(), AppError> {
    if element.value().name() != "meta" {
        return Err(validation_error(format!(
            "head element for {kind} must be meta"
        )));
    }
    let attributes = element.value().attrs().collect::<BTreeMap<_, _>>();
    match kind {
        "charset" => {
            if attributes.len() != 1
                || attributes
                    .get("charset")
                    .is_none_or(|value| !value.trim().eq_ignore_ascii_case(expected_content))
            {
                return Err(validation_error(
                    "the first head element must be exactly <meta charset=\"utf-8\">",
                ));
            }
        }
        "viewport" => {
            if attributes.len() != 2
                || attributes
                    .get("name")
                    .is_none_or(|value| !value.trim().eq_ignore_ascii_case("viewport"))
                || attributes.get("content").map(|value| value.trim()) != Some(expected_content)
            {
                return Err(validation_error(format!(
                    "the second head element must be the exact viewport meta: {expected_content}"
                )));
            }
        }
        "content-security-policy" => {
            if attributes.len() != 2
                || attributes.get("http-equiv").is_none_or(|value| {
                    !value.trim().eq_ignore_ascii_case("content-security-policy")
                })
                || attributes.get("content").map(|value| value.trim()) != Some(expected_content)
            {
                return Err(validation_error(format!(
                    "the third head element must be the exact CSP meta: {expected_content}"
                )));
            }
        }
        _ => unreachable!("static head meta kind"),
    }
    Ok(())
}

fn validate_dom(html: &str, expected_paths: &[String]) -> Result<(), AppError> {
    if expected_paths.is_empty() {
        return Err(validation_error("source file inventory is empty"));
    }

    let document = Html::parse_document(html);
    let html_selector = Selector::parse("html").expect("static html selector");
    let root = document
        .select(&html_selector)
        .next()
        .ok_or_else(|| validation_error("HTML root element is missing"))?;
    if root
        .value()
        .attrs()
        .any(|(name, value)| name != "lang" || value.trim().is_empty())
    {
        return Err(validation_error(
            "the html element may only have a non-empty lang attribute",
        ));
    }
    let head_selector = Selector::parse("html > head").expect("static head selector");
    let head = document
        .select(&head_selector)
        .next()
        .ok_or_else(|| validation_error("document head is missing"))?;
    if head.value().attrs().next().is_some() {
        return Err(validation_error(
            "the head element must not have attributes",
        ));
    }
    let head_child_selector = Selector::parse(":scope > *").expect("static head child selector");
    let head_children = head.select(&head_child_selector).collect::<Vec<_>>();
    if head_children.len() < 3 {
        return Err(validation_error(
            "head must begin with charset, viewport, and CSP meta elements",
        ));
    }
    validate_head_meta(&head_children[0], "charset", "utf-8")?;
    validate_head_meta(
        &head_children[1],
        "viewport",
        "width=device-width, initial-scale=1",
    )?;
    validate_head_meta(&head_children[2], "content-security-policy", REQUIRED_CSP)?;
    let meta_selector = Selector::parse("meta").expect("static meta selector");
    if document.select(&meta_selector).count() != 3 {
        return Err(validation_error(
            "HTML must contain exactly the three required leading meta elements",
        ));
    }

    let all = Selector::parse("*").expect("static all-elements selector");
    let forbidden = [
        "script",
        "iframe",
        "frame",
        "object",
        "embed",
        "form",
        "input",
        "button",
        "textarea",
        "select",
        "option",
        "base",
        "link",
        "video",
        "audio",
        "source",
        "track",
        "foreignobject",
        "animate",
        "set",
        "use",
        "template",
    ];
    let resource_attributes = [
        "href",
        "src",
        "action",
        "formaction",
        "poster",
        "data",
        "xlink:href",
        "background",
        "manifest",
        "longdesc",
    ];

    for element in document.select(&all) {
        let tag = element.value().name();
        if forbidden.contains(&tag) {
            return Err(validation_error(format!(
                "HTML contains forbidden <{tag}> element"
            )));
        }
        for (name, value) in element.value().attrs() {
            let name = name.to_ascii_lowercase();
            if name.starts_with("on") || name == "srcdoc" {
                return Err(validation_error(format!(
                    "HTML contains forbidden {name} attribute"
                )));
            }
            if name == "srcset"
                || name == "imagesrcset"
                || name == "ping"
                || name == "attributionsrc"
            {
                return Err(validation_error(format!(
                    "HTML contains forbidden network-capable {name} attribute"
                )));
            }
            if resource_attributes.contains(&name.as_str()) {
                validate_resource_value(tag, &name, value)?;
            }
            if name == "style" {
                validate_css(value)?;
            }
        }
        if tag == "style" {
            validate_css(&element.text().collect::<String>())?;
        }
    }

    let svg_selector = Selector::parse("svg").expect("static SVG selector");
    let Some(svg) = document.select(&svg_selector).next() else {
        return Err(validation_error(
            "HTML must include at least one informative inline SVG",
        ));
    };
    if !svg
        .value()
        .attr("role")
        .is_some_and(|role| role.eq_ignore_ascii_case("img"))
    {
        return Err(validation_error("the first SVG must declare role=\"img\""));
    }
    let title_selector = Selector::parse("title").expect("static SVG title selector");
    let has_accessible_name = svg
        .value()
        .attr("aria-label")
        .is_some_and(|label| !label.trim().is_empty())
        || svg
            .select(&title_selector)
            .any(|title| title.text().any(|text| !text.trim().is_empty()));
    if !has_accessible_name {
        return Err(validation_error(
            "the first SVG must have an aria-label or title",
        ));
    }

    let coverage_selector =
        Selector::parse("[data-skillstar-file]").expect("static coverage selector");
    let mut coverage = BTreeMap::<String, usize>::new();
    for element in document.select(&coverage_selector) {
        if let Some(path) = element.value().attr("data-skillstar-file") {
            if element.value().name() != "tr" {
                return Err(validation_error(format!(
                    "file coverage entry for {path:?} must be a table row"
                )));
            }
            let cell_selector = Selector::parse(":scope > td").expect("static table-cell selector");
            let cells = element.select(&cell_selector).collect::<Vec<_>>();
            if cells.len() < 4 {
                return Err(validation_error(format!(
                    "file coverage row for {path:?} must include path, role, tutorial location, and evidence cells"
                )));
            }
            let cell_text = cells
                .iter()
                .take(4)
                .map(|cell| cell.text().collect::<String>())
                .collect::<Vec<_>>();
            if cell_text[0].trim() != path {
                return Err(validation_error(format!(
                    "the first file coverage cell must exactly name its path: {path:?}"
                )));
            }
            if cell_text.iter().any(|text| text.trim().is_empty()) {
                return Err(validation_error(format!(
                    "file coverage row contains an empty required cell: {path:?}"
                )));
            }
            if element_is_explicitly_hidden(&element)
                || element
                    .ancestors()
                    .filter_map(scraper::ElementRef::wrap)
                    .find(|ancestor| ancestor.value().name() == "table")
                    .is_some_and(|table| element_is_explicitly_hidden(&table))
            {
                return Err(validation_error(format!(
                    "file coverage row is hidden: {path:?}"
                )));
            }
            *coverage.entry(path.to_string()).or_default() += 1;
        }
    }
    let expected = expected_paths.iter().cloned().collect::<BTreeSet<_>>();
    if expected.len() != expected_paths.len() {
        return Err(validation_error(
            "source inventory contains a duplicate path",
        ));
    }
    if let Some(path) = coverage.keys().find(|path| !expected.contains(*path)) {
        return Err(validation_error(format!(
            "file coverage appendix contains an unknown path: {path:?}"
        )));
    }
    if let Some((path, _)) = coverage.iter().find(|(_, count)| **count != 1) {
        return Err(validation_error(format!(
            "file coverage appendix must contain exactly one element for: {path:?}"
        )));
    }
    let mut missing = Vec::new();
    for path in expected_paths {
        if !coverage.contains_key(path) {
            missing.push(path.as_str());
        }
    }
    if !missing.is_empty() {
        let preview = missing.into_iter().take(12).collect::<Vec<_>>().join(", ");
        return Err(validation_error(format!(
            "file coverage appendix is missing data-skillstar-file entries: {preview}"
        )));
    }
    Ok(())
}

fn element_is_explicitly_hidden(element: &scraper::ElementRef<'_>) -> bool {
    element.value().attr("hidden").is_some()
        || element
            .value()
            .attr("aria-hidden")
            .is_some_and(|value| value.trim().eq_ignore_ascii_case("true"))
        || element.value().attr("style").is_some_and(|style| {
            let normalized = style.to_ascii_lowercase().replace(' ', "");
            normalized.contains("display:none") || normalized.contains("visibility:hidden")
        })
}

fn validate_resource_value(tag: &str, attribute: &str, value: &str) -> Result<(), AppError> {
    let value = value.trim();
    let normalized = value.to_ascii_lowercase();
    if value.is_empty()
        || value.starts_with('#')
        || normalized.starts_with("data:image/")
        || normalized.starts_with("data:font/")
        || normalized.starts_with("data:application/font")
    {
        return Ok(());
    }
    Err(validation_error(format!(
        "<{tag}> {attribute} is not local/offline: {value:?}"
    )))
}

#[cfg(test)]
fn escape_html_attribute(value: &str) -> String {
    value
        .replace('&', "&amp;")
        .replace('"', "&quot;")
        .replace('\'', "&#39;")
        .replace('<', "&lt;")
        .replace('>', "&gt;")
}

fn validation_error(message: impl Into<String>) -> AppError {
    AppError::Other(format!("Invalid Skill tutorial HTML: {}", message.into()))
}

fn missing_artifact(snapshot: &SkillSnapshot) -> TutorialArtifact {
    TutorialArtifact {
        state: TutorialState::Missing,
        current_hash: snapshot.content_hash.clone(),
        html: None,
        metadata: None,
        stale_reason: None,
    }
}

fn artifact_directory(skill_name: &str) -> PathBuf {
    skillstar_core::infra::paths::tutorials_dir().join(artifact_key(skill_name))
}

struct ArtifactIoGuard {
    _process: MutexGuard<'static, ()>,
    _file: File,
}

fn lock_artifact_io(root: &Path) -> Result<ArtifactIoGuard, AppError> {
    let process = ARTIFACT_IO_LOCK
        .lock()
        .map_err(|_| AppError::Other("Skill tutorial artifact lock is poisoned".to_string()))?;
    std::fs::create_dir_all(root)?;

    let lock_path = root.join(".artifacts.lock");
    let mut options = OpenOptions::new();
    options.read(true).write(true).create(true);
    #[cfg(unix)]
    {
        use std::os::unix::fs::OpenOptionsExt as _;
        options.mode(0o600);
    }
    let file = options.open(&lock_path)?;
    #[cfg(unix)]
    {
        use std::os::unix::fs::PermissionsExt as _;
        std::fs::set_permissions(&lock_path, std::fs::Permissions::from_mode(0o600))?;
    }
    file.lock()?;
    Ok(ArtifactIoGuard {
        _process: process,
        _file: file,
    })
}

/// Restore the last committed artifact if the process stopped after moving it
/// aside but before the staged replacement became visible.
fn recover_missing_artifact_directory(final_directory: &Path) -> Result<(), AppError> {
    if final_directory.exists() {
        return Ok(());
    }
    let Some(root) = final_directory.parent() else {
        return Ok(());
    };
    if !root.is_dir() {
        return Ok(());
    }
    let key = final_directory
        .file_name()
        .and_then(|name| name.to_str())
        .ok_or_else(|| AppError::Other("Tutorial artifact key is not valid UTF-8".to_string()))?;
    let prefix = format!(".{key}.");
    let mut backups = Vec::<(SystemTime, PathBuf)>::new();
    for entry in std::fs::read_dir(root)? {
        let entry = entry?;
        if !entry.file_type()?.is_dir() {
            continue;
        }
        let file_name = entry.file_name();
        let Some(file_name) = file_name.to_str() else {
            continue;
        };
        if file_name.starts_with(&prefix) && file_name.ends_with(".bak") {
            let modified = entry
                .metadata()?
                .modified()
                .unwrap_or(SystemTime::UNIX_EPOCH);
            backups.push((modified, entry.path()));
        }
    }
    backups.sort_by_key(|(modified, _)| *modified);
    if let Some((_, backup)) = backups.pop() {
        std::fs::rename(&backup, final_directory).map_err(|error| {
            AppError::Other(format!(
                "Failed to recover Skill tutorial artifact {} from {}: {error}",
                final_directory.display(),
                backup.display()
            ))
        })?;
        sync_directory(root)?;
    }
    Ok(())
}

fn artifact_key(skill_name: &str) -> String {
    let mut hasher = Sha256::new();
    hasher.update(ARTIFACT_KEY_DOMAIN);
    hasher.update((skill_name.len() as u64).to_le_bytes());
    hasher.update(skill_name.as_bytes());
    hex_digest(&hasher.finalize())
}

fn hex_digest(bytes: &[u8]) -> String {
    const HEX: &[u8; 16] = b"0123456789abcdef";
    let mut output = String::with_capacity(bytes.len() * 2);
    for byte in bytes {
        output.push(HEX[(byte >> 4) as usize] as char);
        output.push(HEX[(byte & 0x0f) as usize] as char);
    }
    output
}

fn replace_artifact_directory(
    final_directory: &Path,
    html: &str,
    metadata_json: &str,
) -> Result<(), AppError> {
    let root = final_directory.parent().ok_or_else(|| {
        AppError::Other(format!(
            "Tutorial artifact has no parent: {}",
            final_directory.display()
        ))
    })?;
    std::fs::create_dir_all(root)?;

    let key = final_directory
        .file_name()
        .and_then(|name| name.to_str())
        .ok_or_else(|| AppError::Other("Tutorial artifact key is not valid UTF-8".to_string()))?;
    let nonce = Uuid::new_v4();
    let staging = root.join(format!(".{key}.{nonce}.tmp"));
    let backup = root.join(format!(".{key}.{nonce}.bak"));

    std::fs::create_dir(&staging)?;
    let staged = (|| -> Result<(), AppError> {
        write_synced(&staging.join("tutorial.html"), html.as_bytes())?;
        write_synced(&staging.join("metadata.json"), metadata_json.as_bytes())?;
        sync_directory(&staging)?;
        Ok(())
    })();
    if let Err(error) = staged {
        let _ = std::fs::remove_dir_all(&staging);
        return Err(error);
    }

    let had_previous = final_directory.exists();
    if had_previous {
        if let Err(error) = std::fs::rename(final_directory, &backup) {
            let _ = std::fs::remove_dir_all(&staging);
            return Err(AppError::Other(format!(
                "Failed to stage the previous Skill tutorial artifact: {error}"
            )));
        }
        sync_directory(root)?;
    }
    match std::fs::rename(&staging, final_directory) {
        Ok(()) => {
            sync_directory(root)?;
            if had_previous {
                let _ = std::fs::remove_dir_all(&backup);
                sync_directory(root)?;
            }
            Ok(())
        }
        Err(error) => {
            let _ = std::fs::remove_dir_all(&staging);
            let restore_error = had_previous
                .then(|| std::fs::rename(&backup, final_directory).err())
                .flatten();
            let sync_error = sync_directory(root).err();
            match restore_error {
                Some(restore_error) => Err(AppError::Other(format!(
                    "Failed to replace Skill tutorial artifact: {error}; restoring the previous artifact also failed: {restore_error}"
                ))),
                None if sync_error.is_some() => Err(AppError::Other(format!(
                    "Failed to replace Skill tutorial artifact: {error}; rollback directory sync also failed: {}",
                    sync_error.expect("checked above")
                ))),
                None => Err(AppError::Other(format!(
                    "Failed to replace Skill tutorial artifact: {error}"
                ))),
            }
        }
    }
}

fn write_synced(path: &Path, content: &[u8]) -> Result<(), AppError> {
    let mut file = File::create(path)?;
    file.write_all(content)?;
    file.sync_all()?;
    Ok(())
}

#[cfg(unix)]
fn sync_directory(path: &Path) -> Result<(), AppError> {
    File::open(path)?.sync_all()?;
    Ok(())
}

#[cfg(not(unix))]
fn sync_directory(_path: &Path) -> Result<(), AppError> {
    Ok(())
}

#[cfg(test)]
mod tests;
