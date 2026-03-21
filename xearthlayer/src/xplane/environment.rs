//! X-Plane environment detection and path resolution.

use std::path::{Path, PathBuf};

use super::detection::{detect_xplane_installs, XPlanePathError};
use super::paths;

/// Represents a detected X-Plane 12 installation environment.
///
/// Provides convenient access to various X-Plane directories and resources.
/// Create via [`XPlaneEnvironment::detect()`], [`XPlaneEnvironment::detect_all()`],
/// or [`XPlaneEnvironment::from_path()`].
#[derive(Debug, Clone)]
pub struct XPlaneEnvironment {
    /// Root X-Plane 12 installation directory.
    installation_path: PathBuf,
}

impl XPlaneEnvironment {
    /// Detect X-Plane 12 installation automatically.
    ///
    /// Uses the X-Plane install reference file to find the installation path.
    ///
    /// # Errors
    ///
    /// Returns an error if:
    /// - No X-Plane 12 installation is found
    /// - Multiple installations are found (user must choose via [`detect_all()`](Self::detect_all))
    pub fn detect() -> Result<Self, XPlanePathError> {
        let installations = detect_xplane_installs();

        match installations.len() {
            0 => {
                let reference_path = paths::get_install_reference_path()
                    .unwrap_or_else(|_| PathBuf::from("~/.x-plane/x-plane_install_12.txt"));
                Err(XPlanePathError::InstallFileNotFound(reference_path))
            }
            1 => Ok(Self {
                installation_path: installations.into_iter().next().unwrap(),
            }),
            _ => Err(XPlanePathError::MultipleInstallations(installations)),
        }
    }

    /// Detect all X-Plane 12 installations.
    ///
    /// Returns a vector of all valid X-Plane 12 installations found on the system.
    pub fn detect_all() -> Vec<Self> {
        detect_xplane_installs()
            .into_iter()
            .map(|path| Self {
                installation_path: path,
            })
            .collect()
    }

    /// Detect X-Plane installation that contains a specific Custom Scenery path.
    ///
    /// Given a Custom Scenery directory path, finds the X-Plane installation
    /// that contains it.
    pub fn from_custom_scenery_path<P: AsRef<Path>>(
        custom_scenery_path: P,
    ) -> Result<Self, XPlanePathError> {
        let path = custom_scenery_path.as_ref();

        // Custom Scenery is at {X-Plane 12}/Custom Scenery, so parent is the installation
        let installation_path = path
            .parent()
            .map(PathBuf::from)
            .ok_or_else(|| XPlanePathError::InstallPathNotFound(path.to_path_buf()))?;

        if !installation_path.exists() {
            return Err(XPlanePathError::InstallPathNotFound(installation_path));
        }

        Ok(Self { installation_path })
    }

    /// Create from an explicit X-Plane installation path.
    pub fn from_path<P: AsRef<Path>>(path: P) -> Result<Self, XPlanePathError> {
        let installation_path = path.as_ref().to_path_buf();

        if !installation_path.exists() {
            return Err(XPlanePathError::InstallPathNotFound(installation_path));
        }

        Ok(Self { installation_path })
    }

    /// Get the X-Plane 12 installation root path.
    pub fn installation_path(&self) -> &Path {
        &self.installation_path
    }

    /// Get the Custom Scenery directory path.
    pub fn custom_scenery_path(&self) -> PathBuf {
        self.installation_path.join("Custom Scenery")
    }

    /// Get the Resources directory path.
    pub fn resources_path(&self) -> PathBuf {
        self.installation_path.join("Resources")
    }

    /// Get the path to apt.dat (airport database).
    ///
    /// Returns the path if the file exists, otherwise `None`.
    pub fn apt_dat_path(&self) -> Option<PathBuf> {
        // X-Plane 12 location: Global Scenery/Global Airports/Earth nav data/apt.dat
        let xp12_path = self
            .installation_path
            .join("Global Scenery")
            .join("Global Airports")
            .join("Earth nav data");

        // Check X-Plane 12 location first
        let xp12_apt = xp12_path.join("apt.dat");
        if xp12_apt.exists() {
            return Some(xp12_apt);
        }

        let xp12_apt_gz = xp12_path.join("apt.dat.gz");
        if xp12_apt_gz.exists() {
            return Some(xp12_apt_gz);
        }

        // Fallback: X-Plane 11 location
        let xp11_path = self
            .installation_path
            .join("Resources")
            .join("default scenery")
            .join("default apt dat")
            .join("Earth nav data");

        let xp11_apt = xp11_path.join("apt.dat");
        if xp11_apt.exists() {
            return Some(xp11_apt);
        }

        let xp11_apt_gz = xp11_path.join("apt.dat.gz");
        if xp11_apt_gz.exists() {
            return Some(xp11_apt_gz);
        }

        None
    }

    /// Get the path to the Earth nav data directory.
    pub fn earth_nav_data_path(&self) -> PathBuf {
        // X-Plane 12 location
        let xp12_path = self
            .installation_path
            .join("Global Scenery")
            .join("Global Airports")
            .join("Earth nav data");

        if xp12_path.exists() {
            return xp12_path;
        }

        // Fallback to X-Plane 11 location
        self.installation_path
            .join("Resources")
            .join("default scenery")
            .join("default apt dat")
            .join("Earth nav data")
    }

    /// Check if Custom Scenery directory exists.
    pub fn has_custom_scenery(&self) -> bool {
        self.custom_scenery_path().exists()
    }

    /// Check if apt.dat exists.
    pub fn has_apt_dat(&self) -> bool {
        self.apt_dat_path().is_some()
    }

    /// Derive a mountpoint path for a scenery pack within Custom Scenery.
    pub fn mountpoint_for(&self, pack_name: &str) -> PathBuf {
        self.custom_scenery_path().join(pack_name)
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_from_path_nonexistent() {
        let result = XPlaneEnvironment::from_path("/nonexistent/path");
        assert!(result.is_err());
    }

    #[test]
    fn test_path_construction() {
        let temp_dir = std::env::temp_dir().join("xearthlayer_test_xplane");
        let _ = std::fs::create_dir_all(&temp_dir);

        if let Ok(env) = XPlaneEnvironment::from_path(&temp_dir) {
            assert_eq!(env.installation_path(), temp_dir.as_path());
            assert_eq!(env.custom_scenery_path(), temp_dir.join("Custom Scenery"));
            assert_eq!(env.resources_path(), temp_dir.join("Resources"));
            assert_eq!(
                env.mountpoint_for("my_pack"),
                temp_dir.join("Custom Scenery").join("my_pack")
            );
        }

        let _ = std::fs::remove_dir_all(&temp_dir);
    }

    #[test]
    fn test_apt_dat_path_missing() {
        let temp_dir = std::env::temp_dir().join("xearthlayer_test_xplane_apt");
        let _ = std::fs::create_dir_all(&temp_dir);

        if let Ok(env) = XPlaneEnvironment::from_path(&temp_dir) {
            assert!(env.apt_dat_path().is_none());
            assert!(!env.has_apt_dat());
        }

        let _ = std::fs::remove_dir_all(&temp_dir);
    }
}
