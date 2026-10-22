//! Layout of CAIDA's RouteViews prefix-to-AS datasets and snapshot selection.
//!
//! CAIDA publishes one snapshot per day under
//! `https://publicdata.caida.org/datasets/routing/routeviews-prefix2as/YYYY/MM/`
//! (IPv4, files named `routeviews-rv2-YYYYMMDD-HHMM.pfx2as.gz`) and
//! `.../routeviews6-prefix2as/YYYY/MM/` (IPv6, `routeviews-rv6-...`). Each dataset
//! has an index, `pfx2as-creation.log`, listing every file ever published as
//! `seqnum<TAB>unix-timestamp<TAB>YYYY/MM/file`, but it is a rolling window of the
//! last thirty days or so. Older snapshots are found through the monthly directory
//! listings (`YYYY/MM/`), which this module parses too.

use std::fmt;

use jiff::civil::Date;

/// The address family of a dataset.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, serde::Serialize)]
pub enum Family {
    /// `routeviews-prefix2as`, IPv4.
    #[serde(rename = "ipv4")]
    V4,
    /// `routeviews6-prefix2as`, IPv6.
    #[serde(rename = "ipv6")]
    V6,
}

impl Family {
    /// The dataset directory name on `publicdata.caida.org`.
    pub fn dataset(self) -> &'static str {
        match self {
            Family::V4 => "routeviews-prefix2as",
            Family::V6 => "routeviews6-prefix2as",
        }
    }

    /// Base URL of the dataset, with a trailing slash.
    pub fn base_url(self) -> String {
        format!(
            "https://publicdata.caida.org/datasets/routing/{}/",
            self.dataset()
        )
    }

    /// URL of the dataset's index file.
    pub fn index_url(self) -> String {
        format!("{}{}", self.base_url(), INDEX_FILE)
    }

    /// `ipv4` or `ipv6`.
    pub fn label(self) -> &'static str {
        match self {
            Family::V4 => "ipv4",
            Family::V6 => "ipv6",
        }
    }
}

impl fmt::Display for Family {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.write_str(self.label())
    }
}

/// Name of the index file inside each dataset directory.
pub const INDEX_FILE: &str = "pfx2as-creation.log";

/// The year the IPv4 dataset begins (May 2005; IPv6 starts in 2007). Searches never go
/// further back.
pub const DATASET_START_YEAR: i16 = 2005;

/// A `(year, month)` pair.
pub type YearMonth = (i16, i8);

/// The month before `(year, month)`.
pub fn previous_month((year, month): YearMonth) -> YearMonth {
    if month == 1 {
        (year - 1, 12)
    } else {
        (year, month - 1)
    }
}

/// URL of a month's directory listing.
pub fn listing_url(family: Family, (year, month): YearMonth) -> String {
    format!("{}{year:04}/{month:02}/", family.base_url())
}

/// Parses the HTML directory listing of one month (`YYYY/MM/`) into snapshots, oldest
/// first. Only `href`s naming a `*.pfx2as.gz` file with a date are kept; everything else
/// in the page (sort links, parent directory, sizes) is ignored.
pub fn parse_listing(family: Family, (year, month): YearMonth, html: &str) -> Vec<Snapshot> {
    let mut snapshots: Vec<Snapshot> = html
        .split("href=\"")
        .skip(1)
        .filter_map(|rest| rest.split('"').next())
        .filter(|name| name.ends_with(".pfx2as.gz") && !name.contains('/'))
        .filter_map(|name| {
            let date = date_from_file_name(name)?;
            Some(Snapshot {
                family,
                seqnum: None,
                created: None,
                path: format!("{year:04}/{month:02}/{name}"),
                date,
            })
        })
        .collect();
    snapshots.sort_by(|a, b| a.date.cmp(&b.date).then(a.path.cmp(&b.path)));
    snapshots.dedup_by(|a, b| a.path == b.path);
    snapshots
}

