use std::path::{Path, PathBuf};

use pullbay_domain::download::{DownloadEntry, Status};
use pullbay_domain::id::{DownloadId, PackageId};
use pullbay_domain::package::{archive_sets, complete, Package, Password, Unpack, UnpackError, UnpackFailure};
use pullbay_domain::ports::{DownloadFiles, DownloadList, FilesError, PackageStore, UnpackJob, Unpacker};
use pullbay_domain::settings::Settings;

/// Why a package's unpacking was not started.
#[derive(Debug, Clone, PartialEq, Eq, thiserror::Error)]
pub enum UnpackPackageError {
    #[error("There is no package {0}.")]
    NoPackage(PackageId),
    #[error("The package is being unpacked already.")]
    Busy,
}

/// What unpacking a package came to.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct UnpackOutcome {
    /// The package's state now.
    pub state: Unpack,
    /// Archives that were to go to the Recycle Bin but stayed, and why.
    pub not_recycled: Vec<(PathBuf, FilesError)>,
}

/// Unpack a package's archives once its parts are in: every archive among
/// its files (split ones by their first part) into the folder the settings
/// name, then, when the settings say so, its archives to the Recycle Bin.
/// An encrypted archive leaves the package waiting for a password, which
/// the user is asked for; running it again with the password goes on.
pub struct UnpackPackage<'a, P, D, U: ?Sized, F> {
    packages: &'a P,
    downloads: &'a D,
    unpacker: &'a U,
    files: &'a F,
}

impl<'a, P, D, U, F> UnpackPackage<'a, P, D, U, F>
where
    P: PackageStore,
    D: DownloadList,
    U: Unpacker + ?Sized,
    F: DownloadFiles,
{
    pub fn new(packages: &'a P, downloads: &'a D, unpacker: &'a U, files: &'a F) -> Self {
        UnpackPackage { packages, downloads, unpacker, files }
    }

    /// The package download `finished` is in, when it should be unpacked on
    /// its own now: the settings unpack automatically, and every one of its
    /// downloads is done while it has not been unpacked or tried yet.
    pub fn due_after(&self, finished: DownloadId, settings: &Settings) -> Option<PackageId> {
        if !settings.unpack_auto {
            return None;
        }
        let package = self.packages.edit_packages(|list| {
            list.iter().find(|p| p.downloads.contains(&finished) && p.unpack == Unpack::Waiting).cloned()
        })?;
        let statuses: Vec<Option<Status>> = package.downloads.iter().map(|id| self.downloads.status(*id)).collect();
        complete(&statuses).then_some(package.id)
    }

    /// Mark package `id` as being unpacked, so it is not started twice. A
    /// package unpacked before may be unpacked again.
    pub fn begin(&self, id: PackageId) -> Result<(), UnpackPackageError> {
        let begun = self.packages.edit_packages(|list| {
            let p = list.iter_mut().find(|p| p.id == id).ok_or(UnpackPackageError::NoPackage(id))?;
            if p.unpack == Unpack::Unpacking {
                return Err(UnpackPackageError::Busy);
            }
            p.unpack = Unpack::Unpacking;
            Ok(())
        });
        if begun.is_ok() {
            self.packages.packages_changed();
        }
        begun
    }

    /// Unpack package `id`, begun with `begin`, with `password` for
    /// encrypted archives. A package that went meanwhile comes to `Waiting`.
    pub fn run(&self, id: PackageId, password: Option<&Password>, settings: &Settings) -> UnpackOutcome {
        let mut outcome = UnpackOutcome { state: Unpack::Waiting, not_recycled: Vec::new() };
        let Some(package) = self.packages.edit_packages(|list| list.iter().find(|p| p.id == id).cloned()) else {
            return outcome;
        };
        outcome.state = match self.jobs(&package, settings) {
            Err(reason) => Unpack::Failed { reason },
            Ok(jobs) => self.unpack(&jobs, password, settings, &mut outcome.not_recycled),
        };
        let state = outcome.state.clone();
        let found = self.packages.edit_packages(|list| list.iter_mut().find(|p| p.id == id).map(|p| p.unpack = state));
        if found.is_some() {
            self.packages.packages_changed();
        }
        outcome
    }

    /// The archives among the package's files, each with its parts in order
    /// and the folder it goes into.
    fn jobs(&self, package: &Package, settings: &Settings) -> Result<Vec<UnpackJob>, UnpackFailure> {
        let entries: Vec<Option<DownloadEntry>> =
            package.downloads.iter().map(|id| self.downloads.entry(*id)).collect();
        let names: Vec<String> = entries.iter().map(|e| e.as_ref().map(file_name).unwrap_or_default()).collect();
        let names: Vec<&str> = names.iter().map(String::as_str).collect();
        let sets = archive_sets(&names);
        if sets.is_empty() {
            return Err(UnpackFailure::NoArchives);
        }
        sets.iter()
            .map(|set| {
                let archive = set.first_name(&names).to_string();
                if let Some(&part) = set.missing.first() {
                    return Err(UnpackFailure::MissingPart { archive, part });
                }
                let parts = set
                    .parts
                    .iter()
                    .map(|&i| {
                        let e = entries.get(i)?.as_ref()?;
                        e.path.as_deref().filter(|_| e.status == Status::Done).map(PathBuf::from)
                    })
                    .collect::<Option<Vec<PathBuf>>>()
                    .ok_or(UnpackFailure::NotComplete)?;
                let beside = parts.first().and_then(|p| p.parent()).unwrap_or(Path::new(""));
                let into = settings.unpack_folder_for(beside, &package.name);
                Ok(UnpackJob { archive, kind: set.kind, family: set.family, parts, into })
            })
            .collect()
    }

    fn unpack(
        &self,
        jobs: &[UnpackJob],
        password: Option<&Password>,
        settings: &Settings,
        not_recycled: &mut Vec<(PathBuf, FilesError)>,
    ) -> Unpack {
        let mut files = 0;
        for job in jobs {
            match self.unpacker.unpack(job, password, settings) {
                Ok(done) => files += done.files,
                Err(UnpackError::NeedsPassword) => return Unpack::NeedsPassword { wrong: false },
                Err(UnpackError::WrongPassword) => return Unpack::NeedsPassword { wrong: true },
                Err(UnpackError::Failed(reason)) => return Unpack::Failed { reason },
            }
        }
        if settings.unpack_recycle {
            for part in jobs.iter().flat_map(|j| &j.parts) {
                if let Err(e) = self.files.recycle(Some(&part.display().to_string()), None) {
                    not_recycled.push((part.clone(), e));
                }
            }
        }
        let folder = jobs.first().map(|j| j.into.display().to_string()).unwrap_or_default();
        Unpack::Done { folder, files }
    }
}

