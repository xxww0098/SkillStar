//! SkillStar CLI types and dispatcher.

use clap::{Parser, Subcommand};

mod commands;
pub use commands::*;

mod helpers;
pub use helpers::*;

/// CLI root type — owned by this crate.
#[derive(Parser)]
#[command(
    name = "skillstar",
    about = "SkillStar — Skill management for AI agents"
)]
pub struct Cli {
    #[command(subcommand)]
    pub command: Commands,
}

/// All top-level CLI commands.
#[derive(Subcommand)]
pub enum Commands {
    /// List installed skills
    List,
    /// Install a skill from a Git URL
    Install {
        /// Install to hub only (do not link into current project)
        #[arg(long)]
        global: bool,
        /// Target project path for project-level install (defaults to current dir)
        #[arg(long, conflicts_with = "global")]
        project: Option<String>,
        /// Target agent id(s), repeatable or comma-separated, e.g. --agent codex,opencode
        #[arg(long = "agent", value_delimiter = ',')]
        agent: Vec<String>,
        /// Explicit skill name (useful when one repo contains multiple skills)
        #[arg(long)]
        name: Option<String>,
        /// Skill name filter(s) for selective install from multi-skill repos (repeatable or comma-separated)
        #[arg(long = "skill", value_delimiter = ',')]
        skill: Vec<String>,
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
    /// Create a new skill template
    Create,
    /// Publish current directory as a skill to GitHub
    Publish,
    /// Scan a skill folder for security threats
    Scan {
        /// Path to the skill directory to scan
        path: String,
        /// Skip AI analysis, run static patterns only
        #[arg(long)]
        static_only: bool,
    },
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
    /// Launch agent CLIs for a project
    Launch {
        #[command(subcommand)]
        action: LaunchAction,
    },
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

/// Launch sub-commands.
#[derive(Subcommand)]
pub enum LaunchAction {
    /// Deploy a project's saved launch configuration
    Deploy {
        /// Project name (as registered in SkillStar)
        project_name: String,
    },
    /// Directly launch a single agent CLI in the current directory
    Run {
        /// Agent CLI to launch: claude | codex | opencode | gemini
        agent: String,
        /// Provider profile to use (optional)
        #[arg(long)]
        provider: Option<String>,
        /// Enable safe/dangerously-skip-permissions mode
        #[arg(long)]
        safe: bool,
        /// Extra arguments passed through to the CLI
        #[arg(trailing_var_arg = true, allow_hyphen_values = true)]
        args: Vec<String>,
    },
}

pub struct CliHandlers {
    pub migrate_and_run: fn(),
    pub install: fn(
        url: &str,
        name: Option<&str>,
        skill: &[String],
        global: bool,
        project: Option<&str>,
        agent: &[String],
        preview: bool,
    ),
    pub update: fn(name: Option<&str>),
    pub publish: fn(),
    pub scan: fn(path: &str, static_only: bool),
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
        Commands::List => cmd_list(),
        Commands::Install {
            url,
            global,
            project,
            agent,
            name,
            skill,
            preview,
        } => (handlers.install)(
            &url,
            name.as_deref(),
            &skill,
            global,
            project.as_deref(),
            &agent,
            preview,
        ),
        Commands::Update { name } => (handlers.update)(name.as_deref()),
        Commands::Create => cmd_create(),
        Commands::Publish => (handlers.publish)(),
        Commands::Scan { path, static_only } => (handlers.scan)(&path, static_only),
        Commands::Doctor { name } => (handlers.doctor)(name.as_deref()),
        Commands::Pack { action } => match action {
            PackAction::List => (handlers.pack_list)(),
            PackAction::Remove { name } => (handlers.pack_remove)(&name),
        },
        Commands::Gui => (handlers.gui)(),
        Commands::Launch { action } => match action {
            LaunchAction::Deploy { project_name } => cmd_launch_deploy(&project_name),
            LaunchAction::Run {
                agent,
                provider,
                safe,
                args,
            } => cmd_launch_run(&agent, provider.as_deref(), safe, &args),
        },
    }
}

#[cfg(test)]
mod clap_tests {
    use super::*;

