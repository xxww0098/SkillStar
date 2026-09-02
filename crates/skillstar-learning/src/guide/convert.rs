//! Explicit HTML → Block JSON conversion. Fail closed on unknown structure.

use scraper::{ElementRef, Html, Node, Selector};
use skillstar_core::infra::error::AppError;

use super::{
    CalloutTone, GuideBlock, GuideDraft, GuideStep, GuideStepKind, blocks::BLOCK_SCHEMA_VERSION,
};
use crate::tutorial::{PrivateTutorial, TutorialState};

const ALLOWED_FLOW: &[&str] = &[
    "h1", "h2", "h3", "p", "ul", "ol", "li", "pre", "code", "blockquote", "strong", "em", "span",
    "div", "table", "thead", "tbody", "tr", "th", "td", "br", "hr", "svg", "title", "path", "g",
    "circle", "rect", "line", "polyline", "polygon", "text", "tspan", "style",
];

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct ConversionPreview {
    pub title: String,
    pub locale: String,
    pub source_tutorial_key: String,
    pub skill_identity: crate::identity::SkillIdentity,
    pub skill_revision: crate::identity::SkillRevision,
    pub steps: Vec<GuideStep>,
}

pub fn preview(tutorial: &PrivateTutorial, locale: &str) -> Result<ConversionPreview, AppError> {
    if !tutorial.bound {
        return Err(AppError::Other(
            "Unbound legacy tutorials cannot be converted into a Guide Draft".to_string(),
        ));
    }
    if tutorial.state == TutorialState::Missing {
        return Err(AppError::Other(
            "A Guide Draft requires a stored private tutorial".to_string(),
        ));
    }
    let metadata = tutorial.metadata.as_ref().ok_or_else(|| {
        AppError::Other("A Guide Draft requires bound tutorial metadata".to_string())
    })?;
    let identity = metadata
        .identity
        .clone()
        .ok_or_else(|| AppError::Other("A Guide Draft requires a bound Skill identity".to_string()))?
        .verified()?;
    let revision = metadata
        .generated_revision
        .clone()
        .ok_or_else(|| AppError::Other("A Guide Draft requires a bound Skill revision".to_string()))?
        .verified(&identity)?;
    let html = tutorial.html.as_deref().ok_or_else(|| {
        AppError::Other("A Guide Draft requires stored tutorial HTML".to_string())
    })?;
    let locale = locale.trim();
    if locale.is_empty() {
        return Err(AppError::Other(
            "Guide Draft conversion requires a locale".to_string(),
        ));
    }
    let document = Html::parse_document(html);
    reject_unknown_tags(&document)?;
    let body = document
        .select(&Selector::parse("body").expect("static body selector"))
        .next()
        .ok_or_else(|| AppError::Other("Tutorial HTML has no body".to_string()))?;
    let steps = steps_from_body(body)?;
    let title = document
        .select(&Selector::parse("h1").expect("static h1 selector"))
        .next()
        .map(|node| collapse_text(node))
        .filter(|text| !text.is_empty())
        .unwrap_or_else(|| "Converted Guide Draft".to_string());
    Ok(ConversionPreview {
        title,
        locale: locale.to_string(),
        source_tutorial_key: identity.key.storage_segment(),
        skill_identity: identity,
        skill_revision: revision,
        steps,
    })
}

impl ConversionPreview {
    pub fn into_draft(self, converted_at: String, revision_key: String) -> GuideDraft {
        GuideDraft {
            id: format!("draft:{}", self.source_tutorial_key),
            title: self.title,
            locale: self.locale,
            schema_version: BLOCK_SCHEMA_VERSION.to_string(),
            skill_identity: self.skill_identity,
            skill_revision: self.skill_revision,
            source_tutorial_key: self.source_tutorial_key,
            converted_at,
            revision_key,
            steps: self.steps,
        }
    }
}

fn reject_unknown_tags(document: &Html) -> Result<(), AppError> {
    let all = Selector::parse("*").expect("static all selector");
    let structural = [
        "html", "head", "body", "meta", "title", "style", "svg", "path", "g", "circle", "rect",
        "line", "polyline", "polygon", "text", "tspan", "defs", "title",
    ];
    for element in document.select(&all) {
        let tag = element.value().name();
        if structural.contains(&tag) || ALLOWED_FLOW.contains(&tag) {
            continue;
        }
        return Err(AppError::Other(format!(
            "Guide Draft conversion failed closed on unknown <{tag}> element"
        )));
    }
    Ok(())
}

