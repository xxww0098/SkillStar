use crate::marketplace::{MarketplaceManifest, PluginEntry, PluginOwner};
use crate::validate::{
    validate_entry_name, validate_marketplace, validate_marketplace_name,
    validate_relative_source, MarketplaceError,
};

fn assert_invalid_name(name: &str, reason_part: &str) {
    let error = validate_entry_name(name).unwrap_err();
    let MarketplaceError::InvalidName { value, reason, .. } = &error else {
        panic!("expected InvalidName for {name:?}, got {error:?}");
    };
    assert_eq!(value, name);
    assert!(reason.contains(reason_part), "unexpected reason: {reason}");
}

#[test]
fn accepts_kebab_case_names() {
    for name in ["a", "my-skill", "superpowers-chrome", "skill-42", "a1b2"] {
        validate_entry_name(name).unwrap();
        validate_marketplace_name(name).unwrap();
    }
}

#[test]
fn rejects_malformed_names() {
    assert_invalid_name("", "empty");
    assert_invalid_name("-leading", "start and end");
    assert_invalid_name("trailing-", "start and end");
    assert_invalid_name("Upper-Case", "lowercase");
    assert_invalid_name("with space", "lowercase");
    assert_invalid_name("under_score", "lowercase");
    assert_invalid_name("dot.name", "lowercase");
    assert_invalid_name(&"x".repeat(65), "64 characters");
}

#[test]
fn rejects_reserved_marketplace_name() {
    let error = validate_marketplace_name("claude-plugins-official").unwrap_err();
    assert!(matches!(error, MarketplaceError::ReservedName(_)));
}

#[test]
fn accepts_wellformed_relative_sources() {
    for source in [
        "./plugins/x",
        "./plugins/nested/deep/plugin",
        "./plugins/plugin-42",
    ] {
        validate_relative_source(source).unwrap();
    }
}

#[test]
fn rejects_malformed_relative_sources() {
    for (source, reason_part) in [
        ("plugins/x", "must start with ./"),
        ("/absolute/x", "must start with ./"),
        ("./x/../y", "dot components"),
        ("./x/./y", "dot components"),
        ("./x//y", "empty path component"),
        ("./x/", "empty path component"),
        ("./", "path is empty"),
        (".\\x", "must start with ./"),
        ("./ok\\x", "backslashes"),
    ] {
        let error = validate_relative_source(source).unwrap_err();
        let MarketplaceError::InvalidSourcePath { value, reason } = &error else {
            panic!("expected InvalidSourcePath for {source:?}, got {error:?}");
        };
        assert_eq!(value, source);
        assert!(reason.contains(reason_part), "unexpected reason: {reason}");
    }
}

#[test]
fn validates_whole_marketplace() {
    let mut manifest =
        MarketplaceManifest::new("team-channel", PluginOwner::new("acme"));
    manifest.plugins.push(PluginEntry::relative("pr-review", "./plugins/pr-review"));
    manifest.plugins.push(PluginEntry::relative("tdd", "./plugins/tdd"));
    validate_marketplace(&manifest).unwrap();

    let mut duplicate = manifest.clone();
    duplicate.plugins.push(PluginEntry::relative("tdd", "./plugins/tdd-2"));
    assert!(matches!(
        validate_marketplace(&duplicate).unwrap_err(),
        MarketplaceError::DuplicateEntry(_)
    ));

    let mut reserved = MarketplaceManifest::new(
        "claude-plugins-official",
        PluginOwner::new("acme"),
    );
    reserved.plugins.push(PluginEntry::relative("x", "./plugins/x"));
    assert!(matches!(
        validate_marketplace(&reserved).unwrap_err(),
        MarketplaceError::ReservedName(_)
    ));
}
