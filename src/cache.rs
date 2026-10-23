//! Inventory and pruning of the local cache.
//!
//! The cache mirrors CAIDA's layout (see [`crate::fetch`]): one directory per dataset
//! holding its index, `YYYY/MM/` directories holding the snapshots, and a `.listing`
//! in each month visited. Snapshots never change once published, so nothing expires
//! them by age of download; a prune keeps them by the day in the file name
//! ([`Retention::keep_days`]) or by count ([`Retention::keep_last`]).
//!
//! Only files that follow the layout are ever removed: snapshots, and a month's
//! `.listing` once the month directory holds nothing else, after which the emptied
//! month and year directories go too. The index is never touched, and neither is
//! anything the tool did not put there.

use std::collections::HashSet;
use std::fs;
use std::io;
use std::path::{Path, PathBuf};

use jiff::civil::Date;

use crate::caida::{self, Family};
use crate::fetch::LISTING_FILE;

/// What went wrong while reading or pruning the cache.
#[derive(Debug, thiserror::Error)]
pub enum CacheError {
    /// A directory could not be read, or a file or directory could not be removed.
    #[error("{path}: {source}")]
    Io {
        /// The file or directory involved.
        path: PathBuf,
        /// The I/O error.
        #[source]
        source: io::Error,
    },
}

/// One snapshot file in the cache.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct CachedSnapshot {
    /// Which dataset it belongs to.
    pub family: Family,
    /// The day the snapshot was taken, parsed from the file name.
    pub date: Date,
    /// Where it is.
    pub path: PathBuf,
    /// Its size on disk.
    pub bytes: u64,
}

impl CachedSnapshot {
    /// The file name without directories.
    pub fn file_name(&self) -> String {
        self.path
            .file_name()
            .map_or_else(String::new, |name| name.to_string_lossy().into_owned())
    }
}

/// What the cache holds for one dataset.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct FamilyInventory {
    /// The dataset.
    pub family: Family,
    /// Its snapshots, oldest first (then by path).
    pub snapshots: Vec<CachedSnapshot>,
    /// The size of every file in the dataset directory and its year and month
    /// directories: the snapshots, the index, the listings and anything else there.
    pub bytes: u64,
}

impl FamilyInventory {
    /// The size of the snapshots alone.
    pub fn snapshot_bytes(&self) -> u64 {
        self.snapshots.iter().map(|snapshot| snapshot.bytes).sum()
    }
}

/// What the cache holds, per dataset.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Inventory {
    /// The cache directory scanned.
    pub cache_dir: PathBuf,
    /// One entry per family, IPv4 first.
    pub families: Vec<FamilyInventory>,
}

impl Inventory {
    /// Reads what is cached under `cache_dir`. A directory that does not exist is an
    /// empty cache, not an error.
    pub fn scan(cache_dir: &Path) -> Result<Self, CacheError> {
        let families = [Family::V4, Family::V6]
            .into_iter()
            .map(|family| scan_family(cache_dir, family))
            .collect::<Result<Vec<_>, _>>()?;
        Ok(Self {
            cache_dir: cache_dir.to_path_buf(),
            families,
        })
    }

    /// Every cached snapshot, IPv4 first, oldest first.
    pub fn snapshots(&self) -> impl Iterator<Item = &CachedSnapshot> {
        self.families.iter().flat_map(|family| family.snapshots.iter())
    }

    /// The size of every file in the cache.
    pub fn bytes(&self) -> u64 {
        self.families.iter().map(|family| family.bytes).sum()
    }

    /// The inventory of one family.
    pub fn family(&self, family: Family) -> Option<&FamilyInventory> {
        self.families.iter().find(|inventory| inventory.family == family)
    }
}

