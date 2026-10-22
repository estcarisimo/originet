//! Filling a [`Table`] from a local file or from CAIDA, with the provenance of each load.

use std::path::Path;

use jiff::civil::Date;

use crate::caida::{self, DATASET_START_YEAR, Family, SelectError, Snapshot, previous_month};
use crate::fetch::{FetchError, Fetcher};
use crate::output::{SnapshotInfo, Source};
use crate::pfx2as::ReadError;
use crate::table::Table;

/// Why a table could not be loaded.
#[derive(Debug, thiserror::Error)]
pub enum LoadError {
    /// The file could not be read or parsed.
    #[error("loading {path}: {source}")]
    Read {
        /// The file.
        path: String,
        /// What went wrong.
        #[source]
        source: ReadError,
    },
    /// The file has no prefixes at all.
    #[error("{path} has no {family} prefixes; is it the right file?")]
    WrongFamily {
        /// The file.
        path: String,
        /// The family that was expected.
        family: Family,
    },
    /// The index or the snapshot could not be fetched.
    #[error(transparent)]
    Fetch(#[from] FetchError),
    /// No snapshot matched the requested date.
    #[error(transparent)]
    Select(#[from] SelectError),
}

fn load_file(table: &mut Table, family: Family, path: &Path) -> Result<usize, LoadError> {
    let prefixes = table
        .load_path_family(path, family)
        .map_err(|source| LoadError::Read {
            path: path.display().to_string(),
            source,
        })?;
    if prefixes == 0 {
        return Err(LoadError::WrongFamily {
            path: path.display().to_string(),
            family,
        });
    }
    Ok(prefixes)
}

/// Loads a local `pfx2as` file (plain or gzip) for one address family. The snapshot
/// date is taken from the file name when it follows CAIDA's naming.
pub fn load_local(table: &mut Table, family: Family, path: &Path) -> Result<SnapshotInfo, LoadError> {
    let prefixes = load_file(table, family, path)?;
    let file = path.file_name().map_or_else(
        || path.display().to_string(),
        |name| name.to_string_lossy().into_owned(),
    );
    Ok(SnapshotInfo {
        family,
        source: Source::Local,
        date: caida::date_from_file_name(&file).map(|date| date.to_string()),
        file,
        location: path.display().to_string(),
        prefixes,
    })
}

/// How many monthly listings `select_snapshot` walks back from the requested month.
pub const MAX_MONTHS_BACK: u32 = 12;

/// Resolves which snapshot answers for `date`: the newest overall when `None`, else the
/// newest taken on or before the date. Recent dates come from CAIDA's rolling index;
/// older ones from the monthly directory listings, walking back month by month.
pub fn select_snapshot(fetcher: &Fetcher, family: Family, date: Option<Date>) -> Result<Snapshot, LoadError> {
    let index = fetcher.index(family)?;
    let Some(requested) = date else {
        return Ok(index.latest().clone());
    };
    if index.covers(requested)
        && let Some(snapshot) = index.on_or_before(requested)
    {
        return Ok(snapshot.clone());
    }
    if requested.year() < DATASET_START_YEAR {
        return Err(SelectError::BeforeDataset { family, requested }.into());
    }
    let mut year_month = (requested.year(), requested.month());
    let mut searched = 0;
    while searched < MAX_MONTHS_BACK && year_month.0 >= DATASET_START_YEAR {
        let listing = fetcher.listing(family, year_month)?;
        if let Some(snapshot) = caida::newest_on_or_before(&listing, requested) {
            return Ok(snapshot.clone());
        }
        searched += 1;
        year_month = previous_month(year_month);
    }
    let (year, month) = if searched == 0 {
        year_month
    } else {
        previous_month_inverse(year_month)
    };
    Err(SelectError::NoSnapshot {
        family,
        requested,
        months: searched,
        year,
        month,
    }
    .into())
}

/// The month after `(year, month)`: the last month actually searched, for the error message.
fn previous_month_inverse((year, month): (i16, i8)) -> (i16, i8) {
    if month == 12 {
        (year + 1, 1)
    } else {
        (year, month + 1)
    }
}

/// Resolves the snapshot for `date` (see [`select_snapshot`]), fetches it and loads it.
pub fn load_caida(
    table: &mut Table,
    fetcher: &Fetcher,
    family: Family,
    date: Option<Date>,
) -> Result<SnapshotInfo, LoadError> {
    let snapshot = select_snapshot(fetcher, family, date)?;
    if let Some(requested) = date
        && snapshot.date != requested
    {
        log::warn!(
            "no {family} snapshot on {requested}; using the closest earlier one, {}",
            snapshot.date
        );
    }
    let path = fetcher.snapshot(&snapshot)?;
    let prefixes = load_file(table, family, &path)?;
    Ok(SnapshotInfo {
        family,
        source: Source::Caida,
        date: Some(snapshot.date.to_string()),
        file: snapshot.file_name().to_owned(),
        location: snapshot.url(),
        prefixes,
    })
}

#[cfg(test)]
mod tests {
    use super::*;
    use jiff::civil::date;
    use std::fs;

    fn seeded_cache(dir: &Path) -> Fetcher {
        let dataset = dir.join("routeviews-prefix2as");
        fs::create_dir_all(dataset.join("2024/01")).unwrap();
        fs::create_dir_all(dataset.join("2024/02")).unwrap();
        fs::create_dir_all(dataset.join("2023/12")).unwrap();
        fs::write(
            dataset.join("pfx2as-creation.log"),
            "6776\t1792599333\t2026/10/routeviews-rv2-20261020-1000.pfx2as.gz\n6777\t1792685687\t2026/10/routeviews-rv2-20261021-0800.pfx2as.gz\n",
        )
        .unwrap();
        fs::write(
            dataset.join("2024/01/.listing"),
            "routeviews-rv2-20240101-1200.pfx2as.gz\nrouteviews-rv2-20240115-1200.pfx2as.gz\nrouteviews-rv2-20240131-1200.pfx2as.gz\n",
        )
        .unwrap();
        fs::write(dataset.join("2024/02/.listing"), "").unwrap();
        fs::write(dataset.join("2023/12/.listing"), "").unwrap();
        Fetcher::new(dir.to_path_buf()).offline(true)
    }

    #[test]
    fn selects_from_the_index_or_the_listings() {
        let dir = tempfile::tempdir().unwrap();
        let fetcher = seeded_cache(dir.path());
        let pick = |date| select_snapshot(&fetcher, Family::V4, date).map(|s| s.path);
        assert_eq!(
            pick(None).unwrap(),
            "2026/10/routeviews-rv2-20261021-0800.pfx2as.gz"
        );
        assert_eq!(
            pick(Some(date(2026, 10, 20))).unwrap(),
            "2026/10/routeviews-rv2-20261020-1000.pfx2as.gz"
        );
        // Inside January 2024: the newest on or before the day.
        assert_eq!(
            pick(Some(date(2024, 1, 20))).unwrap(),
            "2024/01/routeviews-rv2-20240115-1200.pfx2as.gz"
        );
        assert_eq!(
            pick(Some(date(2024, 1, 1))).unwrap(),
            "2024/01/routeviews-rv2-20240101-1200.pfx2as.gz"
        );
        // February 2024 is empty: walk back to January's last snapshot.
        assert_eq!(
            pick(Some(date(2024, 2, 10))).unwrap(),
            "2024/01/routeviews-rv2-20240131-1200.pfx2as.gz"
        );
        // Before the dataset.
        assert!(matches!(
            pick(Some(date(2004, 6, 1))),
            Err(LoadError::Select(SelectError::BeforeDataset { .. }))
        ));
        // December 2023 is empty and November is not cached: offline stops there.
        assert!(matches!(pick(Some(date(2023, 12, 5))), Err(LoadError::Fetch(_))));
    }

    #[test]
    fn local_load_reports_provenance() {
        let dir = tempfile::tempdir().unwrap();
        let path = dir.path().join("routeviews-rv2-20240102-1200.pfx2as");
        fs::write(&path, "8.8.8.0\t24\t15169\n").unwrap();
        let mut table = Table::new();
        let info = load_local(&mut table, Family::V4, &path).unwrap();
        assert_eq!(info.prefixes, 1);
        assert_eq!(info.date.as_deref(), Some("2024-01-02"));
        assert_eq!(info.source, Source::Local);
        assert_eq!(info.file, "routeviews-rv2-20240102-1200.pfx2as");
        let error = load_local(&mut table, Family::V6, &path).unwrap_err();
        assert!(matches!(error, LoadError::Read { .. }), "{error}");
        assert!(
            error
                .to_string()
                .contains("expected an ipv6 prefix, got 8.8.8.0/24"),
            "{error}"
        );
        assert_eq!(table.len_v6(), 0);
        let empty = dir.path().join("routeviews-rv2-20240103-1200.pfx2as");
        fs::write(&empty, "# nothing\n").unwrap();
        assert!(matches!(
            load_local(&mut table, Family::V4, &empty),
            Err(LoadError::WrongFamily {
                family: Family::V4,
                ..
            })
        ));
        assert!(matches!(
            load_local(&mut table, Family::V4, &dir.path().join("missing")),
            Err(LoadError::Read { .. })
        ));
    }
}
