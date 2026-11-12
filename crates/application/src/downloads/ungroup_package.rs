use udm_domain::id::PackageId;
use udm_domain::ports::PackageStore;

use super::UnpackPackageError;

/// Stop keeping a package: its downloads stay in the list, on their own.
pub struct UngroupPackage<'a, P> {
    packages: &'a P,
}

impl<'a, P: PackageStore> UngroupPackage<'a, P> {
    pub fn new(packages: &'a P) -> Self {
        UngroupPackage { packages }
    }

    pub fn run(&self, id: PackageId) -> Result<(), UnpackPackageError> {
        let found = self.packages.edit_packages(|list| {
            let before = list.len();
            list.retain(|p| p.id != id);
            list.len() != before
        });
        if !found {
            return Err(UnpackPackageError::NoPackage(id));
        }
        self.packages.packages_changed();
        Ok(())
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::memory::Packages;
    use udm_domain::id::DownloadId;
    use udm_domain::package::{Package, PackageName};

    #[test]
    fn the_package_goes_and_its_downloads_stay() {
        let p = Package::new(PackageId::new(3), PackageName::parse("P").unwrap(), None, vec![DownloadId::new(1)]);
        let packages = Packages::with(vec![p]);
        UngroupPackage::new(&packages).run(PackageId::new(3)).unwrap();
        assert!(packages.list().is_empty());
        assert_eq!(packages.changes(), 1);
        let gone = UngroupPackage::new(&packages).run(PackageId::new(3));
        assert_eq!(gone, Err(UnpackPackageError::NoPackage(PackageId::new(3))));
    }
}
