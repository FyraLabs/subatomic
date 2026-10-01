//! Wrappers and Utilities for `libsubatomic`.
#![warn(rust_2018_idioms)]
#![feature(slice_split_once)]

pub mod comp;
pub mod rpm;
pub mod store;

pub use async_compression;
pub use sha2;