/// Reads the file names of a cached listing (one per line) back into snapshots.
pub fn parse_cached_listing(family: Family, (year, month): YearMonth, text: &str) -> Vec<Snapshot> {
    let html =
        text.lines()
            .map(str::trim)
            .filter(|line| !line.is_empty())
            .fold(String::new(), |mut html, name| {
                html.push_str("href=\"");
                html.push_str(name);
                html.push('"');
                html
            });
    parse_listing(family, (year, month), &html)
}

/// The newest snapshot on or before `date` among `snapshots` (any order).
pub fn newest_on_or_before(snapshots: &[Snapshot], date: Date) -> Option<&Snapshot> {
    snapshots
        .iter()
        .filter(|snapshot| snapshot.date <= date)
        .max_by(|a, b| a.date.cmp(&b.date).then(a.path.cmp(&b.path)))
}

/// One published snapshot, as listed in the index.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Snapshot {
    /// Which dataset it belongs to.
    pub family: Family,
    /// Sequence number in the index; unknown for a snapshot found through a listing.
    pub seqnum: Option<u64>,
    /// When CAIDA created the file (Unix seconds); unknown for a listing.
    pub created: Option<i64>,
    /// Path relative to the dataset directory, e.g. `2026/10/routeviews-rv2-20261021-0800.pfx2as.gz`.
    pub path: String,
    /// The day the snapshot was taken, parsed from the file name.
    pub date: Date,
}

impl Snapshot {
    /// The file name without directories.
    pub fn file_name(&self) -> &str {
        self.path.rsplit('/').next().unwrap_or(&self.path)
    }

    /// Full download URL.
    pub fn url(&self) -> String {
        format!("{}{}", self.family.base_url(), self.path)
    }
}

/// Parses the date out of a snapshot file name like `routeviews-rv2-20261021-0800.pfx2as.gz`.
pub fn date_from_file_name(name: &str) -> Option<Date> {
    name.split('-')
        .find(|part| part.len() == 8 && part.bytes().all(|b| b.is_ascii_digit()))
        .and_then(|digits| {
            let year: i16 = digits[0..4].parse().ok()?;
            let month: i8 = digits[4..6].parse().ok()?;
            let day: i8 = digits[6..8].parse().ok()?;
            Date::new(year, month, day).ok()
        })
}

/// Why the index could not be parsed.
#[derive(Debug, thiserror::Error, PartialEq, Eq)]
pub enum IndexError {
    /// A data line without three tab-separated fields.
    #[error("index line {line}: expected `seqnum<TAB>timestamp<TAB>path`, got {text:?}")]
    Fields {
        /// 1-based line number.
        line: usize,
        /// The offending line.
        text: String,
    },
    /// A file name without a `YYYYMMDD` date.
    #[error("index line {line}: no date in file name {path:?}")]
    Date {
        /// 1-based line number.
        line: usize,
        /// The path field.
        path: String,
    },
    /// The index has no entries at all.
    #[error("the {0} index is empty")]
    Empty(Family),
}

/// The parsed index of a dataset, sorted by date.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Index {
    family: Family,
    snapshots: Vec<Snapshot>,
}

impl Index {
    /// Parses `pfx2as-creation.log`. Comment lines start with `#`.
    pub fn parse(family: Family, text: &str) -> Result<Self, IndexError> {
        let mut snapshots = Vec::new();
        for (index, line) in text.lines().enumerate() {
            let line = line.trim_end();
            if line.is_empty() || line.starts_with('#') {
                continue;
            }
            let number = index + 1;
            let mut fields = line.split('\t');
            let (Some(seqnum), Some(created), Some(path)) = (fields.next(), fields.next(), fields.next())
            else {
                return Err(IndexError::Fields {
                    line: number,
                    text: line.to_owned(),
                });
            };
            let fields_error = || IndexError::Fields {
                line: number,
                text: line.to_owned(),
            };
            let seqnum = Some(seqnum.trim().parse().map_err(|_| fields_error())?);
            let created = Some(created.trim().parse().map_err(|_| fields_error())?);
            let path = path.trim().to_owned();
            let date =
                date_from_file_name(path.rsplit('/').next().unwrap_or(&path)).ok_or(IndexError::Date {
                    line: number,
                    path: path.clone(),
                })?;
            snapshots.push(Snapshot {
                family,
                seqnum,
                created,
                path,
                date,
            });
        }
        if snapshots.is_empty() {
            return Err(IndexError::Empty(family));
        }
        // The log is chronological already; sorting makes the selection below robust
        // to a reordered line and keeps "last of a day" deterministic.
        snapshots.sort_by(|a, b| a.date.cmp(&b.date).then(a.seqnum.cmp(&b.seqnum)));
        snapshots.dedup_by(|a, b| a.path == b.path);
        Ok(Self { family, snapshots })
    }

