Now I have a comprehensive view of the codebase. Let me analyze and design a cleaner project structure, then identify which files can be deleted/merged.
Analysis Summary
The current structure has:

129 Rust files total in src-tauri/src
19 backward-compatibility shims - files with 2-10 lines that just delegate to the new domain structure
Multiple levels of re-exports creating indirection
The migration is clearly in progress: the team created new domain modules under core/ (like core/git/, core/skills/, core/infra/) but hasn't yet cleaned up the old flat module structure or the backward-compatibility shims.

Proposed Cleaner Structure
Rather than cleaning up individual files, I should redesign the overall structure to be more streamlined. The key is reducing unnecessary indirection, consolidating tiny files, and organizing by domain rather than having scattered re-exports. The main opportunities are removing those 19 shims entirely and reorganizing the core/mod.rs to directly expose the relevant modules instead of layering re-exports through intermediate files.

Looking at the specific files, most of the stub files and delegation modules could be eliminated by either moving their logic directly where it's used or by restructuring the imports. Files like core/skills/discover.rs are just 8 lines of delegation, and core/git/mod.rs is similarly minimal at 9 lines - these could be absorbed into their parent modules or consolidated with sibling files. The core/terminal/mod.rs and core/ai/mod.rs delegation files are prime candidates for removal if their functionality gets migrated into more comprehensive parent modules.

Now I have the full picture. Let me analyze the findings and propose a redesign.

Code Review Summary
Statistics
Total .rs files: 129
Files < 20 lines: 19 (all backward-compatibility shims)
Files < 50 lines: 26 total
Largest files: security_scan/mod.rs (4656 lines), security_scan/orchestrator.rs (2405 lines), ai_provider/mod.rs (2462 lines)
The Core Problem
The codebase is in the middle of a module reorganization. The team created new domain-grouped subdirectories under core/:

core/infra/ - infrastructure
core/git/ - git operations
core/skills/ - skill lifecycle
core/config/ - config
core/ai/ - AI domain
core/projects/ - project management
core/terminal/ - terminal
But the old flat module structure is still intact as backward-compatibility shims, creating massive indirection.

Proposed Redesign
Here is the target structure and which files to delete/merge:

Files to DELETE (19 backward-compatibility shims)
These files exist ONLY for import compatibility during the module migration. They add indirection with zero logic:

Shim File	Lines	Delegates To
core/update_checker.rs
2
skills::update_checker
core/gh_manager.rs
2
git::gh_manager
core/skill_discover.rs
2
skills::discover
core/git_ops.rs
2
git::ops
core/launch_deck.rs
2
terminal::config
core/translation_cache.rs
2
ai::translation_cache
core/github_mirror.rs
2
config::github_mirror
core/proxy.rs
2
config::proxy
core/sync.rs
2
projects::sync
core/dismissed_skills.rs
2
git::dismissed_skills
core/source_resolver.rs
2
git::source_resolver
core/repo_history.rs
2
git::repo_history
core/agent_profile.rs
2
projects::agents
core/util.rs
4
infra::util
core/db_pool.rs
5
infra::db_pool
core/error.rs
4
infra::error
core/terminal/mod.rs
5
terminal_backend
core/ai/mod.rs
5
ai_provider
core/projects/mod.rs
5
projects::agents, projects::sync
core/config/mod.rs
3
github_mirror, proxy
Files to MERGE (small files with minimal logic)
File	Lines	Merge Into	Reason
core/skills/discover.rs
8
core/skills/mod.rs
Just pub use discover::*;
core/git/mod.rs
9
core/git/ops.rs or a new core/git.rs
Only pub mod declarations + docs
core/infra/mod.rs
10
core/infra/paths.rs or core/infra.rs
Only pub mod declarations + docs
core/terminal_backend/session.rs
20
core/terminal_backend/mod.rs
Small session type
Before and After Comparison
Metric	Before	After	Reduction
Total .rs files
129
~107
-22 files (-17%)
Shim files (< 20 lines)
19
0
-19
Stub mod.rs files
4
0
-4
New Flat Structure (Proposed)
Rather than having both flat + domain-grouped, consolidate:

src-tauri/src/
├── lib.rs / main.rs / cli.rs
├── commands/                          # Keep as-is (well-organized)
│   ├── mod.rs
│   ├── acp.rs
│   ├── agents.rs
│   ├── ai/           # Keep subdirectory (has translate/scan/summarize)
│   ├── bundles.rs
│   ├── github.rs
│   ├── launch.rs
│   ├── marketplace.rs
│   ├── models.rs
│   ├── network.rs
│   ├── patrol.rs
│   ├── projects.rs
│   ├── shell.rs
│   ├── skill_content.rs
│   ├── skill_groups.rs
│   ├── skill_paths.rs
│   ├── skills.rs
│   └── updater.rs
└── core/
    ├── mod.rs                     # All domain re-exports in ONE place
    ├── infra.rs                   # 78 lines - NOT a dir, just fs_ops+paths+migration
    ├── infra/                     # Cross-cutting utilities
    │   ├── db_pool.rs
    │   ├── error.rs
    │   ├── fs_ops.rs              # 404 lines
    │   ├── migration.rs           # 166 lines
    │   ├── paths.rs               # 221 lines
    │   └── util.rs                # 33 lines
    ├── ai_provider.rs             # 2462 lines - consider splitting
    ├── config.rs                  # github_mirror + proxy combined
    ├── git.rs                     # ops + gh_manager + repo_history combined
    ├── projects.rs                 # agents + sync combined
    ├── terminal.rs                # config + terminal_backend combined
    ├── skills/                    # Keep subdirectory (multiple related files)
    │   ├── mod.rs
    │   ├── discover.rs
    │   ├── installed_skill.rs
    │   ├── local_skill.rs
    │   ├── repo_scanner/
    │   ├── skill_bundle.rs
    │   ├── skill_group.rs
    │   ├── skill_install.rs
    │   ├── skill_pack.rs
    │   └── skill_update.rs
    ├── security_scan/             # Keep (massive - 4656+2405 lines)
    ├── model_config/              # Keep (multiple related files)
    ├── marketplace_snapshot/       # Keep (single file + future)
    └── project_manifest/          # Keep (single massive file - 1134 lines)
Recommended Cleanup Order
Phase 1: Delete all 19 backward-compatibility shims (no logic loss)
Phase 2: Update all use crate::core::X imports to point to new domain paths
Phase 3: Merge small mod.rs stub files into parent modules
Phase 4: Consider flattening core/infra/ and core/git/ into single files if they remain small
Should I proceed with this cleanup?
This will require:

Updating ~60-80 import statements across the codebase that reference the old shim paths
The reward is ~22 fewer files and a much cleaner module graph
Want me to proceed with the implementation?