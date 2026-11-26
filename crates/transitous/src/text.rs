//! The text output for people. Every result ends with the attribution of
//! the data.
//!
//! Texts from the API, like stop names and alerts, go through [`clean`], so
//! that they cannot send control sequences to the terminal.

use std::io::Write;

use transitous_core::model::{
    Alert, Attribution, Connection, Departure, DeparturesResult, ErrorBody, FindStopResult,
    JourneyResult, JourneyStop, Leg, StopEvent, Time, TripResult, UsageSummary,
};

type Result = anyhow::Result<()>;

/// `text` without control characters: line breaks and tabs become spaces,
/// others are left out.
pub(crate) fn clean(text: &str) -> String {
    text.chars()
        .filter_map(|c| match c {
            '\n' | '\r' | '\t' => Some(' '),
            c if c.is_control() => None,
            c => Some(c),
        })
        .collect()
}

/// The clock time of an RFC 3339 time, like `07:42`.
fn clock(time: &str) -> &str {
    time.get(11..16).unwrap_or(time)
}

/// The time by the schedule with the delay, like `07:42 +3`, or the
/// schedule alone without realtime data or delay.
fn time(time: &Time) -> String {
    let scheduled = clock(&time.scheduled);
    match time.delay_minutes {
        Some(delay) if delay != 0 => format!("{scheduled} {delay:+}"),
        _ => scheduled.to_owned(),
    }
}

fn track(stop: &StopEvent) -> String {
    match (&stop.track, &stop.scheduled_track) {
        (Some(track), Some(scheduled)) if track != scheduled => {
            format!(" track {} (was {})", clean(track), clean(scheduled))
        }
        (Some(track), _) => format!(" track {}", clean(track)),
        (None, _) => String::new(),
    }
}

fn footer(attribution: &Attribution, warnings: &[String], out: &mut impl Write) -> Result {
    for warning in warnings {
        writeln!(out, "warning: {}", clean(warning))?;
    }
    writeln!(out, "{}", attribution.text)?;
    Ok(())
}

fn alerts(alerts: &[Alert], indent: &str, out: &mut impl Write) -> Result {
    for alert in alerts {
        writeln!(out, "{indent}! {}", clean(&alert.header))?;
    }
    Ok(())
}

pub(crate) fn stops(result: &FindStopResult, out: &mut impl Write) -> Result {
    if result.stops.is_empty() {
        writeln!(out, "No stop matches {:?}.", clean(&result.query))?;
    }
    for stop in &result.stops {
        let area = stop
            .area
            .as_deref()
            .map(|area| format!(", {}", clean(area)))
            .unwrap_or_default();
        writeln!(out, "{}{area}", clean(&stop.name))?;
        writeln!(out, "  {}", clean(&stop.id))?;
    }
    footer(&result.attribution, &result.warnings, out)
}

fn line(mode: &str, line: Option<&String>, headsign: Option<&String>) -> String {
    let name = line.map_or_else(|| mode.to_lowercase(), |line| clean(line));
    match headsign {
        Some(headsign) => format!("{name} → {}", clean(headsign)),
        None => name,
    }
}

fn departure(departure: &Departure, out: &mut impl Write) -> Result {
    let state = if departure.trip_cancelled {
        "  trip cancelled"
    } else if departure.cancelled {
        "  cancelled"
    } else if departure.realtime {
        ""
    } else {
        "  no realtime"
    };
    let at = time(&departure.stop.time);
    let name = line(
        &departure.mode,
        departure.line.as_ref(),
        departure.headsign.as_ref(),
    );
    let track = track(&departure.stop);
    writeln!(out, "{at:<9} {name}{track}{state}")?;
    alerts(&departure.alerts, "          ", out)
}

pub(crate) fn departures(result: &DeparturesResult, out: &mut impl Write) -> Result {
    let kind = if result.arrivals {
        "Arrivals"
    } else {
        "Departures"
    };
    writeln!(out, "{kind} at {}", clean(&result.stop.name))?;
    if result.departures.is_empty() {
        writeln!(out, "None.")?;
    }
    for item in &result.departures {
        departure(item, out)?;
    }
    footer(&result.attribution, &result.warnings, out)
}

