//! The diagnostic log on stderr: `-v`, `-vv`, `TRANSITOUS_LOG` and
//! `TRANSITOUS_LOG_FILE`. Never on stdout, which belongs to the output and to the
//! MCP transport.

use std::fs::{File, OpenOptions};
use std::io;
use std::path::Path;
use std::sync::Mutex;

use anyhow::Context as _;
use tracing_subscriber::filter::{EnvFilter, LevelFilter};
use tracing_subscriber::layer::SubscriberExt as _;
use tracing_subscriber::util::SubscriberInitExt as _;
use tracing_subscriber::{Layer as _, fmt};

/// The environment variable with the level or filter of the log.
pub(crate) const ENV_LOG: &str = "TRANSITOUS_LOG";

/// The environment variable with a file that gets the log too.
pub(crate) const ENV_LOG_FILE: &str = "TRANSITOUS_LOG_FILE";

/// The crates that a bare level like `debug` applies to. Other crates log
/// only when named, like `hyper=debug`.
const CRATES: &[&str] = &["transitous", "transitous_core", "motis"];

/// The filter of the log: `info` for `-v`, `debug` for `-vv` and more, else
/// the value of [`ENV_LOG`], else none.
pub(crate) fn filter(verbose: u8, variable: Option<&str>) -> anyhow::Result<Option<EnvFilter>> {
    let spec = match (verbose, variable.map(str::trim)) {
        (0, None | Some("")) => return Ok(None),
        (0, Some(spec)) => spec,
        (1, _) => "info",
        _ => "debug",
    };
    let parts: Vec<&str> = spec
        .split(',')
        .map(str::trim)
        .filter(|d| !d.is_empty())
        .collect();
    // A crate that is named keeps its own level.
    let named = |c: &&str| parts.iter().any(|d| d.split(['=', '[']).next() == Some(*c));
    let directives: Vec<String> = parts
        .iter()
        .flat_map(|directive| match directive.parse::<LevelFilter>() {
            Ok(_) => CRATES
                .iter()
                .filter(|c| !named(c))
                .map(|c| format!("{c}={directive}"))
                .collect(),
            Err(_) => vec![(*directive).to_owned()],
        })
        .collect();
    let filter = EnvFilter::builder()
        .with_default_directive(LevelFilter::OFF.into())
        .parse(directives.join(","))
        .with_context(|| format!("invalid {ENV_LOG} {spec:?}"))?;
    Ok(Some(filter))
}

/// Starts the log, if `-v` or [`ENV_LOG`] asks for it.
pub(crate) fn init(verbose: u8) -> anyhow::Result<()> {
    let variable = std::env::var(ENV_LOG).ok();
    let Some(filter) = filter(verbose, variable.as_deref())? else {
        return Ok(());
    };
    let file = std::env::var_os(ENV_LOG_FILE)
        .map(|path| open(Path::new(&path)))
        .transpose()?;
    let stderr = fmt::layer().with_writer(io::stderr).with_ansi(false);
    let file = file.map(|file| fmt::layer().with_writer(Mutex::new(file)).with_ansi(false));
    tracing_subscriber::registry()
        .with(stderr.and_then(file).with_filter(filter))
        .try_init()
        .context("cannot start the log")
}

/// Opens the log file for appending, readable only by the user, also if it
/// existed with other permissions.
fn open(path: &Path) -> anyhow::Result<File> {
    let mut options = OpenOptions::new();
    options.create(true).append(true);
    #[cfg(unix)]
    std::os::unix::fs::OpenOptionsExt::mode(&mut options, 0o600);
    let open = || -> io::Result<File> {
        let file = options.open(path)?;
        #[cfg(unix)]
        {
            use std::os::unix::fs::PermissionsExt as _;
            file.set_permissions(std::fs::Permissions::from_mode(0o600))?;
        }
        Ok(file)
    };
    open().with_context(|| format!("cannot open the log file {}", path.display()))
}

#[cfg(test)]
mod tests {
    use super::*;

    fn shown(verbose: u8, variable: Option<&str>) -> Option<String> {
        filter(verbose, variable).unwrap().map(|f| f.to_string())
    }

    #[test]
    fn chooses_the_level() {
        assert_eq!(shown(0, None), None);
        assert_eq!(shown(0, Some(" ")), None);
        let info = shown(1, Some("trace")).unwrap();
        assert!(info.contains("motis=info") && info.contains("transitous_core=info"));
        assert!(!info.contains("trace"));
        assert!(shown(2, None).unwrap().contains("transitous=debug"));
        assert!(shown(3, None).unwrap().contains("transitous=debug"));
        let named = shown(0, Some("motis=trace, debug")).unwrap();
        assert!(named.contains("motis=trace") && named.contains("transitous_core=debug"));
        assert!(!named.contains("motis=debug"), "{named}");
        let error = filter(0, Some("motis=loud")).unwrap_err();
        assert_eq!(error.to_string(), r#"invalid TRANSITOUS_LOG "motis=loud""#);
    }

    #[test]
    fn opens_the_file_for_the_user() {
        let dir = tempfile::tempdir().unwrap();
        let path = dir.path().join("transitous.log");
        open(&path).unwrap();
        #[cfg(unix)]
        {
            use std::os::unix::fs::PermissionsExt as _;
            let mode = || std::fs::metadata(&path).unwrap().permissions().mode() & 0o777;
            assert_eq!(mode(), 0o600);
            // An existing file that others can read is restricted.
            std::fs::set_permissions(&path, std::fs::Permissions::from_mode(0o644)).unwrap();
            open(&path).unwrap();
            assert_eq!(mode(), 0o600);
        }
        let error = open(&dir.path().join("missing/transitous.log")).unwrap_err();
        assert!(error.to_string().starts_with("cannot open the log file "));
    }
}
