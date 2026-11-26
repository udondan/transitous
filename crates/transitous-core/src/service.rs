//! The operations of the CLI and the MCP server, with the usage rules
//! applied: a contact in every request, the cache first, then the usage
//! counter, then one request, never a retry.

use std::fmt;
use std::sync::Arc;

use jiff::Timestamp;
use motis::{Client, Geocode, LocationType, Mode, Plan, RealtimeMode, Request, Stoptimes, Trip};

use crate::cache::{self, Cache};
use crate::config::Config;
use crate::model::{
    Attribution, Connection, Departure, DeparturesResult, FindStopResult, JourneyResult, Location,
    PageCursor, StopMatch, TripResult, UsageSummary,
};
use crate::usage::{Endpoint, Usage};
use crate::{Error, agent, resolve, status};

/// The most connections of one search.
pub const MAX_CONNECTIONS: u32 = 20;
/// The most departures of one search.
pub const MAX_DEPARTURES: u32 = 100;
/// The most stops of one search.
pub const MAX_STOPS: u32 = 20;
/// The connections an arrive-by search asks for at least, so the earlier
/// ones are there to switch to.
const MIN_ARRIVE_BY: u32 = 3;
/// The geocoding matches a name is chosen from.
const RESOLVE_MATCHES: u32 = 10;

/// The current time. Replaceable for tests.
pub type Clock = Arc<dyn Fn() -> Timestamp + Send + Sync>;

/// A request type of the API with its endpoint.
pub trait Call: Request {
    const ENDPOINT: Endpoint;
}

impl Call for Geocode {
    const ENDPOINT: Endpoint = Endpoint::Geocode;
}
impl Call for motis::ReverseGeocode {
    const ENDPOINT: Endpoint = Endpoint::ReverseGeocode;
}
impl Call for Plan {
    const ENDPOINT: Endpoint = Endpoint::Plan;
}
impl Call for Stoptimes {
    const ENDPOINT: Endpoint = Endpoint::Stoptimes;
}
impl Call for Trip {
    const ENDPOINT: Endpoint = Endpoint::Trip;
}

/// A search for stops.
#[derive(Debug, Clone, Default, PartialEq)]
pub struct StopsQuery {
    pub text: String,
    /// Prefer stops near `(lat, lon)`.
    pub near: Option<(f64, f64)>,
    /// At most [`MAX_STOPS`].
    pub count: u32,
}

/// A search for departures or arrivals.
#[derive(Debug, Clone, Default, PartialEq, Eq)]
pub struct DeparturesQuery {
    /// A stop ID, or a name that names one stop.
    pub stop: String,
    /// `None`: now.
    pub time: Option<Timestamp>,
    /// How many to ask for, at most [`MAX_DEPARTURES`]. The API can return
    /// more, and the result has them all.
    pub count: u32,
    pub arrivals: bool,
    /// Only these modes. Empty: all.
    pub modes: Vec<Mode>,
    pub page_cursor: Option<String>,
}

/// A search for connections.
#[derive(Debug, Clone, Default, PartialEq, Eq)]
#[allow(clippy::struct_excessive_bools, reason = "independent options")]
pub struct TripQuery {
    /// A stop ID, a name that names one stop, or `LAT,LON`.
    pub from: String,
    pub to: String,
    /// `None`: now.
    pub time: Option<Timestamp>,
    /// Whether `time` is the latest arrival rather than the earliest
    /// departure.
    pub arrive_by: bool,
    /// How many connections to ask for, at most [`MAX_CONNECTIONS`]. The
    /// result has all the API returns, which can be more: the page cursors
    /// continue after the whole page, so nothing is dropped.
    pub count: u32,
    /// Only these transit modes. Empty: all.
    pub modes: Vec<Mode>,
    pub max_transfers: Option<u32>,
    pub page_cursor: Option<String>,
    /// Route on the realtime timetable: cancelled trips and missed transfers
    /// drop out instead of showing with their status.
    pub realtime_routing: bool,
}

/// Transitous with the usage rules.
#[derive(Clone)]
pub struct Transitous {
    client: Client,
    cache: Cache,
    usage: Usage,
    delay_threshold_minutes: u32,
    clock: Clock,
}

impl fmt::Debug for Transitous {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.debug_struct("Transitous")
            .field("client", &self.client)
            .field("cache", &self.cache)
            .field("usage", &self.usage)
            .finish_non_exhaustive()
    }
}

impl Transitous {
    /// A client for `config`, naming `product` (like `transitous/1.2.0`) in
    /// the User-Agent.
    ///
    /// Fails with [`Error::ContactMissing`] without a contact: no request is
    /// possible then.
    pub fn new(config: &Config, product: &str) -> Result<Self, Error> {
        let user_agent = agent::user_agent(product, config.contact.as_deref())?;
        let client = Client::builder(&config.base_url)
            .user_agent(user_agent)
            .timeout(config.timeout)
            .build()?;
        Ok(Self {
            client,
            cache: Cache::new(config.cache_dir.clone()),
            usage: Usage::new(config.state_dir.as_deref(), config.plan_daily_limit),
            delay_threshold_minutes: config.delay_threshold_minutes,
            clock: Arc::new(Timestamp::now),
        })
    }

