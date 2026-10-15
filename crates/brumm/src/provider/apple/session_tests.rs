use super::session::Apple;
use crate::{
    config::{Config, testenv},
    provider::{registry, session::*},
};

#[test]
fn apple_definition_preserves_navigation_and_private_playback_setup() {
    let cfg = Config {
        developer_token: "fictional-developer".into(),
        user_token: "fictional-user".into(),
        ..Default::default()
    };
    let provider = registry::definition(cfg.effective_provider()).unwrap();
    assert_eq!(provider.id(), "apple");
    assert!(provider.signed_in(&cfg));
    provider.validate(&cfg).unwrap();
    let page = provider.playback(&cfg, "fixture").unwrap();
    assert!(page.html.contains("fictional-user"));
    let refresh = page.refresh.unwrap();
    assert_eq!(refresh.method, "brumm.setToken");
    assert_eq!(refresh.args, vec![serde_json::json!("fictional-user")]);
    let fixture: serde_json::Value =
        serde_json::from_str(include_str!("../../../../../dev/fakedaemon/provider.json")).unwrap();
    let entries: Vec<NavigationEntry> =
        serde_json::from_value(fixture["navigation"].clone()).unwrap();
    assert_eq!(navigation(provider.navigation()), entries);
    assert_eq!(
        serde_json::to_value(provider.capabilities()).unwrap(),
        fixture["capabilities"]
    );
    assert!(
        !serde_json::to_string(&registry::selection(&cfg))
            .unwrap()
            .contains("fictional-user")
    );
}

#[test]
fn legacy_apple_cache_is_imported_once_and_accounts_are_isolated() {
    let Some(_env) = testenv::isolated() else {
        return;
    };
    let mut cfg = Config {
        user_token: "fictional-account-one".into(),
        ..Default::default()
    };
    cfg.save().unwrap();
    let legacy = crate::config::cache_dir();
    std::fs::create_dir_all(&legacy).unwrap();
    std::fs::write(legacy.join("library.json"), b"{\"items\":{}}").unwrap();
    crate::config::migrate_session_cache().unwrap();
    let first = crate::config::session_cache_dir();
    assert!(first.join("library.json").exists());
    assert!(legacy.join("library.json").exists());
    cfg.user_token = "fictional-account-two".into();
    cfg.save().unwrap();
    crate::config::migrate_session_cache().unwrap();
    let second = crate::config::session_cache_dir();
    assert_ne!(first, second);
    assert!(!second.join("library.json").exists());
    assert_ne!(Apple.account_key(&cfg), "fictional-account-two");
}
