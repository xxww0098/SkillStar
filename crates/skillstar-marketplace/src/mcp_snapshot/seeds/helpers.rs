//! Builders for the curated MCP seed catalog.
//!
//! Every catalog row is declared once as a [`CuratedSpec`] and expanded by
//! [`build`] into the three shapes the snapshot stores: the typed
//! [`McpRegistryServer`] the store queries and renders, the package / remote
//! summaries the install wizard reads, and the `raw_server_json` provenance
//! blob.
//!
//! Writing those three out by hand at every call site is how this catalog
//! drifted. `xapi` was seeded without its `mcp <url>` arguments in the typed
//! package summary (only the hand-written JSON carried them, and the install
//! path reads the typed fields), and several rows pointed at packages upstream
//! had already archived — `@modelcontextprotocol/server-git`,
//! `server-fetch` and `server-brave-search` all moved to other registries or
//! other publishers. One declaration, one expansion, no second copy to go
//! stale.

use serde_json::{Map, Value, json};

use crate::mcp_models::{
    McpArgument, McpInput, McpKeyValueInput, McpRegistryPackageSummary, McpRegistryRemoteSummary,
    McpRegistryServer, McpServerKind,
};

/// Which registry a stdio server's package lives in. This fixes both the
/// `registry_type` we publish and the launcher the install wizard runs it
/// with, so the two can never disagree.
#[derive(Clone, Copy, PartialEq, Eq)]
pub(super) enum Registry {
    /// npm — launched as `npx -y <pkg>`.
    Npm,
    /// PyPI — launched as `uvx <pkg>`.
    Pypi,
}

impl Registry {
    fn registry_type(self) -> &'static str {
        match self {
            Registry::Npm => "npm",
            Registry::Pypi => "pypi",
        }
    }

    fn runtime_hint(self) -> &'static str {
        match self {
            Registry::Npm => "npx",
            Registry::Pypi => "uvx",
        }
    }
}

/// One input a curated server reads from its environment.
pub(super) struct EnvVar {
    pub name: &'static str,
    pub description: &'static str,
    pub default: Option<&'static str>,
    /// Secrets are never shipped in the payload, so the install form always
    /// asks for them and the field renders masked.
    pub secret: bool,
    pub required: bool,
}

// No catalog row carries env vars today, so these constructors have no
// caller — they stay because they are the DSL's vocabulary for the next
// env-bearing curated row, not because deleting them is hard.
#[allow(dead_code)]
impl EnvVar {
    /// A credential the user must paste before the server can do anything.
    pub(super) const fn token(name: &'static str, description: &'static str) -> Self {
        Self {
            name,
            description,
            default: None,
            secret: true,
            required: true,
        }
    }

    /// A non-secret value with no usable default — a host, a username.
    pub(super) const fn required(name: &'static str, description: &'static str) -> Self {
        Self {
            name,
            description,
            default: None,
            secret: false,
            required: true,
        }
    }
}

/// How a curated server is reached.
#[derive(Clone, Copy)]
pub(super) enum Launch {
    /// A package from a language registry, launched over stdio.
    Stdio {
        registry: Registry,
        identifier: &'static str,
        /// Named launcher options placed *before* the identifier — `(flag,
        /// value)` pairs. uvx's `--from <source>` is the canonical case: it
        /// selects the package when the executable's name differs from it
        /// (`uvx --from git+… serena` runs `serena` out of the repo, not a
        /// `serena` PyPI package).
        runtime_args: &'static [(&'static str, &'static str)],
        /// Arguments the package needs *after* its identifier — a subcommand,
        /// a mode flag. `npx -y <pkg> serve --mcp` is `args: &["serve",
        /// "--mcp"]`.
        args: &'static [&'static str],
    },
    /// A hosted streamable-http endpoint.
    ///
    /// `auth_header` is the header a publisher expects a bearer token in. An
    /// empty string means the endpoint authenticates with OAuth (or not at
    /// all), so the wizard asks for nothing up front and the first connection
    /// goes through the OAuth flow instead.
    Remote {
        url: &'static str,
        auth_header: &'static str,
    },
}

/// One curated catalog row.
pub(super) struct CuratedSpec {
    pub id: &'static str,
    pub name: &'static str,
    /// Publisher bucket. Also the value the `source` column is grouped by.
    pub source: &'static str,
    pub description: &'static str,
    pub homepage: &'static str,
    pub launch: Launch,
    pub env: &'static [EnvVar],
    /// Rows flagged here surface as the add-dialog's recommended chips. The
    /// rest of the catalog stays behind the store's own list.
    pub recommended: bool,
}

/// Expand one spec into the stored row.
pub(super) fn build(spec: &CuratedSpec) -> McpRegistryServer {
    let (kind, runtimes, packages, remotes) = match spec.launch {
        Launch::Stdio {
            registry,
            identifier,
            runtime_args,
            args,
        } => (
            McpServerKind::Stdio,
            vec![registry.runtime_hint().to_string()],
            vec![package_summary(registry, identifier, runtime_args, args, spec.env)],
            Vec::new(),
        ),
        Launch::Remote { url, auth_header } => (
            McpServerKind::Remote,
            Vec::new(),
            Vec::new(),
            vec![remote_summary(url, auth_header)],
        ),
    };

    McpRegistryServer {
        id: spec.id.to_string(),
        name: spec.name.to_string(),
        namespace: spec.id.to_string(),
        description: spec.description.to_string(),
        repo_url: spec.homepage.to_string(),
        kind,
        runtimes,
        readme: Some(format!("# {}\n\n{}", spec.name, spec.description)),
        packages,
        remotes,
        raw_server_json: raw_server_json(spec),
        recommended: spec.recommended,
        source: Some(spec.source.to_string()),
        ..Default::default()
    }
}

