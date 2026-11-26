//! `transitous`: CLI and MCP server for public transport connections and
//! departures on the Transitous API, with realtime status.
//!
//! Transitous (<https://transitous.org>) is run by volunteers. Every request
//! names a contact in the User-Agent, so nothing is sent until one is
//! configured.

mod completion;
mod logging;
mod mcp;
mod output;
mod settings;
mod text;
mod time;

use std::io::{self, IsTerminal as _, Write as _};
use std::net::SocketAddr;
use std::process::ExitCode;
use std::sync::Arc;

use clap::{ArgAction, Args, CommandFactory as _, Parser, Subcommand};
use jiff::Timestamp;
use motis::Mode;
use transitous_core::config::Settings;
use transitous_core::{Config, DeparturesQuery, StopsQuery, Transitous, TripQuery};

use crate::output::Format;

/// The product in the User-Agent.
const PRODUCT: &str = concat!("transitous/", env!("CARGO_PKG_VERSION"));

/// Sets the current time, as RFC 3339, for the status of connections and for
/// times like `07:50`. For tests, and to see what the status would be at
/// another time.
const ENV_NOW: &str = "TRANSITOUS_NOW";

/// Public transport connections and departures with realtime status, from
/// Transitous.
///
/// Transitous (<https://transitous.org>) is run by volunteers. Every request
/// names a contact in the User-Agent: set it once with `transitous config
/// set contact <url or email>`, or with `TRANSITOUS_CONTACT`. Stops are given
/// by ID, or by a name that fits exactly one stop.
#[derive(Debug, Parser)]
#[command(name = "transitous", version, about)]
struct Cli {
    #[command(flatten)]
    global: Global,
    #[command(subcommand)]
    command: Command,
}

#[derive(Debug, Args)]
struct Global {
    /// How to print results: text, JSON or YAML.
    ///
    /// JSON is the default when the output is not a terminal. Errors are
    /// printed in the same format on stderr.
    #[arg(short, long, global = true, value_enum, env = "TRANSITOUS_OUTPUT")]
    output: Option<Format>,
    /// Log the requests to stderr: -v, -vv.
    ///
    /// `TRANSITOUS_LOG` sets the level instead, like `debug`, also per crate
    /// like `motis=debug`, and `TRANSITOUS_LOG_FILE` writes the log to a
    /// file too.
    #[arg(short, long, global = true, action = ArgAction::Count)]
    verbose: u8,
    /// Ask Transitous instead of the cache, and do not store the answers.
    /// Also `TRANSITOUS_NO_CACHE=1`.
    #[arg(long, global = true)]
    no_cache: bool,
}

