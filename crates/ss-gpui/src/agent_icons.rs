//! Brand glyphs for the skill-card agent rail.
//! Embedded SVG bytes. Do not point this module at a deleted frontend tree.

use std::borrow::Cow;

/// Mono ink baked for the dark shell (`fill="#e8eef8"`).
const DARK_MONO_INK: &[u8] = b"#e8eef8";
/// [`crate::theme`] light foreground. Same length as [`DARK_MONO_INK`].
const PAPER_MONO_INK: &[u8] = b"#16213c";

const _: () = assert!(DARK_MONO_INK.len() == PAPER_MONO_INK.len());

/// Path for `img`. Paper appends `#paper` so GPUI's embedded-image cache
/// does not keep the dark-ink raster after a theme switch.
pub fn agent_icon_path(id: &str) -> String {
    if crate::theme::is_light() {
        format!("agents/{id}.svg#paper")
    } else {
        format!("agents/{id}.svg")
    }
}

/// Bytes for an `agents/{id}.svg` or `agents/{id}.svg#paper` request.
/// Other paths return `None` so the asset source can fall through.
pub fn load_agent_icon_path(path: &str) -> Option<Cow<'static, [u8]>> {
    let rest = path.strip_prefix("agents/")?;
    let (file, paper) = match rest.split_once('#') {
        Some((file, "paper")) => (file, true),
        Some(_) => return None,
        None => (rest, false),
    };
    let id = file.strip_suffix(".svg")?;
    if id.is_empty() || id.contains('/') {
        return None;
    }
    let bytes = agent_icon_svg(id);
    if paper
        && bytes
            .windows(DARK_MONO_INK.len())
            .any(|w| w == DARK_MONO_INK)
    {
        Some(Cow::Owned(recolor_paper_ink(bytes)))
    } else {
        Some(Cow::Borrowed(bytes))
    }
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