fn steps_from_body(body: ElementRef<'_>) -> Result<Vec<GuideStep>, AppError> {
    let mut steps = Vec::new();
    let mut current_id = 1u32;
    let mut current_title = "Overview".to_string();
    let mut current_kind = GuideStepKind::Reading;
    let mut current_blocks: Vec<GuideBlock> = Vec::new();
    let mut saw_diagram = false;

    for child in body.children() {
        let Node::Element(_) = child.value() else {
            continue;
        };
        let Some(element) = ElementRef::wrap(child) else {
            continue;
        };
        let tag = element.value().name();
        if tag == "h1" || tag == "h2" {
            if !current_blocks.is_empty() {
                steps.push(finish_step(
                    current_id,
                    current_title.clone(),
                    current_kind,
                    std::mem::take(&mut current_blocks),
                )?);
                current_id += 1;
            }
            current_title = collapse_text(element);
            current_kind = classify_heading(&current_title);
            current_blocks.push(GuideBlock::Heading {
                level: if tag == "h1" { 1 } else { 2 },
                text: current_title.clone(),
            });
            continue;
        }
        if tag == "svg" {
            if !saw_diagram {
                current_blocks.push(GuideBlock::Callout {
                    tone: CalloutTone::Info,
                    text: "Inline diagram from the private tutorial (SVG is not embedded in Block JSON).".into(),
                });
                saw_diagram = true;
            }
            continue;
        }
        if tag == "style" {
            continue;
        }
        push_blocks(&mut current_blocks, element)?;
    }

    if current_blocks.is_empty() {
        return Err(AppError::Other(
            "Guide Draft conversion produced no blocks".to_string(),
        ));
    }
    steps.push(finish_step(
        current_id,
        current_title,
        current_kind,
        current_blocks,
    )?);
    Ok(steps)
}

fn finish_step(
    index: u32,
    title: String,
    kind: GuideStepKind,
    blocks: Vec<GuideBlock>,
) -> Result<GuideStep, AppError> {
    let requires_skill = kind == GuideStepKind::Practice;
    Ok(GuideStep {
        id: format!("d{index}"),
        kind,
        title,
        requires_skill,
        blocks: super::blocks::verify_blocks(blocks)?,
    })
}

fn classify_heading(title: &str) -> GuideStepKind {
    let lower = title.to_ascii_lowercase();
    if lower.contains("practice") || lower.contains("实践") || lower.contains("workshop") {
        GuideStepKind::Practice
    } else if lower.contains("verify") || lower.contains("验收") || lower.contains("check") {
        GuideStepKind::Verify
    } else {
        GuideStepKind::Reading
    }
}

fn push_blocks(blocks: &mut Vec<GuideBlock>, element: ElementRef<'_>) -> Result<(), AppError> {
    match element.value().name() {
        "h3" => blocks.push(GuideBlock::Heading {
            level: 3,
            text: collapse_text(element),
        }),
        "p" => {
            let text = collapse_text(element);
            if !text.is_empty() {
                blocks.push(GuideBlock::Paragraph { text });
            }
        }
        "ul" | "ol" => {
            let items = list_items(element)?;
            if !items.is_empty() {
                blocks.push(GuideBlock::List {
                    ordered: element.value().name() == "ol",
                    items,
                });
            }
        }
        "pre" | "code" => {
            let code = element.text().collect::<String>();
            if !code.trim().is_empty() {
                blocks.push(GuideBlock::Code {
                    language: String::new(),
                    code: code.trim_end().to_string(),
                });
            }
        }
        "blockquote" => {
            let text = collapse_text(element);
            if !text.is_empty() {
                blocks.push(GuideBlock::Callout {
                    tone: CalloutTone::Info,
                    text,
                });
            }
        }
        "table" => blocks.push(GuideBlock::Callout {
            tone: CalloutTone::Info,
            text: table_as_text(element),
        }),
        "div" | "span" => {
            for child in element.children() {
                if let Some(child) = ElementRef::wrap(child) {
                    push_blocks(blocks, child)?;
                }
            }
        }
        "hr" | "br" => {}
        other if ALLOWED_FLOW.contains(&other) => {}
        other => {
            return Err(AppError::Other(format!(
                "Guide Draft conversion failed closed on unknown <{other}> element"
            )));
        }
    }
    Ok(())
}

fn list_items(list: ElementRef<'_>) -> Result<Vec<String>, AppError> {
    let mut items = Vec::new();
    for child in list.children() {
        let Some(element) = ElementRef::wrap(child) else {
            continue;
        };
        match element.value().name() {
            "li" => {
                let text = collapse_text(element);
                if !text.is_empty() {
                    items.push(text);
                }
            }
            other => {
                return Err(AppError::Other(format!(
                    "Guide Draft conversion failed closed on unknown list child <{other}>"
                )));
            }
        }
    }
    Ok(items)
}

fn table_as_text(table: ElementRef<'_>) -> String {
    table
        .text()
        .map(str::trim)
        .filter(|part| !part.is_empty())
        .collect::<Vec<_>>()
        .join(" · ")
}

fn collapse_text(element: ElementRef<'_>) -> String {
    element
        .text()
        .map(str::trim)
        .filter(|part| !part.is_empty())
        .collect::<Vec<_>>()
        .join(" ")
}
