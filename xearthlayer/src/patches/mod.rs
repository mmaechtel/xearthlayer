//! Tile patches module for custom Ortho4XP mesh/elevation support.
//!
//! This module provides functionality for managing patch tiles - pre-built
//! Ortho4XP tiles with custom mesh/elevation data from airport addon developers.
//!
//! # Overview
//!
//! Patches enable users to use custom elevation/mesh data (from airport addons)
//! while XEL generates textures dynamically using its configured imagery provider.
//! This ensures visual consistency across tile boundaries.
//!
//! # Architecture
//!
//! ```text
//! ~/.xearthlayer/patches/
//! ├── A_KDEN_Mesh/           # Patch folder (alphabetically-first = highest priority)
//! │   ├── Earth nav data/    # Contains DSF files with custom mesh
//! │   ├── terrain/           # Terrain definition files (.ter)
//! │   └── textures/          # Optional - XEL generates these on-demand
//! └── B_KLAX_Mesh/           # Second priority patch folder
//!     └── ...
//! ```
//!
//! The [`PatchUnionIndex`] merges all patch folders into a single virtual
//! file structure for FUSE mounting. Collision resolution uses alphabetical
//! folder naming (A < B < Z).
//!
//! # Example
//!
//! ```ignore
//! use xearthlayer::patches::{PatchDiscovery, PatchUnionIndex};
//! use std::path::Path;
//!
//! // Discover patches in the default directory
//! let patches_dir = Path::new("~/.xearthlayer/patches");
//! let discovery = PatchDiscovery::new(patches_dir);
//! let patches = discovery.find_patches()?;
//!
//! // Build the union index for FUSE mounting
//! let index = PatchUnionIndex::build(&patches)?;
//!
//! // Resolve a virtual path to its real source
//! if let Some(real_path) = index.resolve(Path::new("Earth nav data/+30-120/+33-119.dsf")) {
//!     println!("Real path: {:?}", real_path);
//! }
//! ```

mod discovery;
mod union_index;

pub use discovery::{PatchDiscovery, PatchInfo, PatchValidation, ValidationError};
pub use union_index::{DirEntry, PatchUnionIndex};