/// SVG bytes for a built-in agent id. Unknown ids use the LobeHub glyph.
pub fn agent_icon_svg(agent_id: &str) -> &'static [u8] {
    match agent_id {
        "adal" => include_bytes!("../assets/agents/adal.svg"),
        "aider-desk" => include_bytes!("../assets/agents/aider-desk.svg"),
        "amp" => include_bytes!("../assets/agents/amp.svg"),
        "antigravity" => include_bytes!("../assets/agents/antigravity.svg"),
        "astrbot" => include_bytes!("../assets/agents/astrbot.svg"),
        "augment" => include_bytes!("../assets/agents/augment.svg"),
        "autohand-code" => include_bytes!("../assets/agents/autohand-code.svg"),
        "bob" => include_bytes!("../assets/agents/bob.svg"),
        "chatgpt" => include_bytes!("../assets/agents/chatgpt.svg"),
        "claude" => include_bytes!("../assets/agents/claude.svg"),
        "cline" => include_bytes!("../assets/agents/cline.svg"),
        "codearts-agent" => include_bytes!("../assets/agents/codearts-agent.svg"),
        "codebuddy" => include_bytes!("../assets/agents/codebuddy.svg"),
        "codemaker" => include_bytes!("../assets/agents/codemaker.svg"),
        "codestudio" => include_bytes!("../assets/agents/codestudio.svg"),
        "codex" => include_bytes!("../assets/agents/codex.svg"),
        "command-code" => include_bytes!("../assets/agents/command-code.svg"),
        "continue" => include_bytes!("../assets/agents/continue.svg"),
        "cortex" => include_bytes!("../assets/agents/cortex.svg"),
        "crush" => include_bytes!("../assets/agents/crush.svg"),
        "cursor" => include_bytes!("../assets/agents/cursor.svg"),
        "deepagents" => include_bytes!("../assets/agents/deepagents.svg"),
        "deepseek" => include_bytes!("../assets/agents/deepseek.svg"),
        "devin" => include_bytes!("../assets/agents/devin.svg"),
        "devin-desktop" => include_bytes!("../assets/agents/devin-desktop.svg"),
        "dexto" => include_bytes!("../assets/agents/dexto.svg"),
        "droid" => include_bytes!("../assets/agents/droid.svg"),
        "eve" => include_bytes!("../assets/agents/eve.svg"),
        "firebender" => include_bytes!("../assets/agents/firebender.svg"),
        "forgecode" => include_bytes!("../assets/agents/forgecode.svg"),
        "fx" => include_bytes!("../assets/agents/fx.svg"),
        "gemini-cli" => include_bytes!("../assets/agents/gemini-cli.svg"),
        "github-copilot" => include_bytes!("../assets/agents/github-copilot.svg"),
        "goose" => include_bytes!("../assets/agents/goose.svg"),
        "grok" => include_bytes!("../assets/agents/grok.svg"),
        "hermes" => include_bytes!("../assets/agents/hermes.svg"),
        "iflow-cli" => include_bytes!("../assets/agents/iflow-cli.svg"),
        "inference-sh" => include_bytes!("../assets/agents/inference-sh.svg"),
        "jazz" => include_bytes!("../assets/agents/jazz.svg"),
        "junie" => include_bytes!("../assets/agents/junie.svg"),
        "kilo" => include_bytes!("../assets/agents/kilo.svg"),
        "kimchi" => include_bytes!("../assets/agents/kimchi.svg"),
        "kimi-code-cli" => include_bytes!("../assets/agents/kimi-code-cli.svg"),
        "kiro" => include_bytes!("../assets/agents/kiro.svg"),
        "kode" => include_bytes!("../assets/agents/kode.svg"),
        "lingma" => include_bytes!("../assets/agents/lingma.svg"),
        "loaf" => include_bytes!("../assets/agents/loaf.svg"),
        "mcpjam" => include_bytes!("../assets/agents/mcpjam.svg"),
        "minimax-code" => include_bytes!("../assets/agents/minimax-code.svg"),
        "mistral-vibe" => include_bytes!("../assets/agents/mistral-vibe.svg"),
        "moxby" => include_bytes!("../assets/agents/moxby.svg"),
        "mux" => include_bytes!("../assets/agents/mux.svg"),
        "neovate" => include_bytes!("../assets/agents/neovate.svg"),
        "omp" => include_bytes!("../assets/agents/omp.svg"),
        "ona" => include_bytes!("../assets/agents/ona.svg"),
        "openclaw" => include_bytes!("../assets/agents/openclaw.svg"),
        "opencode" => include_bytes!("../assets/agents/opencode.svg"),
        "openhands" => include_bytes!("../assets/agents/openhands.svg"),
        "pi" => include_bytes!("../assets/agents/pi.svg"),
        "pochi" => include_bytes!("../assets/agents/pochi.svg"),
        "posit-assistant" => include_bytes!("../assets/agents/posit-assistant.svg"),
        "promptscript" => include_bytes!("../assets/agents/promptscript.svg"),
        "qoder" => include_bytes!("../assets/agents/qoder.svg"),
        "qoder-cn" => include_bytes!("../assets/agents/qoder-cn.svg"),
        "qwen-code" => include_bytes!("../assets/agents/qwen-code.svg"),
        "reasonix" => include_bytes!("../assets/agents/reasonix.svg"),
        "replit" => include_bytes!("../assets/agents/replit.svg"),
        "roo" => include_bytes!("../assets/agents/roo.svg"),
        "rovodev" => include_bytes!("../assets/agents/rovodev.svg"),
        "sarvam-code" => include_bytes!("../assets/agents/sarvam-code.svg"),
        "tabnine-cli" => include_bytes!("../assets/agents/tabnine-cli.svg"),
        "terramind" => include_bytes!("../assets/agents/terramind.svg"),
        "tinycloud" => include_bytes!("../assets/agents/tinycloud.svg"),
        "trae" => include_bytes!("../assets/agents/trae.svg"),
        "trae-cn" => include_bytes!("../assets/agents/trae-cn.svg"),
        "universal" => include_bytes!("../assets/agents/universal.svg"),
        "warp" => include_bytes!("../assets/agents/warp.svg"),
        "workbuddy" => include_bytes!("../assets/agents/workbuddy.svg"),
        "zcode" => include_bytes!("../assets/agents/zcode.svg"),
        "zed" => include_bytes!("../assets/agents/zed.svg"),
        "zencoder" => include_bytes!("../assets/agents/zencoder.svg"),
        "zenflow" => include_bytes!("../assets/agents/zenflow.svg"),
        _ => include_bytes!("../assets/agents/_fallback.svg"),
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn text(bytes: &[u8]) -> &str {
        std::str::from_utf8(bytes).unwrap()
    }

    #[test]
    fn paper_request_recolors_only_the_baked_mono_ink() {
        let paper = load_agent_icon_path("agents/cursor.svg#paper").unwrap();
        assert!(text(&paper).contains("#16213c"));
        assert!(!text(&paper).contains("#e8eef8"));
        assert_eq!(paper.len(), agent_icon_svg("cursor").len());

        let dark = load_agent_icon_path("agents/cursor.svg").unwrap();
        assert!(matches!(dark, Cow::Borrowed(_)));
        assert!(text(&dark).contains("#e8eef8"));
    }

    #[test]
    fn paper_recolor_keeps_explicit_brand_fills() {
        let qoder = load_agent_icon_path("agents/qoder.svg#paper").unwrap();
        let qoder = text(&qoder);
        assert!(qoder.contains("#16213c"));
        assert!(qoder.contains("#2ADB5C"));
        assert!(!qoder.contains("#e8eef8"));

        for id in ["kiro", "codex", "chatgpt", "codebuddy", "antigravity"] {
            let paper = load_agent_icon_path(&format!("agents/{id}.svg#paper")).unwrap();
            assert!(
                matches!(paper, Cow::Borrowed(_)),
                "{id} has no mono ink to rewrite"
            );
            assert_eq!(&*paper, agent_icon_svg(id));
        }
        assert!(text(agent_icon_svg("codex")).contains("fill=\"#fff\""));
        assert!(text(agent_icon_svg("kiro")).contains("fill=\"#fff\""));
        let chatgpt = text(agent_icon_svg("chatgpt"));
        assert!(chatgpt.contains("fill=\"#fff\""));
        assert!(chatgpt.contains("fill=\"#000\""));
        assert_ne!(chatgpt, text(agent_icon_svg("codex")));
    }

    #[test]
    fn unknown_icon_variant_is_not_an_agent_asset() {
        assert!(load_agent_icon_path("agents/cursor.svg#dark").is_none());
        assert!(load_agent_icon_path("icons/cursor.svg").is_none());
        assert!(load_agent_icon_path("agents/cursor.png").is_none());
    }
}
