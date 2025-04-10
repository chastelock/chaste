// SPDX-FileCopyrightText: 2024 The Chaste Authors
// SPDX-License-Identifier: Apache-2.0 OR BSD-2-Clause

pub static PACKAGE_JSON_FILENAME: &str = "package.json";

pub use crate::chastefile::*;
pub use crate::checksums::*;
pub use crate::dependency::*;
pub use crate::derivation::*;
pub use crate::error::{Error, Result};
pub use crate::installation::*;
pub use crate::module_path::*;
pub use crate::name::*;
pub use crate::package::*;
pub use crate::quirks::*;
pub use crate::source::*;
pub use crate::svs::*;

mod chastefile;
mod checksums;
mod dependency;
mod derivation;
pub mod error;
mod installation;
mod misc;
mod module_path;
mod name;
mod package;
mod quirks;
mod source;
mod svs;
