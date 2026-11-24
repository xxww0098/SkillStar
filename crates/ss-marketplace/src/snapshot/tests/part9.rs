use super::*;
use crate::snapshot::*;

/// An adopted Skill of unknown origin owns the name `ui-ux-pro-max`; the
/// marketplace Skill of that name, installed from its own source, lands in
/// `ui-ux-pro-max-nextlevelbuilder` and records the upstream name.
fn coexisting_installs() -> Vec<Skill> {
    let mut adopted = Skill::from_skills_sh(
        "ui-ux-pro-max".into(),
        String::new(),
        0,
        String::new(),
        String::new(),
    );
    // Without a lock entry the installed list names the Skill as its source.
    adopted.source = Some("ui-ux-pro-max".into());
    adopted.installed = true;

    let mut renamed = Skill::from_skills_sh(
        "ui-ux-pro-max".into(),
        String::new(),
        0,
        String::new(),
        "https://github.com/nextlevelbuilder/ui-ux-pro-max-skill".into(),
    );
    renamed.name = "ui-ux-pro-max-nextlevelbuilder".into();
    renamed.source = Some("nextlevelbuilder/ui-ux-pro-max-skill".into());
    renamed.called = Some("ui-ux-pro-max".into());
    renamed.installed = true;

    vec![adopted, renamed]
}

fn market_row(source: &str, name: &str) -> Skill {
    let mut row = Skill::from_skills_sh(
        name.into(),
        String::new(),
        0,
        String::new(),
        format!("https://github.com/{source}"),
    );
    row.source = Some(source.into());
    row
}

fn runtime_with_installed(
    root: &Path,
    loader: fn() -> InstalledSkillsFuture,
) -> tokio::runtime::Runtime {
    configure_runtime(SnapshotRuntimeConfig::new(
        root.join("marketplace.db"),
        root.to_path_buf(),
        HashSet::new,
        loader,
    ));
    tokio::runtime::Builder::new_current_thread()
        .build()
        .expect("build tokio runtime")
}

#[test]
fn a_row_and_the_folder_it_uninstalls_agree_across_a_coexist_rename() {
    with_temp_data_root(|root| {
        let runtime = runtime_with_installed(root, || {
            Box::pin(async { Ok(coexisting_installs()) })
        });

        // The row is installed through the Skill its own source put in the
        // renamed folder, not through the same-named Skill of unknown origin.
        let rows = runtime.block_on(apply_installed_state(vec![
            market_row("nextlevelbuilder/ui-ux-pro-max-skill", "ui-ux-pro-max"),
            market_row("other/repo", "ui-ux-pro-max"),
            market_row("nextlevelbuilder/ui-ux-pro-max-skill", "missing"),
        ]));
        assert!(rows[0].installed);
        assert_eq!(rows[0].called.as_deref(), Some("ui-ux-pro-max"));
        assert!(rows[1].installed, "a same-named installed Skill still marks its row");
        assert_eq!(rows[1].called, None);
        assert!(!rows[2].installed);

        // Uninstalling a row removes exactly what makes it installed.
        assert_eq!(
            runtime.block_on(installed_folder_for(
                "nextlevelbuilder/ui-ux-pro-max-skill",
                "ui-ux-pro-max"
            )),
            Some("ui-ux-pro-max-nextlevelbuilder".into())
        );
        assert_eq!(
            runtime.block_on(installed_folder_for("other/repo", "ui-ux-pro-max")),
            Some("ui-ux-pro-max".into())
        );
        assert_eq!(
            runtime.block_on(installed_folder_for(
                "nextlevelbuilder/ui-ux-pro-max-skill",
                "missing"
            )),
            None
        );
        // A row without a source finds its Skill by folder name.
        assert_eq!(
            runtime.block_on(installed_folder_for("", "ui-ux-pro-max-nextlevelbuilder")),
            Some("ui-ux-pro-max-nextlevelbuilder".into())
        );
    });
}

#[test]
fn nothing_is_installed_when_the_installed_skills_cannot_be_read() {
    with_temp_data_root(|root| {
        let runtime = runtime_with_installed(root, || {
            Box::pin(async { Err(anyhow::anyhow!("install lock unreadable")) })
        });

        let mut row = market_row("nextlevelbuilder/ui-ux-pro-max-skill", "ui-ux-pro-max");
        row.stars = 7;
        let rows = runtime.block_on(apply_installed_state(vec![row]));
        assert!(!rows[0].installed, "a failed read marks nothing installed");
        assert_eq!(rows[0].stars, 7, "the row is otherwise untouched");
        assert_eq!(
            runtime.block_on(installed_folder_for(
                "nextlevelbuilder/ui-ux-pro-max-skill",
                "ui-ux-pro-max"
            )),
            None
        );
    });
}
