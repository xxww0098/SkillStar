//! Sequence layout copied from receptron/laya `rl_common.build_sequence`.
//! Each skill is its own noul question. Option order is false, then true,
//! so the calibrated probability of true is softmax slot 1.

use std::path::Path;

use serde_json::Value;

use super::ranker::RankedCandidate;

pub(crate) const MAX_SCORED: usize = 12;
const NOUL_QTYPE: i64 = 2;
const FALSE_OPTION: &str = "false: no, the statement does not hold";
const TRUE_OPTION: &str = "true: yes, the statement holds";

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub(crate) enum BundleLanguage {
    English,
    Multilingual,
}

#[derive(Clone, Debug)]
pub(crate) struct Calibration {
    pub max_len: usize,
    pub head_max_len: usize,
    pub noul_temperature: f32,
    pub language: BundleLanguage,
    pub cls_token: String,
    pub sep_token: String,
    pub mask_token: String,
    pub pad_token: String,
}

#[derive(Clone, Debug)]
pub(crate) struct SpecialIds {
    pub cls: u32,
    pub sep: u32,
    pub mask: u32,
    pub pad: u32,
    pub mask_token: String,
}

#[derive(Clone, Debug)]
pub(crate) struct PackedNoul {
    pub ids: Vec<i64>,
    pub markers: [i64; 2],
}

pub(crate) fn configured_bundle_dir() -> Option<std::path::PathBuf> {
    if let Some(dir) = std::env::var_os("SKILLSTAR_LAYA_ONNX") {
        if dir.is_empty() {
            return None;
        }
        return Some(std::path::PathBuf::from(dir));
    }
    // Tests must not open the developer's real bundle just because it sits
    // under ~/.skillstar. The app binary still defaults there.
    #[cfg(test)]
    {
        return None;
    }
    #[cfg(not(test))]
    Some(skillstar_core::infra::paths::laya_model_dir())
}

pub(crate) fn bundle_is_complete(dir: &Path) -> bool {
    dir.join("laya.onnx").is_file()
        && dir.join("laya.onnx.data").is_file()
        && dir.join("laya_config.json").is_file()
        && dir.join("tokenizer").join("tokenizer.json").is_file()
        && dir
            .join("tokenizer")
            .join("tokenizer_config.json")
            .is_file()
}

pub(crate) fn task_requires_multilingual(task: &str) -> bool {
    task.chars().any(|c| {
        matches!(
            c,
            '\u{3400}'..='\u{4DBF}' | '\u{4E00}'..='\u{9FFF}' | '\u{F900}'..='\u{FAFF}'
        )
    })
}

pub(crate) fn read_calibration(dir: &Path) -> Option<Calibration> {
    let config = std::fs::read_to_string(dir.join("laya_config.json")).ok()?;
    let tokenizer =
        std::fs::read_to_string(dir.join("tokenizer").join("tokenizer_config.json")).ok()?;
    calibration_from_json(&config, &tokenizer)
}

pub(crate) fn calibration_from_json(
    laya_config: &str,
    tokenizer_config: &str,
) -> Option<Calibration> {
    let config: Value = serde_json::from_str(laya_config).ok()?;
    let tokenizer: Value = serde_json::from_str(tokenizer_config).ok()?;
    let max_len = config.get("max_len")?.as_u64()? as usize;
    let head_max_len = config.get("head_max_len")?.as_u64()? as usize;
    if max_len < 8 || head_max_len < 8 {
        return None;
    }
    let language = language_of(&config, &tokenizer)?;
    let cls_token = token_string(tokenizer.get("cls_token")?)?.to_string();
    let sep_token = token_string(tokenizer.get("sep_token")?)?.to_string();
    let mask_token = token_string(tokenizer.get("mask_token")?)?.to_string();
    let pad_token = token_string(tokenizer.get("pad_token")?)?.to_string();
    Some(Calibration {
        max_len,
        head_max_len,
        noul_temperature: noul_temperature(&config),
        language,
        cls_token,
        sep_token,
        mask_token,
        pad_token,
    })
}

pub(crate) fn language_allows(task: &str, language: BundleLanguage) -> bool {
    !task_requires_multilingual(task) || language == BundleLanguage::Multilingual
}