    // ── Install command parsing ────────────────────────────────────────────────

    #[test]
    fn test_install_minimal() {
        let cli = Cli::parse_from(["skillstar", "install", "https://github.com/user/my-skill"]);
        match cli.command {
            Commands::Install {
                url,
                global,
                project,
                agent,
                name,
                skill,
                preview,
            } => {
                assert_eq!(url, "https://github.com/user/my-skill");
                assert!(!global);
                assert!(project.is_none());
                assert!(agent.is_empty());
                assert!(name.is_none());
                assert!(skill.is_empty());
                assert!(!preview);
            }
            _ => panic!("expected Install command"),
        }
    }

    #[test]
    fn test_install_global_flag() {
        let cli = Cli::parse_from([
            "skillstar",
            "install",
            "--global",
            "https://github.com/user/my-skill",
        ]);
        match cli.command {
            Commands::Install {
                url,
                global,
                preview,
                ..
            } => {
                assert!(global);
                assert!(!preview);
                assert_eq!(url, "https://github.com/user/my-skill");
            }
            _ => panic!("expected Install command"),
        }
    }

    #[test]
    fn test_install_with_project() {
        let cli = Cli::parse_from([
            "skillstar",
            "install",
            "--project",
            "/path/to/project",
            "https://github.com/user/my-skill",
        ]);
        match cli.command {
            Commands::Install {
                url,
                project,
                preview,
                ..
            } => {
                assert_eq!(project, Some("/path/to/project".to_string()));
                assert!(!preview);
                assert_eq!(url, "https://github.com/user/my-skill");
            }
            _ => panic!("expected Install command"),
        }
    }

    #[test]
    fn test_install_with_single_agent() {
        let cli = Cli::parse_from([
            "skillstar",
            "install",
            "--agent",
            "codex",
            "https://github.com/user/my-skill",
        ]);
        match cli.command {
            Commands::Install { agent, preview, .. } => {
                assert_eq!(agent, vec!["codex"]);
                assert!(!preview);
            }
            _ => panic!("expected Install command"),
        }
    }

    #[test]
    fn test_install_with_multiple_agents_comma_separated() {
        let cli = Cli::parse_from([
            "skillstar",
            "install",
            "--agent",
            "codex,opencode",
            "https://github.com/user/my-skill",
        ]);
        match cli.command {
            Commands::Install { agent, preview, .. } => {
                assert_eq!(agent, vec!["codex", "opencode"]);
                assert!(!preview);
            }
            _ => panic!("expected Install command"),
        }
    }

    #[test]
    fn test_install_with_name() {
        let cli = Cli::parse_from([
            "skillstar",
            "install",
            "--name",
            "my-cool-skill",
            "https://github.com/user/my-skill",
        ]);
        match cli.command {
            Commands::Install {
                url, name, preview, ..
            } => {
                assert_eq!(name, Some("my-cool-skill".to_string()));
                assert!(!preview);
                assert_eq!(url, "https://github.com/user/my-skill");
            }
            _ => panic!("expected Install command"),
        }
    }

    #[test]
    fn test_install_all_flags() {
        let cli = Cli::parse_from([
            "skillstar",
            "install",
            "--agent",
            "claude",
            "--name",
            "my-skill",
            "https://github.com/user/repo",
        ]);
        match cli.command {
            Commands::Install {
                url,
                global,
                project,
                agent,
                name,
                skill,
                preview,
            } => {
                assert!(!global);
                assert!(project.is_none());
                assert_eq!(agent, vec!["claude"]);
                assert_eq!(name, Some("my-skill".to_string()));
                assert!(skill.is_empty());
                assert!(!preview);
                assert_eq!(url, "https://github.com/user/repo");
            }
            _ => panic!("expected Install command"),
        }
    }

