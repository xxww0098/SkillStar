use super::*;

#[test]
fn parses_stdio_package_server() {
    let body = r#"{
            "servers": [
                {
                    "server": {
                        "id": "abc123",
                        "name": "microsoft/markitdown",
                        "description": "Convert files to Markdown.",
                        "packages": [
                            { "registry_type": "pypi", "identifier": "markitdown-mcp", "version": "0.0.1a4", "runtime_hint": "uvx",
                              "environment_variables": [ { "name": "MD_TOKEN", "is_secret": true } ] }
                        ],
                        "remotes": [],
                        "repository": { "url": "https://github.com/microsoft/markitdown", "source": "github", "readme": "hi" },
                        "version_detail": { "version": "1.0.0" },
                        "updated_at": "2026-01-21T09:35:10Z"
                    },
                    "x-github": { "stars": 42, "license": "MIT" }
                }
            ],
            "metadata": { "next_cursor": "CURSOR2" }
        }"#;

    let (servers, cursor) = parse_servers_response(body).unwrap();
    assert_eq!(cursor.as_deref(), Some("CURSOR2"));
    assert_eq!(servers.len(), 1);
    let s = &servers[0];
    assert_eq!(s.name, "markitdown");
    assert_eq!(s.namespace, "microsoft/markitdown");
    assert_eq!(s.kind, McpServerKind::Stdio);
    assert_eq!(s.stars, 42);
    assert_eq!(s.license.as_deref(), Some("MIT"));
    assert_eq!(s.version.as_deref(), Some("1.0.0"));
    assert_eq!(s.runtimes, vec!["uvx".to_string()]);
    assert_eq!(s.packages.len(), 1);
    assert_eq!(s.packages[0].runtime, "uvx");
    assert_eq!(s.packages[0].identifier, "markitdown-mcp");
    assert_eq!(s.packages[0].required_env, vec!["MD_TOKEN".to_string()]);
    assert!(!s.raw_server_json.is_empty());
    // No `_meta`: an unversioned source's rows are the latest we know of.
    assert!(s.is_latest);
    assert_eq!(s.status, McpServerStatus::Active);
}

#[test]
fn parses_remote_server_with_secret_header() {
    let body = r#"{
            "servers": [
                {
                    "server": {
                        "name": "io.github.netdata/mcp-server",
                        "description": "Monitoring.",
                        "packages": [],
                        "remotes": [
                            { "transport_type": "streamable-http", "url": "https://app.netdata.cloud/api/v1/mcp",
                              "headers": [ { "name": "Authorization", "value": "Bearer {TOKEN}", "is_secret": true } ] }
                        ],
                        "repository": { "url": "https://github.com/netdata/netdata", "source": "github" }
                    }
                }
            ]
        }"#;

    let (servers, cursor) = parse_servers_response(body).unwrap();
    assert!(cursor.is_none());
    let s = &servers[0];
    assert_eq!(s.name, "mcp-server");
    assert_eq!(s.kind, McpServerKind::Remote);
    assert_eq!(s.remotes.len(), 1);
    assert_eq!(s.remotes[0].transport, "http");
    assert_eq!(s.remotes[0].url, "https://app.netdata.cloud/api/v1/mcp");
    assert_eq!(
        s.remotes[0].required_headers,
        vec!["Authorization".to_string()]
    );
    // The header's full Input semantics now survive parsing, not just its name.
    let header = &s.remotes[0].headers[0];
    assert_eq!(header.name, "Authorization");
    assert!(header.input.is_secret);
    assert_eq!(header.input.value.as_deref(), Some("Bearer {TOKEN}"));
    assert_eq!(
        s.remotes[0].transport_type.as_deref(),
        Some("streamable-http")
    );
}

#[test]
fn runtime_command_falls_back_to_registry_type() {
    assert_eq!(runtime_command_for("npm", ""), "npx");
    assert_eq!(runtime_command_for("pypi", ""), "uvx");
    assert_eq!(runtime_command_for("oci", ""), "docker");
    assert_eq!(runtime_command_for("cargo", ""), "cargo");
    assert_eq!(runtime_command_for("mcpb", ""), "mcpb");
    assert_eq!(runtime_command_for("npm", "bunx"), "bunx"); // hint wins
}

#[test]
fn handles_bare_server_element_without_envelope() {
    let body = r#"{ "servers": [ { "name": "acme/thing", "packages": [], "remotes": [] } ] }"#;
    let (servers, _) = parse_servers_response(body).unwrap();
    assert_eq!(servers.len(), 1);
    assert_eq!(servers[0].name, "thing");
    assert_eq!(servers[0].kind, McpServerKind::Unknown);
}