fn scan_family(cache_dir: &Path, family: Family) -> Result<FamilyInventory, CacheError> {
    let dataset = cache_dir.join(family.dataset());
    let mut inventory = FamilyInventory {
        family,
        snapshots: Vec::new(),
        bytes: 0,
    };
    if !dataset.is_dir() {
        return Ok(inventory);
    }
    for year_entry in entries(&dataset)? {
        let year_meta = metadata(&year_entry)?;
        if !year_meta.is_dir() {
            inventory.bytes += year_meta.len();
            continue;
        }
        if numeric_name(&year_entry, 4).is_none() {
            log::debug!("{}: not a year directory, skipped", year_entry.path().display());
            continue;
        }
        for month_entry in entries(&year_entry.path())? {
            let month_meta = metadata(&month_entry)?;
            if !month_meta.is_dir() {
                inventory.bytes += month_meta.len();
                continue;
            }
            if numeric_name(&month_entry, 2).is_none() {
                log::debug!("{}: not a month directory, skipped", month_entry.path().display());
                continue;
            }
            for entry in entries(&month_entry.path())? {
                let meta = metadata(&entry)?;
                if meta.is_dir() {
                    log::debug!("{}: unexpected directory, skipped", entry.path().display());
                    continue;
                }
                inventory.bytes += meta.len();
                let name = entry.file_name();
                let Some(name) = name.to_str() else {
                    continue;
                };
                if name.ends_with(".pfx2as.gz")
                    && let Some(date) = caida::date_from_file_name(name)
                {
                    inventory.snapshots.push(CachedSnapshot {
                        family,
                        date,
                        path: entry.path(),
                        bytes: meta.len(),
                    });
                }
            }
        }
    }
    inventory
        .snapshots
        .sort_by(|a, b| a.date.cmp(&b.date).then_with(|| a.path.cmp(&b.path)));
    Ok(inventory)
}

/// The directory's entries, sorted by name.
fn entries(dir: &Path) -> Result<Vec<fs::DirEntry>, CacheError> {
    let io_error = |source| CacheError::Io {
        path: dir.to_path_buf(),
        source,
    };
    let mut entries = fs::read_dir(dir)
        .map_err(io_error)?
        .collect::<Result<Vec<_>, _>>()
        .map_err(io_error)?;
    entries.sort_by_key(fs::DirEntry::file_name);
    Ok(entries)
}

/// The entry's metadata, without following a symbolic link.
fn metadata(entry: &fs::DirEntry) -> Result<fs::Metadata, CacheError> {
    entry.metadata().map_err(|source| CacheError::Io {
        path: entry.path(),
        source,
    })
}

/// The entry's name as a number when it is exactly `digits` ASCII digits (`2026`, `10`).
fn numeric_name(entry: &fs::DirEntry, digits: usize) -> Option<u32> {
    let name = entry.file_name();
    let name = name.to_str()?;
    (name.len() == digits && name.bytes().all(|b| b.is_ascii_digit()))
        .then(|| name.parse().ok())
        .flatten()
}

/// Which snapshots a prune keeps. Each rule is a bound of its own; with both set, a
/// snapshot stays only when both keep it.
#[derive(Debug, Clone, Copy, Default, PartialEq, Eq)]
pub struct Retention {
    /// Remove snapshots taken more than this many days before today; `Some(0)` keeps
    /// only today's.
    pub keep_days: Option<u32>,
    /// Keep only this many of the newest snapshots of each family; `Some(0)` keeps none.
    pub keep_last: Option<usize>,
}

impl Retention {
    /// Whether no rule is set, in which case a prune would remove nothing.
    pub fn is_empty(self) -> bool {
        self.keep_days.is_none() && self.keep_last.is_none()
    }

    /// The snapshots of `inventory` the rules do not keep, oldest first. `today` is the
    /// day `keep_days` counts back from.
    pub fn expired(self, inventory: &FamilyInventory, today: Date) -> Vec<&CachedSnapshot> {
        let cutoff = self
            .keep_days
            .map(|days| today.saturating_sub(jiff::Span::new().days(i64::from(days))));
        let count = inventory.snapshots.len();
        let newest_from = self.keep_last.map_or(0, |keep| count.saturating_sub(keep));
        inventory
            .snapshots
            .iter()
            .enumerate()
            .filter(|(position, snapshot)| {
                *position < newest_from || cutoff.is_some_and(|cutoff| snapshot.date < cutoff)
            })
            .map(|(_, snapshot)| snapshot)
            .collect()
    }
}

