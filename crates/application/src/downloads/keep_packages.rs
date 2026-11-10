use pullbay_domain::download::Download;
use pullbay_domain::id::{DownloadId, PackageId};
use pullbay_domain::package::{plan_packages, Package, Packaging};
use pullbay_domain::ports::PackageStore;

/// Keep downloads added together as packages: all of them as one, under a
/// name and in one folder, or each split archive among them as one of its
/// own (see `pullbay_domain::package::plan_packages`).
pub struct KeepPackages<'a, P> {
    packages: &'a P,
}

impl<'a, P: PackageStore> KeepPackages<'a, P> {
    pub fn new(packages: &'a P) -> Self {
        KeepPackages { packages }
    }

    /// Send every item to the package's folder, when one was chosen, before
    /// they are added.
    pub fn prepare(&self, items: &mut [Download], packaging: &Packaging) {
        if let Packaging::One { folder: Some(folder), .. } = packaging {
            for d in items {
                d.folder = Some(folder.clone());
            }
        }
    }

    /// Make the packages the new downloads form: `added` holds each new
    /// download with its file name, in the order they were added. Returns
    /// the new packages.
    pub fn keep(&self, added: &[(DownloadId, String)], packaging: &Packaging) -> Vec<PackageId> {
        let names: Vec<&str> = added.iter().map(|(_, name)| name.as_str()).collect();
        let plan = plan_packages(&names, packaging);
        if plan.is_empty() {
            return Vec::new();
        }
        let folder = match packaging {
            Packaging::One { folder, .. } => folder.clone(),
            Packaging::None | Packaging::Archives => None,
        };
        let ids = self.packages.edit_packages(|list| {
            let mut next = list.iter().map(|p| p.id).max().unwrap_or_default().next();
            plan.into_iter()
                .map(|(name, indexes)| {
                    let id = next;
                    next = next.next();
                    let downloads = indexes.iter().filter_map(|&i| added.get(i).map(|(d, _)| *d)).collect();
                    list.push(Package::new(id, name, folder.clone(), downloads));
                    id
                })
                .collect()
        });
        self.packages.packages_changed();
        ids
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::memory::Packages;
    use pullbay_domain::package::{PackageName, Unpack};

    fn added(names: &[&str]) -> Vec<(DownloadId, String)> {
        names.iter().enumerate().map(|(i, n)| (DownloadId::new(i as u64 + 10), n.to_string())).collect()
    }

    #[test]
    fn links_kept_together_make_one_package_in_its_folder() {
        let packages = Packages::default();
        let keep = KeepPackages::new(&packages);
        let packaging = Packaging::One { name: PackageName::parse("Trip").unwrap(), folder: Some("/dl/Trip".into()) };
        let mut items = vec![Download::default(), Download::default()];
        keep.prepare(&mut items, &packaging);
        assert!(items.iter().all(|d| d.folder.as_deref() == Some("/dl/Trip")));
        let ids = keep.keep(&added(&["a.jpg", "b.jpg"]), &packaging);
        assert_eq!(ids, [PackageId::new(1)]);
        let p = packages.get(1).unwrap();
        assert_eq!((p.name.as_str(), p.folder.as_deref(), p.unpack), ("Trip", Some("/dl/Trip"), Unpack::Waiting));
        assert_eq!(p.downloads, [DownloadId::new(10), DownloadId::new(11)]);
        assert_eq!(packages.changes(), 1);
    }

    #[test]
    fn each_split_archive_becomes_its_own_package_after_the_others() {
        let packages = Packages::with(vec![Package::new(
            PackageId::new(4),
            PackageName::parse("Old").unwrap(),
            None,
            vec![DownloadId::new(1)],
        )]);
        let keep = KeepPackages::new(&packages);
        let names = ["x.part1.rar", "cover.jpg", "x.part2.rar", "y.7z.001", "y.7z.002", "single.zip"];
        let ids = keep.keep(&added(&names), &Packaging::Archives);
        assert_eq!(ids, [PackageId::new(5), PackageId::new(6)]);
        let x = packages.get(5).unwrap();
        assert_eq!((x.name.as_str(), x.folder.clone()), ("x", None));
        assert_eq!(x.downloads, [DownloadId::new(10), DownloadId::new(12)]);
        assert_eq!(packages.get(6).unwrap().downloads, [DownloadId::new(13), DownloadId::new(14)]);
    }

    #[test]
    fn nothing_is_kept_without_packaging_or_split_archives() {
        let packages = Packages::default();
        let keep = KeepPackages::new(&packages);
        assert!(keep.keep(&added(&["a.part1.rar", "a.part2.rar"]), &Packaging::None).is_empty());
        assert!(keep.keep(&added(&["a.zip", "b.zip"]), &Packaging::Archives).is_empty());
        assert_eq!(packages.changes(), 0, "nothing to announce");
    }
}
