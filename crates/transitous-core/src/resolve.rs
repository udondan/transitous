//! From what the caller typed to a stop ID: an ID is used as it is, a name
//! only if it names one stop. A wrong stop gives wrong connections that look
//! right, so a doubtful name is an error with the candidates, never a guess.
//!
//! The scores of the geocoder do not tell a good match from a bad one, so
//! only the names count: a match whose name is the query (ignoring case,
//! punctuation and spacing), else the only match that contains every word
//! of the query. Matches with the same name in the same area, like the
//! platforms of a station, count as one.

use motis::{LocationType, Match};

use crate::Error;
use crate::model::{StopMatch, area};

/// The most candidates an [`Error::AmbiguousStop`] lists.
pub const MAX_CANDIDATES: usize = 10;

/// Whether the input is a stop ID, like `de-DELFI_de:06412:1_G`, rather
/// than a name.
pub fn is_stop_id(input: &str) -> bool {
    !input.is_empty() && !input.chars().any(char::is_whitespace) && input.contains(['_', ':'])
}

/// Coordinates `LAT,LON`, which the routing accepts as a place.
pub fn coordinates(input: &str) -> Option<(f64, f64)> {
    let (lat, lon) = input.split_once(',')?;
    let lat: f64 = lat.trim().parse().ok()?;
    let lon: f64 = lon.trim().parse().ok()?;
    ((-90.0..=90.0).contains(&lat) && (-180.0..=180.0).contains(&lon)).then_some((lat, lon))
}

/// The stop `query` names among the geocoding `matches`.
pub fn choose<'a>(query: &str, matches: &'a [Match]) -> Result<&'a Match, Error> {
    let stops: Vec<&Match> = matches
        .iter()
        .filter(|found| found.kind == LocationType::Stop)
        .collect();
    if stops.is_empty() {
        return Err(Error::StopNotFound {
            query: query.to_owned(),
        });
    }
    let wanted = normalize(query);
    let exact: Vec<&Match> = stops
        .iter()
        .copied()
        .filter(|found| normalize(&found.name) == wanted)
        .collect();
    if let Some(found) = single(&exact) {
        return Ok(found);
    }
    if exact.is_empty() {
        let words: Vec<&str> = wanted.split(' ').collect();
        let containing: Vec<&Match> = stops
            .iter()
            .copied()
            .filter(|found| {
                let name = normalize(&found.name);
                let name: Vec<&str> = name.split(' ').collect();
                words.iter().all(|word| name.contains(word))
            })
            .collect();
        if let Some(found) = single(&containing) {
            return Ok(found);
        }
    }
    let candidates = if exact.is_empty() { stops } else { exact };
    Err(Error::AmbiguousStop {
        query: query.to_owned(),
        candidates: candidates
            .into_iter()
            .take(MAX_CANDIDATES)
            .map(StopMatch::from)
            .collect(),
    })
}

/// The first match, if all have the same name and area.
fn single<'a>(matches: &[&'a Match]) -> Option<&'a Match> {
    let first = *matches.first()?;
    let key = |found: &Match| (normalize(&found.name), area(found));
    let same = matches.iter().all(|found| key(found) == key(first));
    same.then_some(first)
}

/// Lowercase letters and digits, words separated by one space.
fn normalize(text: &str) -> String {
    text.chars()
        .map(|c| {
            if c.is_alphanumeric() {
                c.to_lowercase().next().unwrap_or(c)
            } else {
                ' '
            }
        })
        .collect::<String>()
        .split_whitespace()
        .collect::<Vec<_>>()
        .join(" ")
}

#[cfg(test)]
mod tests {
    use super::*;
    use motis::Area;

    fn stop(id: &str, name: &str, area: &str) -> Match {
        Match {
            kind: LocationType::Stop,
            id: id.into(),
            name: name.into(),
            areas: vec![Area {
                name: area.into(),
                default: Some(true),
                ..Area::default()
            }],
            ..Match::default()
        }
    }

    fn chosen(query: &str, matches: &[Match]) -> String {
        choose(query, matches).unwrap().id.clone()
    }

