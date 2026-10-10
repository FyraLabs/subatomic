//! Wrappers and Utilities for `libsubatomic`.
#![warn(rust_2018_idioms)]
#![feature(error_generic_member_access)]
#![feature(slice_split_once)]

pub mod comp;
pub mod ftmm;
pub mod kiri;
pub mod link;
pub mod rpm;
pub mod store;

pub use ftmm::{Ftmm, FtmmDigest};
pub use kiri::{Kiri, Kirifuda};
pub use link::{Link, LinkBuf};

mod sealed {
    pub trait Sealed {}
}
pub(crate) use sealed::Sealed;
