//! Serves the fixtures on 127.0.0.1 until interrupted (Ctrl-C, SIGINT), for
//! tests in any language.

use std::process::ExitCode;

use motis_sim::Sim;

const USAGE: &str = "Usage: motis-sim [--port PORT]

Serves recorded MOTIS 2 responses on 127.0.0.1 until interrupted.
--port is the port to listen on; without it, a free one is chosen.";

fn main() -> ExitCode {
    let port = match port(std::env::args().skip(1)) {
        Ok(Some(port)) => port,
        Ok(None) => {
            println!("{USAGE}");
            return ExitCode::SUCCESS;
        }
        Err(message) => {
            eprintln!("motis-sim: {message}\n\n{USAGE}");
            return ExitCode::from(2);
        }
    };
    let runtime = tokio::runtime::Builder::new_current_thread()
        .enable_all()
        .build()
        .expect("a Tokio runtime");
    runtime.block_on(async {
        match Sim::start_on(port).await {
            Ok(sim) => {
                // Handle Ctrl-C before announcing the sim, so an interrupt
                // right after the announcement ends it cleanly too.
                let interrupted = tokio::spawn(tokio::signal::ctrl_c());
                tokio::task::yield_now().await;
                println!("listening on {}", sim.base_url());
                interrupted
                    .await
                    .expect("the signal task runs")
                    .expect("a handler for Ctrl-C");
                ExitCode::SUCCESS
            }
            Err(error) => {
                eprintln!("motis-sim: cannot listen on port {port}: {error}");
                ExitCode::FAILURE
            }
        }
    })
}

/// The port from the arguments, 0 without `--port`, or `None` for `--help`.
fn port(mut args: impl Iterator<Item = String>) -> Result<Option<u16>, String> {
    let mut port = 0;
    while let Some(arg) = args.next() {
        match arg.as_str() {
            "-h" | "--help" => return Ok(None),
            "--port" | "-p" => {
                let value = args.next().ok_or("--port needs a value")?;
                port = value
                    .parse()
                    .map_err(|_| format!("invalid port: {value}"))?;
            }
            other => return Err(format!("unknown argument: {other}")),
        }
    }
    Ok(Some(port))
}
