//! System hardware detection for setup wizard.
//!
//! Detects CPU cores, system memory, and storage type to recommend
//! optimal configuration settings.

use std::path::Path;

use xearthlayer::config::format_size;
use xearthlayer::pipeline::DiskIoProfile;

/// Detected system hardware information.
#[derive(Debug, Clone)]
pub struct SystemInfo {
    /// Number of logical CPU cores
    pub cpu_cores: usize,
    /// Total system memory in bytes
    pub total_memory: usize,
    /// Detected storage type for a given path
    pub storage_type: DiskIoProfile,
}

impl SystemInfo {
    /// Detect system information for a given cache path.
    pub fn detect(cache_path: &Path) -> Self {
        let cpu_cores = std::thread::available_parallelism()
            .map(|p| p.get())
            .unwrap_or(4);

        let total_memory = detect_total_memory();
        let storage_type = DiskIoProfile::Auto.resolve_for_path(cache_path);

        Self {
            cpu_cores,
            total_memory,
            storage_type,
        }
    }

    /// Get recommended memory cache size based on total system memory.
    ///
    /// Rules:
    /// - < 8GB RAM: 2GB cache
    /// - 8-31GB RAM: 8GB cache
    /// - 32-63GB RAM: 12GB cache
    /// - 64+ GB RAM: 16GB cache
    pub fn recommended_memory_cache(&self) -> usize {
        const GB: usize = 1024 * 1024 * 1024;

        match self.total_memory {
            m if m < 8 * GB => 2 * GB,
            m if m < 32 * GB => 8 * GB,
            m if m < 64 * GB => 12 * GB,
            _ => 16 * GB,
        }
    }

    /// Get recommended disk cache size (default 40GB).
    pub fn recommended_disk_cache(&self) -> usize {
        40 * 1024 * 1024 * 1024 // 40GB
    }

    /// Get recommended disk I/O profile string for config.
    ///
    /// Returns "nvme" if NVMe detected, otherwise "auto".
    pub fn recommended_io_profile(&self) -> &'static str {
        match self.storage_type {
            DiskIoProfile::Nvme => "nvme",
            _ => "auto",
        }
    }

    /// Get formatted memory string (e.g., "32 GB").
    pub fn memory_display(&self) -> String {
        format_size(self.total_memory)
    }

    /// Get formatted recommended cache size (e.g., "8 GB").
    pub fn recommended_cache_display(&self) -> String {
        format_size(self.recommended_memory_cache())
    }

    /// Get storage type display string.
    pub fn storage_display(&self) -> &'static str {
        match self.storage_type {
            DiskIoProfile::Nvme => "NVMe SSD",
            DiskIoProfile::Ssd => "SATA SSD",
            DiskIoProfile::Hdd => "HDD",
            DiskIoProfile::Auto => "Unknown (defaulting to SSD)",
        }
    }
}

/// Detect total system memory in bytes.
#[cfg(target_os = "linux")]
fn detect_total_memory() -> usize {
    use std::fs;

    // Parse /proc/meminfo
    if let Ok(content) = fs::read_to_string("/proc/meminfo") {
        for line in content.lines() {
            if line.starts_with("MemTotal:") {
                // Format: "MemTotal:       16384000 kB"
                let parts: Vec<&str> = line.split_whitespace().collect();
                if parts.len() >= 2 {
                    if let Ok(kb) = parts[1].parse::<usize>() {
                        return kb * 1024; // Convert to bytes
                    }
                }
            }
        }
    }
    // Fallback: 8GB
    8 * 1024 * 1024 * 1024
}

#[cfg(not(target_os = "linux"))]
fn detect_total_memory() -> usize {
    // Fallback for non-Linux: 8GB
    8 * 1024 * 1024 * 1024
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_recommended_memory_cache() {
        const GB: usize = 1024 * 1024 * 1024;

        // 4GB system -> 2GB cache
        let info = SystemInfo {
            cpu_cores: 4,
            total_memory: 4 * GB,
            storage_type: DiskIoProfile::Ssd,
        };
        assert_eq!(info.recommended_memory_cache(), 2 * GB);

        // 16GB system -> 8GB cache
        let info = SystemInfo {
            cpu_cores: 8,
            total_memory: 16 * GB,
            storage_type: DiskIoProfile::Ssd,
        };
        assert_eq!(info.recommended_memory_cache(), 8 * GB);

        // 32GB system -> 12GB cache
        let info = SystemInfo {
            cpu_cores: 16,
            total_memory: 32 * GB,
            storage_type: DiskIoProfile::Nvme,
        };
        assert_eq!(info.recommended_memory_cache(), 12 * GB);

        // 128GB system -> 16GB cache
        let info = SystemInfo {
            cpu_cores: 32,
            total_memory: 128 * GB,
            storage_type: DiskIoProfile::Nvme,
        };
        assert_eq!(info.recommended_memory_cache(), 16 * GB);
    }

    #[test]
    fn test_io_profile_recommendation() {
        const GB: usize = 1024 * 1024 * 1024;

        let info = SystemInfo {
            cpu_cores: 8,
            total_memory: 16 * GB,
            storage_type: DiskIoProfile::Nvme,
        };
        assert_eq!(info.recommended_io_profile(), "nvme");

        let info = SystemInfo {
            cpu_cores: 8,
            total_memory: 16 * GB,
            storage_type: DiskIoProfile::Ssd,
        };
        assert_eq!(info.recommended_io_profile(), "auto");
    }
}
