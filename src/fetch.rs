//! Downloads CAIDA's index and snapshots into a local cache.
//!
//! Snapshot files never change once published, so they are cached forever. The index
//! grows daily, so a cached copy is reused for a few hours and then refreshed. In
//! offline mode nothing is downloaded and a missing file is an error.

use std::fs;
use std::io::{self, Write};
use std::path::{Path, PathBuf};
use std::time::{Duration, SystemTime};

use crate::USER_AGENT;
use crate::caida::{self, Family, INDEX_FILE, Index, IndexError, Snapshot, YearMonth};

/// How long a cached index (or the current month's listing) is trusted before it is
/// fetched again.
pub const INDEX_MAX_AGE: Duration = Duration::from_secs(6 * 60 * 60);

/// Name of the cached, parsed listing inside a month's cache directory.
pub const LISTING_FILE: &str = ".listing";

/// What went wrong while fetching.
#[derive(Debug, thiserror::Error)]
pub enum FetchError {
    /// A download failed.
    #[error("downloading {url}: {source}")]
    Http {
        /// The URL requested.
        url: String,
        /// The transport or status error.
        #[source]
        source: Box<ureq::Error>,
    },
    /// Reading or writing the cache failed.
    #[error("{path}: {source}")]
    Io {
        /// The file involved.
        path: PathBuf,
        /// The I/O error.
        #[source]
        source: io::Error,
    },
    /// The index could not be parsed.
    #[error("{location}: {source}")]
    Index {
        /// Where the text came from: the URL when just downloaded, else the cached file.
        location: String,
        /// The parse error.
        #[source]
        source: IndexError,
    },
    /// Offline mode and the file is not in the cache.
    #[error("offline and {what} is not cached at {path}")]
    Offline {
        /// What was needed.
        what: String,
        /// Where it would be.
        path: PathBuf,
    },
}

/// Downloads with a local cache.
#[derive(Debug)]
pub struct Fetcher {
    cache_dir: PathBuf,
    offline: bool,
    refresh_index: bool,
    agent: ureq::Agent,
}

impl Fetcher {
    /// A fetcher caching under `cache_dir`.
    pub fn new(cache_dir: PathBuf) -> Self {
        let config = ureq::Agent::config_builder()
            .user_agent(USER_AGENT)
            .timeout_connect(Some(Duration::from_secs(30)))
            .timeout_global(Some(Duration::from_secs(15 * 60)))
            .build();
        Self {
            cache_dir,
            offline: false,
            refresh_index: false,
            agent: ureq::Agent::new_with_config(config),
        }
    }

    /// Never touch the network; fail when something is not cached.
    #[must_use]
    pub fn offline(mut self, offline: bool) -> Self {
        self.offline = offline;
        self
    }

    /// Re-download the index even if the cached copy is fresh.
    #[must_use]
    pub fn refresh_index(mut self, refresh: bool) -> Self {
        self.refresh_index = refresh;
        self
    }

    /// The cache directory.
    pub fn cache_dir(&self) -> &Path {
        &self.cache_dir
    }

    /// Where the index of a dataset is cached.
    pub fn index_path(&self, family: Family) -> PathBuf {
        self.cache_dir.join(family.dataset()).join(INDEX_FILE)
    }

    /// Where a month's parsed listing is cached.
    pub fn listing_path(&self, family: Family, (year, month): YearMonth) -> PathBuf {
        self.cache_dir
            .join(family.dataset())
            .join(format!("{year:04}"))
            .join(format!("{month:02}"))
            .join(LISTING_FILE)
    }

    /// Where a snapshot is cached.
    pub fn snapshot_path(&self, snapshot: &Snapshot) -> PathBuf {
        let mut path = self.cache_dir.join(snapshot.family.dataset());
        for part in snapshot.path.split('/') {
            path.push(part);
        }
        path
    }

    fn is_fresh(path: &Path, max_age: Duration) -> bool {
        fs::metadata(path)
            .and_then(|meta| meta.modified())
            .ok()
            .and_then(|modified| SystemTime::now().duration_since(modified).ok())
            .is_some_and(|age| age < max_age)
    }

    /// The dataset's index (CAIDA's rolling `pfx2as-creation.log`), from the cache when
    /// fresh, else downloaded.
    pub fn index(&self, family: Family) -> Result<Index, FetchError> {
        let path = self.index_path(family);
        let url = family.index_url();
        let cached_is_fresh = !self.refresh_index && Self::is_fresh(&path, INDEX_MAX_AGE);
        let mut location = path.display().to_string();
        if self.offline {
            if !path.is_file() {
                return Err(FetchError::Offline {
                    what: format!("the {family} index"),
                    path,
                });
            }
            log::info!("offline: using the cached {family} index at {}", path.display());
        } else if cached_is_fresh {
            log::info!("using the cached {family} index at {}", path.display());
        } else {
            match self.download(&url, &path) {
                Ok(bytes) => {
                    log::info!("downloaded the {family} index ({bytes} bytes) from {url}");
                    location = url;
                }
                Err(error) if path.is_file() => {
                    log::warn!("{error}; using the stale cached index at {}", path.display());
                }
                Err(error) => return Err(error),
            }
        }
        let text = fs::read_to_string(&path).map_err(|source| FetchError::Io {
            path: path.clone(),
            source,
        })?;
        Index::parse(family, &text).map_err(|source| FetchError::Index { location, source })
    }