    /// Uses `clock` for the current time.
    #[must_use]
    pub fn with_clock(mut self, clock: Clock) -> Self {
        self.clock = clock;
        self
    }

    /// The usage counter.
    pub fn usage(&self) -> &Usage {
        &self.usage
    }

    /// The requests of the last 24 hours.
    pub fn usage_summary(&self) -> Result<UsageSummary, Error> {
        self.usage.summary((self.clock)())
    }

    /// One request: from the cache, or counted and sent once.
    async fn call<R: Call>(
        &self,
        request: &R,
        warnings: &mut Vec<String>,
    ) -> Result<R::Response, Error> {
        let key = self.client.url(request).to_string();
        let now = (self.clock)();
        let cached = self
            .cache
            .get(&key, cache::ttl(R::ENDPOINT), now)
            .and_then(|body| Client::decode::<R>(&body).ok());
        if let Some(response) = cached {
            return Ok(response);
        }
        warnings.extend(self.usage.admit(R::ENDPOINT, now)?);
        let body = self.client.fetch(request).await?;
        let response = Client::decode::<R>(&body)?;
        self.cache.put(&key, &body, now);
        Ok(response)
    }

    /// Stops by name.
    pub async fn find_stops(&self, query: &StopsQuery) -> Result<FindStopResult, Error> {
        let mut warnings = Vec::new();
        let request = Geocode {
            types: vec![LocationType::Stop],
            place: query.near.map(|(lat, lon)| format!("{lat},{lon}")),
            num_results: Some(query.count.clamp(1, MAX_STOPS)),
            ..Geocode::new(query.text.clone())
        };
        let matches = self.call(&request, &mut warnings).await?;
        Ok(FindStopResult {
            query: query.text.clone(),
            stops: matches
                .iter()
                .filter(|found| found.kind == LocationType::Stop)
                .map(StopMatch::from)
                .collect(),
            attribution: Attribution::default(),
            warnings,
        })
    }

    /// The stop ID for an ID or a name.
    pub async fn resolve_stop(
        &self,
        input: &str,
        warnings: &mut Vec<String>,
    ) -> Result<String, Error> {
        let input = input.trim();
        if resolve::is_stop_id(input) {
            return Ok(input.to_owned());
        }
        if input.is_empty() {
            return Err(Error::StopNotFound {
                query: input.to_owned(),
            });
        }
        let request = Geocode {
            types: vec![LocationType::Stop],
            num_results: Some(RESOLVE_MATCHES),
            ..Geocode::new(input)
        };
        let matches = self.call(&request, warnings).await?;
        Ok(resolve::choose(input, &matches)?.id.clone())
    }

    /// A place for the routing: coordinates as they are, else a stop.
    async fn resolve_place(
        &self,
        input: &str,
        warnings: &mut Vec<String>,
    ) -> Result<String, Error> {
        match resolve::coordinates(input) {
            Some((lat, lon)) => Ok(format!("{lat},{lon}")),
            None => self.resolve_stop(input, warnings).await,
        }
    }

    /// Departures or arrivals at a stop.
    pub async fn departures(&self, query: &DeparturesQuery) -> Result<DeparturesResult, Error> {
        let mut warnings = Vec::new();
        let stop_id = self.resolve_stop(&query.stop, &mut warnings).await?;
        let request = Stoptimes {
            time: query.time,
            arrive_by: Some(query.arrivals),
            n: Some(query.count.clamp(1, MAX_DEPARTURES)),
            mode: query.modes.clone(),
            page_cursor: query.page_cursor.clone(),
            with_alerts: Some(true),
            ..Stoptimes::new(stop_id.clone())
        };
        let response = self.call(&request, &mut warnings).await?;
        let mut stop = Location::from(&response.place);
        stop.id.get_or_insert(stop_id);
        Ok(DeparturesResult {
            stop,
            arrivals: query.arrivals,
            departures: response
                .stop_times
                .iter()
                .map(|stop_time| Departure::new(stop_time, query.arrivals))
                .collect(),
            page_cursor: PageCursor::new(
                &response.previous_page_cursor,
                &response.next_page_cursor,
            ),
            attribution: Attribution::default(),
            warnings,
        })
    }

