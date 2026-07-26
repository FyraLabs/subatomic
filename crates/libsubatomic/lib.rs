//! libsubatomic: handle rpm repositories
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
//! libsubatomic comes with a [`repodata::RepoCache`] that caches XML "fragments". The XML files are
//! created by concatenating [`repodata::RepoCacheFragment`] per package in a [`heed`] database.
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
#![feature(slice_split_once)]
#![feature(try_blocks)]

pub mod err;
pub mod pkg;
pub mod prelude;
pub mod repo;
pub mod repodata;
pub mod sig;

pub use repo::Repo;