    fn candidates(query: &str, matches: &[Match]) -> Vec<String> {
        let error = choose(query, matches).unwrap_err();
        assert_eq!(error.status(), "ambiguous_stop");
        error.body().candidates.into_iter().map(|c| c.id).collect()
    }

    #[test]
    fn tells_ids_from_names() {
        assert!(is_stop_id("de-DELFI_de:06412:1_G"));
        assert!(is_stop_id("de:06412:1"));
        assert!(!is_stop_id("Hauptwache"));
        assert!(!is_stop_id("Frankfurt (Main) Hbf"));
        assert!(!is_stop_id("a_b c"));
        assert!(!is_stop_id(""));
    }

    #[test]
    fn reads_coordinates() {
        assert_eq!(coordinates("50.11, 8.68"), Some((50.11, 8.68)));
        assert_eq!(coordinates("-33.9,151.2"), Some((-33.9, 151.2)));
        assert_eq!(coordinates("91,0"), None);
        assert_eq!(coordinates("0,181"), None);
        assert_eq!(coordinates("Hauptwache"), None);
        assert_eq!(coordinates("a,1"), None);
        assert_eq!(coordinates("1,b"), None);
    }

    #[test]
    fn takes_the_exact_name() {
        let matches = [
            stop("1", "Frankfurt (Main) Hauptwache", "Frankfurt"),
            stop("2", "Frankfurt (Main) Freßgass/Hauptwache", "Frankfurt"),
        ];
        assert_eq!(chosen("frankfurt main  HAUPTWACHE", &matches), "1");
    }

    #[test]
    fn platforms_of_a_station_count_as_one() {
        let matches = [
            stop("1", "Hauptwache", "Frankfurt"),
            stop("2", "Hauptwache", "Frankfurt"),
        ];
        assert_eq!(chosen("Hauptwache", &matches), "1");
    }

    #[test]
    fn the_same_name_in_two_towns_is_ambiguous() {
        let matches = [
            stop("1", "Bahnhof", "Neustadt"),
            stop("2", "Bahnhof", "Altstadt"),
            stop("3", "Bahnhofstraße", "Neustadt"),
        ];
        assert_eq!(candidates("Bahnhof", &matches), ["1", "2"]);
    }

    #[test]
    fn takes_the_only_match_with_every_word() {
        let matches = [
            stop("1", "Frankfurt (Main) Hauptbahnhof", "Frankfurt"),
            stop("2", "Frankfurt (Oder) Bahnhof", "Frankfurt (Oder)"),
        ];
        assert_eq!(chosen("Frankfurt Hauptbahnhof", &matches), "1");
    }

    #[test]
    fn several_matches_with_every_word_are_ambiguous() {
        let matches = [
            stop("1", "Frankfurt (Main) Hauptwache", "Frankfurt"),
            stop("2", "Frankfurt (Main) Freßgass/Hauptwache", "Frankfurt"),
        ];
        assert_eq!(candidates("Hauptwache", &matches), ["1", "2"]);
    }

    #[test]
    fn a_single_fuzzy_match_is_not_taken() {
        let matches = [stop("1", "Hauptbahnhof", "Frankfurt")];
        assert_eq!(candidates("Hauptbanhof", &matches), ["1"]);
    }

    #[test]
    fn lists_at_most_ten_candidates() {
        let matches: Vec<Match> = (0..15)
            .map(|i| stop(&i.to_string(), "Bahnhof", &format!("Town {i}")))
            .collect();
        assert_eq!(candidates("Bahnhof", &matches).len(), MAX_CANDIDATES);
    }

    #[test]
    fn only_stops_count() {
        let address = Match {
            kind: LocationType::Address,
            ..stop("x", "Hauptwache", "Frankfurt")
        };
        assert!(matches!(
            choose("Hauptwache", &[address]),
            Err(Error::StopNotFound { .. })
        ));
        assert!(matches!(
            choose("Hauptwache", &[]),
            Err(Error::StopNotFound { .. })
        ));
    }
}
