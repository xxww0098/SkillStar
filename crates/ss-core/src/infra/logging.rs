//! Console logging for the SkillStar process entry points.
//!
//! Both desktop shells (the Tauri `skillstar` binary and the GPUI
//! `ss-gpui` binary) call [`init`]; the stdio MCP server calls
//! [`init_stderr`]. Keeping one formatter here means the two shells cannot
//! drift apart, and the MCP stdout contract stays untouched.
//!
//! The human format is a single line per event:
//!
//! ```text
//! 06:14:05.383  ℹ  ss_core::infra::migration │ Storage path migration check completed
//! ```
//!
//! * local time with milliseconds, dimmed;
//! * level as a colored glyph — `✗` ERROR, `⚠` WARN, `ℹ` INFO, `○` DEBUG,
//!   `·` TRACE — so failures and successes stand out at a glance;
//! * target, dimmed, closed off by a dim `│` separator;
//! * message and structured fields.
//!
//! `RUST_LOG` selects the filter (default `info`); `SKILLSTAR_LOG_JSON=1`
//! switches to the structured JSON formatter for log shipping. Styling follows
//! the `NO_COLOR`/`CLICOLOR_FORCE` conventions and is otherwise enabled only on
//! a real terminal, so redirected logs stay plain text.

use std::fmt;
use std::io::IsTerminal;

use chrono::Local;
use tracing::{Event, Level, Subscriber};
use tracing_subscriber::EnvFilter;
use tracing_subscriber::fmt::FmtContext;
use tracing_subscriber::fmt::format::{FormatEvent, FormatFields, Writer};
use tracing_subscriber::fmt::time::FormatTime;
use tracing_subscriber::layer::SubscriberExt;
use tracing_subscriber::registry::LookupSpan;
use tracing_subscriber::util::SubscriberInitExt;

const RESET: &str = "\x1b[0m";
const DIM: &str = "\x1b[2m";

/// Install the human/JSON console subscriber on stdout.
///
/// # Panics
///
/// Panics if a global subscriber was already installed; call once per process.
pub fn init() {
    let filter = env_filter("info");

    if std::env::var_os("SKILLSTAR_LOG_JSON").is_some() {
        tracing_subscriber::registry()
            .with(filter)
            .with(
                tracing_subscriber::fmt::layer()
                    .json()
                    .with_target(true)
                    .with_ansi(false)
                    .with_span_events(tracing_subscriber::fmt::format::FmtSpan::CLOSE),
            )
            .init();
        return;
    }

    let ansi = color_enabled();
    tracing_subscriber::registry()
        .with(filter)
        .with(
            tracing_subscriber::fmt::layer()
                .event_format(ConsoleFormat::new(ansi))
                .with_ansi(ansi),
        )
        .init();
}

/// Install a color-free stderr subscriber for protocol processes.
///
/// The MCP stdio server reserves stdout for JSON-RPC, so its diagnostics must
/// go to stderr. A second [`init`] would panic, and this path can run after
/// another entry point has already installed a subscriber, so a failure to
/// install is ignored.
pub fn init_stderr(default_filter: &str) {
    let _ = tracing_subscriber::registry()
        .with(env_filter(default_filter))
        .with(
            tracing_subscriber::fmt::layer()
                .with_writer(std::io::stderr)
                .with_ansi(false)
                .event_format(ConsoleFormat::new(false)),
        )
        .try_init();
}

fn env_filter(default: &str) -> EnvFilter {
    EnvFilter::try_from_default_env().unwrap_or_else(|_| EnvFilter::new(default))
}

/// Whether stdout should receive ANSI styling.
///
/// Follows the `NO_COLOR`/`CLICOLOR_FORCE` conventions; otherwise colors only a
/// real terminal so piped or redirected logs stay plain text.
fn color_enabled() -> bool {
    if std::env::var_os("NO_COLOR").is_some_and(|value| !value.is_empty()) {
        return false;
    }
    if std::env::var_os("CLICOLOR_FORCE").is_some_and(|value| value.to_string_lossy() != "0") {
        return true;
    }
    std::io::stdout().is_terminal()
}

/// The compact single-line event format described at the module level.
struct ConsoleFormat {
    ansi: bool,
    timer: LocalTime,
}

impl ConsoleFormat {
    fn new(ansi: bool) -> Self {
        Self {
            ansi,
            timer: LocalTime,
        }
    }
}

impl<S, N> FormatEvent<S, N> for ConsoleFormat
where
    S: Subscriber + for<'a> LookupSpan<'a>,
    N: for<'a> FormatFields<'a> + 'static,
{
    fn format_event(
        &self,
        ctx: &FmtContext<'_, S, N>,
        mut writer: Writer<'_>,
        event: &Event<'_>,
    ) -> fmt::Result {
        // `with_ansi` drives both this flag and the field formatter; honoring
        // the writer keeps a misconfigured call site from emitting raw escapes.
        let ansi = self.ansi && writer.has_ansi_escapes();
        let meta = event.metadata();

        if ansi {
            writer.write_str(DIM)?;
        }
        self.timer.format_time(&mut writer)?;
        if ansi {
            writer.write_str(RESET)?;
        }
        writer.write_str("  ")?;

        let level = *meta.level();
        if ansi {
            writer.write_str(level_color(level))?;
        }
        writer.write_str(level_glyph(level))?;
        if ansi {
            writer.write_str(RESET)?;
        }
        writer.write_str("  ")?;

        let target = meta.target();
        if !target.is_empty() {
            if ansi {
                writer.write_str(DIM)?;
            }
            writer.write_str(target)?;
            writer.write_str(" │")?;
            if ansi {
                writer.write_str(RESET)?;
            }
            writer.write_str(" ")?;
        }

        ctx.format_fields(writer.by_ref(), event)?;
        writeln!(writer)
    }
}