/// The name of a download's file, or what the list calls it before it has
/// one.
fn file_name(e: &DownloadEntry) -> String {
    e.path
        .as_deref()
        .and_then(|p| Path::new(p).file_name())
        .map(|n| n.to_string_lossy().into_owned())
        .unwrap_or_else(|| e.display_name())
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::memory::{row, Downloads, Files, Packages, Unpacks};
    use pullbay_domain::package::{ArchiveKind, Family, PackageName};
    use pullbay_domain::settings::UnpackTo;

    fn done(id: u64, path: &str) -> DownloadEntry {
        let mut e = row(id, Status::Done);
        e.path = Some(path.into());
        e
    }

    fn package(ids: &[u64]) -> Packages {
        let name = PackageName::parse("Show").unwrap();
        Packages::with(vec![Package::new(
            PackageId::new(1),
            name,
            None,
            ids.iter().map(|i| DownloadId::new(*i)).collect(),
        )])
    }

    fn state(packages: &Packages) -> Unpack {
        packages.get(1).unwrap().unpack
    }

    #[test]
    fn a_split_archive_unpacks_from_its_first_part_into_the_package_folder() {
        let packages = package(&[1, 2, 3]);
        let downloads =
            Downloads::rows([done(1, "/dl/show.part2.rar"), done(2, "/dl/show.part1.rar"), done(3, "/dl/cover.jpg")]);
        let (unpacker, files) = (Unpacks::default(), Files::default());
        let unpack = UnpackPackage::new(&packages, &downloads, &unpacker, &files);
        unpack.begin(PackageId::new(1)).unwrap();
        assert_eq!(state(&packages), Unpack::Unpacking);
        assert_eq!(unpack.begin(PackageId::new(1)), Err(UnpackPackageError::Busy));
        let outcome = unpack.run(PackageId::new(1), None, &Settings::default());
        let job = &unpacker.jobs()[0].0;
        assert_eq!(job.archive, "show.part1.rar");
        assert_eq!((job.kind, job.family), (ArchiveKind::Rar, Family::Parts));
        assert_eq!(job.parts, [PathBuf::from("/dl/show.part1.rar"), PathBuf::from("/dl/show.part2.rar")]);
        assert_eq!(job.into, Path::new("/dl").join("Show"));
        assert_eq!(outcome.state, Unpack::Done { folder: job.into.display().to_string(), files: 1 });
        assert_eq!(state(&packages), outcome.state);
        assert!(files.recycled().is_empty(), "the archives stay unless the settings say otherwise");
    }

    #[test]
    fn the_archives_go_to_the_recycle_bin_when_asked() {
        let packages = package(&[1, 2]);
        let downloads = Downloads::rows([done(1, "/dl/a.7z.001"), done(2, "/dl/a.7z.002")]);
        let (unpacker, files) = (Unpacks::default(), Files::default());
        let settings = Settings { unpack_recycle: true, unpack_to: UnpackTo::Beside, ..Settings::default() };
        let unpack = UnpackPackage::new(&packages, &downloads, &unpacker, &files);
        unpack.begin(PackageId::new(1)).unwrap();
        unpack.run(PackageId::new(1), None, &settings);
        assert_eq!(unpacker.jobs()[0].0.into, PathBuf::from("/dl"));
        let recycled: Vec<Option<String>> = files.recycled().into_iter().map(|(p, _)| p).collect();
        assert_eq!(recycled.len(), 2);
        assert!(recycled.iter().all(|p| p.as_deref().is_some_and(|p| p.contains("a.7z.00"))));
    }

    #[test]
    fn an_encrypted_archive_asks_for_its_password_and_goes_on_with_it() {
        let packages = package(&[1]);
        let downloads = Downloads::rows([done(1, "/dl/secret.zip")]);
        let (unpacker, files) = (Unpacks::locked("open sesame"), Files::default());
        let unpack = UnpackPackage::new(&packages, &downloads, &unpacker, &files);
        let id = PackageId::new(1);
        unpack.begin(id).unwrap();
        assert_eq!(unpack.run(id, None, &Settings::default()).state, Unpack::NeedsPassword { wrong: false });
        unpack.begin(id).unwrap();
        let wrong = Password::new("guess".into());
        assert_eq!(unpack.run(id, wrong.as_ref(), &Settings::default()).state, Unpack::NeedsPassword { wrong: true });
        unpack.begin(id).unwrap();
        let right = Password::new("open sesame".into());
        assert!(matches!(unpack.run(id, right.as_ref(), &Settings::default()).state, Unpack::Done { .. }));
        assert_eq!(unpacker.jobs().last().unwrap().1.as_deref(), Some("open sesame"));
    }

    #[test]
    fn missing_or_unfinished_parts_and_no_archives_fail_before_unpacking() {
        let settings = Settings::default();
        let (unpacker, files) = (Unpacks::default(), Files::default());
        let cases = [
            (
                vec![done(1, "/dl/m.part1.rar"), done(2, "/dl/m.part3.rar")],
                UnpackFailure::MissingPart { archive: "m.part1.rar".into(), part: 2 },
            ),
            (vec![done(1, "/dl/m.part1.rar"), row(2, Status::Downloading)], UnpackFailure::NotComplete),
            (vec![done(1, "/dl/a.jpg")], UnpackFailure::NoArchives),
        ];
        for (rows, reason) in cases {
            let ids: Vec<u64> = rows.iter().map(|e| e.id.get()).collect();
            let packages = package(&ids);
            let mut rows = rows;
            // An unfinished part has a file name but no file yet.
            if let Some(e) = rows.iter_mut().find(|e| e.status == Status::Downloading) {
                e.path = Some("/dl/m.part2.rar".into());
            }
            let downloads = Downloads::rows(rows);
            let unpack = UnpackPackage::new(&packages, &downloads, &unpacker, &files);
            unpack.begin(PackageId::new(1)).unwrap();
            assert_eq!(unpack.run(PackageId::new(1), None, &settings).state, Unpack::Failed { reason });
        }
        assert!(unpacker.jobs().is_empty());
    }

    #[test]
    fn a_failure_is_kept_in_the_package() {
        let failure = UnpackFailure::Unsafe { archive: "evil.zip".into(), entry: "../x".into() };
        let packages = package(&[1]);
        let downloads = Downloads::rows([done(1, "/dl/evil.zip")]);
        let (unpacker, files) = (Unpacks::answering(Err(failure.clone().into())), Files::default());
        let unpack = UnpackPackage::new(&packages, &downloads, &unpacker, &files);
        unpack.begin(PackageId::new(1)).unwrap();
        unpack.run(PackageId::new(1), None, &Settings { unpack_recycle: true, ..Settings::default() });
        assert_eq!(state(&packages), Unpack::Failed { reason: failure });
        assert!(files.recycled().is_empty(), "nothing is recycled after a failure");
    }

    #[test]
    fn a_complete_package_is_due_only_when_unpacking_automatically() {
        let packages = package(&[1, 2]);
        let downloads = Downloads::rows([done(1, "/dl/a.zip"), row(2, Status::Downloading)]);
        let (unpacker, files) = (Unpacks::default(), Files::default());
        let unpack = UnpackPackage::new(&packages, &downloads, &unpacker, &files);
        let auto = Settings { unpack_auto: true, ..Settings::default() };
        let one = DownloadId::new(1);
        assert_eq!(unpack.due_after(one, &auto), None, "one is still downloading");
        downloads.set(2, Status::Done);
        assert_eq!(unpack.due_after(one, &Settings::default()), None, "not unpacking automatically");
        assert_eq!(unpack.due_after(one, &auto), Some(PackageId::new(1)));
        assert_eq!(unpack.due_after(DownloadId::new(9), &auto), None, "in no package");
        unpack.begin(PackageId::new(1)).unwrap();
        assert_eq!(unpack.due_after(one, &auto), None, "already begun");
        assert_eq!(unpack.begin(PackageId::new(7)), Err(UnpackPackageError::NoPackage(PackageId::new(7))));
    }
}