/// The official registry's live shape (2025-12-11 schema, camelCase cursor,
/// element-level `_meta`). Reading it with the GitHub-flavoured snake_case
/// cursor key is exactly the bug that silently stops pagination at page 1.
#[test]
fn parses_official_registry_envelope_with_camel_cursor_and_meta() {
    let body = r#"{
        "servers": [
            {
                "server": {
                    "$schema": "https://static.modelcontextprotocol.io/schemas/2025-12-11/server.schema.json",
                    "name": "ac.inference.sh/mcp",
                    "title": "inference.sh",
                    "description": "Run 150+ AI apps.",
                    "version": "1.0.0",
                    "websiteUrl": "https://inference.sh",
                    "icons": [ { "src": "https://inference.sh/icon.png", "mimeType": "image/png", "sizes": ["48x48"], "theme": "light" } ],
                    "remotes": [ { "type": "streamable-http", "url": "https://api.inference.sh/{region}/mcp",
                                   "variables": { "region": { "description": "Deployment region", "choices": ["us","eu"], "default": "us" } } } ]
                },
                "_meta": {
                    "io.modelcontextprotocol.registry/official": {
                        "status": "deprecated",
                        "publishedAt": "2026-04-13T17:32:20Z",
                        "updatedAt": "2026-04-14T00:00:00Z",
                        "isLatest": false
                    }
                }
            }
        ],
        "metadata": { "nextCursor": "ac.inference.sh/mcp:1.0.1", "count": 1 }
    }"#;

    let page = parse_servers_page(body, McpCursorStyle::Camel, Some("official")).unwrap();
    assert_eq!(
        page.next_cursor.as_deref(),
        Some("ac.inference.sh/mcp:1.0.1")
    );
    let s = &page.servers[0];
    assert_eq!(s.namespace, "ac.inference.sh/mcp");
    assert_eq!(s.title.as_deref(), Some("inference.sh"));
    assert_eq!(s.website_url.as_deref(), Some("https://inference.sh"));
    assert_eq!(s.icons.len(), 1);
    assert_eq!(s.icons[0].mime_type.as_deref(), Some("image/png"));
    assert_eq!(s.status, McpServerStatus::Deprecated);
    assert!(!s.is_latest);
    assert_eq!(s.published_at.as_deref(), Some("2026-04-13T17:32:20Z"));
    assert_eq!(s.updated_at.as_deref(), Some("2026-04-14T00:00:00Z"));
    assert_eq!(s.registry_source.as_deref(), Some("official"));
    assert_eq!(s.contributing_sources, vec!["official".to_string()]);
    // URL template variables carry their Input semantics.
    let region = &s.remotes[0].variables[0];
    assert_eq!(region.name, "region");
    assert_eq!(
        region.input.choices,
        vec!["us".to_string(), "eu".to_string()]
    );
    assert_eq!(region.input.default.as_deref(), Some("us"));
}

/// A camelCase cursor must not be reachable through the snake_case preference
/// alone — the fallback exists, but the style must pick the right key first.
#[test]
fn cursor_style_prefers_its_own_spelling() {
    let body =
        r#"{ "servers": [], "metadata": { "nextCursor": "camel", "next_cursor": "snake" } }"#;
    let camel = parse_servers_page(body, McpCursorStyle::Camel, None).unwrap();
    let snake = parse_servers_page(body, McpCursorStyle::Snake, None).unwrap();
    assert_eq!(camel.next_cursor.as_deref(), Some("camel"));
    assert_eq!(snake.next_cursor.as_deref(), Some("snake"));
}