#[derive(Debug, Subcommand)]
enum Command {
    /// Find stops by name, with their IDs.
    Stops {
        /// The name, or a part of it.
        name: String,
        /// Prefer stops near this place, like `50.11,8.68`.
        #[arg(long, value_name = "LAT,LON", value_parser = coordinates)]
        near: Option<(f64, f64)>,
        /// The most stops to show, up to 20.
        #[arg(long, default_value_t = 10)]
        count: u32,
    },
    /// The next departures or arrivals at a stop.
    Departures {
        /// The stop: its ID, or a name that fits one stop.
        stop: String,
        /// From this time: HH:MM today, a local time like 2026-11-27T07:50,
        /// or RFC 3339. Defaults to now.
        #[arg(long, value_parser = time::arg)]
        time: Option<Timestamp>,
        /// The most departures to show, up to 100.
        #[arg(long, default_value_t = 10)]
        count: u32,
        /// Arrivals instead of departures.
        #[arg(long)]
        arrivals: bool,
        /// Only these modes, like `subway,bus`. Separate them with commas.
        #[arg(long, value_delimiter = ',', value_parser = mode)]
        modes: Vec<Mode>,
        /// The page before or after, from `page_cursor` of a result.
        #[arg(long)]
        page_cursor: Option<String>,
    },
    /// Connections between two stops or places, each with its status.
    ///
    /// The status is one of `ok`, `delayed`, `cancelled`, `unreachable` (a transfer
    /// is missed or the first ride has left) and `no_realtime`. With
    /// `--arrive`, the connections arriving last before the time come with
    /// the earlier ones, to switch to.
    Trip {
        /// Where from: a stop ID, a stop name that fits one stop, or
        /// `LAT,LON`.
        from: String,
        /// Where to, like `from`.
        to: String,
        /// Arrive by this time: HH:MM today, a local time or RFC 3339.
        #[arg(long, value_parser = time::arg, conflicts_with = "depart")]
        arrive: Option<Timestamp>,
        /// Depart at this time, like `--arrive`. Defaults to now.
        #[arg(long, value_parser = time::arg)]
        depart: Option<Timestamp>,
        /// The number of connections to ask for, up to 20.
        #[arg(long, default_value_t = 5)]
        count: u32,
        /// Only these transit modes, like `rail,subway`.
        #[arg(long, value_delimiter = ',', value_parser = mode)]
        modes: Vec<Mode>,
        /// The most transfers.
        #[arg(long)]
        max_transfers: Option<u32>,
        /// The page before or after, from `page_cursor` of a result.
        #[arg(long)]
        page_cursor: Option<String>,
        /// Route on the realtime timetable. Cancelled rides and missed
        /// transfers then drop out instead of being reported.
        #[arg(long)]
        realtime_routing: bool,
    },
    /// A trip with all its stops, by its ID from `departures` or `trip`.
    Journey {
        /// The trip ID.
        trip_id: String,
    },
    /// Show and change the configuration file.
    #[command(subcommand)]
    Config(ConfigCommand),
    /// The requests of the last 24 hours and the limits.
    Usage,
    /// Serve the tools as an MCP server, on stdin and stdout.
    ///
    /// The tools are `transitous_find_stop`, `transitous_departures`,
    /// `transitous_trip` and `transitous_journey`; they only read. Without a
    /// contact, the server starts and every tool returns `contact_missing`.
    Mcp {
        /// Serve streamable HTTP at `/mcp` on this address instead, by default
        /// 127.0.0.1:8080. Needs a bearer token of at least 16 characters in
        /// `TRANSITOUS_MCP_TOKEN`.
        #[arg(long, value_name = "ADDR", num_args = 0..=1, default_missing_value = "127.0.0.1:8080")]
        http: Option<SocketAddr>,
    },
    /// Print the completion script for a shell.
    ///
    /// The script calls `transitous` from the `PATH` on every completion,
    /// which never contacts Transitous. Load the script from transitous when
    /// the shell starts, as shown below, so that it follows updates.
    #[command(after_help = completion::INSTALL)]
    Completion {
        /// The shell, by name or by path like `$SHELL`.
        #[arg(value_parser = completion::ShellParser, env = "SHELL", hide_env_values = true, allow_hyphen_values = true)]
        shell: completion::Shell,
    },
}

#[derive(Debug, Subcommand)]
enum ConfigCommand {
    /// The settings in effect, with the environment variables applied.
    Show,
    /// The path of the configuration file.
    Path,
    /// One value of the configuration file. Prints nothing if it is unset.
    Get {
        #[arg(value_parser = clap::builder::PossibleValuesParser::new(Settings::KEYS))]
        key: String,
    },
    /// Set a value in the configuration file.
    Set {
        #[arg(value_parser = clap::builder::PossibleValuesParser::new(Settings::KEYS))]
        key: String,
        value: String,
    },
    /// Remove a value from the configuration file, so that its default
    /// applies.
    Unset {
        #[arg(value_parser = clap::builder::PossibleValuesParser::new(Settings::KEYS))]
        key: String,
    },
}

/// Parses a transit mode, like `subway`.
fn mode(input: &str) -> Result<Mode, String> {
    let name = input.trim().to_ascii_uppercase();
    if Mode::KNOWN.contains(&name.as_str()) {
        Ok(Mode::from(name))
    } else {
        Err(format!(
            "unknown mode, known are: {}",
            Mode::KNOWN.join(", ").to_lowercase()
        ))
    }
}

/// Parses `LAT,LON`.
fn coordinates(input: &str) -> Result<(f64, f64), String> {
    transitous_core::resolve::coordinates(input)
        .ok_or_else(|| format!("{input:?} is not LAT,LON like 50.11,8.68"))
}

/// The environment variables.
fn env(name: &str) -> Option<String> {
    std::env::var(name).ok()
}

