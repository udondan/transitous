//! Core of the `transitous` CLI and MCP server: configuration, the usage
//! rules of the Transitous API, stop resolution and the status of
//! connections.
//!
//! Transitous is run by volunteers. [`Transitous`] keeps to its usage rules
//! (<https://transitous.org/api/>): every request names a contact in the
//! User-Agent, answers are cached, requests are counted and limited, and
//! nothing is retried.

pub mod agent;
pub mod cache;
pub mod config;
pub mod dirs;
mod error;
pub mod model;
pub mod resolve;
pub mod service;
pub mod status;
pub mod time;
pub mod usage;

pub use config::Config;
pub use error::Error;
pub use service::{DeparturesQuery, StopsQuery, Transitous, TripQuery};
pub use status::Status;
pub use time::QueryTime;
