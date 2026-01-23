//! Application error types.

use std::fmt;

use crate::cache::ServiceCacheError;
use crate::service::ServiceError;

/// Errors that can occur during application lifecycle.
#[derive(Debug)]
pub enum AppError {
    /// Failed to start memory cache service.
    MemoryCacheStart(ServiceCacheError),

    /// Failed to start disk cache service.
    DiskCacheStart(ServiceCacheError),

    /// Failed to create the service.
    ServiceCreation(ServiceError),

    /// Configuration error.
    Config(String),

    /// Failed to create the Tokio runtime.
    RuntimeCreation(String),
}

impl fmt::Display for AppError {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            AppError::MemoryCacheStart(e) => {
                write!(f, "Failed to start memory cache service: {}", e)
            }
            AppError::DiskCacheStart(e) => {
                write!(f, "Failed to start disk cache service: {}", e)
            }
            AppError::ServiceCreation(e) => {
                write!(f, "Failed to create service: {}", e)
            }
            AppError::Config(msg) => {
                write!(f, "Configuration error: {}", msg)
            }
            AppError::RuntimeCreation(msg) => {
                write!(f, "Failed to create Tokio runtime: {}", msg)
            }
        }
    }
}

impl std::error::Error for AppError {
    fn source(&self) -> Option<&(dyn std::error::Error + 'static)> {
        match self {
            AppError::MemoryCacheStart(e) => Some(e),
            AppError::DiskCacheStart(e) => Some(e),
            AppError::ServiceCreation(e) => Some(e),
            AppError::Config(_) => None,
            AppError::RuntimeCreation(_) => None,
        }
    }
}

impl From<ServiceCacheError> for AppError {
    fn from(e: ServiceCacheError) -> Self {
        // Default to disk cache error for generic conversions
        AppError::DiskCacheStart(e)
    }
}

impl From<ServiceError> for AppError {
    fn from(e: ServiceError) -> Self {
        AppError::ServiceCreation(e)
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_app_error_display() {
        let err = AppError::Config("missing provider".to_string());
        assert!(err.to_string().contains("Configuration error"));
        assert!(err.to_string().contains("missing provider"));
    }

    #[test]
    fn test_app_error_from_service_error() {
        let service_err = ServiceError::ConfigError("test".to_string());
        let app_err: AppError = service_err.into();
        assert!(matches!(app_err, AppError::ServiceCreation(_)));
    }
}
