//! Satellite imagery provider abstraction
//!
//! This module provides traits and implementations for downloading satellite
//! imagery from various providers (Bing Maps, Google, NAIP, etc.).

mod bing;
mod http;
mod types;

pub use bing::BingMapsProvider;
pub use http::{HttpClient, ReqwestClient};
pub use types::{Provider, ProviderError};

#[cfg(test)]
pub use http::tests::MockHttpClient;