#[test]
fn parses_full_2025_12_11_package_semantics() {
    let body = r#"{
        "servers": [ { "server": {
            "name": "com.example/fs",
            "description": "Files.",
            "version": "0.1.5",
            "packages": [ {
                "registryType": "mcpb",
                "registryBaseUrl": "https://github.com",
                "identifier": "https://github.com/example/mcp/releases/download/v1/fs.mcpb",
                "version": "0.1.5",
                "fileSha256": "abcdef0123456789abcdef0123456789abcdef0123456789abcdef0123456789",
                "runtimeHint": "npx",
                "transport": { "type": "streamable-http", "url": "http://localhost:8080/mcp",
                               "headers": [ { "name": "X-Trace", "description": "trace id" } ] },
                "runtimeArguments": [ { "type": "positional", "value": "-y" } ],
                "packageArguments": [ { "type": "named", "name": "--root", "isRepeated": true, "isRequired": true, "format": "filepath" } ],
                "environmentVariables": [
                    { "name": "GCS_BUCKET", "description": "Bucket.", "isRequired": true },
                    { "name": "GCS_MAKE_PUBLIC", "description": "Public?", "default": "false", "format": "boolean" },
                    { "name": "MODE", "choices": ["fast", "safe"], "default": "safe" }
                ]
            } ],
            "remotes": []
        } } ]
    }"#;

    let page = parse_servers_page(body, McpCursorStyle::Camel, None).unwrap();
    let pkg = &page.servers[0].packages[0];
    assert_eq!(pkg.registry_type.as_deref(), Some("mcpb"));
    assert_eq!(pkg.registry_base_url.as_deref(), Some("https://github.com"));
    assert_eq!(pkg.file_sha256.as_deref().map(str::len), Some(64));
    assert_eq!(pkg.runtime, "npx"); // runtimeHint wins over registryType
    assert_eq!(pkg.runtime_hint.as_deref(), Some("npx"));
    let transport = pkg.transport.as_ref().expect("package transport parsed");
    assert_eq!(transport.transport_type, "streamable-http");
    assert_eq!(transport.url.as_deref(), Some("http://localhost:8080/mcp"));
    assert_eq!(transport.headers[0].name, "X-Trace");

    assert_eq!(pkg.runtime_arguments.len(), 1);
    assert_eq!(pkg.runtime_arguments[0].kind, McpArgumentKind::Positional);
    assert_eq!(pkg.runtime_arguments[0].input.value.as_deref(), Some("-y"));
    assert_eq!(pkg.package_arguments[0].kind, McpArgumentKind::Named);
    assert_eq!(pkg.package_arguments[0].name.as_deref(), Some("--root"));
    assert!(pkg.package_arguments[0].is_repeated);
    assert_eq!(
        pkg.package_arguments[0].input.format,
        McpInputFormat::Filepath
    );

    assert_eq!(pkg.environment_variables.len(), 3);
    assert!(pkg.environment_variables[0].input.is_required);
    assert_eq!(
        pkg.environment_variables[0].input.description.as_deref(),
        Some("Bucket.")
    );
    assert_eq!(
        pkg.environment_variables[1].input.default.as_deref(),
        Some("false")
    );
    assert_eq!(
        pkg.environment_variables[1].input.format,
        McpInputFormat::Boolean
    );
    assert_eq!(
        pkg.environment_variables[2].input.choices,
        vec!["fast".to_string(), "safe".to_string()]
    );
    // Only required/secret vars are surfaced on the card summary.
    assert_eq!(pkg.required_env, vec!["GCS_BUCKET".to_string()]);
}

/// Snapshot rows written by the previous (name-only) model must keep
/// deserializing — `packages_json` / `remotes_json` are never migrated.
#[test]
fn legacy_package_and_remote_json_still_deserializes() {
    let legacy_pkg =
        r#"[{"runtime":"npx","identifier":"@acme/x","version":"1.0.0","requiredEnv":["TOKEN"]}]"#;
    let pkgs: Vec<McpRegistryPackageSummary> = serde_json::from_str(legacy_pkg).unwrap();
    assert_eq!(pkgs[0].identifier, "@acme/x");
    assert_eq!(pkgs[0].required_env, vec!["TOKEN".to_string()]);
    assert!(pkgs[0].environment_variables.is_empty());
    assert!(pkgs[0].transport.is_none());

    let legacy_remote = r#"[{"transport":"http","url":"https://acme.example/mcp","requiredHeaders":["Authorization"]}]"#;
    let remotes: Vec<McpRegistryRemoteSummary> = serde_json::from_str(legacy_remote).unwrap();
    assert_eq!(remotes[0].url, "https://acme.example/mcp");
    assert!(remotes[0].headers.is_empty());
}

#[test]
fn accepts_bare_array_directory_file() {
    let body =
        r#"[ { "name": "local.dev/thing", "description": "d", "packages": [], "remotes": [] } ]"#;
    let page = parse_servers_page(body, McpCursorStyle::Camel, Some("custom:local")).unwrap();
    assert_eq!(page.servers.len(), 1);
    assert_eq!(page.servers[0].namespace, "local.dev/thing");
    assert_eq!(
        page.servers[0].registry_source.as_deref(),
        Some("custom:local")
    );
}

#[test]
fn github_publisher_meta_supplies_stars_license_and_readme() {
    let body = r##"{ "servers": [ { "server": {
        "name": "io.github.acme/tool",
        "description": "d",
        "packages": [],
        "remotes": [],
        "_meta": { "io.modelcontextprotocol.registry/publisher-provided": {
            "github": { "stars": 1234, "license": "Apache-2.0", "readme": "# Tool" }
        } }
    } } ] }"##;
    let page = parse_servers_page(body, McpCursorStyle::Snake, Some("github")).unwrap();
    let s = &page.servers[0];
    assert_eq!(s.stars, 1234);
    assert_eq!(s.license.as_deref(), Some("Apache-2.0"));
    assert_eq!(s.readme.as_deref(), Some("# Tool"));
}