    #[test]
    fn test_install_with_skill_single() {
        let cli = Cli::parse_from([
            "skillstar",
            "install",
            "--skill",
            "my-cool-skill",
            "https://github.com/user/multi-skill-repo",
        ]);
        match cli.command {
            Commands::Install {
                url,
                skill,
                preview,
                ..
            } => {
                assert_eq!(skill, vec!["my-cool-skill"]);
                assert!(!preview);
                assert_eq!(url, "https://github.com/user/multi-skill-repo");
            }
            _ => panic!("expected Install command"),
        }
    }

    #[test]
    fn test_install_with_skill_multiple_comma_separated() {
        let cli = Cli::parse_from([
            "skillstar",
            "install",
            "--skill",
            "skill1,skill2,skill3",
            "https://github.com/user/multi-skill-repo",
        ]);
        match cli.command {
            Commands::Install { skill, preview, .. } => {
                assert_eq!(skill, vec!["skill1", "skill2", "skill3"]);
                assert!(!preview);
            }
            _ => panic!("expected Install command"),
        }
    }

    #[test]
    fn test_install_with_skill_repeatable_flags() {
        let cli = Cli::parse_from([
            "skillstar",
            "install",
            "--skill",
            "skill1",
            "--skill",
            "skill2",
            "https://github.com/user/multi-skill-repo",
        ]);
        match cli.command {
            Commands::Install { skill, preview, .. } => {
                assert_eq!(skill, vec!["skill1", "skill2"]);
                assert!(!preview);
            }
            _ => panic!("expected Install command"),
        }
    }

    #[test]
    fn test_install_with_skill_and_agent() {
        let cli = Cli::parse_from([
            "skillstar",
            "install",
            "--skill",
            "frontend,backend",
            "--agent",
            "claude,codex",
            "https://github.com/user/multi-skill-repo",
        ]);
        match cli.command {
            Commands::Install {
                skill,
                agent,
                preview,
                ..
            } => {
                assert_eq!(skill, vec!["frontend", "backend"]);
                assert_eq!(agent, vec!["claude", "codex"]);
                assert!(!preview);
            }
            _ => panic!("expected Install command"),
        }
    }

    #[test]
    fn test_install_preview_flag() {
        let cli = Cli::parse_from([
            "skillstar",
            "install",
            "--preview",
            "https://github.com/user/my-skill",
        ]);
        match cli.command {
            Commands::Install { url, preview, .. } => {
                assert!(preview);
                assert_eq!(url, "https://github.com/user/my-skill");
            }
            _ => panic!("expected Install command"),
        }
    }

    #[test]
    fn test_install_preview_with_other_flags() {
        let cli = Cli::parse_from([
            "skillstar",
            "install",
            "--preview",
            "--skill",
            "my-skill",
            "--agent",
            "claude",
            "https://github.com/user/multi-skill-repo",
        ]);
        match cli.command {
            Commands::Install {
                url,
                skill,
                agent,
                preview,
                ..
            } => {
                assert!(preview);
                assert_eq!(skill, vec!["my-skill"]);
                assert_eq!(agent, vec!["claude"]);
                assert_eq!(url, "https://github.com/user/multi-skill-repo");
            }
            _ => panic!("expected Install command"),
        }
    }

    // ── Launch::Deploy parsing ─────────────────────────────────────────────────

    #[test]
    fn test_launch_deploy_basic() {
        let cli = Cli::parse_from(["skillstar", "launch", "deploy", "my-project"]);
        match cli.command {
            Commands::Launch { action } => match action {
                LaunchAction::Deploy { project_name } => {
                    assert_eq!(project_name, "my-project");
                }
                _ => panic!("expected LaunchAction::Deploy"),
            },
            _ => panic!("expected Launch command"),
        }
    }

