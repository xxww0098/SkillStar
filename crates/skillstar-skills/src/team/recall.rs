//! BM25 recall over installed skills and local learnings, plus neighbor boost.

use std::collections::{HashMap, HashSet};

use chrono::{DateTime, Utc};
use serde::{Deserialize, Serialize};
use skillstar_core::infra::error::AppError;
use skillstar_core::types::parse_skill_content;

use super::store::{self, RecallEvent};

const K1: f32 = 1.2;
const B: f32 = 0.75;
const NEIGHBOR_BOOST: f32 = 1.5;

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum RecallKind {
    Skill,
    Learning,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct RecallHit {
    pub kind: RecallKind,
    pub id: String,
    pub title: String,
    pub snippet: String,
    pub score: f32,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub skill_name: Option<String>,
}

struct Doc {
    kind: RecallKind,
    id: String,
    title: String,
    snippet: String,
    skill_name: Option<String>,
    tokens: Vec<String>,
    neighbors: Vec<String>,
}

pub fn recall(query: &str, limit: usize, now: DateTime<Utc>) -> Result<Vec<RecallHit>, AppError> {
    let tokens = tokenize(query);
    if tokens.is_empty() {
        return Ok(Vec::new());
    }
    let limit = limit.clamp(1, 50);
    let docs = corpus()?;
    if docs.is_empty() {
        return Ok(Vec::new());
    }

    let scored = score_docs(&tokens, &docs);
    let hits: Vec<RecallHit> = scored
        .into_iter()
        .take(limit)
        .map(|(doc, score)| RecallHit {
            kind: doc.kind,
            id: doc.id.clone(),
            title: doc.title.clone(),
            snippet: doc.snippet.clone(),
            score,
            skill_name: doc.skill_name.clone(),
        })
        .collect();

    if !hits.is_empty() {
        store::mutate(|store| {
            for hit in &hits {
                store.recall_events.push(RecallEvent {
                    id: hit.id.clone(),
                    kind: hit.kind,
                    at: now,
                });
            }
            Ok(())
        })?;
    }

    Ok(hits)
}

/// BM25 over installed skills only. Does not read or write `state/team.json`.
pub struct InstalledSkillHit {
    pub id: String,
    pub title: String,
    pub snippet: String,
    pub score: f32,
}

pub fn search_installed_skills(
    query: &str,
    limit: usize,
) -> Result<Vec<InstalledSkillHit>, AppError> {
    let tokens = tokenize(query);
    if tokens.is_empty() {
        return Ok(Vec::new());
    }
    let limit = limit.clamp(1, 12);
    let docs = skill_docs()?;
    if docs.is_empty() {
        return Ok(Vec::new());
    }
    Ok(score_docs(&tokens, &docs)
        .into_iter()
        .take(limit)
        .map(|(doc, score)| InstalledSkillHit {
            id: doc.id.clone(),
            title: doc.title.clone(),
            snippet: doc.snippet.clone(),
            score,
        })
        .collect())
}

fn skill_docs() -> Result<Vec<Doc>, AppError> {
    let mut docs = Vec::new();
    for name in installed_skill_names() {
        let Ok(raw) = read_skill_text(&name) else {
            continue;
        };
        let parsed = parse_skill_content(name.clone(), raw);
        let description = parsed.description.clone().unwrap_or_default();
        let body = parsed.content.trim();
        let snippet = if description.is_empty() {
            snippet_of(body)
        } else {
            description.clone()
        };
        let weighted = format!("{name} {name} {name} {name} {description} {description} {body}");
        docs.push(Doc {
            kind: RecallKind::Skill,
            id: name.clone(),
            title: name.clone(),
            snippet,
            skill_name: Some(name),
            tokens: tokenize(&weighted),
            neighbors: Vec::new(),
        });
    }
    Ok(docs)
}

pub(crate) fn tokenize(query: &str) -> Vec<String> {
    let mut tokens = Vec::new();
    let mut latin = String::new();
    let mut cjk: Vec<char> = Vec::new();

    let flush_latin = |latin: &mut String, tokens: &mut Vec<String>| {
        if latin.is_empty() {
            return;
        }
        let lowered = latin.to_ascii_lowercase();
        latin.clear();
        if lowered.len() < 2 || is_stopword(&lowered) {
            return;
        }
        tokens.push(lowered);
    };
    let flush_cjk = |cjk: &mut Vec<char>, tokens: &mut Vec<String>| {
        if cjk.is_empty() {
            return;
        }
        for ch in cjk.iter() {
            tokens.push(ch.to_string());
        }
        for window in cjk.windows(2) {
            tokens.push(format!("{}{}", window[0], window[1]));
        }
        cjk.clear();
    };

    for ch in query.chars() {
        if ch.is_ascii_alphanumeric() {
            flush_cjk(&mut cjk, &mut tokens);
            latin.push(ch);
        } else if is_cjk(ch) {
            flush_latin(&mut latin, &mut tokens);
            cjk.push(ch);
        } else {
            flush_latin(&mut latin, &mut tokens);
            flush_cjk(&mut cjk, &mut tokens);
        }
    }
    flush_latin(&mut latin, &mut tokens);
    flush_cjk(&mut cjk, &mut tokens);
    tokens
}

fn is_cjk(ch: char) -> bool {
    matches!(ch,
        '\u{4e00}'..='\u{9fff}'
        | '\u{3400}'..='\u{4dbf}'
        | '\u{f900}'..='\u{faff}'
    )
}

fn is_stopword(token: &str) -> bool {
    matches!(
        token,
        "a" | "an" | "the" | "and" | "or" | "of" | "to" | "for" | "in" | "on" | "is" | "it" | "at"
    )
}

fn corpus() -> Result<Vec<Doc>, AppError> {
    let mut docs = skill_docs()?;
    let store = store::load()?;
    let mut neighbors: HashMap<String, HashSet<String>> = HashMap::new();
    for learning in &store.learnings {
        if let Some(skill) = learning.skill_name.as_ref() {
            neighbors
                .entry(learning.id.clone())
                .or_default()
                .insert(skill.clone());
            neighbors
                .entry(skill.clone())
                .or_default()
                .insert(learning.id.clone());
        }
        let tags = learning.tags.join(" ");
        let weighted = format!(
            "{} {} {} {tags} {}",
            learning.title, learning.title, learning.title, learning.body
        );
        docs.push(Doc {
            kind: RecallKind::Learning,
            id: learning.id.clone(),
            title: learning.title.clone(),
            snippet: snippet_of(&learning.body),
            skill_name: learning.skill_name.clone(),
            tokens: tokenize(&weighted),
            neighbors: learning.skill_name.clone().into_iter().collect(),
        });
    }

    for doc in &mut docs {
        if let Some(set) = neighbors.get(&doc.id) {
            doc.neighbors.extend(set.iter().cloned());
        }
    }
    Ok(docs)
}

pub(crate) fn installed_skill_names() -> Vec<String> {
    let mut names = std::collections::BTreeSet::new();
    for root in [
        skillstar_core::infra::paths::hub_skills_dir(),
        skillstar_core::infra::paths::local_skills_dir(),
    ] {
        let Ok(entries) = std::fs::read_dir(root) else {
            continue;
        };
        for entry in entries.flatten() {
            let name = entry.file_name().to_string_lossy().to_string();
            if name.starts_with('.') {
                continue;
            }
            if crate::content::validate_skill_name(&name).is_err() {
                continue;
            }
            if read_skill_text(&name).is_ok() {
                names.insert(name);
            }
        }
    }
    names.into_iter().collect()
}

fn read_skill_text(name: &str) -> Result<String, AppError> {
    if let Ok(raw) = crate::content::read_raw(name) {
        return Ok(raw);
    }
    let local = skillstar_core::infra::paths::local_skills_dir()
        .join(name)
        .join("SKILL.md");
    if local.is_file() {
        return Ok(std::fs::read_to_string(local)?);
    }
    Err(AppError::SkillNotFound {
        name: name.to_string(),
    })
}

fn snippet_of(text: &str) -> String {
    let flat: String = text
        .lines()
        .map(str::trim)
        .filter(|line| !line.is_empty() && !line.starts_with('#'))
        .collect::<Vec<_>>()
        .join(" ");
    let mut chars = flat.chars();
    let taken: String = chars.by_ref().take(160).collect();
    if chars.next().is_some() {
        format!("{taken}…")
    } else {
        taken
    }
}

fn score_docs<'a>(query: &[String], docs: &'a [Doc]) -> Vec<(&'a Doc, f32)> {
    let n_docs = docs.len() as f32;
    let avgdl = docs.iter().map(|doc| doc.tokens.len() as f32).sum::<f32>() / n_docs.max(1.0);

    let mut df: HashMap<&str, usize> = HashMap::new();
    for doc in docs {
        let unique: HashSet<&str> = doc.tokens.iter().map(String::as_str).collect();
        for token in unique {
            *df.entry(token).or_insert(0) += 1;
        }
    }

    let mut lexical: Vec<(&Doc, f32)> = docs
        .iter()
        .filter_map(|doc| {
            let score = bm25(query, &doc.tokens, avgdl, n_docs, &df);
            (score > 0.0).then_some((doc, score))
        })
        .collect();

    let matched_ids: HashSet<&str> = lexical.iter().map(|(doc, _)| doc.id.as_str()).collect();
    for (doc, score) in &mut lexical {
        if doc
            .neighbors
            .iter()
            .any(|neighbor| matched_ids.contains(neighbor.as_str()))
        {
            *score += NEIGHBOR_BOOST;
        }
    }

    let admitted: HashSet<&str> = lexical.iter().map(|(doc, _)| doc.id.as_str()).collect();
    for doc in docs {
        if admitted.contains(doc.id.as_str()) {
            continue;
        }
        if doc
            .neighbors
            .iter()
            .any(|neighbor| matched_ids.contains(neighbor.as_str()))
        {
            lexical.push((doc, NEIGHBOR_BOOST));
        }
    }

    lexical.sort_by(|a, b| b.1.partial_cmp(&a.1).unwrap_or(std::cmp::Ordering::Equal));
    lexical
}

fn bm25(
    query: &[String],
    doc_tokens: &[String],
    avgdl: f32,
    n_docs: f32,
    df: &HashMap<&str, usize>,
) -> f32 {
    if doc_tokens.is_empty() {
        return 0.0;
    }
    let mut tf: HashMap<&str, usize> = HashMap::new();
    for token in doc_tokens {
        *tf.entry(token.as_str()).or_insert(0) += 1;
    }
    let doc_len = doc_tokens.len() as f32;
    let mut score = 0.0;
    let mut seen = HashSet::new();
    for term in query {
        if !seen.insert(term.as_str()) {
            continue;
        }
        let term_tf = *tf.get(term.as_str()).unwrap_or(&0) as f32;
        if term_tf == 0.0 {
            continue;
        }
        let n = *df.get(term.as_str()).unwrap_or(&0) as f32;
        let idf = ((n_docs - n + 0.5) / (n + 0.5) + 1.0).ln();
        let denom = term_tf + K1 * (1.0 - B + B * (doc_len / avgdl.max(1.0)));
        score += idf * (term_tf * (K1 + 1.0)) / denom;
    }
    score
}
