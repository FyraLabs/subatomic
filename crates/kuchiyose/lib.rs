//! Wrappers and Utilities for `libsubatomic`.
#![warn(rust_2018_idioms)]
#![feature(error_generic_member_access)]
#![feature(slice_split_once)]
#![feature(trim_prefix_suffix)]

pub mod comp;
pub mod ftmm;
pub mod link;
pub mod rpm;
pub mod store;

pub use async_compression;
pub use ftmm::{Ftmm, FtmmDigest};
pub use link::{Link, LinkBuf};
pub use sha2;
