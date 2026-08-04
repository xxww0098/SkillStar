//! SkillStar CLI types and dispatcher.

use clap::{Parser, Subcommand};

mod commands;
mod install;
mod manage;

pub use commands::*;
pub use install::cmd_install;
pub use manage::{cmd_doctor, cmd_pack_list, cmd_pack_remove, cmd_publish, cmd_remove, cmd_update};

mod helpers;
pub use helpers::*;

/// CLI root type — owned by this crate.
#[derive(Parser)]
#[command(
    name = "skillstar",
    about = "SkillStar — Skill management for AI agents",
    version
)]
pub struct Cli {
    #[command(subcommand)]
    pub command: Commands,
}

/// All top-level CLI commands.
#[derive(Subcommand)]
pub enum Commands {
    /// List installed skills (hub + local authored)
    List {
        /// Only show local-authored skills
        #[arg(long)]
        local: bool,
        /// Only show hub (repo-backed) skills
        #[arg(long)]
        hub: bool,
    },
    /// Search skills in the marketplace
    #[command(alias = "search")]
    Find {
        /// Search query — fuzzy matches name, description, author
        query: Option<String>,
        /// Max number of results
        #[arg(long, short = 'n', default_value_t = 20)]
        limit: u32,
        /// Output as JSON (non-interactive)
        #[arg(long)]
        json: bool,
    },
    /// Install a skill from a Git URL, owner/repo, or local path
    #[command(alias = "add")]
    Install {
        /// Install to selected Agent user directories instead of a project
        #[arg(long, short = 'g')]
        global: bool,
        /// Target project path for project-level install (defaults to current dir)
        #[arg(long, conflicts_with = "global")]
        project: Option<String>,
        /// Target agent id(s), repeatable or comma-separated, e.g. --agent codex,opencode
        #[arg(long = "agent", short = 'a', value_delimiter = ',')]
        agent: Vec<String>,
        /// Explicit skill name (useful when one repo contains multiple skills)
        #[arg(long)]
        name: Option<String>,
        /// Skill name filter(s) for selective install from multi-skill repos (repeatable or comma-separated)
        #[arg(long = "skill", short = 's', value_delimiter = ',')]
        skill: Vec<String>,
        /// List skills in the source without installing
        #[arg(long, short = 'l', conflicts_with_all = ["all", "preview"])]
        list: bool,
        /// Install every discovered skill to every supported Agent without prompts
        #[arg(long, conflicts_with_all = ["skill", "name"])]
        all: bool,
        /// Skip prompts (all skills; Settings-enabled Agents; Project scope)
        #[arg(long, short = 'y')]
        yes: bool,
        /// Copy files instead of linking for project or global Agent targets
        #[arg(long)]
        copy: bool,
        /// Preview/dry-run: show what would be installed without mutating hub, lockfile, or project links
        #[arg(long)]
        preview: bool,
        /// Git URL of the skill repository
        url: String,
    },
    /// Update installed skills
    Update {
        /// Name of a specific skill to update (updates all if omitted)
        name: Option<String>,
    },
    /// Remove one or more installed skills
    #[command(alias = "rm", alias = "uninstall")]
    Remove {
        /// Skill name(s) to remove — space- or comma-separated (e.g. `rm a b` or `rm a,b`)
        #[arg(required_unless_present = "all", value_delimiter = ',')]
        names: Vec<String>,
        /// Remove every installed skill
        #[arg(long)]
        all: bool,
        /// Skip confirmation prompt
        #[arg(long, short = 'y')]
        yes: bool,
    },
    /// Create a new skill template in the current directory
    #[command(alias = "create")]
    Init {
        /// Folder name to create (defaults to `my-new-skill`)
        name: Option<String>,
    },
    /// Publish current directory as a skill to GitHub
    Publish,
    /// Run health checks on installed packs or all packs
    Doctor {
        /// Pack name to check (checks all packs if omitted)
        name: Option<String>,
    },
    /// Manage installed skill packs
    Pack {
        #[command(subcommand)]
        action: PackAction,
    },
    /// Force launch GUI mode
    Gui,
}