    /// The dataset this index describes.
    pub fn family(&self) -> Family {
        self.family
    }

    /// All snapshots, oldest first.
    pub fn snapshots(&self) -> &[Snapshot] {
        &self.snapshots
    }

    /// The newest snapshot.
    pub fn latest(&self) -> &Snapshot {
        self.snapshots.last().expect("an index is never empty")
    }

    /// The oldest snapshot.
    pub fn earliest(&self) -> &Snapshot {
        self.snapshots.first().expect("an index is never empty")
    }

    /// The newest snapshot taken on or before `date`, if any.
    pub fn on_or_before(&self, date: Date) -> Option<&Snapshot> {
        let end = self.snapshots.partition_point(|snapshot| snapshot.date <= date);
        self.snapshots[..end].last()
    }

    /// Whether `date` falls inside the window the index covers (on or after its first entry).
    pub fn covers(&self, date: Date) -> bool {
        date >= self.earliest().date
    }
}

/// Why no snapshot matched the requested date.
#[derive(Debug, thiserror::Error, PartialEq, Eq)]
pub enum SelectError {
    /// The date is earlier than the dataset itself.
    #[error("no {family} snapshot on or before {requested}: the dataset starts in {DATASET_START_YEAR}")]
    BeforeDataset {
        /// The dataset.
        family: Family,
        /// The requested date.
        requested: Date,
    },
    /// No month within the search window had a snapshot on or before the date.
    #[error(
        "no {family} snapshot on or before {requested} in the {months} months back to {year:04}-{month:02}"
    )]
    NoSnapshot {
        /// The dataset.
        family: Family,
        /// The requested date.
        requested: Date,
        /// How many months were searched.
        months: u32,
        /// The earliest year searched.
        year: i16,
        /// The earliest month searched.
        month: i8,
    },
}

#[cfg(test)]
mod tests {
    use super::*;
    use jiff::civil::date;

    const LOG: &str = "# Format: 1\n# Fields: seqnum timestamp path\n\
        1\t1117584000\t2005/05/routeviews-rv2-20050601-0000.pfx2as.gz\n\
        6776\t1792599333\t2026/10/routeviews-rv2-20261020-1000.pfx2as.gz\n\
        6777\t1792685687\t2026/10/routeviews-rv2-20261021-0800.pfx2as.gz\n";

    #[test]
    fn parses_the_index() {
        let index = Index::parse(Family::V4, LOG).unwrap();
        assert_eq!(index.snapshots().len(), 3);
        let latest = index.latest();
        assert_eq!(latest.seqnum, Some(6777));
        assert_eq!(latest.created, Some(1_792_685_687));
        assert_eq!(latest.date, date(2026, 10, 21));
        assert_eq!(latest.file_name(), "routeviews-rv2-20261021-0800.pfx2as.gz");
        assert_eq!(
            latest.url(),
            "https://publicdata.caida.org/datasets/routing/routeviews-prefix2as/2026/10/routeviews-rv2-20261021-0800.pfx2as.gz"
        );
        assert_eq!(index.earliest().date, date(2005, 6, 1));
    }

    #[test]
    fn selects_on_or_before() {
        let index = Index::parse(Family::V4, LOG).unwrap();
        assert_eq!(index.latest().seqnum, Some(6777));
        assert_eq!(index.on_or_before(date(2026, 10, 21)).unwrap().seqnum, Some(6777));
        assert_eq!(index.on_or_before(date(2026, 10, 20)).unwrap().seqnum, Some(6776));
        assert_eq!(index.on_or_before(date(2020, 1, 1)).unwrap().seqnum, Some(1));
        assert_eq!(index.on_or_before(date(2030, 1, 1)).unwrap().seqnum, Some(6777));
        assert_eq!(index.on_or_before(date(2005, 5, 31)), None);
        assert!(index.covers(date(2005, 6, 1)));
        assert!(!index.covers(date(2005, 5, 31)));
    }

