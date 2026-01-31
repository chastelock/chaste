// SPDX-FileCopyrightText: 2024 The Chaste Authors
// SPDX-License-Identifier: Apache-2.0 OR BSD-2-Clause

use crate::name::{PackageName, PackageNameBorrowed};
use crate::package::PackageID;
use crate::svs::SourceVersionSpecifier;

#[derive(Debug, PartialEq, Eq, Clone, Copy)]
#[non_exhaustive]
/// The type of a [Dependency].
pub enum DependencyKind {
    /// Either defined as a regular dependency (in the `"dependencies"` field of package.json),
    /// or, in some cases (implementation-dependent), as any other kind that is not [`DependencyKind::DevDependency`].
    Dependency,
    /// Defined in `"devDependencies"`.
    DevDependency,
    /// Defined in `"peerDependencies"`. If known to be [defined as optional],
    /// it will be marked as [`DependencyKind::OptionalPeerDependency`] instead.
    ///
    /// [defined as optional]: https://docs.npmjs.com/cli/v11/configuring-npm/package-json#peerdependenciesmeta
    PeerDependency,
    /// Defined in `"optionalDependencies"`.
    OptionalDependency,
    /// Defined in `"peerDependency"` and known to be [defined as optional].
    ///
    /// [defined as optional]: https://docs.npmjs.com/cli/v11/configuring-npm/package-json#peerdependenciesmeta
    OptionalPeerDependency,
}

impl DependencyKind {
    pub fn is_prod(self) -> bool {
        !matches!(self, DependencyKind::DevDependency)
    }
    pub fn is_dev(self) -> bool {
        matches!(self, DependencyKind::DevDependency)
    }
    pub fn is_optional(self) -> bool {
        matches!(
            self,
            DependencyKind::OptionalDependency | DependencyKind::OptionalPeerDependency
        )
    }
    pub fn is_peer(self) -> bool {
        matches!(
            self,
            DependencyKind::PeerDependency | DependencyKind::OptionalPeerDependency
        )
    }
}

#[derive(Debug, Clone)]
/// A relation of dependency between 2 [`crate::Package`]s
pub struct Dependency {
    /// Type of dependency
    pub kind: DependencyKind,
    /// ID of the package that defined this dependency
    pub from: PackageID,
    /// ID of the package that is being depended on
    pub on: PackageID,
    alias_name: Option<PackageName>,
    svs: Option<SourceVersionSpecifier>,
}

impl Dependency {
    /// The source and version range chosen by the dependent package.
    ///
    /// # Example
    /// ```
    /// # use chaste_types::{ChastefileBuilder, DependencyBuilder, DependencyKind, PackageBuilder, PackageName, SourceVersionSpecifier};
    /// # let mut chastefile_builder = ChastefileBuilder::new(());
    /// # let root_pid = chastefile_builder.add_package(
    /// #     PackageBuilder::new(None, None).build().unwrap(),
    /// # ).unwrap();
    /// # chastefile_builder.set_root_package_id(root_pid);
    /// # let lodash_pid = chastefile_builder.add_package(
    /// #     PackageBuilder::new(
    /// #         Some(PackageName::new("lodash".to_string()).unwrap()),
    /// #         Some("4.2.1".to_string()),
    /// #     ).build().unwrap(),
    /// # ).unwrap();
    /// # let mut dependency_builder = DependencyBuilder::new(DependencyKind::Dependency, root_pid, lodash_pid);
    /// # dependency_builder.svs(SourceVersionSpecifier::new("^4.2.0".to_string()).unwrap());
    /// # chastefile_builder.add_dependency(dependency_builder.build());
    /// # let chastefile = chastefile_builder.build().unwrap();
    /// # let dependencies = chastefile.package_dependencies(root_pid);
    /// # let dependency = dependencies.first().unwrap();
    /// let svs = dependency.svs().unwrap();
    /// assert_eq!(svs, "^4.2.0");
    /// assert!(svs.is_npm());
    /// ```
    pub fn svs(&self) -> Option<&SourceVersionSpecifier> {
        self.svs.as_ref()
    }

    /// If the dependency is from npm, aliasing a package with a different name,
    /// this represents the name under which it's aliased, e.g. if package.json defines
    /// the dependency as `"lodash": "npm:@chastelock/lodash-fork@^4.0.0"`,
    /// [`crate::Package::name`] will be `@chastelock/lodash-fork`, but [`crate::Dependency::alias_name`]
    /// will be `lodash`. (If dependency is not from npm, the behavior is undefined.)
    ///
    /// # Example
    /// ```
    /// # use chaste_types::{ChastefileBuilder, DependencyBuilder, DependencyKind, PackageBuilder, PackageName};
    /// # let mut chastefile_builder = ChastefileBuilder::new(());
    /// # let root_pid = chastefile_builder.add_package(
    /// #     PackageBuilder::new(None, None).build().unwrap(),
    /// # ).unwrap();
    /// # chastefile_builder.set_root_package_id(root_pid);
    /// # let lodash_pid = chastefile_builder.add_package(
    /// #     PackageBuilder::new(
    /// #         Some(PackageName::new("@chastelock/lodash-fork".to_string()).unwrap()),
    /// #         Some("4.0.0".to_string()),
    /// #     ).build().unwrap(),
    /// # ).unwrap();
    /// # let mut dependency_builder = DependencyBuilder::new(DependencyKind::Dependency, root_pid, lodash_pid);
    /// # dependency_builder.alias_name(PackageName::new("lodash".to_string()).unwrap());
    /// # chastefile_builder.add_dependency(dependency_builder.build());
    /// # let chastefile = chastefile_builder.build().unwrap();
    /// let dependencies = chastefile.package_dependencies(root_pid);
    /// let dependency = dependencies.first().unwrap();
    /// assert_eq!(chastefile.package(dependency.on).name().unwrap(), "@chastelock/lodash-fork");
    /// assert_eq!(dependency.alias_name().unwrap(), "lodash");
    /// ```
    pub fn alias_name<'a>(&'a self) -> Option<PackageNameBorrowed<'a>> {
        self.alias_name.as_ref().map(|a| a.as_borrowed())
    }
}

pub struct DependencyBuilder {
    kind: DependencyKind,
    of: PackageID,
    on: PackageID,
    alias_name: Option<PackageName>,
    svs: Option<SourceVersionSpecifier>,
}

impl DependencyBuilder {
    pub fn new(kind: DependencyKind, of: PackageID, on: PackageID) -> DependencyBuilder {
        DependencyBuilder {
            kind,
            of,
            on,
            alias_name: None,
            svs: None,
        }
    }

    pub fn alias_name(&mut self, alias_name: PackageName) {
        self.alias_name = Some(alias_name);
    }

    pub fn svs(&mut self, svs: SourceVersionSpecifier) {
        self.svs = Some(svs);
    }

    pub fn build(self) -> Dependency {
        Dependency {
            kind: self.kind,
            from: self.of,
            on: self.on,
            alias_name: self.alias_name,
            svs: self.svs,
        }
    }
}