fn package_summary(
    registry: Registry,
    identifier: &str,
    runtime_args: &[(&str, &str)],
    args: &[&str],
    env: &[EnvVar],
) -> McpRegistryPackageSummary {
    McpRegistryPackageSummary {
        runtime: registry.runtime_hint().to_string(),
        identifier: identifier.to_string(),
        required_env: env
            .iter()
            .filter(|var| var.required || var.secret)
            .map(|var| var.name.to_string())
            .collect(),
        registry_type: Some(registry.registry_type().to_string()),
        runtime_hint: Some(registry.runtime_hint().to_string()),
        runtime_arguments: runtime_args
            .iter()
            .map(|(name, value)| named(name, value))
            .collect(),
        package_arguments: args.iter().map(|arg| positional(arg)).collect(),
        environment_variables: env.iter().map(key_value).collect(),
        ..Default::default()
    }
}

fn remote_summary(url: &str, auth_header: &str) -> McpRegistryRemoteSummary {
    let headers: Vec<McpKeyValueInput> = if auth_header.is_empty() {
        Vec::new()
    } else {
        vec![McpKeyValueInput {
            name: auth_header.to_string(),
            input: McpInput {
                description: Some(format!(
                    "Paste the full header value, including the scheme: `Bearer <token>`."
                )),
                is_required: true,
                is_secret: true,
                placeholder: Some("Bearer <token>".to_string()),
                ..Default::default()
            },
        }]
    };

    McpRegistryRemoteSummary {
        transport: "http".to_string(),
        url: url.to_string(),
        required_headers: headers.iter().map(|header| header.name.clone()).collect(),
        transport_type: Some("streamable-http".to_string()),
        headers,
        ..Default::default()
    }
}

fn positional(value: &str) -> McpArgument {
    McpArgument {
        input: McpInput {
            value: Some(value.to_string()),
            ..Default::default()
        },
        ..Default::default()
    }
}

fn named(name: &str, value: &str) -> McpArgument {
    McpArgument {
        kind: crate::mcp_models::McpArgumentKind::Named,
        name: Some(name.to_string()),
        input: McpInput {
            value: Some(value.to_string()),
            ..Default::default()
        },
        ..Default::default()
    }
}

fn key_value(var: &EnvVar) -> McpKeyValueInput {
    McpKeyValueInput {
        name: var.name.to_string(),
        input: McpInput {
            description: Some(var.description.to_string()),
            is_required: var.required,
            is_secret: var.secret,
            default: var.default.map(str::to_string),
            ..Default::default()
        },
    }
}

/// The `server.json`-shaped provenance blob, derived from the same spec that
/// builds the typed row.
fn raw_server_json(spec: &CuratedSpec) -> String {
    let mut packages: Vec<Value> = Vec::new();
    let mut remotes: Vec<Value> = Vec::new();

    match spec.launch {
        Launch::Stdio {
            registry,
            identifier,
            runtime_args,
            args,
        } => {
            let mut package = Map::new();
            package.insert("registry_type".to_string(), json!(registry.registry_type()));
            package.insert("identifier".to_string(), json!(identifier));
            package.insert("runtime_hint".to_string(), json!(registry.runtime_hint()));
            if !runtime_args.is_empty() {
                package.insert(
                    "runtime_arguments".to_string(),
                    Value::Array(
                        runtime_args
                            .iter()
                            .map(|(name, value)| json!({ "type": "named", "name": name, "value": value }))
                            .collect(),
                    ),
                );
            }
            if !args.is_empty() {
                package.insert("package_arguments".to_string(), json!(args));
            }
            if !spec.env.is_empty() {
                package.insert(
                    "environment_variables".to_string(),
                    Value::Array(
                        spec.env
                            .iter()
                            .map(|var| {
                                let mut entry = Map::new();
                                entry.insert("name".to_string(), json!(var.name));
                                entry.insert("description".to_string(), json!(var.description));
                                if let Some(default) = var.default {
                                    entry.insert("default".to_string(), json!(default));
                                }
                                if var.required {
                                    entry.insert("is_required".to_string(), json!(true));
                                }
                                if var.secret {
                                    entry.insert("is_secret".to_string(), json!(true));
                                }
                                Value::Object(entry)
                            })
                            .collect(),
                    ),
                );
            }
            packages.push(Value::Object(package));
        }
        Launch::Remote { url, auth_header } => {
            let mut remote = Map::new();
            remote.insert("transport_type".to_string(), json!("streamable-http"));
            remote.insert("url".to_string(), json!(url));
            if !auth_header.is_empty() {
                remote.insert(
                    "headers".to_string(),
                    json!([{
                        "name": auth_header,
                        "value": "Bearer {token}",
                        "is_secret": true,
                        "is_required": true,
                    }]),
                );
            }
            remotes.push(Value::Object(remote));
        }
    }

    json!({
        "id": spec.id,
        "name": spec.name,
        "description": spec.description,
        "packages": packages,
        "remotes": remotes,
        "repository": { "url": spec.homepage, "source": "github" },
    })
    .to_string()
}