    /// Connections between two places, each with its status.
    ///
    /// An arrive-by search returns the connections that arrive last before
    /// the time, so the earlier ones are there to switch to. If one page
    /// has fewer than `count`, it asks once more for the page before.
    pub async fn trip(&self, query: &TripQuery) -> Result<TripResult, Error> {
        let mut warnings = Vec::new();
        let from = self.resolve_place(&query.from, &mut warnings).await?;
        let to = self.resolve_place(&query.to, &mut warnings).await?;
        let count = query.count.clamp(1, MAX_CONNECTIONS);
        let wanted = if query.arrive_by {
            count.max(MIN_ARRIVE_BY)
        } else {
            count
        };
        let now = (self.clock)();
        let request = Plan {
            time: query.time.or(query.arrive_by.then_some(now)),
            arrive_by: Some(query.arrive_by),
            num_itineraries: Some(wanted),
            page_cursor: query.page_cursor.clone(),
            transit_modes: query.modes.clone(),
            max_transfers: query.max_transfers,
            realtime_mode: Some(if query.realtime_routing {
                RealtimeMode::Realtime
            } else {
                RealtimeMode::RealtimeAnnotationOnly
            }),
            ..Plan::new(from, to)
        };
        let response = self.call(&request, &mut warnings).await?;
        // Walking can beat every ride: the API then returns it as a direct
        // route and leaves out the slower rides.
        let mut itineraries = response.itineraries;
        add_new(&mut itineraries, response.direct);
        let mut earlier = response.previous_page_cursor;
        let later = response.next_page_cursor;
        let short = itineraries.len() < count as usize;
        if query.arrive_by && query.page_cursor.is_none() && short && !earlier.is_empty() {
            let previous = Plan {
                page_cursor: Some(earlier.clone()),
                ..request
            };
            match self.call(&previous, &mut warnings).await {
                Ok(page) => {
                    earlier = page.previous_page_cursor;
                    add_new(&mut itineraries, page.itineraries);
                }
                Err(error) => warnings.push(format!("no earlier connections: {error}")),
            }
        }
        itineraries.sort_by_key(departure);
        let now = (self.clock)();
        Ok(TripResult {
            from: Location::from(&response.from),
            to: Location::from(&response.to),
            connections: itineraries
                .iter()
                .map(|itinerary| {
                    let evaluation = status::evaluate(itinerary, now, self.delay_threshold_minutes);
                    Connection::new(itinerary, evaluation)
                })
                .collect(),
            page_cursor: PageCursor::new(&earlier, &later),
            attribution: Attribution::default(),
            warnings,
        })
    }

    /// A trip with all its stops.
    pub async fn journey(&self, trip_id: &str) -> Result<JourneyResult, Error> {
        let mut warnings = Vec::new();
        let itinerary = self.call(&Trip::new(trip_id), &mut warnings).await?;
        Ok(JourneyResult::new(trip_id, &itinerary, warnings))
    }
}

/// The departure of the first leg, by the schedule: the order of the
/// timetable, which delays do not change.
fn departure(itinerary: &motis::Itinerary) -> Timestamp {
    itinerary
        .legs
        .first()
        .map_or(itinerary.start_time, |leg| leg.scheduled_start_time)
}

/// Whether two itineraries are the same connection.
/// Adds the connections of `page` that `known` does not have yet.
fn add_new(known: &mut Vec<motis::Itinerary>, page: Vec<motis::Itinerary>) {
    for itinerary in page {
        if !known.iter().any(|other| same(other, &itinerary)) {
            known.push(itinerary);
        }
    }
}

fn same(a: &motis::Itinerary, b: &motis::Itinerary) -> bool {
    if !a.id.is_empty() && !b.id.is_empty() {
        return a.id == b.id;
    }
    let trips = |it: &motis::Itinerary| -> Vec<(Option<String>, Timestamp)> {
        it.legs
            .iter()
            .map(|leg| (leg.trip_id.clone(), leg.scheduled_start_time))
            .collect()
    };
    trips(a) == trips(b)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn tells_connections_apart() {
        let leg = |trip: &str, minute: i64| motis::Leg {
            trip_id: Some(trip.into()),
            scheduled_start_time: Timestamp::from_second(minute * 60).unwrap(),
            ..motis::Leg::default()
        };
        let with = |id: &str, legs| motis::Itinerary {
            id: id.into(),
            legs,
            ..motis::Itinerary::default()
        };
        assert!(same(&with("a", vec![]), &with("a", vec![leg("x", 1)])));
        assert!(!same(&with("a", vec![]), &with("b", vec![])));
        assert!(same(
            &with("", vec![leg("x", 1)]),
            &with("", vec![leg("x", 1)])
        ));
        assert!(!same(
            &with("", vec![leg("x", 1)]),
            &with("a", vec![leg("x", 2)])
        ));
        assert_eq!(departure(&with("", vec![])), Timestamp::default());
        let mut known = vec![with("a", vec![]), with("b", vec![])];
        add_new(&mut known, vec![with("b", vec![]), with("c", vec![])]);
        let ids: Vec<&str> = known.iter().map(|i| i.id.as_str()).collect();
        assert_eq!(ids, ["a", "b", "c"]);
    }
}
