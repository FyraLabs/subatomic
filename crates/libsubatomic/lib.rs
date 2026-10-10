//! libsubatomic: handle rpm repositories
//!
//! libsubatomic is the underlying library for handling rpm repositories.
//! Requries Rust nightly.
//!
//! # Usage
//!
//! The main entrypoint is [`Repo`]. Each associated methods roughly represent an API operation.
//! These are high level operations that should cover most cases with repository management.
//!
//! # Repo creation
//!
//! Unlike subatomic v0 (which shells out to `createrepo_c`), libsubatomic fully handles the repo
//! creation logic. If you want a quick and simple solution in rust, consider this separate
//! individual implementation: <https://github.com/artifactx-rs/createrepo_rs>
//!
//! The `kiritan` (see `../kiritan`) binary also supports repo creation, but its behaviour is not
//! 100% backwards compatible with `createrepo_c`.
//!
//! libsubatomic comes with a [`Cache`] that caches XML "fragments". The XML files are created by
//! concatenating the fragments per package in a [`heed`] database.
//!
//! # 📃 License
//!
//! ```"not rust"
//! Copyright (C) 2026  Fyra Labs
//!
//! This program is free software: you can redistribute it and/or modify
//! it under the terms of the GNU Affero General Public License as published by
//! the Free Software Foundation, either version 3 of the License, or
//! (at your option) any later version.
//!
//! This program is distributed in the hope that it will be useful,
//! but WITHOUT ANY WARRANTY; without even the implied warranty of
//! MERCHANTABILITY or FITNESS FOR A PARTICULAR PURPOSE.  See the
//! GNU Affero General Public License for more details.
//!
//! You should have received a copy of the GNU Affero General Public License
//! along with this program.  If not, see <https://www.gnu.org/licenses/>.
//! ```
#![warn(rust_2018_idioms)]
#![feature(default_field_values)]
#![feature(try_blocks)]
#![feature(error_generic_member_access)]
#![feature(file_buffered)]

pub mod cache;
mod err;
pub(crate) mod pkg;
pub mod prelude;
pub mod repo;
mod repodata;
pub mod sig;

pub use cache::{Cache, CacheConfig};
pub use err::{Error, Res};
pub use kuchiyose::ftmm::{Ftmm, FtmmDigest};
pub use kuchiyose::kiri::{Kiri, Kirifuda};
pub use kuchiyose::link::{Link, LinkBuf};
pub use repo::Repo;
pub use repodata::metan_prelude;
pub use repodata::repomd::{Checksum, Data, Location};

pub use pgp;
pub use rpm;
pub use smartstring;

/// Mark a struct to be `#[non_exhaustive]`.
///
/// Use `MyStruct { field1, field2, .. }` to create a new instance. This is a workaround for the
/// incompatibility between `#[non_exhaustive]` and `#[feature(default_field_values)]`.
#[derive(Debug, Clone, Copy, Eq, PartialEq, PartialOrd, Ord)]
pub(crate) struct NonExhaustive;
pub(crate) const fn non_exhaustive() -> NonExhaustive {
    NonExhaustive
}