    // ── Launch::Run parsing ────────────────────────────────────────────────────

    #[test]
    fn test_launch_run_basic() {
        let cli = Cli::parse_from(["skillstar", "launch", "run", "claude"]);
        match cli.command {
            Commands::Launch { action } => match action {
                LaunchAction::Run {
                    agent,
                    provider,
                    safe,
                    args,
                } => {
                    assert_eq!(agent, "claude");
                    assert!(provider.is_none());
                    assert!(!safe);
                    assert!(args.is_empty());
                }
                _ => panic!("expected LaunchAction::Run"),
            },
            _ => panic!("expected Launch command"),
        }
    }

    #[test]
    fn test_launch_run_with_provider() {
        let cli = Cli::parse_from([
            "skillstar",
            "launch",
            "run",
            "codex",
            "--provider",
            "my-provider",
        ]);
        match cli.command {
            Commands::Launch { action } => match action {
                LaunchAction::Run {
                    agent,
                    provider,
                    safe,
                    args,
                } => {
                    assert_eq!(agent, "codex");
                    assert_eq!(provider, Some("my-provider".to_string()));
                    assert!(!safe);
                    assert!(args.is_empty());
                }
                _ => panic!("expected LaunchAction::Run"),
            },
            _ => panic!("expected Launch command"),
        }
    }

    #[test]
    fn test_launch_run_safe_mode() {
        let cli = Cli::parse_from(["skillstar", "launch", "run", "gemini", "--safe"]);
        match cli.command {
            Commands::Launch { action } => match action {
                LaunchAction::Run { agent, safe, .. } => {
                    assert_eq!(agent, "gemini");
                    assert!(safe);
                }
                _ => panic!("expected LaunchAction::Run"),
            },
            _ => panic!("expected Launch command"),
        }
    }

    #[test]
    fn test_launch_run_with_trailing_args() {
        let cli = Cli::parse_from([
            "skillstar",
            "launch",
            "run",
            "opencode",
            "arg1",
            "--flag",
            "value",
        ]);
        match cli.command {
            Commands::Launch { action } => match action {
                LaunchAction::Run { agent, args, .. } => {
                    assert_eq!(agent, "opencode");
                    assert_eq!(args, vec!["arg1", "--flag", "value"]);
                }
                _ => panic!("expected LaunchAction::Run"),
            },
            _ => panic!("expected Launch command"),
        }
    }

    #[test]
    fn test_launch_run_provider_and_safe() {
        let cli = Cli::parse_from([
            "skillstar",
            "launch",
            "run",
            "claude",
            "--provider",
            "codex-provider",
            "--safe",
        ]);
        match cli.command {
            Commands::Launch { action } => match action {
                LaunchAction::Run {
                    agent,
                    provider,
                    safe,
                    args,
                } => {
                    assert_eq!(agent, "claude");
                    assert_eq!(provider, Some("codex-provider".to_string()));
                    assert!(safe);
                    assert!(args.is_empty());
                }
                _ => panic!("expected LaunchAction::Run"),
            },
            _ => panic!("expected Launch command"),
        }
    }

    #[test]
    fn test_launch_run_full() {
        let cli = Cli::parse_from([
            "skillstar",
            "launch",
            "run",
            "gemini",
            "--provider",
            "fast-model",
            "--safe",
            "--verbose",
            "file.md",
        ]);
        match cli.command {
            Commands::Launch { action } => match action {
                LaunchAction::Run {
                    agent,
                    provider,
                    safe,
                    args,
                } => {
                    assert_eq!(agent, "gemini");
                    assert_eq!(provider, Some("fast-model".to_string()));
                    assert!(safe);
                    assert_eq!(args, vec!["--verbose", "file.md"]);
                }
                _ => panic!("expected LaunchAction::Run"),
            },
            _ => panic!("expected Launch command"),
        }
    }
}
