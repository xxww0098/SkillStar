//! `POST /_skillstar/claude-mcp/{token}`.
//!
//! The helper runs on this machine. A peer that is not loopback is refused
//! before the token is looked up, so a forwarded request cannot park a call.

use std::net::SocketAddr;
use std::sync::mpsc;

use serde::Deserialize;

use super::{ClaudeBridge, ToolResult};

/// What the listener should write for one callback.
pub enum CallbackOutcome {
    Ready { status: u16, body: String },
    Wait(mpsc::Receiver<ToolResult>),
}

#[derive(Deserialize)]
struct CallBody {
    #[serde(default)]
    tool_call_id: String,
}

/// Token segment of a callback path, without the slash prefix.
pub fn callback_token(path: &str) -> Option<&str> {
    let rest = path.strip_prefix("/_skillstar/claude-mcp/")?;
    if rest.is_empty() || rest.contains('/') {
        return None;
    }
    Some(rest)
}

pub fn begin_callback(
    bridge: &ClaudeBridge,
    peer: SocketAddr,
    token: &str,
    body: &[u8],
) -> CallbackOutcome {
    if !peer.ip().is_loopback() {
        return ready(403, "forbidden");
    }
    let Some(run) = bridge.lookup(token) else {
        return ready(404, "unknown or expired Claude run");
    };
    let call: CallBody = match serde_json::from_slice::<CallBody>(body) {
        Ok(call) if !call.tool_call_id.is_empty() => call,
        _ => return ready(400, "invalid tool call"),
    };
    CallbackOutcome::Wait(run.park_call(call.tool_call_id))
}

fn ready(status: u16, body: &str) -> CallbackOutcome {
    CallbackOutcome::Ready {
        status,
        body: body.to_string(),
    }
}