/// Local wall-clock time down to milliseconds, e.g. `06:14:05.383`.
#[derive(Clone, Copy, Debug, Default)]
struct LocalTime;

impl FormatTime for LocalTime {
    fn format_time(&self, w: &mut Writer<'_>) -> fmt::Result {
        write!(w, "{}", Local::now().format("%H:%M:%S%.3f"))
    }
}

fn level_glyph(level: Level) -> &'static str {
    match level {
        Level::ERROR => "✗",
        Level::WARN => "⚠",
        Level::INFO => "ℹ",
        Level::DEBUG => "○",
        Level::TRACE => "·",
    }
}

fn level_color(level: Level) -> &'static str {
    match level {
        Level::ERROR => "\x1b[1;31m",
        Level::WARN => "\x1b[1;33m",
        Level::INFO => "\x1b[1;32m",
        Level::DEBUG => "\x1b[1;34m",
        Level::TRACE => "\x1b[1;35m",
    }
}

#[cfg(test)]
mod tests {
    use std::io;
    use std::sync::{Arc, Mutex};

    use super::*;

    /// A cloneable `io::Write` sink shared across the layer's writers.
    #[derive(Clone, Default)]
    struct Capture(Arc<Mutex<Vec<u8>>>);

    impl Capture {
        fn text(&self) -> String {
            let bytes = self.0.lock().unwrap().clone();
            String::from_utf8(bytes).expect("log output is utf-8")
        }
    }

    impl io::Write for Capture {
        fn write(&mut self, buf: &[u8]) -> io::Result<usize> {
            self.0.lock().unwrap().extend_from_slice(buf);
            Ok(buf.len())
        }

        fn flush(&mut self) -> io::Result<()> {
            Ok(())
        }
    }

    /// Render one event through `ConsoleFormat` into a buffer.
    fn render(format: ConsoleFormat, ansi: bool, emit: impl FnOnce()) -> String {
        let capture = Capture::default();
        let writer = {
            let capture = capture.clone();
            move || capture.clone()
        };
        let subscriber = tracing_subscriber::registry().with(
            tracing_subscriber::fmt::layer()
                .with_writer(writer)
                .with_ansi(ansi)
                .event_format(format),
        );
        tracing::subscriber::with_default(subscriber, emit);
        capture.text()
    }

    #[test]
    fn plain_format_is_one_line_with_time_glyph_target_and_fields() {
        let output = render(ConsoleFormat::new(false), false, || {
            tracing::info!(
                target: "sync",
                skill_name = "crxhub",
                agent_id = "grok",
                enable = "off",
                "→ toggle_skill_for_agent"
            );
        });

        assert!(output.ends_with('\n'));
        assert_eq!(output.matches('\n').count(), 1);
        assert!(output.contains('ℹ'), "missing level glyph: {output:?}");
        assert!(
            output.contains("sync │ "),
            "missing target separator: {output:?}"
        );
        assert!(
            output.contains("→ toggle_skill_for_agent skill_name=\"crxhub\""),
            "missing message/fields: {output:?}"
        );
        assert!(!output.contains('\x1b'), "plain output must not colorize");
        // `HH:MM:SS.mmm` prefix.
        let time = &output[..12];
        assert_eq!(time.as_bytes()[2], b':');
        assert_eq!(time.as_bytes()[5], b':');
        assert_eq!(time.as_bytes()[8], b'.');
    }

    #[test]
    fn ansi_format_dims_time_and_target_and_colors_the_level() {
        let output = render(ConsoleFormat::new(true), true, || {
            tracing::warn!(target: "paths", "something");
        });

        assert!(
            output.contains(DIM),
            "time/target should be dimmed: {output:?}"
        );
        assert!(
            output.contains("\x1b[1;33m"),
            "WARN should be yellow: {output:?}"
        );
        assert!(output.contains('⚠'), "missing WARN glyph: {output:?}");
        assert!(output.contains(RESET), "styling must be reset: {output:?}");
    }

    #[test]
    fn every_level_has_a_distinct_color() {
        let levels = [
            Level::ERROR,
            Level::WARN,
            Level::INFO,
            Level::DEBUG,
            Level::TRACE,
        ];
        let mut colors: Vec<&str> = levels.iter().map(|level| level_color(*level)).collect();
        colors.sort_unstable();
        colors.dedup();
        assert_eq!(colors.len(), levels.len(), "level colors must be distinct");
        assert!(colors.iter().all(|color| color.starts_with("\x1b[")));
    }

    #[test]
    fn every_level_has_a_distinct_glyph() {
        let levels = [
            Level::ERROR,
            Level::WARN,
            Level::INFO,
            Level::DEBUG,
            Level::TRACE,
        ];
        let mut glyphs: Vec<&str> = levels.iter().map(|level| level_glyph(*level)).collect();
        glyphs.sort_unstable();
        glyphs.dedup();
        assert_eq!(glyphs.len(), levels.len(), "level glyphs must be distinct");
    }
}
