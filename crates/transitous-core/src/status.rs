//! The status of a connection: whether the caller can still take it as
//! planned. Pure functions of the itinerary, the current time and the delay
//! threshold; `docs/status.md` explains the rules.
//!
//! Only rides count (legs whose mode is transit); walks have no realtime
//! data and cannot be cancelled.

use jiff::Timestamp;
use motis::{Itinerary, Leg, PickupDropoffType};
use schemars::JsonSchema;
use serde::{Deserialize, Serialize};

use crate::model::{delay_minutes, minutes};

/// What the caller has to know first, from most to least important.
#[derive(
    Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord, Serialize, Deserialize, JsonSchema,
)]
#[serde(rename_all = "snake_case")]
pub enum Status {
    /// A ride, or boarding or alighting at a stop of it, is cancelled.
    Cancelled,
    /// A transfer cannot be made any more, or the first ride has left.
    Unreachable,
    /// The arrival is late by at least the threshold, or the departure is
    /// off by a minute or more.
    Delayed,
    /// No ride has realtime data: the times are the schedule.
    NoRealtime,
    /// As planned, as far as the realtime data tells.
    Ok,
}

impl Status {
    pub fn as_str(self) -> &'static str {
        match self {
            Self::Cancelled => "cancelled",
            Self::Unreachable => "unreachable",
            Self::Delayed => "delayed",
            Self::NoRealtime => "no_realtime",
            Self::Ok => "ok",
        }
    }
}

/// How much of a connection has realtime data.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize, JsonSchema)]
#[serde(rename_all = "snake_case")]
pub enum Realtime {
    /// Every ride.
    Full,
    /// Some rides.
    Partial,
    /// No ride.
    None,
}

/// Where at a stop.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize, JsonSchema)]
#[serde(rename_all = "snake_case")]
pub enum Side {
    Departure,
    Arrival,
}

/// Why a connection has its status. `leg` is the index in `legs`.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize, JsonSchema)]
#[serde(tag = "kind", rename_all = "snake_case")]
pub enum Reason {
    /// The trip of the leg is cancelled.
    LegCancelled { leg: usize },
    /// Boarding (`departure`) or alighting (`arrival`) is not possible.
    StopCancelled { leg: usize, side: Side },
    /// The ride of `leg` leaves before the previous ride arrives plus the
    /// walk in between. `margin_minutes` is negative.
    TransferMissed { leg: usize, margin_minutes: i64 },
    /// The first ride has left already.
    Departed { leg: usize, minutes_ago: i64 },
    /// The ride is late (positive) or early (negative).
    Delayed {
        leg: usize,
        side: Side,
        minutes: i64,
    },
    /// The ride has no realtime data.
    NoRealtime { leg: usize },
}

impl Reason {
    fn status(&self) -> Status {
        match self {
            Self::LegCancelled { .. } | Self::StopCancelled { .. } => Status::Cancelled,
            Self::TransferMissed { .. } | Self::Departed { .. } => Status::Unreachable,
            Self::Delayed { .. } => Status::Delayed,
            Self::NoRealtime { .. } => Status::NoRealtime,
        }
    }
}

/// The status of a connection with its reasons.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Evaluation {
    pub status: Status,
    /// Most important first.
    pub reasons: Vec<Reason>,
    pub realtime: Realtime,
    pub max_delay_minutes: Option<i64>,
}

