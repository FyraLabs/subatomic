//! Repodata generation and XML type definitions
//!
//! This module contains repodata XML type definitions and the helper functions required to generate
//! those XML files.

pub mod filelists;
pub mod other;
pub mod primary;
pub mod repomd;

pub fn write<I: IntoIterator<Item = rpm::PackageMetadata>>(pkgs: I) {
    todo!()
}