/// What a prune removed, or would remove.
#[derive(Debug, Clone, Default, PartialEq, Eq)]
pub struct Pruned {
    /// The snapshots, IPv4 first, oldest first.
    pub snapshots: Vec<CachedSnapshot>,
    /// The month and year directories emptied by that (each month's `.listing` with it).
    pub directories: Vec<PathBuf>,
    /// Whether nothing was actually removed.
    pub dry_run: bool,
}

impl Pruned {
    /// The size of the snapshots removed.
    pub fn bytes(&self) -> u64 {
        self.snapshots.iter().map(|snapshot| snapshot.bytes).sum()
    }
}

/// Removes the snapshots of `inventory` that `retention` does not keep, then the month
/// directories left with nothing but their `.listing` and the year directories left
/// empty. With `dry_run` nothing is removed and the result says what would be.
///
/// A snapshot that is already gone when its turn comes (another process removed it) is
/// counted as removed; any other failure stops the prune, and what was removed so far
/// stays removed.
pub fn prune(
    inventory: &Inventory,
    retention: Retention,
    today: Date,
    dry_run: bool,
) -> Result<Pruned, CacheError> {
    let mut pruned = Pruned {
        dry_run,
        ..Pruned::default()
    };
    // Everything removed (or, in a dry run, that would be), so a directory's remaining
    // entries can be judged the same way in both modes.
    let mut gone: HashSet<PathBuf> = HashSet::new();
    for family in &inventory.families {
        let mut months: Vec<PathBuf> = Vec::new();
        for snapshot in retention.expired(family, today) {
            if !dry_run {
                remove_file(&snapshot.path)?;
                log::info!("removed {}", snapshot.path.display());
            }
            gone.insert(snapshot.path.clone());
            if let Some(month) = snapshot.path.parent()
                && !months.iter().any(|known| known == month)
            {
                months.push(month.to_path_buf());
            }
            pruned.snapshots.push(snapshot.clone());
        }
        for month in months {
            if !remove_spent_month(&month, &gone, dry_run)? {
                continue;
            }
            gone.insert(month.clone());
            pruned.directories.push(month.clone());
            if let Some(year) = month.parent()
                && remove_empty_dir(year, &gone, dry_run)?
            {
                pruned.directories.push(year.to_path_buf());
            }
        }
    }
    Ok(pruned)
}

/// Removes a month directory holding nothing but its `.listing` once the paths in
/// `gone` are discounted, and the listing with it. Anything else in there (a stray
/// file, a download in progress) keeps the directory.
fn remove_spent_month(dir: &Path, gone: &HashSet<PathBuf>, dry_run: bool) -> Result<bool, CacheError> {
    let mut listing = None;
    for entry in entries(dir)? {
        let path = entry.path();
        if gone.contains(&path) {
            continue;
        }
        if entry.file_name() == LISTING_FILE {
            listing = Some(path);
            continue;
        }
        return Ok(false);
    }
    if !dry_run {
        if let Some(listing) = &listing {
            remove_file(listing)?;
        }
        remove_dir(dir)?;
        log::info!("removed the emptied directory {}", dir.display());
    }
    Ok(true)
}

/// Removes a directory with no entries left once the paths in `gone` are discounted.
fn remove_empty_dir(dir: &Path, gone: &HashSet<PathBuf>, dry_run: bool) -> Result<bool, CacheError> {
    if entries(dir)?.iter().any(|entry| !gone.contains(&entry.path())) {
        return Ok(false);
    }
    if !dry_run {
        remove_dir(dir)?;
        log::info!("removed the emptied directory {}", dir.display());
    }
    Ok(true)
}