pub(crate) fn skill_instruction(name: &str, description: &str) -> String {
    let description = description.trim();
    if description.is_empty() {
        format!("Does the skill {name} apply to this task?")
    } else {
        format!("Does the skill {name} apply to this task? {description}")
    }
}

pub(crate) fn build_noul(
    encode: impl Fn(&str) -> Option<Vec<u32>>,
    special: &SpecialIds,
    instruction: &str,
    state: &str,
    max_len: usize,
    head_max_len: usize,
) -> Option<PackedNoul> {
    let scrub = |text: &str| text.replace(&special.mask_token, " ");
    let mut head_ids = encode(&format!("noul question: {}", scrub(instruction)))?;
    let mut opt_ids = [FALSE_OPTION, TRUE_OPTION].map(|option| {
        let mut ids = vec![special.mask];
        ids.extend(
            encode(&format!(" {}", scrub(option)))
                .unwrap_or_default()
                .into_iter()
                .take(48),
        );
        ids
    });
    if opt_ids.iter().any(|ids| ids.len() < 2) {
        return None;
    }
    let mut opt_budget = head_max_len.saturating_sub(opt_ids.iter().map(Vec::len).sum());
    if opt_budget < 16 {
        let per = (head_max_len.saturating_sub(16) / opt_ids.len().max(1)).max(4);
        for ids in &mut opt_ids {
            ids.truncate(per);
        }
        opt_budget = head_max_len.saturating_sub(opt_ids.iter().map(Vec::len).sum());
    }
    head_ids.truncate(opt_budget.max(8));
    let mut ids = Vec::with_capacity(max_len.min(head_ids.len() + 64));
    ids.push(special.cls);
    ids.extend(head_ids);
    ids.push(special.sep);
    let mut markers = [0_i64; 2];
    for (index, option) in opt_ids.iter().enumerate() {
        markers[index] = i64::try_from(ids.len()).ok()?;
        ids.extend(option.iter().copied());
    }
    ids.push(special.sep);
    let room = max_len.saturating_sub(ids.len().saturating_add(1));
    ids.extend(encode(&scrub(state))?.into_iter().take(room));
    ids.push(special.sep);
    ids.truncate(max_len);
    if markers
        .iter()
        .any(|marker| *marker < 0 || *marker as usize >= ids.len())
    {
        return None;
    }
    if markers
        .iter()
        .any(|marker| ids[*marker as usize] != special.mask)
    {
        return None;
    }
    Some(PackedNoul {
        ids: ids.into_iter().map(i64::from).collect(),
        markers,
    })
}

pub(crate) fn noul_probability(false_logit: f32, true_logit: f32, temperature: f32) -> Option<f32> {
    let temperature = if temperature.is_finite() && temperature > 0.0 {
        temperature
    } else {
        1.0
    };
    let false_z = false_logit / temperature;
    let true_z = true_logit / temperature;
    if !false_z.is_finite() || !true_z.is_finite() {
        return None;
    }
    let max = false_z.max(true_z);
    let false_e = (false_z - max).exp();
    let true_e = (true_z - max).exp();
    let probability = true_e / (false_e + true_e);
    probability.is_finite().then_some(probability)
}

pub(crate) fn order_by_score(candidates: &mut [RankedCandidate]) {
    let scored = candidates.len().min(MAX_SCORED);
    candidates[..scored].sort_by(|left, right| right.score.total_cmp(&left.score));
}

fn language_of(config: &Value, tokenizer: &Value) -> Option<BundleLanguage> {
    if let Some(language) = config.get("language") {
        return match language.as_str()? {
            "en" | "english" => Some(BundleLanguage::English),
            "multilingual" => Some(BundleLanguage::Multilingual),
            _ => None,
        };
    }
    let cls = token_string(tokenizer.get("cls_token")?)?;
    let sep = token_string(tokenizer.get("sep_token")?)?;
    let mask = token_string(tokenizer.get("mask_token")?)?;
    let pad = token_string(tokenizer.get("pad_token")?)?;
    if cls == "[CLS]" && sep == "[SEP]" && mask == "[MASK]" && pad == "[PAD]" {
        return Some(BundleLanguage::English);
    }
    if cls == "<bos>" && sep == "<eos>" && mask == "<mask>" && pad == "<pad>" {
        return Some(BundleLanguage::Multilingual);
    }
    None
}