/// Pack sub-commands.
#[derive(Subcommand)]
pub enum PackAction {
    /// List installed skill packs
    List,
    /// Remove an installed skill pack by name
    Remove {
        /// Pack name to remove
        name: String,
    },
}

/// Options passed to the install handler.
pub struct InstallOpts<'a> {
    pub url: &'a str,
    pub name: Option<&'a str>,
    pub skill: &'a [String],
    pub global: bool,
    pub project: Option<&'a str>,
    pub agent: &'a [String],
    pub list: bool,
    pub all: bool,
    pub yes: bool,
    pub copy: bool,
    pub preview: bool,
}

/// Options passed to the remove handler.
pub struct RemoveOpts<'a> {
    pub names: &'a [String],
    pub all: bool,
    pub yes: bool,
}

pub struct CliHandlers {
    pub migrate_and_run: fn(),
    pub install: fn(InstallOpts<'_>),
    pub update: fn(name: Option<&str>),
    pub remove: fn(RemoveOpts<'_>),
    pub publish: fn(),
    pub doctor: fn(name: Option<&str>),
    pub pack_list: fn(),
    pub pack_remove: fn(name: &str),
    pub gui: fn(),
}

pub fn run(args: Vec<String>, handlers: CliHandlers) {
    // Migration (only runs once at startup)
    (handlers.migrate_and_run)();

    let cli = Cli::parse_from(args);

    match cli.command {
        Commands::List { local, hub } => cmd_list(cmd_list_filter(local, hub)),
        Commands::Find { query, limit, json } => cmd_find(query.as_deref(), limit, json),
        Commands::Install {
            url,
            global,
            project,
            agent,
            name,
            skill,
            list,
            all,
            yes,
            copy,
            preview,
        } => (handlers.install)(InstallOpts {
            url: &url,
            name: name.as_deref(),
            skill: &skill,
            global,
            project: project.as_deref(),
            agent: &agent,
            list,
            all,
            yes,
            copy,
            preview,
        }),
        Commands::Update { name } => (handlers.update)(name.as_deref()),
        Commands::Remove { names, all, yes } => (handlers.remove)(RemoveOpts {
            names: &names,
            all,
            yes,
        }),
        Commands::Init { name } => cmd_init(name.as_deref()),
        Commands::Publish => (handlers.publish)(),
        Commands::Doctor { name } => (handlers.doctor)(name.as_deref()),
        Commands::Pack { action } => match action {
            PackAction::List => (handlers.pack_list)(),
            PackAction::Remove { name } => (handlers.pack_remove)(&name),
        },
        Commands::Gui => (handlers.gui)(),
    }
}

/// Return true when `args[1]` is a known CLI subcommand/flag that must not
/// fall through to GUI mode. Driven from the same Clap `Commands` surface
/// (and its aliases) so new subcommands only need updating here.
pub fn is_cli_subcommand(first_arg: &str) -> bool {
    matches!(
        first_arg,
        "list"
            | "find"
            | "search"
            | "install"
            | "add"
            | "update"
            | "remove"
            | "rm"
            | "uninstall"
            | "init"
            | "create"
            | "publish"
            | "doctor"
            | "pack"
            | "help"
            | "-h"
            | "--help"
            | "-V"
            | "--version"
    )
}

/// `gui` is a CLI subcommand that intentionally falls through to GUI mode.
pub fn is_gui_force_arg(first_arg: &str) -> bool {
    first_arg == "gui"
}

// ── Input classification (install/add routing) ───────────────────────

/// Classify a raw `install`/`add` argument to route to the right installer.
pub enum AddKind {
    Repo,
    Bundle(std::path::PathBuf),
    LocalDir(std::path::PathBuf),
}

pub fn classify_add_input(input: &str) -> AddKind {
    let trimmed = input.trim();
    let lower = trimmed.to_lowercase();
    if lower.starts_with("http://")
        || lower.starts_with("https://")
        || lower.starts_with("git@")
        || lower.starts_with("ssh://")
    {
        return AddKind::Repo;
    }
    let looks_like_path = trimmed.starts_with('.')
        || trimmed.starts_with('/')
        || trimmed.starts_with('~')
        || trimmed.starts_with('\\')
        || (trimmed.len() >= 2 && trimmed.chars().nth(1) == Some(':'));
    if looks_like_path {
        let expanded = expand_tilde(trimmed);
        let path = std::path::PathBuf::from(&expanded);
        if is_bundle_file(&path) {
            return AddKind::Bundle(path);
        }
        if path.is_dir() {
            return AddKind::LocalDir(path);
        }
    }
    AddKind::Repo
}

fn expand_tilde(input: &str) -> String {
    if let Some(rest) = input.strip_prefix("~/")
        && let Some(home) = dirs::home_dir()
    {
        return home.join(rest).to_string_lossy().to_string();
    }
    if input == "~"
        && let Some(home) = dirs::home_dir()
    {
        return home.to_string_lossy().to_string();
    }
    input.to_string()
}

fn is_bundle_file(path: &std::path::Path) -> bool {
    if !path.is_file() {
        return false;
    }
    matches!(
        path.extension().and_then(|ext| ext.to_str()),
        Some("ags") | Some("agd")
    )
}

/// Build CLI handlers that run entirely in skillstar-app (no Tauri).
pub fn default_handlers() -> CliHandlers {
    CliHandlers {
        migrate_and_run: default_migrate_and_run,
        install: cmd_install,
        update: cmd_update,
        remove: cmd_remove,
        publish: cmd_publish,
        doctor: cmd_doctor,
        pack_list: cmd_pack_list,
        pack_remove: cmd_pack_remove,
        gui: || {
            println!("Launching SkillStar GUI...");
        },
    }
}

fn default_migrate_and_run() {
    skillstar_core::infra::migration::migrate_legacy_paths();
    // Configure marketplace snapshot paths so `find` hits the real DB.
    skillstar_marketplace::snapshot::configure_runtime(
        skillstar_marketplace::snapshot::SnapshotRuntimeConfig::new(
            skillstar_core::infra::paths::marketplace_db_path(),
            skillstar_core::infra::paths::data_root(),
            skillstar_skills::installed_skill::installed_snapshot_markers,
            || -> skillstar_marketplace::snapshot::InstalledSkillsFuture {
                Box::pin(skillstar_skills::installed_skill::list_installed_skills_fast())
            },
        ),
    );
    if let Err(err) = skillstar_marketplace::snapshot::initialize() {
        eprintln!("⚠ Marketplace snapshot init failed: {err}");
    }
}

#[cfg(test)]
mod mode_tests {
    use super::{is_cli_subcommand, is_gui_force_arg};

    #[test]
    fn known_cli_subcommands_are_detected() {
        for arg in [
            "list",
            "find",
            "search",
            "install",
            "add",
            "update",
            "remove",
            "rm",
            "uninstall",
            "init",
            "create",
            "publish",
            "doctor",
            "pack",
            "help",
            "-h",
            "--help",
            "-V",
            "--version",
        ] {
            assert!(is_cli_subcommand(arg), "{arg} should be CLI");
            assert!(!is_gui_force_arg(arg));
        }
    }

    #[test]
    fn gui_force_is_not_cli_dispatch() {
        assert!(is_gui_force_arg("gui"));
        assert!(!is_cli_subcommand("gui"));
    }

    #[test]
    fn unknown_args_do_not_look_like_cli() {
        assert!(!is_cli_subcommand("totally-unknown"));
        assert!(!is_cli_subcommand("--some-os-flag"));
    }
}
