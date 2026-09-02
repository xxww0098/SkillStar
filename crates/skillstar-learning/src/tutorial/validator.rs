//! CSP-strict HTML validation and whole-Skill file coverage.

use std::collections::{BTreeMap, BTreeSet};

use regex::Regex;
use scraper::{Html, Selector};
use skillstar_core::infra::error::AppError;

pub const REQUIRED_CSP: &str =
    "default-src 'none'; style-src 'unsafe-inline'; img-src data:; font-src data:";
const MAX_HTML_BYTES: usize = 2 * 1024 * 1024;

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct ValidatedTutorialHtml(pub(crate) String);

impl ValidatedTutorialHtml {
    pub fn as_str(&self) -> &str {
        &self.0
    }

    pub fn into_inner(self) -> String {
        self.0
    }
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

pub(crate) fn validation_error(message: impl Into<String>) -> AppError {
    AppError::Other(format!("Invalid Skill tutorial HTML: {}", message.into()))
}

#[cfg(test)]
pub(crate) fn escape_html_attribute(value: &str) -> String {
    value
        .replace('&', "&amp;")
        .replace('"', "&quot;")
        .replace('\'', "&#39;")
        .replace('<', "&lt;")
        .replace('>', "&gt;")
}
