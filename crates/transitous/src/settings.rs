//! `transitous config`: the configuration file and the settings in effect.

use std::io::Write;
use std::path::PathBuf;

use anyhow::Context as _;
use serde::Serialize;
use transitous_core::Config;
use transitous_core::config::Settings;

use crate::ConfigCommand;
use crate::output::{self, Format};

/// The settings in effect.
#[derive(Debug, Serialize)]
struct Shown {
    /// The configuration file.
    file: Option<String>,
    contact: Option<String>,
    base_url: String,
    timeout_seconds: u64,
    plan_daily_limit: u32,
    delay_threshold_minutes: u32,
    /// `None`: no cache.
    cache_dir: Option<String>,
    state_dir: Option<String>,
}

fn path() -> anyhow::Result<PathBuf> {
    Settings::path(&crate::env)
        .context("the home directory is unknown: set TRANSITOUS_CONFIG to the configuration file")
}

fn display(path: Option<&PathBuf>) -> Option<String> {
    path.map(|path| path.display().to_string())
}

pub(crate) fn run(
    command: ConfigCommand,
    config: &Config,
    format: Format,
    out: &mut impl Write,
) -> anyhow::Result<()> {
    match command {
        ConfigCommand::Show => {
            let shown = Shown {
                file: display(Settings::path(&crate::env).as_ref()),
                contact: config.contact.clone(),
                base_url: config.base_url.clone(),
                timeout_seconds: config.timeout.as_secs(),
                plan_daily_limit: config.plan_daily_limit,
                delay_threshold_minutes: config.delay_threshold_minutes,
                cache_dir: display(config.cache_dir.as_ref()),
                state_dir: display(config.state_dir.as_ref()),
            };
            output::show(&shown, format, out, show)
        }
        ConfigCommand::Path => Ok(writeln!(out, "{}", path()?.display())?),
        ConfigCommand::Get { key } => {
            let value = Settings::load(&path()?)
                .and_then(|settings| settings.get(&key))
                .map_err(transitous_core::Error::from)?;
            if let Some(value) = value {
                writeln!(out, "{value}")?;
            }
            Ok(())
        }
        ConfigCommand::Set { key, value } => change(&key, Some(&value), out),
        ConfigCommand::Unset { key } => change(&key, None, out),
    }
}

fn change(key: &str, value: Option<&str>, out: &mut impl Write) -> anyhow::Result<()> {
    let path = path()?;
    let mut settings = Settings::load(&path).map_err(transitous_core::Error::from)?;
    settings
        .set(key, value)
        .and_then(|()| settings.save(&path))
        .map_err(transitous_core::Error::from)?;
    let verb = if value.is_some() { "set" } else { "unset" };
    writeln!(out, "{verb} {key} in {}", path.display())?;
    Ok(())
}

fn show(shown: &Shown, out: &mut impl Write) -> anyhow::Result<()> {
    let unset = || "(unset)".to_owned();
    let rows = [
        ("file", shown.file.clone().unwrap_or_else(unset)),
        ("contact", shown.contact.clone().unwrap_or_else(unset)),
        ("base_url", shown.base_url.clone()),
        ("timeout_seconds", shown.timeout_seconds.to_string()),
        ("plan_daily_limit", shown.plan_daily_limit.to_string()),
        (
            "delay_threshold_minutes",
            shown.delay_threshold_minutes.to_string(),
        ),
        (
            "cache_dir",
            shown
                .cache_dir
                .clone()
                .unwrap_or_else(|| "(off)".to_owned()),
        ),
        ("state_dir", shown.state_dir.clone().unwrap_or_else(unset)),
    ];
    for (key, value) in rows {
        writeln!(out, "{key:<24} {value}")?;
    }
    Ok(())
}
