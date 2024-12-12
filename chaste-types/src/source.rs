// SPDX-FileCopyrightText: 2024 The Chaste Authors
// SPDX-License-Identifier: Apache-2.0 OR BSD-2-Clause

#[derive(Debug, PartialEq, Eq, Clone, Copy)]
#[non_exhaustive]
pub enum PackageSourceType {
    /// An npm registry (not necessarily registry.npmjs.com)
    Npm,
    /// Arbitrary URL to a .tar.gz file, no registry involved.
    TarballURL,
    /// Git repository.
    Git,
}

#[derive(Debug, PartialEq, Eq, Clone)]
#[non_exhaustive]
/// This is meant as a supplement to [`crate::Package`] and isn't very useful without it.
///
/// The [special `github:` type](https://docs.npmjs.com/cli/v10/configuring-npm/package-json#github-urls)
/// is currently not recognized, and resolves to either [`PackageSource::Git`] or [`PackageSource::TarballURL`],
/// depending on the package manager.
pub enum PackageSource {
    /// An npm registry. This has no properties because the only variables
    /// are [crate::Package::name], [crate::Package::version], and the registry URL,
    /// which is out of scope for this project.
    Npm,

    TarballURL {
        // TODO: use url::URL?
        url: String,
    },

    Git {
        // TODO: not url::URL, this can be SSH
        url: String,
    },
}

impl PackageSource {
    pub fn source_type(&self) -> PackageSourceType {
        match self {
            PackageSource::Npm => PackageSourceType::Npm,
            PackageSource::TarballURL { .. } => PackageSourceType::TarballURL,
            PackageSource::Git { .. } => PackageSourceType::Git,
        }
    }
}