fn leg(leg: &Leg, out: &mut impl Write) -> Result {
    let name = line(&leg.mode, leg.line.as_ref(), leg.headsign.as_ref());
    let mut notes = Vec::new();
    if leg.cancelled {
        notes.push("cancelled");
    } else if leg.from.cancelled || leg.to.cancelled {
        notes.push("stop cancelled");
    }
    if leg.stay_seated {
        notes.push("stay seated");
    }
    if leg.trip_id.is_some() && !leg.realtime {
        notes.push("no realtime");
    }
    let notes = if notes.is_empty() {
        String::new()
    } else {
        format!("  [{}]", notes.join(", "))
    };
    writeln!(out, "   {name}{notes}")?;
    for stop in [&leg.from, &leg.to] {
        let (at, name, track) = (time(&stop.time), clean(&stop.name), track(stop));
        writeln!(out, "     {at:<9} {name}{track}")?;
    }
    alerts(&leg.alerts, "     ", out)
}

fn connection(number: usize, connection: &Connection, out: &mut impl Write) -> Result {
    let transfers = match connection.transfers {
        1 => "1 transfer".to_owned(),
        n => format!("{n} transfers"),
    };
    let departure = time(&connection.departure);
    let arrival = time(&connection.arrival);
    let summary = format!(
        "{} min, {transfers}  {}",
        connection.duration_minutes,
        connection.status.as_str()
    );
    writeln!(out, "{number}. {departure} → {arrival}  {summary}")?;
    for item in &connection.legs {
        leg(item, out)?;
    }
    Ok(())
}

pub(crate) fn trip(result: &TripResult, out: &mut impl Write) -> Result {
    let (from, to) = (clean(&result.from.name), clean(&result.to.name));
    writeln!(out, "{from} → {to}")?;
    if result.connections.is_empty() {
        writeln!(out, "No connections.")?;
    }
    for (index, item) in result.connections.iter().enumerate() {
        connection(index + 1, item, out)?;
    }
    footer(&result.attribution, &result.warnings, out)
}

fn journey_stop(stop: &JourneyStop, out: &mut impl Write) -> Result {
    let arrival = stop.arrival.as_ref().map(time).unwrap_or_default();
    let departure = stop.departure.as_ref().map(time).unwrap_or_default();
    let track = stop
        .track
        .as_deref()
        .map(|track| format!(" track {}", clean(track)))
        .unwrap_or_default();
    let cancelled = if stop.cancelled { "  cancelled" } else { "" };
    let name = clean(&stop.name);
    writeln!(out, "{arrival:<9} {departure:<9} {name}{track}{cancelled}")?;
    Ok(())
}

pub(crate) fn journey(result: &JourneyResult, out: &mut impl Write) -> Result {
    let state = if result.cancelled {
        "  cancelled"
    } else if result.realtime {
        ""
    } else {
        "  no realtime"
    };
    let name = line(&result.mode, result.line.as_ref(), result.headsign.as_ref());
    writeln!(out, "{name}{state}")?;
    alerts(&result.alerts, "", out)?;
    for stop in &result.stops {
        journey_stop(stop, out)?;
    }
    footer(&result.attribution, &result.warnings, out)
}

pub(crate) fn usage(summary: &UsageSummary, out: &mut impl Write) -> Result {
    writeln!(out, "Requests in the last 24 hours:")?;
    for endpoint in &summary.last_24h {
        writeln!(out, "  {:<16} {}", endpoint.endpoint, endpoint.requests)?;
    }
    let (minute, per_minute) = (summary.last_minute, summary.per_minute_limit);
    writeln!(out, "Last minute: {minute} of {per_minute}")?;
    let (left, daily) = (summary.plan_remaining, summary.plan_daily_limit);
    writeln!(out, "Plan requests left today: {left} of {daily}")?;
    writeln!(out, "Counter: {}", summary.path)?;
    Ok(())
}

/// An error for people, with the candidates of an ambiguous stop name.
pub(crate) fn error(body: &ErrorBody, out: &mut impl Write) -> Result {
    writeln!(out, "Error: {}", clean(&body.error))?;
    if !body.candidates.is_empty() {
        writeln!(out, "Candidates:")?;
    }
    for stop in &body.candidates {
        let area = stop
            .area
            .as_deref()
            .map(|area| format!(", {}", clean(area)))
            .unwrap_or_default();
        writeln!(out, "  {}  {}{area}", clean(&stop.id), clean(&stop.name))?;
    }
    Ok(())
}

#[cfg(test)]
mod tests {
    use transitous_core::model::{Location, PageCursor};

    use super::*;
    use crate::output::tests::text;

    #[test]
    fn removes_control_characters() {
        assert_eq!(clean("a\nb\tc\u{1b}[31md\u{7}"), "a b c[31md");
    }

