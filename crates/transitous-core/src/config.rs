//! The configuration: the file, overridden by environment variables.
//!
//! ```toml
//! contact = "mailto:me@example.org"
//! base_url = "https://api.transitous.org"
//! timeout_seconds = 20
//! plan_daily_limit = 720
//! delay_threshold_minutes = 2
//! ```
//!
//! The file is `config.toml` in the directory of [`dirs::config`], or the
//! one `TRANSITOUS_CONFIG` names. `TRANSITOUS_CONTACT` and
//! `TRANSITOUS_BASE_URL` override the file, and `TRANSITOUS_NO_CACHE` turns
//! the cache off.

use std::io;
use std::path::{Path, PathBuf};
use std::time::Duration;

use serde::{Deserialize, Serialize};

use crate::dirs;

/// The public instance.
pub const DEFAULT_BASE_URL: &str = "https://api.transitous.org";
pub const DEFAULT_TIMEOUT_SECONDS: u64 = 20;
/// The number of `plan` requests per 24 hours that Transitous allows without
/// asking.
pub const DEFAULT_PLAN_DAILY_LIMIT: u32 = 720;
/// From how many minutes late an arrival counts as delayed.
pub const DEFAULT_DELAY_THRESHOLD_MINUTES: u32 = 2;

/// The configuration file could not be read or written, or a value is
/// invalid.
#[derive(Debug, thiserror::Error)]
pub enum Error {
    #[error("cannot read {}", path.display())]
    Read { path: PathBuf, source: io::Error },
    #[error("{} is not a valid configuration", path.display())]
    Parse {
        path: PathBuf,
        source: toml::de::Error,
    },
    #[error("cannot write {}", path.display())]
    Write { path: PathBuf, source: io::Error },
    #[error("unknown setting {0}, known are: {keys}", keys = Settings::KEYS.join(", "))]
    UnknownKey(String),
    #[error("invalid value for {key}: {reason}")]
    InvalidValue { key: String, reason: String },
}

/// The contents of the configuration file. Unset values take their default.
#[derive(Debug, Default, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct Settings {
    /// How Transitous can reach the operator: a URL, a `mailto:` URL or an
    /// email address. Required for every request.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub contact: Option<String>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub base_url: Option<String>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub timeout_seconds: Option<u64>,
    /// Raise only after agreeing on more with Transitous.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub plan_daily_limit: Option<u32>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub delay_threshold_minutes: Option<u32>,
}

impl Settings {
    /// The names of the settings, as in the file.
    pub const KEYS: &[&str] = &[
        "contact",
        "base_url",
        "timeout_seconds",
        "plan_daily_limit",
        "delay_threshold_minutes",
    ];

    /// The file `TRANSITOUS_CONFIG` names, else `config.toml` in
    /// [`dirs::config`]. `None` if the home directory is unknown.
    pub fn path(env: &impl Fn(&str) -> Option<String>) -> Option<PathBuf> {
        variable(env, "TRANSITOUS_CONFIG")
            .map(PathBuf::from)
            .or_else(|| dirs::config().map(|dir| dir.join("config.toml")))
    }

    /// Reads the file, or returns empty settings if it does not exist.
    pub fn load(path: &Path) -> Result<Self, Error> {
        let text = match std::fs::read_to_string(path) {
            Ok(text) => text,
            Err(error) if error.kind() == io::ErrorKind::NotFound => return Ok(Self::default()),
            Err(source) => {
                return Err(Error::Read {
                    path: path.to_owned(),
                    source,
                });
            }
        };
        toml::from_str(&text).map_err(|source| Error::Parse {
            path: path.to_owned(),
            source,
        })
    }

    /// Writes the file, creating its directory if needed. Replaces the file
    /// at once, so a failed write leaves the old one intact.
    pub fn save(&self, path: &Path) -> Result<(), Error> {
        let write = |source| Error::Write {
            path: path.to_owned(),
            source,
        };
        let text = toml::to_string(self)
            .map_err(io::Error::other)
            .map_err(write)?;
        // A relative file name has an empty parent, which needs no creation.
        std::fs::create_dir_all(path.parent().unwrap_or(Path::new(""))).map_err(write)?;
        let temporary = path.with_extension("toml.tmp");
        std::fs::write(&temporary, text).map_err(write)?;
        std::fs::rename(&temporary, path).map_err(write)
    }