    /// The snapshots listed in one month's directory, oldest first; empty when CAIDA has
    /// no directory for that month. Past months are cached forever, the current month
    /// for a few hours.
    pub fn listing(&self, family: Family, year_month: YearMonth) -> Result<Vec<Snapshot>, FetchError> {
        let path = self.listing_path(family, year_month);
        let url = caida::listing_url(family, year_month);
        // A listing is final only once it was fetched after its month ended (UTC):
        // one cached while the month was still running could miss later snapshots.
        let cached_is_final = path.is_file() && Self::fetched_after_month_end(&path, year_month);
        let cached_is_fresh = cached_is_final
            || (path.is_file() && !self.refresh_index && Self::is_fresh(&path, INDEX_MAX_AGE));
        if self.offline {
            if !path.is_file() {
                return Err(FetchError::Offline {
                    what: format!("the {family} listing for {:04}-{:02}", year_month.0, year_month.1),
                    path,
                });
            }
        } else if !cached_is_fresh {
            let html = match self.get(&url) {
                Ok(Some(html)) => html,
                Ok(None) => {
                    log::info!("{url} does not exist (no snapshots that month)");
                    String::new()
                }
                Err(error) if path.is_file() => {
                    log::warn!("{error}; using the cached listing at {}", path.display());
                    return Self::read_listing(family, year_month, &path);
                }
                Err(error) => return Err(error),
            };
            let snapshots = caida::parse_listing(family, year_month, &html);
            if snapshots.is_empty() && !html.is_empty() {
                log::warn!("{url} exists but lists no snapshot files");
            }
            let names = snapshots.iter().fold(String::new(), |mut names, snapshot| {
                names.push_str(snapshot.file_name());
                names.push('\n');
                names
            });
            Self::write_atomically(&path, names.as_bytes())?;
            log::info!("listed {} snapshots at {url}", snapshots.len());
            return Ok(snapshots);
        }
        Self::read_listing(family, year_month, &path)
    }

    /// Whether the file at `path` was written after the end of `(year, month)`, in UTC.
    fn fetched_after_month_end(path: &Path, (year, month): YearMonth) -> bool {
        let Some(modified) = fs::metadata(path).and_then(|meta| meta.modified()).ok() else {
            return false;
        };
        let Ok(modified) = jiff::Timestamp::try_from(modified) else {
            return false;
        };
        let Ok(first_of_month) = jiff::civil::Date::new(year, month, 1) else {
            return false;
        };
        let Ok(next_month) = first_of_month.checked_add(jiff::Span::new().months(1)) else {
            return false;
        };
        let Ok(month_end) = next_month.to_zoned(jiff::tz::TimeZone::UTC) else {
            return false;
        };
        modified >= month_end.timestamp()
    }

    fn read_listing(family: Family, year_month: YearMonth, path: &Path) -> Result<Vec<Snapshot>, FetchError> {
        let text = fs::read_to_string(path).map_err(|source| FetchError::Io {
            path: path.to_path_buf(),
            source,
        })?;
        log::info!("using the cached listing at {}", path.display());
        Ok(caida::parse_cached_listing(family, year_month, &text))
    }

    /// The local file of a snapshot, downloading it on first use.
    pub fn snapshot(&self, snapshot: &Snapshot) -> Result<PathBuf, FetchError> {
        let path = self.snapshot_path(snapshot);
        if path.is_file() {
            log::info!("using the cached snapshot {}", path.display());
            return Ok(path);
        }
        if self.offline {
            return Err(FetchError::Offline {
                what: format!("snapshot {}", snapshot.file_name()),
                path,
            });
        }
        let url = snapshot.url();
        let bytes = self.download(&url, &path)?;
        log::info!("downloaded {url} ({bytes} bytes) to {}", path.display());
        Ok(path)
    }

    /// Fetches a small text resource; `Ok(None)` when the server answers 404.
    fn get(&self, url: &str) -> Result<Option<String>, FetchError> {
        let http_error = |source| FetchError::Http {
            url: url.to_owned(),
            source: Box::new(source),
        };
        match self.agent.get(url).call() {
            Ok(mut response) => {
                let text = response.body_mut().read_to_string().map_err(http_error)?;
                Ok(Some(text))
            }
            Err(ureq::Error::StatusCode(404)) => Ok(None),
            Err(error) => Err(http_error(error)),
        }
    }

    /// Writes `bytes` to `path` through a temporary file in the same directory, so a
    /// reader never sees a partial file.
    fn write_atomically(path: &Path, bytes: &[u8]) -> Result<(), FetchError> {
        let io_error = |source| FetchError::Io {
            path: path.to_path_buf(),
            source,
        };
        let parent = path.parent().unwrap_or(Path::new("."));
        fs::create_dir_all(parent).map_err(io_error)?;
        let mut temporary = tempfile::Builder::new()
            .prefix(".download-")
            .tempfile_in(parent)
            .map_err(io_error)?;
        temporary.as_file_mut().write_all(bytes).map_err(io_error)?;
        temporary.persist(path).map_err(|error| io_error(error.error))?;
        Ok(())
    }