    #[test]
    fn shows_times_with_delays() {
        let at = |delay| Time {
            scheduled: "2026-11-27T07:42:00+01:00".to_owned(),
            actual: "2026-11-27T07:45:00+01:00".to_owned(),
            delay_minutes: delay,
        };
        assert_eq!(time(&at(Some(3))), "07:42 +3");
        assert_eq!(time(&at(Some(-1))), "07:42 -1");
        assert_eq!(time(&at(Some(0))), "07:42");
        assert_eq!(time(&at(None)), "07:42");
        assert_eq!(clock("07:42"), "07:42");
    }

    #[test]
    fn shows_tracks() {
        let mut stop = StopEvent {
            id: None,
            name: "Hbf".to_owned(),
            time: Time {
                scheduled: String::new(),
                actual: String::new(),
                delay_minutes: None,
            },
            track: None,
            scheduled_track: Some("4".to_owned()),
            cancelled: false,
        };
        assert_eq!(track(&stop), "");
        stop.track = Some("4".to_owned());
        assert_eq!(track(&stop), " track 4");
        stop.track = Some("5".to_owned());
        assert_eq!(track(&stop), " track 5 (was 4)");
    }

    #[test]
    fn shows_empty_results() {
        let stops = FindStopResult {
            query: "Nowhere".to_owned(),
            stops: Vec::new(),
            attribution: Attribution::default(),
            warnings: vec!["careful".to_owned()],
        };
        let shown = text(|out| self::stops(&stops, out));
        assert!(shown.starts_with("No stop matches \"Nowhere\".\nwarning: careful\nData: "));
        let body = ErrorBody {
            error: "boom".to_owned(),
            status: "error".to_owned(),
            retry_after_seconds: None,
            candidates: Vec::new(),
        };
        assert_eq!(text(|out| error(&body, out)), "Error: boom\n");
        let departures = DeparturesResult {
            stop: Location {
                id: None,
                name: "Hbf".to_owned(),
            },
            arrivals: true,
            departures: Vec::new(),
            page_cursor: PageCursor::default(),
            attribution: Attribution::default(),
            warnings: Vec::new(),
        };
        let shown = text(|out| self::departures(&departures, out));
        assert!(shown.starts_with("Arrivals at Hbf\nNone.\n"), "{shown}");
        let trip = TripResult {
            from: departures.stop.clone(),
            to: departures.stop,
            connections: Vec::new(),
            page_cursor: PageCursor::default(),
            attribution: Attribution::default(),
            warnings: Vec::new(),
        };
        let shown = text(|out| self::trip(&trip, out));
        assert!(shown.starts_with("Hbf → Hbf\nNo connections.\n"), "{shown}");
    }

    fn at(clock: &str, delay: Option<i64>) -> Time {
        Time {
            scheduled: format!("2026-11-27T{clock}:00+01:00"),
            actual: format!("2026-11-27T{clock}:00+01:00"),
            delay_minutes: delay,
        }
    }

    fn event(name: &str, clock: &str, cancelled: bool) -> StopEvent {
        StopEvent {
            id: Some(format!("de:{name}")),
            name: name.to_owned(),
            time: at(clock, None),
            track: None,
            scheduled_track: None,
            cancelled,
        }
    }

    fn alert() -> Alert {
        Alert {
            header: "Works\non the line".to_owned(),
            description: String::new(),
            effect: None,
            severity: None,
            url: None,
            periods: Vec::new(),
        }
    }

    fn departure_with(realtime: bool, cancelled: bool, trip_cancelled: bool) -> Departure {
        Departure {
            mode: "SUBWAY".to_owned(),
            line: Some("U4".to_owned()),
            operator: None,
            headsign: Some("Enkheim".to_owned()),
            trip_id: "t".to_owned(),
            stop: event("Hbf", "07:42", cancelled),
            realtime,
            cancelled,
            trip_cancelled,
            alerts: vec![alert()],
        }
    }

