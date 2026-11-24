//! Agent brand SVG glyphs for the card switch carousel, toolbar filter strip,
//! drawers, projects, and settings.
//!
//! Embedded directly from `gui/assets/agents/*.svg` so both `nsh`
//! test hosts and the standalone single-file binary render without reading an
//! external assets directory at runtime.

use std::collections::HashMap;
use std::sync::{Mutex, OnceLock};

/// Mono ink baked into dark-shell SVGs (`fill="#e8eef8"`).
const DARK_MONO_INK: &[u8] = b"#e8eef8";
/// Paper-theme foreground ink matching `theme.json` (`#18181b`).
const PAPER_MONO_INK: &[u8] = b"#18181b";

const _: () = assert!(DARK_MONO_INK.len() == PAPER_MONO_INK.len());

static ICON_URI_CACHE: OnceLock<Mutex<HashMap<(String, bool), String>>> = OnceLock::new();

/// Normalize an incoming agent id or alias onto the canonical SVG asset key.
pub fn normalize_agent_icon_id(agent_id: &str) -> &str {
    let trimmed = agent_id.trim();
    let compat = ss_skills::agents::compatible_profile_id(trimmed);
    match compat {
        "agy" | "antigravity" => "antigravity",
        "claude-desktop" => "claude",
        "commandcode" => "command-code",
        "copilot" => "github-copilot",
        "dsh" => "deepseek",
        "gemini" => "gemini-cli",
        "grok-app" => "grok",
        "kimi" => "kimi-code-cli",
        "kiro-cli" => "kiro",
        "oh-my-pi" => "omp",
        "roo-code" => "roo",
        "workbuddy-ai" => "workbuddy",
        canonical => canonical,
    }
}

