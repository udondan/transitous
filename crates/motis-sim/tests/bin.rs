//! Runs the `motis-sim` binary.

use std::io::{BufRead, BufReader, Read, Write};
use std::net::TcpStream;
use std::process::{Child, Command, ExitStatus, Stdio};

fn sim() -> Command {
    Command::new(env!("CARGO_BIN_EXE_motis-sim"))
}

/// Interrupts the sim as Ctrl-C does and waits for it to end.
#[cfg(unix)]
fn interrupt(mut child: Child) -> ExitStatus {
    let sent = Command::new("kill")
        .args(["-INT", &child.id().to_string()])
        .status()
        .unwrap();
    assert!(sent.success());
    child.wait().unwrap()
}

#[cfg(unix)]
#[test]
fn serves_the_fixtures_until_interrupted() {
    let mut child = sim().stdout(Stdio::piped()).spawn().unwrap();
    let mut line = String::new();
    BufReader::new(child.stdout.take().unwrap())
        .read_line(&mut line)
        .unwrap();
    let url = line
        .trim()
        .strip_prefix("listening on ")
        .unwrap()
        .to_owned();
    let address = url.strip_prefix("http://").unwrap();

    let mut stream = TcpStream::connect(address).unwrap();
    write!(
        stream,
        "GET /api/v1/geocode?text=Hauptwache HTTP/1.1\r\nHost: {address}\r\nConnection: close\r\n\r\n"
    )
    .unwrap();
    let mut response = String::new();
    stream.read_to_string(&mut response).unwrap();
    assert!(interrupt(child).success());
    assert!(response.starts_with("HTTP/1.1 200"), "{response}");
    assert!(response.contains("Hauptwache"), "{response}");
}

#[cfg(unix)]
#[test]
fn listens_on_the_given_port() {
    let free = std::net::TcpListener::bind("127.0.0.1:0").unwrap();
    let port = free.local_addr().unwrap().port();
    drop(free);
    let mut child = sim()
        .args(["--port", &port.to_string()])
        .stdout(Stdio::piped())
        .spawn()
        .unwrap();
    let mut line = String::new();
    BufReader::new(child.stdout.take().unwrap())
        .read_line(&mut line)
        .unwrap();
    assert!(interrupt(child).success());
    assert_eq!(line.trim(), format!("listening on http://127.0.0.1:{port}"));
}

#[test]
fn fails_on_a_port_in_use() {
    let taken = std::net::TcpListener::bind("127.0.0.1:0").unwrap();
    let port = taken.local_addr().unwrap().port().to_string();
    let output = sim().args(["-p", &port]).output().unwrap();
    assert_eq!(output.status.code(), Some(1));
    let stderr = String::from_utf8(output.stderr).unwrap();
    assert!(stderr.contains("cannot listen on port"), "{stderr}");
}

#[test]
fn prints_the_usage() {
    let output = sim().arg("--help").output().unwrap();
    assert!(output.status.success());
    assert!(
        String::from_utf8(output.stdout)
            .unwrap()
            .starts_with("Usage: motis-sim")
    );
}

#[test]
fn rejects_bad_arguments() {
    for args in [&["--port"][..], &["--port", "many"], &["--verbose"]] {
        let output = sim().args(args).output().unwrap();
        assert_eq!(output.status.code(), Some(2), "{args:?}");
        let stderr = String::from_utf8(output.stderr).unwrap();
        assert!(stderr.contains("Usage: motis-sim"), "{args:?}: {stderr}");
    }
}
