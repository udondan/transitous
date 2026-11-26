//! The string enums of the API.
//!
//! Every enum keeps values it does not know in `Other`, so a newer server
//! does not break decoding.

use std::fmt;

/// Defines an enum of API strings with an `Other(String)` fallback. The first
/// variant is the default.
macro_rules! string_enum {
    (
        $(#[$meta:meta])*
        $name:ident {
            $( $(#[$vmeta:meta])* $variant:ident = $value:literal, )+
        }
    ) => {
        $(#[$meta])*
        #[derive(Debug, Clone, PartialEq, Eq, Hash, serde::Serialize, serde::Deserialize)]
        #[serde(from = "String", into = "String")]
        pub enum $name {
            $( $(#[$vmeta])* $variant, )+
            /// A value this version of the crate does not know.
            Other(String),
        }

        impl $name {
            /// The values this version of the crate knows, as the API spells them.
            pub const KNOWN: &'static [&'static str] = &[$($value),+];

            /// The value as the API spells it.
            pub fn as_str(&self) -> &str {
                match self {
                    $( Self::$variant => $value, )+
                    Self::Other(value) => value,
                }
            }
        }

        impl Default for $name {
            fn default() -> Self {
                Self::from(Self::KNOWN[0].to_owned())
            }
        }

        impl From<String> for $name {
            fn from(value: String) -> Self {
                match value.as_str() {
                    $( $value => Self::$variant, )+
                    _ => Self::Other(value),
                }
            }
        }

        impl From<$name> for String {
            fn from(value: $name) -> Self {
                match value {
                    $name::Other(value) => value,
                    known => known.as_str().to_owned(),
                }
            }
        }

        impl fmt::Display for $name {
            fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
                f.write_str(self.as_str())
            }
        }
    };
}

string_enum! {
    /// A means of transport, or a group of them like [`Mode::Transit`].
    Mode {
        Walk = "WALK",
        Bike = "BIKE",
        Rental = "RENTAL",
        Car = "CAR",
        Hgv = "HGV",
        CarParking = "CAR_PARKING",
        CarDropoff = "CAR_DROPOFF",
        Odm = "ODM",
        RideSharing = "RIDE_SHARING",
        Flex = "FLEX",
        DebugBusRoute = "DEBUG_BUS_ROUTE",
        DebugRailwayRoute = "DEBUG_RAILWAY_ROUTE",
        DebugFerryRoute = "DEBUG_FERRY_ROUTE",
        /// Every transit mode.
        Transit = "TRANSIT",
        Tram = "TRAM",
        Subway = "SUBWAY",
        Ferry = "FERRY",
        Airplane = "AIRPLANE",
        Bus = "BUS",
        Coach = "COACH",
        /// Every rail mode.
        Rail = "RAIL",
        HighspeedRail = "HIGHSPEED_RAIL",
        LongDistance = "LONG_DISTANCE",
        NightRail = "NIGHT_RAIL",
        RegionalFastRail = "REGIONAL_FAST_RAIL",
        RegionalRail = "REGIONAL_RAIL",
        Suburban = "SUBURBAN",
        Funicular = "FUNICULAR",
        AerialLift = "AERIAL_LIFT",
        /// The mode `OTHER` of the API, not an unknown value.
        OtherMode = "OTHER",
        /// Old spelling of [`Mode::AerialLift`].
        ArealLift = "AREAL_LIFT",
        Metro = "METRO",
        CableCar = "CABLE_CAR",
    }
}

impl Mode {
    /// Whether this is a means of public transport, not walking, cycling,
    /// driving or a rental.
    pub fn is_transit(&self) -> bool {
        !matches!(
            self,
            Self::Walk
                | Self::Bike
                | Self::Rental
                | Self::Car
                | Self::Hgv
                | Self::CarParking
                | Self::CarDropoff
                | Self::Odm
                | Self::RideSharing
                | Self::Flex
                | Self::DebugBusRoute
                | Self::DebugRailwayRoute
                | Self::DebugFerryRoute
        )
    }
}

string_enum! {
    /// The kind of a geocoding match.
    LocationType {
        Address = "ADDRESS",
        Place = "PLACE",
        Stop = "STOP",
    }
}

string_enum! {
    /// Whether passengers may board or alight. Cancellations and skipped
    /// stops show as [`PickupDropoffType::NotAllowed`].
    PickupDropoffType {
        Normal = "NORMAL",
        NotAllowed = "NOT_ALLOWED",
    }
}

string_enum! {
    /// Where a time comes from.
    TimestampReason {
        /// No realtime data: the time is the schedule.
        NoRtData = "NO_RT_DATA",
        InconsistentRt = "INCONSISTENT_RT",
        Observed = "OBSERVED",
        Propagated = "PROPAGATED",
        Predicted = "PREDICTED",
    }
}

string_enum! {
    /// The kind of a place in a street routing.
    VertexType {
        Normal = "NORMAL",
        Bikeshare = "BIKESHARE",
        Transit = "TRANSIT",
    }
}

string_enum! {
    /// The effect of an alert on the service (GTFS-RT `Effect`).
    AlertEffect {
        NoService = "NO_SERVICE",
        ReducedService = "REDUCED_SERVICE",
        SignificantDelays = "SIGNIFICANT_DELAYS",
        Detour = "DETOUR",
        AdditionalService = "ADDITIONAL_SERVICE",
        ModifiedService = "MODIFIED_SERVICE",
        OtherEffect = "OTHER_EFFECT",
        UnknownEffect = "UNKNOWN_EFFECT",
        StopMoved = "STOP_MOVED",
        NoEffect = "NO_EFFECT",
        AccessibilityIssue = "ACCESSIBILITY_ISSUE",
    }
}

string_enum! {
    /// The cause of an alert (GTFS-RT `Cause`).
    AlertCause {
        UnknownCause = "UNKNOWN_CAUSE",
        OtherCause = "OTHER_CAUSE",
        TechnicalProblem = "TECHNICAL_PROBLEM",
        Strike = "STRIKE",
        Demonstration = "DEMONSTRATION",
        Accident = "ACCIDENT",
        Holiday = "HOLIDAY",
        Weather = "WEATHER",
        Maintenance = "MAINTENANCE",
        Construction = "CONSTRUCTION",
        PoliceActivity = "POLICE_ACTIVITY",
        MedicalEmergency = "MEDICAL_EMERGENCY",
        SpecialEvent = "SPECIAL_EVENT",
    }
}

string_enum! {
    /// How severe an alert is.
    AlertSeverityLevel {
        UnknownSeverity = "UNKNOWN_SEVERITY",
        Info = "INFO",
        Warning = "WARNING",
        Severe = "SEVERE",
    }
}

string_enum! {
    /// How routing uses realtime data.
    RealtimeMode {
        /// Route on the realtime timetable: cancelled trips and missed
        /// transfers drop out of the results. The API default.
        Realtime = "REALTIME",
        /// Route on the schedule and annotate the result with realtime data:
        /// cancelled trips stay in the results, flagged as cancelled.
        RealtimeAnnotationOnly = "REALTIME_ANNOTATION_ONLY",
        /// Ignore realtime data.
        Off = "OFF",
    }
}

string_enum! {
    /// The direction of a departure search without a page cursor.
    Direction {
        Later = "LATER",
        Earlier = "EARLIER",
    }
}