/// Raw embedded SVG bytes for a canonical (or aliased) agent id.
/// Unknown ids fall back to `_fallback.svg`.
pub fn agent_icon_svg(agent_id: &str) -> &'static [u8] {
    match normalize_agent_icon_id(agent_id) {
        "adal" => include_bytes!("../../assets/agents/adal.svg"),
        "aider-desk" => include_bytes!("../../assets/agents/aider-desk.svg"),
        "alma" => include_bytes!("../../assets/agents/alma.svg"),
        "amp" => include_bytes!("../../assets/agents/amp.svg"),
        "antigravity" => include_bytes!("../../assets/agents/antigravity.svg"),
        "astrbot" => include_bytes!("../../assets/agents/astrbot.svg"),
        "atomcode" => include_bytes!("../../assets/agents/atomcode.svg"),
        "augment" => include_bytes!("../../assets/agents/augment.svg"),
        "autohand-code" => include_bytes!("../../assets/agents/autohand-code.svg"),
        "bob" => include_bytes!("../../assets/agents/bob.svg"),
        "chatgpt" => include_bytes!("../../assets/agents/chatgpt.svg"),
        "claude" => include_bytes!("../../assets/agents/claude.svg"),
        "cline" => include_bytes!("../../assets/agents/cline.svg"),
        "codearts-agent" => include_bytes!("../../assets/agents/codearts-agent.svg"),
        "codebuddy" => include_bytes!("../../assets/agents/codebuddy.svg"),
        "codemaker" => include_bytes!("../../assets/agents/codemaker.svg"),
        "codestudio" => include_bytes!("../../assets/agents/codestudio.svg"),
        "codex" => include_bytes!("../../assets/agents/codex.svg"),
        "command-code" => include_bytes!("../../assets/agents/command-code.svg"),
        "continue" => include_bytes!("../../assets/agents/continue.svg"),
        "cortex" => include_bytes!("../../assets/agents/cortex.svg"),
        "crush" => include_bytes!("../../assets/agents/crush.svg"),
        "cursor" => include_bytes!("../../assets/agents/cursor.svg"),
        "deepagents" => include_bytes!("../../assets/agents/deepagents.svg"),
        "deepseek" => include_bytes!("../../assets/agents/deepseek.svg"),
        "deveco" => include_bytes!("../../assets/agents/deveco.svg"),
        "devin" => include_bytes!("../../assets/agents/devin.svg"),
        "devin-desktop" => include_bytes!("../../assets/agents/devin-desktop.svg"),
        "dexto" => include_bytes!("../../assets/agents/dexto.svg"),
        "droid" => include_bytes!("../../assets/agents/droid.svg"),
        "eve" => include_bytes!("../../assets/agents/eve.svg"),
        "firebender" => include_bytes!("../../assets/agents/firebender.svg"),
        "forgecode" => include_bytes!("../../assets/agents/forgecode.svg"),
        "fx" => include_bytes!("../../assets/agents/fx.svg"),
        "gemini-cli" => include_bytes!("../../assets/agents/gemini-cli.svg"),
        // Google Gemini icon: https://commons.wikimedia.org/wiki/File:Google_Gemini_icon_2025.svg
        "github-copilot" => include_bytes!("../../assets/agents/github-copilot.svg"),
        "goose" => include_bytes!("../../assets/agents/goose.svg"),
        "grok" => include_bytes!("../../assets/agents/grok.svg"),
        "hermes" => include_bytes!("../../assets/agents/hermes.svg"),
        "iflow-cli" => include_bytes!("../../assets/agents/iflow-cli.svg"),
        "inference-sh" => include_bytes!("../../assets/agents/inference-sh.svg"),
        "jazz" => include_bytes!("../../assets/agents/jazz.svg"),
        "junie" => include_bytes!("../../assets/agents/junie.svg"),
        "kilo" => include_bytes!("../../assets/agents/kilo.svg"),
        "kimchi" => include_bytes!("../../assets/agents/kimchi.svg"),
        "kimi-code-cli" => include_bytes!("../../assets/agents/kimi-code-cli.svg"),
        "kiro" => include_bytes!("../../assets/agents/kiro.svg"),
        "kode" => include_bytes!("../../assets/agents/kode.svg"),
        "lingma" => include_bytes!("../../assets/agents/lingma.svg"),
        "liveagent" => include_bytes!("../../assets/agents/liveagent.svg"),
        "loaf" => include_bytes!("../../assets/agents/loaf.svg"),
        "mcpjam" => include_bytes!("../../assets/agents/mcpjam.svg"),
        "mimocode" => include_bytes!("../../assets/agents/mimocode.svg"),
        "minimax-code" => include_bytes!("../../assets/agents/minimax-code.svg"),
        "mistral-vibe" => include_bytes!("../../assets/agents/mistral-vibe.svg"),
        "moxby" => include_bytes!("../../assets/agents/moxby.svg"),
        "mu" => include_bytes!("../../assets/agents/mu.svg"),
        "mux" => include_bytes!("../../assets/agents/mux.svg"),
        "neovate" => include_bytes!("../../assets/agents/neovate.svg"),
        "omo" => include_bytes!("../../assets/agents/omo.svg"),
        "omp" => include_bytes!("../../assets/agents/omp.svg"),
        "ona" => include_bytes!("../../assets/agents/ona.svg"),
        "openclaw" => include_bytes!("../../assets/agents/openclaw.svg"),
        "opencode" => include_bytes!("../../assets/agents/opencode.svg"),
        "openhands" => include_bytes!("../../assets/agents/openhands.svg"),
        "pi" => include_bytes!("../../assets/agents/pi.svg"),
        "pochi" => include_bytes!("../../assets/agents/pochi.svg"),
        "posit-assistant" => include_bytes!("../../assets/agents/posit-assistant.svg"),
        "promptscript" => include_bytes!("../../assets/agents/promptscript.svg"),
        "qoder" => include_bytes!("../../assets/agents/qoder.svg"),
        "qoder-cn" => include_bytes!("../../assets/agents/qoder-cn.svg"),
        "qwen-code" => include_bytes!("../../assets/agents/qwen-code.svg"),
        "reasonix" => include_bytes!("../../assets/agents/reasonix.svg"),
        "replit" => include_bytes!("../../assets/agents/replit.svg"),
        "roo" => include_bytes!("../../assets/agents/roo.svg"),
        "rovodev" => include_bytes!("../../assets/agents/rovodev.svg"),
        "sarvam-code" => include_bytes!("../../assets/agents/sarvam-code.svg"),
        "tabnine-cli" => include_bytes!("../../assets/agents/tabnine-cli.svg"),
        "terramind" => include_bytes!("../../assets/agents/terramind.svg"),
        "tinycloud" => include_bytes!("../../assets/agents/tinycloud.svg"),
        "trae" => include_bytes!("../../assets/agents/trae.svg"),
        "trae-cn" => include_bytes!("../../assets/agents/trae-cn.svg"),
        "universal" => include_bytes!("../../assets/agents/universal.svg"),
        "warp" => include_bytes!("../../assets/agents/warp.svg"),
        "workbuddy" => include_bytes!("../../assets/agents/workbuddy.svg"),
        "zcode" => include_bytes!("../../assets/agents/zcode.svg"),
        "zed" => include_bytes!("../../assets/agents/zed.svg"),
        "zencoder" => include_bytes!("../../assets/agents/zencoder.svg"),
        "zenflow" => include_bytes!("../../assets/agents/zenflow.svg"),
        _ => include_bytes!("../../assets/agents/_fallback.svg"),
    }
}

/// 测试辅助：确认受管 agent id 是否存在专属 SVG（非 `_fallback.svg`）。
#[cfg(test)]
pub(crate) fn has_dedicated_svg(agent_id: &str) -> bool {
    let svg = agent_icon_svg(agent_id);
    let fallback = include_bytes!("../../assets/agents/_fallback.svg");
    svg != fallback.as_slice()
}