#[tokio::main(flavor = "current_thread")]
async fn main() -> ExitCode {
    completion::complete(Cli::command);
    let cli = Cli::parse();
    let format = cli
        .global
        .output
        .unwrap_or_else(|| Format::for_terminal(io::stdout().is_terminal()));
    let result = match logging::init(cli.global.verbose) {
        Ok(()) => run(cli.command, &cli.global, format).await,
        Err(error) => Err(error),
    };
    match result {
        Ok(()) => ExitCode::SUCCESS,
        Err(error) => {
            let _ = io::stdout().flush();
            let body = output::failure(&error);
            let stderr = &mut io::stderr().lock();
            let _ = match format {
                Format::Text => text::error(&body, stderr),
                Format::Json => output::json(&body, stderr),
                Format::Yaml => output::yaml(&body, stderr),
            };
            ExitCode::FAILURE
        }
    }
}

/// The configuration, with `--no-cache` applied.
fn config(global: &Global) -> anyhow::Result<Config> {
    let mut config = Config::load(&env)?;
    if global.no_cache {
        config.cache_dir = None;
    }
    Ok(config)
}

/// The time from `TRANSITOUS_NOW`, if it is set.
fn fixed_now() -> anyhow::Result<Option<Timestamp>> {
    env(ENV_NOW)
        .map(|now| {
            now.parse()
                .map_err(|error| anyhow::anyhow!("invalid {ENV_NOW} {now:?}: {error}"))
        })
        .transpose()
}

/// The client. Fails without a contact, before any request.
fn service(global: &Global) -> anyhow::Result<Transitous> {
    let service = Transitous::new(&config(global)?, PRODUCT)?;
    Ok(match fixed_now()? {
        Some(now) => service.with_clock(Arc::new(move || now)),
        None => service,
    })
}

async fn run(command: Command, global: &Global, format: Format) -> anyhow::Result<()> {
    // Not locked: the MCP server writes to stdout itself.
    let out = &mut io::stdout();
    match command {
        Command::Stops { name, near, count } => {
            let query = StopsQuery {
                text: name,
                near,
                count,
            };
            let result = service(global)?.find_stops(&query).await?;
            output::show(&result, format, out, text::stops)
        }
        Command::Departures {
            stop,
            time,
            count,
            arrivals,
            modes,
            page_cursor,
        } => {
            let query = DeparturesQuery {
                stop,
                time,
                count,
                arrivals,
                modes,
                page_cursor,
            };
            let result = service(global)?.departures(&query).await?;
            output::show(&result, format, out, text::departures)
        }
        Command::Trip {
            from,
            to,
            arrive,
            depart,
            count,
            modes,
            max_transfers,
            page_cursor,
            realtime_routing,
        } => {
            let query = TripQuery {
                from,
                to,
                time: arrive.or(depart),
                arrive_by: arrive.is_some(),
                count,
                modes,
                max_transfers,
                page_cursor,
                realtime_routing,
            };
            let result = service(global)?.trip(&query).await?;
            output::show(&result, format, out, text::trip)
        }
        Command::Journey { trip_id } => {
            let result = service(global)?.journey(&trip_id).await?;
            output::show(&result, format, out, text::journey)
        }
        Command::Config(command) => settings::run(command, &config(global)?, format, out),
        Command::Usage => {
            let config = config(global)?;
            let usage = transitous_core::usage::Usage::new(
                config.state_dir.as_deref(),
                config.plan_daily_limit,
            );
            let summary = usage.summary(fixed_now()?.unwrap_or_else(Timestamp::now))?;
            output::show(&summary, format, out, text::usage)
        }
        Command::Completion { shell } => Ok(completion::script(shell, out)?),
        Command::Mcp { http } => {
            let service = service(global).map_err(|error| output::failure(&error));
            match http {
                Some(address) => mcp::http(service, address, env(mcp::ENV_TOKEN)).await,
                None => mcp::stdio(service).await,
            }
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn parses_modes_and_places() {
        assert_eq!(mode(" subway").unwrap(), Mode::Subway);
        assert_eq!(mode("REGIONAL_RAIL").unwrap(), Mode::RegionalRail);
        assert!(mode("rocket").unwrap_err().contains("subway"));
        assert_eq!(coordinates("50.11,8.68").unwrap(), (50.11, 8.68));
        assert!(coordinates("Hauptwache").is_err());
    }

    #[test]
    fn checks_the_commands() {
        Cli::command().debug_assert();
    }
}
