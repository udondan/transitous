//! Where `transitous` keeps its files. On Linux and macOS the configuration
//! and the state follow the XDG base directories: `~/.config/transitous` and
//! `~/.local/state/transitous`, or below `$XDG_CONFIG_HOME` and
//! `$XDG_STATE_HOME`. On Windows they are the usual ones below `%APPDATA%`
//! and `%LOCALAPPDATA%`.
//!
//! The cache follows the platform: `~/.cache/transitous` or below
//! `$XDG_CACHE_HOME` on Linux, `~/Library/Caches/transitous` on macOS, and
//! below `%LOCALAPPDATA%` on Windows.

#[cfg(not(windows))]
use std::ffi::OsString;
#[cfg(not(windows))]
use std::path::Path;
use std::path::PathBuf;

const NAME: &str = "transitous";

/// The directory of the configuration. `None` if the home directory is
/// unknown.
#[cfg(not(windows))]
pub fn config() -> Option<PathBuf> {
    xdg(
        std::env::var_os("XDG_CONFIG_HOME"),
        home().as_deref(),
        ".config",
    )
}

/// The directory of the configuration. `None` if the home directory is
/// unknown.
#[cfg(windows)]
pub fn config() -> Option<PathBuf> {
    directories::ProjectDirs::from("", "", NAME).map(|dirs| dirs.config_dir().to_owned())
}

/// The directory of the state: the usage counter. `None` if the home
/// directory is unknown.
#[cfg(not(windows))]
pub fn state() -> Option<PathBuf> {
    xdg(
        std::env::var_os("XDG_STATE_HOME"),
        home().as_deref(),
        ".local/state",
    )
}

/// The directory of the state: the usage counter. `None` if the home
/// directory is unknown.
#[cfg(windows)]
pub fn state() -> Option<PathBuf> {
    directories::ProjectDirs::from("", "", NAME).map(|dirs| dirs.data_local_dir().to_owned())
}

/// The directory of the cache of responses. `None` if the home directory is
/// unknown.
#[cfg(not(any(windows, target_os = "macos")))]
pub fn cache() -> Option<PathBuf> {
    xdg(
        std::env::var_os("XDG_CACHE_HOME"),
        home().as_deref(),
        ".cache",
    )
}

/// The directory of the cache of responses. `None` if the home directory is
/// unknown.
#[cfg(target_os = "macos")]
pub fn cache() -> Option<PathBuf> {
    directories::BaseDirs::new().map(|dirs| dirs.cache_dir().join(NAME))
}

/// The directory of the cache of responses. `None` if the home directory is
/// unknown.
#[cfg(windows)]
pub fn cache() -> Option<PathBuf> {
    directories::ProjectDirs::from("", "", NAME).map(|dirs| dirs.cache_dir().to_owned())
}

#[cfg(not(windows))]
fn home() -> Option<PathBuf> {
    directories::BaseDirs::new().map(|dirs| dirs.home_dir().to_owned())
}

/// The directory of `transitous` below the XDG variable, or below its
/// default in the home directory. The specification ignores relative paths.
#[cfg(not(windows))]
fn xdg(variable: Option<OsString>, home: Option<&Path>, default: &str) -> Option<PathBuf> {
    let base = variable
        .map(PathBuf::from)
        .filter(|path| path.is_absolute())
        .or_else(|| home.map(|home| home.join(default)))?;
    Some(base.join(NAME))
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    #[cfg(not(windows))]
    fn follows_the_variable() {
        assert_eq!(
            xdg(Some("/xdg".into()), Some(Path::new("/home/me")), ".config"),
            Some(PathBuf::from("/xdg/transitous"))
        );
    }

    #[test]
    #[cfg(not(windows))]
    fn falls_back_to_the_home_directory() {
        let home = Some(Path::new("/home/me"));
        let expected = Some(PathBuf::from("/home/me/.local/state/transitous"));
        assert_eq!(xdg(None, home, ".local/state"), expected);
        assert_eq!(xdg(Some("".into()), home, ".local/state"), expected);
        assert_eq!(xdg(Some("relative".into()), home, ".local/state"), expected);
    }

    #[test]
    #[cfg(not(windows))]
    fn needs_a_home_directory() {
        assert_eq!(xdg(None, None, ".config"), None);
    }

    #[test]
    fn lies_in_a_directory_of_transitous() {
        for dir in [config().unwrap(), state().unwrap(), cache().unwrap()] {
            assert!(
                dir.components().any(|part| part.as_os_str() == NAME),
                "{}",
                dir.display()
            );
        }
    }
}