fn recolor_paper_ink(bytes: &[u8]) -> Vec<u8> {
    let mut out = bytes.to_vec();
    let mut i = 0;
    while i + DARK_MONO_INK.len() <= out.len() {
        if &out[i..i + DARK_MONO_INK.len()] == DARK_MONO_INK {
            out[i..i + PAPER_MONO_INK.len()].copy_from_slice(PAPER_MONO_INK);
            i += DARK_MONO_INK.len();
        } else {
            i += 1;
        }
    }
    out
}

/// Cached `data:image/svg+xml;base64,…` URI for an agent brand icon.
/// When `light` is `true`, dark-shell mono ink (`#e8eef8`) is recolored to
/// paper foreground (`#18181b`) while multi-color brand fills stay intact.
pub fn agent_icon_data_uri(agent_id: &str, light: bool) -> String {
    let lower = agent_id.trim().to_ascii_lowercase();
    let canonical = normalize_agent_icon_id(&lower).to_owned();
    let cache = ICON_URI_CACHE.get_or_init(|| Mutex::new(HashMap::new()));
    if let Ok(guard) = cache.lock()
        && let Some(cached) = guard.get(&(canonical.clone(), light))
    {
        return cached.clone();
    }

    let raw = agent_icon_svg(&canonical);
    let uri = if light && raw.windows(DARK_MONO_INK.len()).any(|w| w == DARK_MONO_INK) {
        let recolored = recolor_paper_ink(raw);
        crate::encode_data_uri("data:image/svg+xml;base64,", &recolored)
    } else {
        crate::encode_data_uri("data:image/svg+xml;base64,", raw)
    };

    if let Ok(mut guard) = cache.lock() {
        guard.insert((canonical, light), uri.clone());
    }
    uri
}

#[cfg(test)]
mod tests {
    use super::*;

    fn decode_svg(uri: &str) -> String {
        let (mime, bytes) = nsh_core::image::decode_data_uri(uri).expect("valid SVG data URI");
        assert_eq!(mime, "image/svg+xml");
        String::from_utf8(bytes).expect("valid UTF-8 SVG")
    }

    #[test]
    fn paper_recolors_only_baked_mono_ink() {
        let light_uri = agent_icon_data_uri("cursor", true);
        let dark_uri = agent_icon_data_uri("cursor", false);
        assert_ne!(light_uri, dark_uri);

        let light_svg = decode_svg(&light_uri);
        assert!(light_svg.contains("#18181b"));
        assert!(!light_svg.contains("#e8eef8"));

        let dark_svg = decode_svg(&dark_uri);
        assert!(dark_svg.contains("#e8eef8"));
        assert!(!dark_svg.contains("#18181b"));
    }

    #[test]
    fn paper_preserves_brand_fills() {
        let qoder = decode_svg(&agent_icon_data_uri("qoder", true));
        assert!(qoder.contains("#18181b"));
        assert!(qoder.contains("#2ADB5C"));
        assert!(!qoder.contains("#e8eef8"));

        for id in ["kiro", "codex", "chatgpt", "codebuddy", "antigravity"] {
            assert_eq!(
                agent_icon_data_uri(id, true),
                agent_icon_data_uri(id, false),
                "{id} has no mono ink to recolor"
            );
        }
    }

    #[test]
    fn aliases_and_fallback_resolve_cleanly() {
        assert_eq!(
            agent_icon_data_uri("claude-code", false),
            agent_icon_data_uri("claude", false)
        );
        assert_eq!(
            agent_icon_data_uri("windsurf", false),
            agent_icon_data_uri("devin-desktop", false)
        );
        assert_eq!(
            agent_icon_data_uri("copilot", true),
            agent_icon_data_uri("github-copilot", true)
        );
        assert_eq!(
            agent_icon_data_uri("unknown-custom-agent", false),
            agent_icon_data_uri("_fallback", false)
        );
    }

    #[test]
    fn all_embedded_svgs_decode_as_valid_data_uris() {
        for profile in ss_skills::agents::list_profiles() {
            for light in [false, true] {
                let uri = agent_icon_data_uri(&profile.id, light);
                let svg = decode_svg(&uri);
                assert!(
                    svg.contains("<svg"),
                    "decoded asset for {} (light={light}) should contain <svg",
                    profile.id
                );
            }
        }
    }

    #[test]
    fn every_mcp_agent_has_dedicated_svg_asset() {
        let temp = tempfile::TempDir::new().unwrap();
        let roots = ss_mcp_hub::McpPathRoots::from_sandbox(temp.path().to_path_buf());
        for target in ss_mcp_hub::builtin_targets(&roots) {
            assert!(
                has_dedicated_svg(&target.id),
                "MCP agent '{}' should have a dedicated SVG asset",
                target.id
            );
            for light in [false, true] {
                let svg = decode_svg(&agent_icon_data_uri(&target.id, light));
                assert!(svg.contains("<svg"), "invalid SVG for {}", target.id);
            }
        }
        assert!(has_dedicated_svg("agy"));
    }
}