    #[test]
    fn parses_a_monthly_listing() {
        let html = include_str!("../tests/fixtures/listing-2015-01.html");
        let snapshots = parse_listing(Family::V4, (2015, 1), html);
        assert!(snapshots.len() >= 5, "{}", snapshots.len());
        assert_eq!(
            snapshots[0].path,
            "2015/01/routeviews-rv2-20150101-1200.pfx2as.gz"
        );
        assert_eq!(snapshots[0].date, date(2015, 1, 1));
        assert_eq!(snapshots[0].seqnum, None);
        assert_eq!(
            snapshots[0].url(),
            "https://publicdata.caida.org/datasets/routing/routeviews-prefix2as/2015/01/routeviews-rv2-20150101-1200.pfx2as.gz"
        );
        assert!(snapshots.windows(2).all(|pair| pair[0].date < pair[1].date));

        // The sort links, the parent directory and anything that is not a snapshot are dropped.
        assert!(
            parse_listing(
                Family::V4,
                (2015, 1),
                "<a href=\"?C=N;O=D\">Name</a><a href=\"/x/2015/\">Parent</a>"
            )
            .is_empty()
        );
        // The oldest files use another prefix; the date is still found.
        let old = parse_listing(
            Family::V4,
            (2005, 5),
            "<a href=\"routeviews-oix-20050509-1200.pfx2as.gz\">x</a>",
        );
        assert_eq!(old[0].date, date(2005, 5, 9));

        let cached = parse_cached_listing(
            Family::V4,
            (2015, 1),
            "routeviews-rv2-20150102-1200.pfx2as.gz\nrouteviews-rv2-20150101-1200.pfx2as.gz\n",
        );
        assert_eq!(cached.len(), 2);
        assert_eq!(
            newest_on_or_before(&cached, date(2015, 1, 1)).unwrap().date,
            date(2015, 1, 1)
        );
        assert_eq!(
            newest_on_or_before(&cached, date(2015, 1, 9)).unwrap().date,
            date(2015, 1, 2)
        );
        assert_eq!(newest_on_or_before(&cached, date(2014, 12, 31)), None);
    }

    #[test]
    fn month_arithmetic_and_urls() {
        assert_eq!(previous_month((2026, 1)), (2025, 12));
        assert_eq!(previous_month((2026, 3)), (2026, 2));
        assert_eq!(
            listing_url(Family::V6, (2015, 1)),
            "https://publicdata.caida.org/datasets/routing/routeviews6-prefix2as/2015/01/"
        );
    }

    #[test]
    fn rejects_bad_index_lines() {
        assert!(matches!(
            Index::parse(Family::V6, "1 2 3\n"),
            Err(IndexError::Fields { line: 1, .. })
        ));
        assert!(matches!(
            Index::parse(Family::V6, "1\t2\t2026/10/routeviews-rv6.pfx2as.gz\n"),
            Err(IndexError::Date { line: 1, .. })
        ));
        assert_eq!(
            Index::parse(Family::V6, "# only comments\n"),
            Err(IndexError::Empty(Family::V6))
        );
    }

    #[test]
    fn urls_follow_the_dataset_layout() {
        assert_eq!(
            Family::V6.index_url(),
            "https://publicdata.caida.org/datasets/routing/routeviews6-prefix2as/pfx2as-creation.log"
        );
        assert_eq!(
            date_from_file_name("routeviews-rv6-20261019-1200.pfx2as.gz"),
            Some(date(2026, 10, 19))
        );
        assert_eq!(
            date_from_file_name("routeviews-rv6-20261319-1200.pfx2as.gz"),
            None
        );
        assert_eq!(date_from_file_name("whatever.gz"), None);
    }
}