/// Evaluates a connection at `now`. Arrivals late by at least
/// `delay_threshold_minutes` count as delayed.
pub fn evaluate(itinerary: &Itinerary, now: Timestamp, delay_threshold_minutes: u32) -> Evaluation {
    let rides: Vec<(usize, &Leg)> = itinerary
        .legs
        .iter()
        .enumerate()
        .filter(|(_, leg)| leg.mode.is_transit())
        .collect();
    let mut reasons = Vec::new();
    for &(index, leg) in &rides {
        cancellations(index, leg, &mut reasons);
    }
    transfers(itinerary, &rides, &mut reasons);
    if let Some(&(index, leg)) = rides.first()
        && leg.start_time < now
    {
        reasons.push(Reason::Departed {
            leg: index,
            minutes_ago: minutes(now.as_second() - leg.start_time.as_second()),
        });
    }
    delays(&rides, delay_threshold_minutes, &mut reasons);
    for &(index, leg) in &rides {
        if !leg.real_time {
            reasons.push(Reason::NoRealtime { leg: index });
        }
    }

    let with_realtime = rides.iter().filter(|(_, leg)| leg.real_time).count();
    let realtime = if with_realtime == rides.len() {
        Realtime::Full
    } else if with_realtime == 0 {
        Realtime::None
    } else {
        Realtime::Partial
    };
    // Rides without realtime data make the status `no_realtime` only if no
    // ride has any.
    let status = reasons
        .iter()
        .map(Reason::status)
        .filter(|status| *status != Status::NoRealtime || realtime == Realtime::None)
        .min()
        .unwrap_or(Status::Ok);
    reasons.sort_by_key(Reason::status);
    let max_delay_minutes = rides
        .iter()
        .filter(|(_, leg)| leg.real_time)
        .flat_map(|(_, leg)| {
            [
                delay_minutes(leg.scheduled_start_time, leg.start_time),
                delay_minutes(leg.scheduled_end_time, leg.end_time),
            ]
        })
        .max();
    Evaluation {
        status,
        reasons,
        realtime,
        max_delay_minutes,
    }
}

fn cancellations(index: usize, leg: &Leg, reasons: &mut Vec<Reason>) {
    if leg.cancelled == Some(true) {
        reasons.push(Reason::LegCancelled { leg: index });
        return;
    }
    let not_allowed = Some(PickupDropoffType::NotAllowed);
    // A passenger who stays seated does not board.
    let boards = leg.interline_with_previous_leg != Some(true);
    if boards && (leg.from.cancelled == Some(true) || leg.from.pickup_type == not_allowed) {
        reasons.push(Reason::StopCancelled {
            leg: index,
            side: Side::Departure,
        });
    }
    if leg.to.cancelled == Some(true) || leg.to.dropoff_type == not_allowed {
        reasons.push(Reason::StopCancelled {
            leg: index,
            side: Side::Arrival,
        });
    }
}

/// Transfers whose next ride leaves before the previous one arrives plus
/// the scheduled time of the walks in between.
fn transfers(itinerary: &Itinerary, rides: &[(usize, &Leg)], reasons: &mut Vec<Reason>) {
    for (&(previous_index, previous), &(index, next)) in rides.iter().zip(rides.iter().skip(1)) {
        if next.interline_with_previous_leg == Some(true) {
            continue;
        }
        let walk: i64 = itinerary.legs[previous_index + 1..index]
            .iter()
            .map(|leg| leg.scheduled_end_time.as_second() - leg.scheduled_start_time.as_second())
            .sum();
        let margin = next.start_time.as_second() - previous.end_time.as_second() - walk;
        if margin < 0 {
            reasons.push(Reason::TransferMissed {
                leg: index,
                margin_minutes: -minutes(-margin).max(1),
            });
        }
    }
}

