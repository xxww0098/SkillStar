use thiserror::Error;

/// Errors returned by the external MCP management & built-in store domain.
#[derive(Debug, Error)]
pub enum McpHubError {
    #[error("Invalid MCP server configuration: {0}")]
    InvalidServer(String),

    #[error("MCP server already exists: {0}")]
    AlreadyExists(String),

    #[error("MCP server not found: {0}")]
    NotFound(String),

    #[error("Unknown MCP target agent: {0}")]
    UnknownAgent(String),

    #[error("Agent {agent} does not support this server: {reason}")]
    UnsupportedForAgent { agent: String, reason: String },

    #[error("Failed to parse config at {path}: {detail}")]
    ConfigParse { path: String, detail: String },

    #[error("Failed to edit config at {path}: {detail}")]
    ConfigEdit { path: String, detail: String },

    #[error("MCP library lock error: {0}")]
    Lock(String),

    #[error("Store error: {0}")]
    Store(String),

    #[error("Probe error: {0}")]
    Probe(String),

    #[error(transparent)]
    Io(#[from] std::io::Error),

    #[error(transparent)]
    Other(#[from] anyhow::Error),
}

pub type McpHubResult<T> = std::result::Result<T, McpHubError>;