fn noul_temperature(config: &Value) -> f32 {
    config
        .get("temperature_by_options")
        .and_then(|value| value.get("noul:2"))
        .and_then(Value::as_f64)
        .or_else(|| {
            config
                .get("temperature")
                .and_then(Value::as_array)
                .and_then(|values| values.get(NOUL_QTYPE as usize))
                .and_then(Value::as_f64)
        })
        .map(|value| value as f32)
        .filter(|value| value.is_finite() && *value > 0.0)
        .unwrap_or(1.0)
}

fn token_string(value: &Value) -> Option<&str> {
    value
        .as_str()
        .or_else(|| value.get("content").and_then(Value::as_str))
}

#[cfg(test)]
mod tests {
    use super::*;

    fn english_tokenizer() -> &'static str {
        r#"{"cls_token":"[CLS]","sep_token":"[SEP]","mask_token":"[MASK]","pad_token":"[PAD]"}"#
    }

    fn multilingual_tokenizer() -> &'static str {
        r#"{"cls_token":"<bos>","sep_token":"<eos>","mask_token":"<mask>","pad_token":"<pad>"}"#
    }

    fn config(language: &str) -> String {
        format!(
            r#"{{"max_len":512,"head_max_len":192,"temperature":[1.0,1.0,1.5],"language":"{language}"}}"#
        )
    }

    #[test]
    fn published_english_tokenizer_is_english_and_not_multilingual() {
        let config = r#"{"max_len":512,"head_max_len":192,"temperature":[1.0,1.0,2.0],"temperature_by_options":{"noul:2":1.25}}"#;
        let calibration = calibration_from_json(config, english_tokenizer()).unwrap();
        assert_eq!(calibration.language, BundleLanguage::English);
        assert!((calibration.noul_temperature - 1.25).abs() < 1e-6);
        assert!(!language_allows("请推荐退款技能", calibration.language));
        assert!(language_allows("refund this invoice", calibration.language));
    }

    #[test]
    fn mmbert_tokenizer_is_multilingual_and_explicit_language_wins() {
        let bare = r#"{"max_len":1024,"head_max_len":256,"temperature":[1.0,1.0,1.0]}"#;
        let calibration = calibration_from_json(bare, multilingual_tokenizer()).unwrap();
        assert_eq!(calibration.language, BundleLanguage::Multilingual);
        assert!(language_allows("请推荐退款技能", calibration.language));

        let forced = calibration_from_json(&config("multilingual"), english_tokenizer()).unwrap();
        assert_eq!(forced.language, BundleLanguage::Multilingual);
    }

    #[test]
    fn unknown_language_refuses_the_bundle() {
        let bare = r#"{"max_len":512,"head_max_len":192}"#;
        let tokenizer =
            r#"{"cls_token":"<s>","sep_token":"</s>","mask_token":"<mask>","pad_token":"<pad>"}"#;
        assert!(calibration_from_json(bare, tokenizer).is_none());
        assert!(calibration_from_json(&config("fr"), english_tokenizer()).is_none());
        assert!(task_requires_multilingual("部署技能"));
        assert!(!task_requires_multilingual("deploy the skill"));
    }

    #[test]
    fn noul_markers_point_at_mask_tokens_and_ties_keep_bm25_order() {
        let special = SpecialIds {
            cls: 1,
            sep: 2,
            mask: 3,
            pad: 0,
            mask_token: "[MASK]".into(),
        };
        let encode = |text: &str| Some(text.chars().map(|c| c as u32).collect());
        let packed = build_noul(&encode, &special, "apply?", "task", 512, 192).unwrap();
        assert_eq!(packed.ids[0], 1);
        assert_eq!(*packed.ids.last().unwrap(), 2);
        assert_eq!(packed.markers.len(), 2);
        for marker in packed.markers {
            assert_eq!(packed.ids[marker as usize], 3);
        }
        assert!(packed.ids.len() <= 512);

        let mut tied = vec![
            RankedCandidate {
                name: "a".into(),
                description: String::new(),
                score: 0.5,
            },
            RankedCandidate {
                name: "b".into(),
                description: String::new(),
                score: 0.2,
            },
            RankedCandidate {
                name: "c".into(),
                description: String::new(),
                score: 0.5,
            },
        ];
        order_by_score(&mut tied);
        assert_eq!(
            tied.iter()
                .map(|item| item.name.as_str())
                .collect::<Vec<_>>(),
            ["a", "c", "b"]
        );
    }
}