fn delays(rides: &[(usize, &Leg)], threshold: u32, reasons: &mut Vec<Reason>) {
    if let Some(&(index, leg)) = rides.first()
        && leg.real_time
    {
        let minutes = delay_minutes(leg.scheduled_start_time, leg.start_time);
        if minutes != 0 {
            reasons.push(Reason::Delayed {
                leg: index,
                side: Side::Departure,
                minutes,
            });
        }
    }
    if let Some(&(index, leg)) = rides.last()
        && leg.real_time
    {
        let minutes = delay_minutes(leg.scheduled_end_time, leg.end_time);
        if minutes >= i64::from(threshold) && minutes > 0 {
            reasons.push(Reason::Delayed {
                leg: index,
                side: Side::Arrival,
                minutes,
            });
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use motis::{Mode, Place};

    fn at(minute: i64) -> Timestamp {
        Timestamp::from_second(1_800_000_000 + minute * 60).unwrap()
    }

    /// A leg by the schedule from `start` to `end` (minutes), shifted by
    /// `delay` minutes.
    fn leg(mode: Mode, start: i64, end: i64, delay: i64) -> Leg {
        Leg {
            mode,
            scheduled_start_time: at(start),
            scheduled_end_time: at(end),
            start_time: at(start + delay),
            end_time: at(end + delay),
            real_time: true,
            ..Leg::default()
        }
    }

    fn ride(start: i64, end: i64, delay: i64) -> Leg {
        leg(Mode::Subway, start, end, delay)
    }

    fn itinerary(legs: Vec<Leg>) -> Itinerary {
        Itinerary {
            legs,
            ..Itinerary::default()
        }
    }

    fn check(legs: Vec<Leg>) -> Evaluation {
        evaluate(&itinerary(legs), at(-10), 2)
    }

    #[test]
    fn on_time_is_ok() {
        let result = check(vec![
            leg(Mode::Walk, -2, 0, 0),
            ride(0, 5, 0),
            leg(Mode::Walk, 5, 7, 0),
            ride(8, 12, 0),
        ]);
        assert_eq!(result.status, Status::Ok);
        assert_eq!(result.reasons, []);
        assert_eq!(result.realtime, Realtime::Full);
        assert_eq!(result.max_delay_minutes, Some(0));
    }

    #[test]
    fn a_cancelled_trip_cancels_the_connection() {
        let mut cancelled = ride(8, 12, 0);
        cancelled.cancelled = Some(true);
        let result = check(vec![ride(0, 5, 0), cancelled]);
        assert_eq!(result.status, Status::Cancelled);
        assert_eq!(result.reasons, [Reason::LegCancelled { leg: 1 }]);
    }

    #[test]
    fn a_cancelled_stop_cancels_the_connection() {
        let mut skipped = ride(0, 5, 0);
        skipped.from.cancelled = Some(true);
        skipped.to.dropoff_type = Some(PickupDropoffType::NotAllowed);
        let result = check(vec![skipped]);
        assert_eq!(result.status, Status::Cancelled);
        assert_eq!(
            result.reasons,
            [
                Reason::StopCancelled {
                    leg: 0,
                    side: Side::Departure
                },
                Reason::StopCancelled {
                    leg: 0,
                    side: Side::Arrival
                },
            ]
        );
    }

    #[test]
    fn pickup_not_allowed_cancels_boarding_unless_seated() {
        let mut next = ride(5, 9, 0);
        next.from.pickup_type = Some(PickupDropoffType::NotAllowed);
        assert_eq!(
            check(vec![ride(0, 5, 0), next.clone()]).status,
            Status::Cancelled
        );
        next.interline_with_previous_leg = Some(true);
        assert_eq!(check(vec![ride(0, 5, 0), next]).status, Status::Ok);
    }

    #[test]
    fn walks_are_not_checked_for_cancellations() {
        let mut walk = leg(Mode::Walk, 5, 7, 0);
        walk.cancelled = Some(true);
        walk.to.cancelled = Some(true);
        walk.real_time = false;
        assert_eq!(
            check(vec![ride(0, 5, 0), walk, ride(8, 12, 0)]).status,
            Status::Ok
        );
    }

    #[test]
    fn a_late_feeder_misses_the_transfer() {
        // Arrives 5 + 4 = 9, walks 2, the next ride leaves at 8.
        let result = check(vec![
            ride(0, 5, 4),
            leg(Mode::Walk, 5, 7, 4),
            ride(8, 12, 0),
        ]);
        assert_eq!(result.status, Status::Unreachable);
        assert_eq!(
            result.reasons[0],
            Reason::TransferMissed {
                leg: 2,
                margin_minutes: -3
            }
        );
    }

    #[test]
    fn an_exact_transfer_still_works() {
        let result = check(vec![
            ride(0, 5, 1),
            leg(Mode::Walk, 5, 7, 1),
            ride(8, 12, 0),
        ]);
        assert!(
            !result
                .reasons
                .iter()
                .any(|r| matches!(r, Reason::TransferMissed { .. })),
            "{:?}",
            result.reasons
        );
    }

    #[test]
    fn a_missed_transfer_by_seconds_counts_a_minute() {
        let mut feeder = ride(0, 5, 0);
        feeder.end_time = Timestamp::from_second(at(5).as_second() + 10).unwrap();
        let result = check(vec![feeder, ride(5, 9, 0)]);
        assert_eq!(
            result.reasons[0],
            Reason::TransferMissed {
                leg: 1,
                margin_minutes: -1
            }
        );
    }

    #[test]
    fn staying_seated_is_no_transfer() {
        let mut next = ride(5, 9, 0);
        next.interline_with_previous_leg = Some(true);
        assert_eq!(check(vec![ride(0, 6, 3), next]).status, Status::Delayed);
    }

    #[test]
    fn a_departed_first_ride_is_unreachable() {
        let legs = vec![leg(Mode::Walk, -5, 0, 0), ride(0, 5, 2)];
        let result = evaluate(&itinerary(legs), at(3), 2);
        assert_eq!(result.status, Status::Unreachable);
        assert_eq!(
            result.reasons[0],
            Reason::Departed {
                leg: 1,
                minutes_ago: 1
            }
        );
    }

    #[test]
    fn a_late_arrival_is_delayed() {
        let result = check(vec![
            ride(0, 5, 0),
            ride(6, 10, 0),
            leg(Mode::Walk, 10, 12, 0),
        ]);
        assert_eq!(result.status, Status::Ok);
        let mut late = ride(6, 10, 0);
        late.end_time = at(13);
        let result = check(vec![ride(0, 5, 0), late]);
        assert_eq!(result.status, Status::Delayed);
        assert_eq!(
            result.reasons,
            [Reason::Delayed {
                leg: 1,
                side: Side::Arrival,
                minutes: 3
            }]
        );
        assert_eq!(result.max_delay_minutes, Some(3));
    }

    #[test]
    fn a_delay_below_the_threshold_is_ok() {
        let mut late = ride(0, 5, 0);
        late.end_time = at(6);
        assert_eq!(check(vec![late.clone()]).status, Status::Ok);
        assert_eq!(
            evaluate(&itinerary(vec![late]), at(-10), 1).status,
            Status::Delayed
        );
    }

    #[test]
    fn an_early_or_late_departure_is_delayed() {
        for delay in [-1, 1] {
            let mut ride = ride(0, 5, 0);
            ride.start_time = at(delay);
            let result = check(vec![ride]);
            assert_eq!(result.status, Status::Delayed, "{delay}");
            assert_eq!(
                result.reasons,
                [Reason::Delayed {
                    leg: 0,
                    side: Side::Departure,
                    minutes: delay
                }]
            );
        }
    }

    #[test]
    fn without_realtime_data_the_status_says_so() {
        let mut first = ride(0, 5, 0);
        first.real_time = false;
        let mut second = ride(6, 10, 0);
        second.real_time = false;
        let result = check(vec![first.clone(), second]);
        assert_eq!(result.status, Status::NoRealtime);
        assert_eq!(result.realtime, Realtime::None);
        assert_eq!(result.max_delay_minutes, None);
        assert_eq!(
            result.reasons,
            [Reason::NoRealtime { leg: 0 }, Reason::NoRealtime { leg: 1 }]
        );

        let result = check(vec![first, ride(6, 10, 0)]);
        assert_eq!(result.status, Status::Ok);
        assert_eq!(result.realtime, Realtime::Partial);
        assert_eq!(result.reasons, [Reason::NoRealtime { leg: 0 }]);
    }

    #[test]
    fn the_most_important_reason_comes_first() {
        let mut cancelled = ride(8, 12, 0);
        cancelled.cancelled = Some(true);
        let result = check(vec![ride(0, 5, 5), cancelled, ride(13, 15, 0)]);
        assert_eq!(result.status, Status::Cancelled);
        assert_eq!(result.reasons[0], Reason::LegCancelled { leg: 1 });
        assert!(
            result.reasons[1..]
                .iter()
                .all(|r| r.status() > Status::Cancelled),
            "{:?}",
            result.reasons
        );
    }

    #[test]
    fn a_walk_only_connection_is_ok() {
        let result = check(vec![Leg {
            from: Place::default(),
            ..leg(Mode::Walk, 0, 10, 0)
        }]);
        assert_eq!(result.status, Status::Ok);
        assert_eq!(result.realtime, Realtime::Full);
        assert_eq!(result.max_delay_minutes, None);
    }

    #[test]
    fn names_the_statuses() {
        let names: Vec<&str> = [
            Status::Cancelled,
            Status::Unreachable,
            Status::Delayed,
            Status::NoRealtime,
            Status::Ok,
        ]
        .into_iter()
        .map(Status::as_str)
        .collect();
        assert_eq!(
            names,
            ["cancelled", "unreachable", "delayed", "no_realtime", "ok"]
        );
        for status in [Status::Cancelled, Status::NoRealtime] {
            assert_eq!(
                serde_json::to_value(status).unwrap(),
                serde_json::Value::from(status.as_str())
            );
        }
    }
}