fn remove_file(path: &Path) -> Result<(), CacheError> {
    match fs::remove_file(path) {
        Ok(()) => Ok(()),
        Err(source) if source.kind() == io::ErrorKind::NotFound => {
            log::debug!("{}: already gone", path.display());
            Ok(())
        }
        Err(source) => Err(CacheError::Io {
            path: path.to_path_buf(),
            source,
        }),
    }
}

fn remove_dir(path: &Path) -> Result<(), CacheError> {
    fs::remove_dir(path).map_err(|source| CacheError::Io {
        path: path.to_path_buf(),
        source,
    })
}

/// A size for people: `512 B`, `3.9 MB`, `57.7 MB`, rounded to one decimal in decimal
/// units, like the dataset's own documentation.
pub fn format_bytes(bytes: u64) -> String {
    const UNITS: [&str; 5] = ["KB", "MB", "GB", "TB", "PB"];
    if bytes < 1000 {
        return format!("{bytes} B");
    }
    let tenths_in = |divisor: u128| (u128::from(bytes) * 10 + divisor / 2) / divisor;
    let mut divisor: u128 = 1000;
    let mut unit = 0;
    let mut tenths = tenths_in(divisor);
    // Rounding can carry into the next unit (999,950 B is 1.0 MB, not 1000.0 KB).
    while tenths >= 10_000 && unit + 1 < UNITS.len() {
        unit += 1;
        divisor *= 1000;
        tenths = tenths_in(divisor);
    }
    format!("{}.{} {}", tenths / 10, tenths % 10, UNITS[unit])
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::caida::INDEX_FILE;
    use jiff::civil::date;

    /// A cache with two IPv4 months and one IPv6 month, plus the things a prune must
    /// leave alone: both indexes, a stray file in a month, a non-layout directory.
    fn seed(root: &Path) {
        let v4 = root.join(Family::V4.dataset());
        let v6 = root.join(Family::V6.dataset());
        for (dir, name, size) in [
            (v4.join("2015/01"), "routeviews-rv2-20150101-1200.pfx2as.gz", 10),
            (v4.join("2015/01"), "routeviews-rv2-20150102-1200.pfx2as.gz", 20),
            (v4.join("2026/10"), "routeviews-rv2-20261020-1000.pfx2as.gz", 30),
            (v4.join("2026/10"), "routeviews-rv2-20261021-0800.pfx2as.gz", 40),
            (v6.join("2026/10"), "routeviews-rv6-20261021-1200.pfx2as.gz", 50),
        ] {
            fs::create_dir_all(&dir).unwrap();
            fs::write(dir.join(name), vec![b'x'; size]).unwrap();
        }
        fs::write(
            v4.join("2015/01").join(LISTING_FILE),
            "routeviews-rv2-20150101-1200.pfx2as.gz\n",
        )
        .unwrap();
        fs::write(v4.join("2026/10").join(LISTING_FILE), "").unwrap();
        fs::write(
            v4.join(INDEX_FILE),
            "6777\t1\t2026/10/routeviews-rv2-20261021-0800.pfx2as.gz\n",
        )
        .unwrap();
        fs::write(
            v6.join(INDEX_FILE),
            "4947\t1\t2026/10/routeviews-rv6-20261021-1200.pfx2as.gz\n",
        )
        .unwrap();
        fs::write(v6.join("2026/10/.download-abc"), "partial").unwrap();
        fs::create_dir_all(root.join("something-else")).unwrap();
        fs::write(root.join("something-else/file"), "not ours").unwrap();
    }

    fn dates(snapshots: &[&CachedSnapshot]) -> Vec<Date> {
        snapshots.iter().map(|snapshot| snapshot.date).collect()
    }

    #[test]
    fn scan_finds_snapshots_and_sizes() {
        let dir = tempfile::tempdir().unwrap();
        seed(dir.path());
        let inventory = Inventory::scan(dir.path()).unwrap();
        let v4 = inventory.family(Family::V4).unwrap();
        assert_eq!(
            dates(&v4.snapshots.iter().collect::<Vec<_>>()),
            [
                date(2015, 1, 1),
                date(2015, 1, 2),
                date(2026, 10, 20),
                date(2026, 10, 21)
            ]
        );
        assert_eq!(v4.snapshot_bytes(), 100);
        // Snapshots, two listings (39 and 0 bytes) and the index (54 bytes).
        assert_eq!(v4.bytes, 100 + 39 + 54);
        let v6 = inventory.family(Family::V6).unwrap();
        assert_eq!(v6.snapshots.len(), 1);
        assert_eq!(
            v6.snapshots[0].file_name(),
            "routeviews-rv6-20261021-1200.pfx2as.gz"
        );
        // The partial download counts as bytes but is not a snapshot.
        assert_eq!(v6.bytes, 50 + "partial".len() as u64 + 54);
        assert_eq!(inventory.snapshots().count(), 5);
        assert_eq!(inventory.bytes(), v4.bytes + v6.bytes);
    }

    #[test]
    fn a_missing_cache_is_empty() {
        let dir = tempfile::tempdir().unwrap();
        let inventory = Inventory::scan(&dir.path().join("nope")).unwrap();
        assert_eq!(inventory.snapshots().count(), 0);
        assert_eq!(inventory.bytes(), 0);
        assert_eq!(inventory.families.len(), 2);
    }

    #[test]
    fn retention_rules_are_bounds() {
        let dir = tempfile::tempdir().unwrap();
        seed(dir.path());
        let inventory = Inventory::scan(dir.path()).unwrap();
        let v4 = inventory.family(Family::V4).unwrap();
        let today = date(2026, 10, 22);

        assert!(Retention::default().is_empty());
        assert_eq!(
            Retention::default().expired(v4, today),
            Vec::<&CachedSnapshot>::new()
        );

        let by_count = Retention {
            keep_last: Some(3),
            ..Retention::default()
        };
        assert_eq!(dates(&by_count.expired(v4, today)), [date(2015, 1, 1)]);

        // "Older than 2 days": 2026-10-20 is exactly two days old and stays.
        let by_age = Retention {
            keep_days: Some(2),
            ..Retention::default()
        };
        assert_eq!(
            dates(&by_age.expired(v4, today)),
            [date(2015, 1, 1), date(2015, 1, 2)]
        );
        let today_only = Retention {
            keep_days: Some(0),
            ..Retention::default()
        };
        assert_eq!(today_only.expired(v4, today).len(), 4);

        // Both: the newest three that are also within one day, so only 2026-10-21 stays.
        let both = Retention {
            keep_days: Some(1),
            keep_last: Some(3),
        };
        assert_eq!(
            dates(&both.expired(v4, today)),
            [date(2015, 1, 1), date(2015, 1, 2), date(2026, 10, 20)]
        );
        let none = Retention {
            keep_last: Some(0),
            ..Retention::default()
        };
        assert_eq!(none.expired(v4, today).len(), 4);
        assert_eq!(
            Retention {
                keep_last: Some(10),
                ..Retention::default()
            }
            .expired(v4, today)
            .len(),
            0
        );
    }

    #[test]
    fn dry_run_reports_and_removes_nothing() {
        let dir = tempfile::tempdir().unwrap();
        seed(dir.path());
        let before = Inventory::scan(dir.path()).unwrap();
        let pruned = prune(
            &before,
            Retention {
                keep_last: Some(1),
                ..Retention::default()
            },
            date(2026, 10, 22),
            true,
        )
        .unwrap();
        assert!(pruned.dry_run);
        assert_eq!(pruned.snapshots.len(), 3);
        assert_eq!(pruned.bytes(), 60);
        let v4 = dir.path().join(Family::V4.dataset());
        assert_eq!(pruned.directories, [v4.join("2015/01"), v4.join("2015")]);
        assert_eq!(Inventory::scan(dir.path()).unwrap(), before);
    }

    #[test]
    fn prune_removes_snapshots_then_emptied_directories_only() {
        let dir = tempfile::tempdir().unwrap();
        seed(dir.path());
        let inventory = Inventory::scan(dir.path()).unwrap();
        let pruned = prune(
            &inventory,
            Retention {
                keep_last: Some(1),
                ..Retention::default()
            },
            date(2026, 10, 22),
            false,
        )
        .unwrap();
        assert!(!pruned.dry_run);
        assert_eq!(
            dates(&pruned.snapshots.iter().collect::<Vec<_>>()),
            [date(2015, 1, 1), date(2015, 1, 2), date(2026, 10, 20)]
        );
        let v4 = dir.path().join(Family::V4.dataset());
        let v6 = dir.path().join(Family::V6.dataset());
        // January 2015 lost both snapshots: its listing, the month and the year are gone.
        assert_eq!(pruned.directories, [v4.join("2015/01"), v4.join("2015")]);
        assert!(!v4.join("2015").exists());
        // October 2026 keeps a snapshot, so its listing stays; the indexes are untouched.
        assert!(v4.join("2026/10").join(LISTING_FILE).is_file());
        assert!(
            v4.join("2026/10/routeviews-rv2-20261021-0800.pfx2as.gz")
                .is_file()
        );
        assert!(!v4.join("2026/10/routeviews-rv2-20261020-1000.pfx2as.gz").exists());
        assert!(v4.join(INDEX_FILE).is_file());
        assert!(v6.join(INDEX_FILE).is_file());
        assert!(
            v6.join("2026/10/routeviews-rv6-20261021-1200.pfx2as.gz")
                .is_file()
        );
        assert!(dir.path().join("something-else/file").is_file());

        let after = Inventory::scan(dir.path()).unwrap();
        assert_eq!(after.snapshots().count(), 2);
        assert_eq!(after.family(Family::V4).unwrap().bytes, 40 + 54);

        // Removing everything: the IPv6 month keeps its partial download, so the
        // directory stays, and the IPv4 month with only a listing left goes.
        let pruned = prune(
            &after,
            Retention {
                keep_last: Some(0),
                ..Retention::default()
            },
            date(2026, 10, 22),
            false,
        )
        .unwrap();
        assert_eq!(pruned.snapshots.len(), 2);
        assert_eq!(pruned.directories, [v4.join("2026/10"), v4.join("2026")]);
        assert!(!v4.join("2026").exists());
        assert!(v4.join(INDEX_FILE).is_file());
        assert!(v6.join("2026/10/.download-abc").is_file());
        assert_eq!(Inventory::scan(dir.path()).unwrap().snapshots().count(), 0);
    }

    #[test]
    fn a_snapshot_already_gone_is_not_an_error() {
        let dir = tempfile::tempdir().unwrap();
        seed(dir.path());
        let inventory = Inventory::scan(dir.path()).unwrap();
        let v4 = dir.path().join(Family::V4.dataset());
        fs::remove_file(v4.join("2015/01/routeviews-rv2-20150101-1200.pfx2as.gz")).unwrap();
        let pruned = prune(
            &inventory,
            Retention {
                keep_last: Some(2),
                ..Retention::default()
            },
            date(2026, 10, 22),
            false,
        )
        .unwrap();
        assert_eq!(pruned.snapshots.len(), 2);
        assert!(!v4.join("2015").exists());
    }

    #[test]
    fn sizes_for_people() {
        assert_eq!(format_bytes(0), "0 B");
        assert_eq!(format_bytes(999), "999 B");
        assert_eq!(format_bytes(1000), "1.0 KB");
        assert_eq!(format_bytes(2572), "2.6 KB");
        assert_eq!(format_bytes(907_871), "907.9 KB");
        assert_eq!(format_bytes(999_950), "1.0 MB");
        assert_eq!(format_bytes(3_894_592), "3.9 MB");
        assert_eq!(format_bytes(57_712_345), "57.7 MB");
        assert_eq!(format_bytes(1_000_000_000), "1.0 GB");
        assert_eq!(format_bytes(u64::MAX), "18446.7 PB");
    }
}
