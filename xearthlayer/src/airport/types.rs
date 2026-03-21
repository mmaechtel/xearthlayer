//! Airport types and validation.

use std::path::Path;

use crate::xplane::XPlaneEnvironment;

use super::index::{AirportIndex, AirportIndexError};

/// Error returned when airport validation fails.
#[derive(Debug, thiserror::Error)]
pub enum AirportValidationError {
    /// Failed to locate X-Plane installation from Custom Scenery path.
    #[error("Failed to locate X-Plane installation: {0}")]
    XPlaneNotFound(#[from] crate::xplane::XPlanePathError),

    /// Failed to load or parse the airport database.
    #[error("Failed to load airport database: {0}")]
    DatabaseError(#[from] AirportIndexError),

    /// The specified airport ICAO code was not found.
    #[error("Airport '{0}' not found in X-Plane's apt.dat database")]
    AirportNotFound(String),
}

/// Validate that an airport ICAO code exists in X-Plane's database.
///
/// This performs early validation before heavy initialization, allowing
/// the CLI to report invalid airport codes immediately.
pub fn validate_airport_icao<P: AsRef<Path>>(
    custom_scenery_path: P,
    icao: &str,
) -> Result<(), AirportValidationError> {
    let xplane_env = XPlaneEnvironment::from_custom_scenery_path(custom_scenery_path)?;
    let airport_index = AirportIndex::from_xplane_path(xplane_env.installation_path())?;

    if airport_index.get(icao).is_none() {
        return Err(AirportValidationError::AirportNotFound(icao.to_string()));
    }

    Ok(())
}

/// An airport with ICAO code and location.
#[derive(Debug, Clone)]
pub struct Airport {
    /// ICAO code (e.g., "LFBO", "KJFK").
    pub icao: String,
    /// Airport name.
    pub name: String,
    /// Latitude in decimal degrees.
    pub latitude: f64,
    /// Longitude in decimal degrees.
    pub longitude: f64,
    /// Elevation in feet.
    pub elevation_ft: f32,
}

impl Airport {
    /// Create a new airport.
    pub fn new(icao: &str, name: &str, latitude: f64, longitude: f64, elevation_ft: f32) -> Self {
        Self {
            icao: icao.to_string(),
            name: name.to_string(),
            latitude,
            longitude,
            elevation_ft,
        }
    }
}
