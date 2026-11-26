//! The configuration file and the environment variables.

use std::collections::HashMap;
use std::time::Duration;

use transitous_core::config::{Config, DEFAULT_BASE_URL, Error, Settings};

fn env(pairs: &[(&str, &str)]) -> impl Fn(&str) -> Option<String> {
    let map: HashMap<String, String> = pairs
        .iter()
        .map(|(k, v)| ((*k).to_owned(), (*v).to_owned()))
        .collect();
    move |name| map.get(name).cloned()
}

#[test]
fn defaults_without_a_file() {
    let dir = tempfile::tempdir().unwrap();
    let path = dir.path().join("missing.toml");
    let path = path.to_str().unwrap();
    let config = Config::load(&env(&[("TRANSITOUS_CONFIG", path)])).unwrap();
    assert_eq!(config.contact, None);
    assert_eq!(config.base_url, DEFAULT_BASE_URL);
    assert_eq!(config.timeout, Duration::from_secs(20));
    assert_eq!(config.plan_daily_limit, 720);
    assert_eq!(config.delay_threshold_minutes, 2);
}

#[test]
fn reads_the_file_and_lets_the_environment_override_it() {
    let dir = tempfile::tempdir().unwrap();
    let path = dir.path().join("config.toml");
    std::fs::write(
        &path,
        "contact = \"mailto:a@example.org\"\nbase_url = \"http://127.0.0.1:1\"\n\
         timeout_seconds = 5\nplan_daily_limit = 100\ndelay_threshold_minutes = 4\n",
    )
    .unwrap();
    let file = path.to_str().unwrap();
    let config = Config::load(&env(&[("TRANSITOUS_CONFIG", file)])).unwrap();
    assert_eq!(config.contact.as_deref(), Some("mailto:a@example.org"));
    assert_eq!(config.base_url, "http://127.0.0.1:1");
    assert_eq!(config.timeout, Duration::from_secs(5));
    assert_eq!(config.plan_daily_limit, 100);
    assert_eq!(config.delay_threshold_minutes, 4);

    let config = Config::load(&env(&[
        ("TRANSITOUS_CONFIG", file),
        ("TRANSITOUS_CONTACT", "https://example.org"),
        ("TRANSITOUS_BASE_URL", "http://127.0.0.1:2"),
        // Empty counts as unset.
        ("TRANSITOUS_NO_CACHE", ""),
    ]))
    .unwrap();
    assert_eq!(config.contact.as_deref(), Some("https://example.org"));
    assert_eq!(config.base_url, "http://127.0.0.1:2");
}

#[test]
fn the_cache_can_be_turned_off() {
    let off = |value| Config::new(Settings::default(), &env(&[("TRANSITOUS_NO_CACHE", value)]));
    assert_eq!(off("1").cache_dir, None);
    assert_eq!(off("true").cache_dir, None);
    assert_eq!(off("0").cache_dir, off("false").cache_dir);
}

#[test]
fn reports_a_broken_file() {
    let dir = tempfile::tempdir().unwrap();
    let path = dir.path().join("config.toml");
    std::fs::write(&path, "colour = \"blue\"\n").unwrap();
    let error = Settings::load(&path).unwrap_err();
    assert!(matches!(error, Error::Parse { .. }), "{error:?}");
    let file = path.to_str().unwrap();
    let error = Config::load(&env(&[("TRANSITOUS_CONFIG", file)])).unwrap_err();
    assert!(matches!(error, Error::Parse { .. }), "{error:?}");
    assert!(error.to_string().contains("config.toml"));
    // A directory cannot be read as a file.
    let error = Settings::load(dir.path()).unwrap_err();
    assert!(matches!(error, Error::Read { .. }), "{error:?}");
}

#[test]
fn the_default_path_is_in_the_config_directory() {
    let path = Settings::path(&env(&[]));
    if let Some(path) = path {
        assert!(
            path.ends_with("transitous/config.toml"),
            "{}",
            path.display()
        );
    }
}

#[test]
fn saves_and_loads_settings() {
    let dir = tempfile::tempdir().unwrap();
    let path = dir.path().join("sub/config.toml");
    let mut settings = Settings::default();
    settings.set("contact", Some("me@example.org")).unwrap();
    settings
        .set("base_url", Some("http://127.0.0.1:1"))
        .unwrap();
    settings.set("timeout_seconds", Some("7")).unwrap();
    settings.set("plan_daily_limit", Some("10")).unwrap();
    settings.set("delay_threshold_minutes", Some("3")).unwrap();
    settings.save(&path).unwrap();
    let loaded = Settings::load(&path).unwrap();
    assert_eq!(loaded, settings);
    for (key, value) in [
        ("contact", "me@example.org"),
        ("base_url", "http://127.0.0.1:1"),
        ("timeout_seconds", "7"),
        ("plan_daily_limit", "10"),
        ("delay_threshold_minutes", "3"),
    ] {
        assert_eq!(loaded.get(key).unwrap().as_deref(), Some(value), "{key}");
    }

    settings.set("contact", None).unwrap();
    settings.set("base_url", None).unwrap();
    settings.set("timeout_seconds", None).unwrap();
    assert_eq!(settings.get("contact").unwrap(), None);
    settings.save(&path).unwrap();
    assert_eq!(
        std::fs::read_to_string(&path).unwrap(),
        "plan_daily_limit = 10\ndelay_threshold_minutes = 3\n"
    );
}

#[test]
fn a_failed_save_names_the_file() {
    let dir = tempfile::tempdir().unwrap();
    let blocker = dir.path().join("file");
    std::fs::write(&blocker, "").unwrap();
    let error = Settings::default()
        .save(&blocker.join("config.toml"))
        .unwrap_err();
    assert!(matches!(error, Error::Write { .. }), "{error:?}");
}

#[test]
fn checks_values() {
    let mut settings = Settings::default();
    for (key, value) in [
        ("contact", "nobody"),
        ("base_url", "not a url"),
        ("timeout_seconds", "-1"),
        ("plan_daily_limit", "many"),
        ("delay_threshold_minutes", "1.5"),
    ] {
        let error = settings.set(key, Some(value)).unwrap_err();
        assert!(
            matches!(&error, Error::InvalidValue { key: k, .. } if k == key),
            "{error:?}"
        );
    }
    assert_eq!(settings, Settings::default());
    let error = settings.set("colour", Some("blue")).unwrap_err();
    assert!(
        error
            .to_string()
            .starts_with("unknown setting colour, known are: contact")
    );
    assert!(matches!(settings.get("colour"), Err(Error::UnknownKey(_))));
}