    #[test]
    fn shows_the_state_of_departures() {
        let result = DeparturesResult {
            stop: Location {
                id: None,
                name: "Hbf".to_owned(),
            },
            arrivals: false,
            departures: vec![
                departure_with(true, false, false),
                departure_with(true, true, false),
                departure_with(true, true, true),
                departure_with(false, false, false),
            ],
            page_cursor: PageCursor::default(),
            attribution: Attribution::default(),
            warnings: Vec::new(),
        };
        let shown = text(|out| departures(&result, out));
        let lines: Vec<&str> = shown.lines().collect();
        assert_eq!(lines[0], "Departures at Hbf");
        assert_eq!(lines[1], "07:42     U4 → Enkheim");
        assert_eq!(lines[2], "          ! Works on the line");
        assert_eq!(lines[3], "07:42     U4 → Enkheim  cancelled");
        assert_eq!(lines[5], "07:42     U4 → Enkheim  trip cancelled");
        assert_eq!(lines[7], "07:42     U4 → Enkheim  no realtime");
    }

    fn leg_with(cancelled: bool, stop_cancelled: bool, realtime: bool) -> Leg {
        Leg {
            mode: "BUS".to_owned(),
            line: None,
            operator: None,
            headsign: None,
            trip_id: Some("t".to_owned()),
            from: event("A", "07:00", stop_cancelled),
            to: event("B", "07:10", false),
            duration_minutes: 10,
            realtime,
            cancelled,
            stay_seated: true,
            alerts: vec![alert()],
        }
    }

    #[test]
    fn shows_connections_with_their_legs() {
        let connection = |transfers| Connection {
            status: transitous_core::Status::Cancelled,
            reasons: Vec::new(),
            realtime: transitous_core::status::Realtime::Partial,
            departure: at("07:00", Some(2)),
            arrival: at("07:10", None),
            duration_minutes: 10,
            transfers,
            max_delay_minutes: None,
            legs: vec![
                leg_with(true, false, true),
                leg_with(false, true, false),
                leg_with(false, false, true),
            ],
        };
        let result = TripResult {
            from: Location {
                id: None,
                name: "A".to_owned(),
            },
            to: Location {
                id: None,
                name: "B".to_owned(),
            },
            connections: vec![connection(1), connection(2)],
            page_cursor: PageCursor::default(),
            attribution: Attribution::default(),
            warnings: Vec::new(),
        };
        let shown = text(|out| trip(&result, out));
        assert!(
            shown.contains("1. 07:00 +2 → 07:10  10 min, 1 transfer  cancelled\n"),
            "{shown}"
        );
        assert!(shown.contains("2. 07:00 +2 → 07:10  10 min, 2 transfers"));
        assert!(
            shown.contains("   bus  [cancelled, stay seated]\n"),
            "{shown}"
        );
        assert!(
            shown.contains("   bus  [stop cancelled, stay seated, no realtime]\n"),
            "{shown}"
        );
        assert!(shown.contains("     07:00     A\n     07:10     B\n     ! Works"));
    }

    #[test]
    fn shows_journeys() {
        let stop = |cancelled| JourneyStop {
            id: None,
            name: "A".to_owned(),
            arrival: None,
            departure: Some(at("07:00", Some(1))),
            track: Some("2".to_owned()),
            scheduled_track: None,
            cancelled,
        };
        let journey_with = |realtime, cancelled| JourneyResult {
            trip_id: "t".to_owned(),
            mode: "BUS".to_owned(),
            line: Some("36".to_owned()),
            operator: None,
            headsign: None,
            realtime,
            cancelled,
            stops: vec![stop(cancelled)],
            alerts: vec![alert()],
            attribution: Attribution::default(),
            warnings: Vec::new(),
        };
        let shown = text(|out| journey(&journey_with(true, false), out));
        assert!(
            shown.starts_with("36\n! Works on the line\n          07:00 +1  A track 2\n"),
            "{shown}"
        );
        let shown = text(|out| journey(&journey_with(true, true), out));
        assert!(shown.starts_with("36  cancelled\n"), "{shown}");
        assert!(shown.contains("A track 2  cancelled\n"), "{shown}");
        let shown = text(|out| journey(&journey_with(false, false), out));
        assert!(shown.starts_with("36  no realtime\n"), "{shown}");
    }

    #[test]
    fn shows_the_usage() {
        let summary = UsageSummary {
            last_24h: vec![transitous_core::model::EndpointUsage {
                endpoint: "plan".to_owned(),
                requests: 3,
            }],
            last_minute: 1,
            per_minute_limit: 20,
            plan_daily_limit: 720,
            plan_remaining: 717,
            path: "/state/usage.json".to_owned(),
        };
        assert_eq!(
            text(|out| usage(&summary, out)),
            "Requests in the last 24 hours:\n  plan             3\nLast minute: 1 of 20\n\
             Plan requests left today: 717 of 720\nCounter: /state/usage.json\n"
        );
    }
}
