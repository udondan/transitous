//! How commands print their results: as text, JSON or YAML.
//!
//! JSON and YAML quote the strings from the API, like stop names and alert
//! texts, so that they cannot change the structure of the document.

use std::io::Write;

use serde::Serialize;
use transitous_core::model::ErrorBody;

/// How to print the result.
#[derive(Debug, Clone, Copy, PartialEq, Eq, clap::ValueEnum)]
pub(crate) enum Format {
    Text,
    Json,
    Yaml,
}

impl Format {
    /// Text for a terminal, else JSON.
    pub(crate) fn for_terminal(terminal: bool) -> Self {
        if terminal { Self::Text } else { Self::Json }
    }
}

/// Prints `value` as JSON or YAML, or as text with `text`. Commands print
/// their results with this, so that every format works for all of them.
pub(crate) fn show<T, W>(
    value: &T,
    format: Format,
    out: &mut W,
    text: impl FnOnce(&T, &mut W) -> anyhow::Result<()>,
) -> anyhow::Result<()>
where
    T: Serialize + ?Sized,
    W: Write,
{
    match format {
        Format::Text => text(value, out),
        Format::Json => json(value, out),
        Format::Yaml => yaml(value, out),
    }
}

pub(crate) fn json(value: &(impl Serialize + ?Sized), out: &mut impl Write) -> anyhow::Result<()> {
    serde_json::to_writer_pretty(&mut *out, value)?;
    writeln!(out)?;
    Ok(())
}

pub(crate) fn yaml(value: &(impl Serialize + ?Sized), out: &mut impl Write) -> anyhow::Result<()> {
    serde_saphyr::to_io_writer(out, &value)?;
    Ok(())
}

/// An error for machines: its message, a stable status like
/// `ambiguous_stop`, the seconds to wait before trying again and the
/// candidates of an ambiguous stop name.
pub(crate) fn failure(error: &anyhow::Error) -> ErrorBody {
    let core = error
        .chain()
        .find_map(|e| e.downcast_ref::<transitous_core::Error>());
    match core {
        Some(core) => ErrorBody {
            error: format!("{error:#}"),
            ..core.body()
        },
        None => ErrorBody {
            error: format!("{error:#}"),
            status: "error".to_owned(),
            retry_after_seconds: None,
            candidates: Vec::new(),
        },
    }
}

#[cfg(test)]
pub(crate) mod tests {
    use serde_json::{Value, json};
    use transitous_core::model::StopMatch;

    use super::*;

    /// What `write` prints.
    pub(crate) fn text(write: impl FnOnce(&mut Vec<u8>) -> anyhow::Result<()>) -> String {
        let mut out = Vec::new();
        write(&mut out).unwrap();
        String::from_utf8(out).unwrap()
    }

    #[test]
    fn prints_text_only_to_terminals() {
        assert_eq!(Format::for_terminal(true), Format::Text);
        assert_eq!(Format::for_terminal(false), Format::Json);
    }

    /// Stop names that would change the structure of a YAML document if
    /// they were not quoted.
    const TRICKY: [&str; 12] = [
        "a: b",
        "line\nbreak",
        "# comment",
        "- item",
        "[list]",
        "{map}",
        "&anchor",
        "*alias",
        "!tag",
        "---",
        "null",
        "no",
    ];

    #[test]
    fn quotes_strings_in_yaml() {
        for name in TRICKY {
            let value = json!({ "stops": [{ "name": name }] });
            let yaml = text(|out| yaml(&value, out));
            let back: Value =
                serde_saphyr::from_str(&yaml).unwrap_or_else(|e| panic!("{e}: {yaml}"));
            assert_eq!(back, value, "{name:?}");
        }
    }

    #[test]
    fn prints_the_formats() {
        let value = json!({ "name": "Hauptwache" });
        let print = |format| {
            text(|out| {
                show(&value, format, out, |v, out| {
                    Ok(writeln!(out, "{}", v["name"].as_str().unwrap())?)
                })
            })
        };
        assert_eq!(print(Format::Text), "Hauptwache\n");
        assert_eq!(print(Format::Json), "{\n  \"name\": \"Hauptwache\"\n}\n");
        assert_eq!(print(Format::Yaml), "name: Hauptwache\n");
    }

    #[test]
    fn describes_errors_for_machines() {
        let error = anyhow::anyhow!("cannot read").context("cannot read the file");
        let body = failure(&error);
        assert_eq!(body.status, "error");
        assert_eq!(body.error, "cannot read the file: cannot read");
        let candidate = StopMatch {
            id: "a:1".to_owned(),
            name: "Hauptwache".to_owned(),
            area: None,
            lat: 50.1,
            lon: 8.7,
            modes: Vec::new(),
        };
        let ambiguous = transitous_core::Error::AmbiguousStop {
            query: "Haupt".to_owned(),
            candidates: vec![candidate.clone()],
        };
        let body = failure(&anyhow::Error::new(ambiguous).context("cannot find the stop"));
        assert_eq!(body.status, "ambiguous_stop");
        assert_eq!(body.candidates, vec![candidate]);
        assert!(
            body.error.starts_with("cannot find the stop: "),
            "{}",
            body.error
        );
        let limited = transitous_core::Error::RateLimited {
            retry_after_seconds: 30,
        };
        assert_eq!(failure(&limited.into()).retry_after_seconds, Some(30));
    }
}