    /// Downloads `url` to `path` through a temporary file, so an interrupted download
    /// never leaves a truncated file in the cache. Returns the byte count.
    fn download(&self, url: &str, path: &Path) -> Result<u64, FetchError> {
        let io_error = |source| FetchError::Io {
            path: path.to_path_buf(),
            source,
        };
        let parent = path.parent().unwrap_or(Path::new("."));
        fs::create_dir_all(parent).map_err(io_error)?;
        let mut response = self.agent.get(url).call().map_err(|source| FetchError::Http {
            url: url.to_owned(),
            source: Box::new(source),
        })?;
        let mut body = response.body_mut().as_reader();
        let mut temporary = tempfile::Builder::new()
            .prefix(".download-")
            .tempfile_in(parent)
            .map_err(io_error)?;
        let bytes = io::copy(&mut body, temporary.as_file_mut()).map_err(|source| {
            match source.downcast::<ureq::Error>() {
                Ok(error) => FetchError::Http {
                    url: url.to_owned(),
                    source: Box::new(error),
                },
                Err(source) => io_error(source),
            }
        })?;
        temporary.persist(path).map_err(|error| io_error(error.error))?;
        Ok(bytes)
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use jiff::civil::date;

    fn snapshot() -> Snapshot {
        Snapshot {
            family: Family::V6,
            seqnum: Some(4947),
            created: Some(1_792_670_764),
            path: "2026/10/routeviews-rv6-20261021-1200.pfx2as.gz".into(),
            date: date(2026, 10, 21),
        }
    }

    #[test]
    fn cache_paths_mirror_the_dataset_layout() {
        let fetcher = Fetcher::new(PathBuf::from("/cache"));
        assert_eq!(
            fetcher.index_path(Family::V4),
            PathBuf::from("/cache/routeviews-prefix2as/pfx2as-creation.log")
        );
        assert_eq!(
            fetcher.snapshot_path(&snapshot()),
            PathBuf::from("/cache/routeviews6-prefix2as/2026/10/routeviews-rv6-20261021-1200.pfx2as.gz")
        );
    }

    #[test]
    fn offline_without_cache_is_an_error() {
        let dir = tempfile::tempdir().unwrap();
        let fetcher = Fetcher::new(dir.path().to_path_buf()).offline(true);
        assert!(matches!(
            fetcher.index(Family::V4),
            Err(FetchError::Offline { .. })
        ));
        assert!(matches!(
            fetcher.snapshot(&snapshot()),
            Err(FetchError::Offline { .. })
        ));
    }

    #[test]
    fn offline_uses_cached_files() {
        let dir = tempfile::tempdir().unwrap();
        let fetcher = Fetcher::new(dir.path().to_path_buf()).offline(true);
        let index_path = fetcher.index_path(Family::V6);
        fs::create_dir_all(index_path.parent().unwrap()).unwrap();
        // An unparsable cached index is reported under the cached file, not the URL.
        fs::write(&index_path, "1\t1\t2026/10/../escape.pfx2as.gz\n").unwrap();
        let error = fetcher.index(Family::V6).unwrap_err().to_string();
        assert!(error.starts_with(&index_path.display().to_string()), "{error}");
        fs::write(
            &index_path,
            "4947\t1792670764\t2026/10/routeviews-rv6-20261021-1200.pfx2as.gz\n",
        )
        .unwrap();
        let index = fetcher.index(Family::V6).unwrap();
        assert_eq!(index.latest().seqnum, Some(4947));

        assert!(matches!(
            fetcher.listing(Family::V6, (2024, 1)),
            Err(FetchError::Offline { .. })
        ));
        let listing_path = fetcher.listing_path(Family::V6, (2024, 1));
        fs::create_dir_all(listing_path.parent().unwrap()).unwrap();
        fs::write(&listing_path, "routeviews-rv6-20240131-1200.pfx2as.gz\n").unwrap();
        let listed = fetcher.listing(Family::V6, (2024, 1)).unwrap();
        assert_eq!(listed.len(), 1);
        assert_eq!(listed[0].path, "2024/01/routeviews-rv6-20240131-1200.pfx2as.gz");
        // Written now, long after January 2024 ended: final. A month that has not ended is not.
        assert!(Fetcher::fetched_after_month_end(&listing_path, (2024, 1)));
        assert!(!Fetcher::fetched_after_month_end(&listing_path, (2999, 1)));

        let snapshot_path = fetcher.snapshot_path(&snapshot());
        fs::create_dir_all(snapshot_path.parent().unwrap()).unwrap();
        fs::write(&snapshot_path, "2001::\t32\t6939\n").unwrap();
        assert_eq!(fetcher.snapshot(&snapshot()).unwrap(), snapshot_path);
    }
}