    /// The value of a setting, as text. `None` if unset.
    pub fn get(&self, key: &str) -> Result<Option<String>, Error> {
        Ok(match key {
            "contact" => self.contact.clone(),
            "base_url" => self.base_url.clone(),
            "timeout_seconds" => self.timeout_seconds.map(|v| v.to_string()),
            "plan_daily_limit" => self.plan_daily_limit.map(|v| v.to_string()),
            "delay_threshold_minutes" => self.delay_threshold_minutes.map(|v| v.to_string()),
            _ => return Err(Error::UnknownKey(key.to_owned())),
        })
    }

    /// Sets a setting from text, or unsets it with `None`. Checks the value.
    pub fn set(&mut self, key: &str, value: Option<&str>) -> Result<(), Error> {
        let invalid = |reason: String| Error::InvalidValue {
            key: key.to_owned(),
            reason,
        };
        match key {
            "contact" => {
                if let Some(contact) = value {
                    crate::agent::check_contact(contact).map_err(|e| invalid(e.to_string()))?;
                }
                self.contact = value.map(str::to_owned);
            }
            "base_url" => {
                if let Some(url) = value {
                    motis::Url::parse(url).map_err(|e| invalid(e.to_string()))?;
                }
                self.base_url = value.map(str::to_owned);
            }
            "timeout_seconds" => self.timeout_seconds = number(value).map_err(invalid)?,
            "plan_daily_limit" => self.plan_daily_limit = number(value).map_err(invalid)?,
            "delay_threshold_minutes" => {
                self.delay_threshold_minutes = number(value).map_err(invalid)?;
            }
            _ => return Err(Error::UnknownKey(key.to_owned())),
        }
        Ok(())
    }
}

fn number<T: std::str::FromStr<Err: std::fmt::Display>>(
    value: Option<&str>,
) -> Result<Option<T>, String> {
    value
        .map(|v| v.parse().map_err(|e| format!("{e}")))
        .transpose()
}

/// A non-empty environment variable.
fn variable(env: &impl Fn(&str) -> Option<String>, name: &str) -> Option<String> {
    env(name).filter(|value| !value.is_empty())
}

/// The configuration in effect.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Config {
    pub contact: Option<String>,
    pub base_url: String,
    pub timeout: Duration,
    pub plan_daily_limit: u32,
    pub delay_threshold_minutes: u32,
    /// `None`: no cache.
    pub cache_dir: Option<PathBuf>,
    /// Where the usage counter lives. `None`: the temporary directory.
    pub state_dir: Option<PathBuf>,
}

impl Config {
    /// Reads the file and applies the environment variables `env` returns.
    pub fn load(env: &impl Fn(&str) -> Option<String>) -> Result<Self, Error> {
        let settings = Settings::path(env)
            .map(|path| Settings::load(&path))
            .transpose()?
            .unwrap_or_default();
        Ok(Self::new(settings, env))
    }

    /// The settings with the environment variables `env` returns applied.
    pub fn new(settings: Settings, env: &impl Fn(&str) -> Option<String>) -> Self {
        let no_cache = variable(env, "TRANSITOUS_NO_CACHE")
            .is_some_and(|value| !matches!(value.as_str(), "0" | "false"));
        Self {
            contact: variable(env, "TRANSITOUS_CONTACT").or(settings.contact),
            base_url: variable(env, "TRANSITOUS_BASE_URL")
                .or(settings.base_url)
                .unwrap_or_else(|| DEFAULT_BASE_URL.to_owned()),
            timeout: Duration::from_secs(
                settings.timeout_seconds.unwrap_or(DEFAULT_TIMEOUT_SECONDS),
            ),
            plan_daily_limit: settings
                .plan_daily_limit
                .unwrap_or(DEFAULT_PLAN_DAILY_LIMIT),
            delay_threshold_minutes: settings
                .delay_threshold_minutes
                .unwrap_or(DEFAULT_DELAY_THRESHOLD_MINUTES),
            cache_dir: if no_cache { None } else { dirs::cache() },
            state_dir: dirs::state(),
        }
    }
}
