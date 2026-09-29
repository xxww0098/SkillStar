//! Hold a server-sent stream until its first content byte.
//!
//! An error before that byte can still be answered by another upstream, so
//! those bytes stay off the downstream writer. Lead events are not content.
//! Past 15 seconds, or a buffer bigger than 1 MiB, the bytes go downstream
//! and this reply is no longer swappable. The clock starts at the first byte.

use std::io::{self, Write};
use std::time::Duration;

use serde::Deserialize;
use serde_json::{Map, Value, value::RawValue};

/// How long a stream is held waiting for content. Magpie uses `>`.
pub const HOLD_LONGEST: Duration = Duration::from_secs(15);
/// How much of a stream is held waiting for content. Magpie uses `>`.
pub const HOLD_MOST: usize = 1 << 20;

/// Buffers one upstream stream. `clock` returns a monotonic instant; tests
/// move it instead of sleeping.
pub struct HoldWriter<W, C> {
    inner: W,
    clock: C,
    held: Vec<u8>,
    started: Option<Duration>,
    scanned: usize,
    phase: Phase,
}

enum Phase {
    Open,
    Failed,
    Committed,
}

impl<W, C> HoldWriter<W, C>
where
    W: Write,
    C: Fn() -> Duration,
{
    pub fn new(inner: W, clock: C) -> Self {
        Self {
            inner,
            clock,
            held: Vec::new(),
            started: None,
            scanned: 0,
            phase: Phase::Open,
        }
    }

    /// The agent-facing writer. Empty until the reply is committed.
    pub fn downstream(&self) -> &W {
        &self.inner
    }

    pub fn committed(&self) -> bool {
        matches!(self.phase, Phase::Committed)
    }

    /// An error event arrived before any content byte. The bytes stayed off
    /// the downstream writer, so another upstream may still answer.
    pub fn failed_before_content(&self) -> bool {
        matches!(self.phase, Phase::Failed)
    }
}

impl<W, C> Write for HoldWriter<W, C>
where
    W: Write,
    C: Fn() -> Duration,
{
    fn write(&mut self, buf: &[u8]) -> io::Result<usize> {
        match self.phase {
            Phase::Committed => self.inner.write(buf),
            Phase::Failed => {
                self.held.extend_from_slice(buf);
                Ok(buf.len())
            }
            Phase::Open => {
                if self.started.is_none() {
                    self.started = Some((self.clock)());
                }
                self.held.extend_from_slice(buf);
                self.scan()?;
                Ok(buf.len())
            }
        }
    }

    fn flush(&mut self) -> io::Result<()> {
        if self.committed() {
            self.inner.flush()
        } else {
            Ok(())
        }
    }
}

impl<W, C> HoldWriter<W, C>
where
    W: Write,
    C: Fn() -> Duration,
{
    fn scan(&mut self) -> io::Result<()> {
        if !matches!(self.phase, Phase::Open) {
            return Ok(());
        }
        while let Some(end) = event_end(&self.held[self.scanned..]) {
            let kind = event_kind(&self.held[self.scanned..self.scanned + end]);
            self.scanned += end;
            match kind {
                EventKind::Lead => continue,
                // An error is decided before the time and size limits. A
                // late error with no content still stays off the agent.
                EventKind::Error => {
                    self.phase = Phase::Failed;
                    return Ok(());
                }
                EventKind::Content => return self.commit(),
            }
        }
        let elapsed = (self.clock)().saturating_sub(self.started.unwrap_or_default());
        if self.held.len() > HOLD_MOST || elapsed > HOLD_LONGEST {
            self.commit()?;
        }
        Ok(())
    }

    fn commit(&mut self) -> io::Result<()> {
        self.inner.write_all(&self.held)?;
        self.held.clear();
        self.phase = Phase::Committed;
        Ok(())
    }
}

enum EventKind {
    Content,
    Lead,
    Error,
}

fn event_end(buf: &[u8]) -> Option<usize> {
    let mut index = 0;
    while index + 1 < buf.len() {
        if buf[index] == b'\n' {
            if buf[index + 1] == b'\n' {
                return Some(index + 2);
            }
            if buf[index + 1] == b'\r' && index + 2 < buf.len() && buf[index + 2] == b'\n' {
                return Some(index + 3);
            }
        }
        index += 1;
    }
    None
}

/// What one server-sent event is. Bytes that are not JSON are content:
/// `[DONE]` is how a Chat stream ends, and it must not be dropped.
fn event_kind(event: &[u8]) -> EventKind {
    let (name, data) = sse_parts(event);
    if name.is_empty() && data.is_empty() {
        return EventKind::Lead;
    }
    let Ok(parsed) = serde_json::from_slice::<StreamEvent>(&data) else {
        return EventKind::Content;
    };
    let typ = parsed
        .kind
        .as_deref()
        .filter(|kind| !kind.is_empty())
        .unwrap_or(name.as_str());
    if typ == "error" || typ == "response.failed" {
        return EventKind::Error;
    }
    if parsed.error.is_some() {
        return EventKind::Error;
    }
    if matches!(
        typ,
        "ping" | "message_start" | "response.created" | "response.in_progress" | "response.queued"
    ) || typ.starts_with("codex.")
    {
        return EventKind::Lead;
    }
    if typ.is_empty()
        && let Some(choices) = &parsed.choices
    {
        for choice in choices {
            if choice.finish_reason.is_some() {
                return EventKind::Content;
            }
            if choice.delta.as_ref().is_some_and(delta_has_content) {
                return EventKind::Content;
            }
        }
        return EventKind::Lead;
    }
    EventKind::Content
}

fn delta_has_content(delta: &Map<String, Value>) -> bool {
    delta
        .iter()
        .any(|(key, value)| key != "role" && !value.is_null() && value.as_str() != Some(""))
}

fn sse_parts(event: &[u8]) -> (String, Vec<u8>) {
    let mut name = String::new();
    let mut data = Vec::new();
    for line in event.split(|byte| *byte == b'\n') {
        let line = line.strip_suffix(b"\r").unwrap_or(line);
        if let Some(rest) = line.strip_prefix(b"event:") {
            name = String::from_utf8_lossy(rest.trim_ascii()).into_owned();
        } else if let Some(rest) = line.strip_prefix(b"data:") {
            data.extend_from_slice(rest.trim_ascii());
        }
    }
    (name, data)
}

#[derive(Deserialize)]
struct StreamEvent {
    #[serde(rename = "type")]
    kind: Option<String>,
    error: Option<Box<RawValue>>,
    choices: Option<Vec<StreamChoice>>,
}

#[derive(Deserialize)]
struct StreamChoice {
    delta: Option<Map<String, Value>>,
    finish_reason: Option<String>,
}
