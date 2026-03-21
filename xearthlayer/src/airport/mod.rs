//! Airport database for X-Plane scenery.
//!
//! This module provides airport coordinate lookup for features like
//! pre-warming the tile cache around a specific airport on startup.

mod index;
mod parser;
mod types;

pub use index::{AirportIndex, AirportIndexError};
pub use parser::{AptDatParser, ParseError};
pub use types::{validate_airport_icao, Airport, AirportValidationError};
