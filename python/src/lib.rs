//! Python bindings: a `Table` that loads CAIDA's prefix-to-AS snapshots and answers
//! longest-prefix-match lookups one at a time or in bulk, without holding the GIL.

use std::net::IpAddr;
use std::path::PathBuf;

use pyo3::exceptions::{PyIOError, PyRuntimeError, PyValueError};
use pyo3::prelude::*;
use pyo3::types::PyDict;

use map_ip_to_asn::caida::Family;
use map_ip_to_asn::fetch::{FetchError, Fetcher};
use map_ip_to_asn::input::Item;
use map_ip_to_asn::load::{self, LoadError};
use map_ip_to_asn::output::SnapshotInfo;
use map_ip_to_asn::table;

/// The answer for one address: the longest matching prefix and its origin ASN(s).
#[pyclass(frozen, get_all, module = "map_ip_to_asn")]
#[derive(Debug, Clone)]
struct Match {
    /// The address that was looked up, normalized.
    ip: String,
    /// The longest matching prefix in CIDR notation.
    prefix: String,
    /// The origin ASN (the first one of a multi-origin or AS-set field).
    asn: u32,
    /// All origins in CAIDA's notation: `A`, `A_B` (multi-origin) or `A,B` (AS set).
    origins: String,
}

#[pymethods]
impl Match {
    /// Every origin ASN of the field, in order.
    fn asns(&self) -> Vec<u32> {
        self.origins
            .split(['_', ','])
            .filter_map(|t| t.parse().ok())
            .collect()
    }

    /// True when several ASes originate the prefix (`A_B`).
    #[getter]
    fn is_multi_origin(&self) -> bool {
        self.origins.contains('_')
    }

    /// True when the origin is an AS set (`A,B`).
    #[getter]
    fn is_as_set(&self) -> bool {
        self.origins.contains(',')
    }

    fn __repr__(&self) -> String {
        format!(
            "Match(ip={:?}, prefix={:?}, asn={}, origins={:?})",
            self.ip, self.prefix, self.asn, self.origins
        )
    }

    fn __eq__(&self, other: &Self) -> bool {
        self.ip == other.ip && self.prefix == other.prefix && self.origins == other.origins
    }

    fn __hash__(&self) -> u64 {
        use std::hash::{Hash, Hasher};
        let mut hasher = std::collections::hash_map::DefaultHasher::new();
        (&self.ip, &self.prefix, &self.origins).hash(&mut hasher);
        hasher.finish()
    }
}

fn to_match(address: IpAddr, hit: &table::Match<'_>) -> Match {
    Match {
        ip: address.to_string(),
        prefix: hit.prefix.to_string(),
        asn: hit.origins.primary(),
        origins: hit.origins.raw().to_owned(),
    }
}

fn load_error(error: LoadError) -> PyErr {
    match error {
        LoadError::Read { .. } | LoadError::WrongFamily { .. } => PyIOError::new_err(error.to_string()),
        LoadError::Fetch(FetchError::Io { .. } | FetchError::Offline { .. }) => {
            PyIOError::new_err(error.to_string())
        }
        LoadError::Select(_) => PyValueError::new_err(error.to_string()),
        LoadError::Fetch(_) => PyRuntimeError::new_err(error.to_string()),
    }
}

fn parse_address(text: &str) -> PyResult<IpAddr> {
    match Item::parse(text) {
        Item::Address(address) => Ok(address),
        Item::Invalid { text, reason } => Err(PyValueError::new_err(format!("{text:?}: {reason}"))),
    }
}

/// A prefix-to-origin table for IPv4 and IPv6 with longest-prefix-match lookups.
///
/// Build one with `Table.from_files(...)` for local `pfx2as` files or
/// `Table.from_caida(...)` to download (and cache) CAIDA's snapshots.
#[pyclass(frozen, module = "map_ip_to_asn")]
struct Table {
    inner: table::Table,
    snapshots: Vec<SnapshotInfo>,
}

#[pymethods]
impl Table {
    /// Loads local `pfx2as` files (plain or gzip). Give one or both families.
    #[staticmethod]
    #[pyo3(signature = (ipv4=None, ipv6=None))]
    fn from_files(ipv4: Option<PathBuf>, ipv6: Option<PathBuf>) -> PyResult<Self> {
        if ipv4.is_none() && ipv6.is_none() {
            return Err(PyValueError::new_err("give at least one of ipv4= or ipv6="));
        }
        let mut inner = table::Table::new();
        let mut snapshots = Vec::new();
        for (family, path) in [(Family::V4, ipv4), (Family::V6, ipv6)] {
            if let Some(path) = path {
                snapshots.push(load::load_local(&mut inner, family, &path).map_err(load_error)?);
            }
        }
        Ok(Self { inner, snapshots })
    }

    /// Downloads CAIDA's snapshot for `date` (``YYYY-MM-DD``; the newest on or before
    /// it, or the newest overall when ``None``) into `cache_dir` and loads it.
    ///
    /// Parameters
    /// ----------
    /// date : str | None
    ///     Snapshot day. ``None`` means the newest available.
    /// cache_dir : str | os.PathLike | None
    ///     Where indexes and snapshots are kept. ``None`` uses the platform cache
    ///     directory (or ``MAP_IP_TO_ASN_CACHE_DIR`` when set).
    /// ipv4, ipv6 : bool
    ///     Which families to load; at least one must be true.
    /// offline : bool
    ///     Never download; fail unless everything needed is cached.
    /// refresh : bool
    ///     Re-download the snapshot index even if the cached copy is fresh.
    #[staticmethod]
    #[pyo3(signature = (date=None, *, cache_dir=None, ipv4=true, ipv6=true, offline=false, refresh=false))]
    fn from_caida(
        py: Python<'_>,
        date: Option<&str>,
        cache_dir: Option<PathBuf>,
        ipv4: bool,
        ipv6: bool,
        offline: bool,
        refresh: bool,
    ) -> PyResult<Self> {
        if !ipv4 && !ipv6 {
            return Err(PyValueError::new_err(
                "at least one of ipv4= or ipv6= must be true",
            ));
        }
        let date = date
            .map(|text| {
                text.parse::<jiff::civil::Date>().map_err(|error| {
                    PyValueError::new_err(format!("date {text:?}: expected YYYY-MM-DD: {error}"))
                })
            })
            .transpose()?;
        let cache_dir = match cache_dir {
            Some(dir) => dir,
            None => default_cache_dir()?,
        };
        py.detach(|| {
            let fetcher = Fetcher::new(cache_dir).offline(offline).refresh_index(refresh);
            let mut inner = table::Table::new();
            let mut snapshots = Vec::new();
            for (family, wanted) in [(Family::V4, ipv4), (Family::V6, ipv6)] {
                if wanted {
                    snapshots.push(load::load_caida(&mut inner, &fetcher, family, date).map_err(load_error)?);
                }
            }
            Ok(Self { inner, snapshots })
        })
    }

    /// The longest matching prefix for one address, or ``None`` when no prefix covers it.
    ///
    /// Raises ``ValueError`` when `ip` is not an IP address.
    fn lookup(&self, ip: &str) -> PyResult<Option<Match>> {
        let address = parse_address(ip)?;
        Ok(self.inner.lookup(address).map(|hit| to_match(address, &hit)))
    }

    /// Looks up many addresses at once, releasing the GIL while it works.
    ///
    /// Returns one entry per input, in order: a `Match`, or ``None`` when no prefix
    /// covers the address. With ``strict=True`` (the default) an input that is not an
    /// IP address raises ``ValueError``; with ``strict=False`` it yields ``None``.
    #[pyo3(signature = (ips, *, strict=true))]
    fn lookup_many(&self, py: Python<'_>, ips: Vec<String>, strict: bool) -> PyResult<Vec<Option<Match>>> {
        py.detach(|| {
            ips.iter()
                .map(|text| match Item::parse(text) {
                    Item::Address(address) => {
                        Ok(self.inner.lookup(address).map(|hit| to_match(address, &hit)))
                    }
                    Item::Invalid { .. } if !strict => Ok(None),
                    Item::Invalid { text, reason } => {
                        Err(PyValueError::new_err(format!("{text:?}: {reason}")))
                    }
                })
                .collect()
        })
    }

    /// Only the origin ASN per address (``None`` when not found or, with
    /// ``strict=False``, invalid). Cheaper than `lookup_many` when the prefix is not needed.
    #[pyo3(signature = (ips, *, strict=true))]
    fn lookup_asns(&self, py: Python<'_>, ips: Vec<String>, strict: bool) -> PyResult<Vec<Option<u32>>> {
        py.detach(|| {
            ips.iter()
                .map(|text| match Item::parse(text) {
                    Item::Address(address) => Ok(self.inner.lookup(address).map(|hit| hit.origins.primary())),
                    Item::Invalid { .. } if !strict => Ok(None),
                    Item::Invalid { text, reason } => {
                        Err(PyValueError::new_err(format!("{text:?}: {reason}")))
                    }
                })
                .collect()
        })
    }

    /// Number of IPv4 prefixes loaded.
    #[getter]
    fn len_v4(&self) -> usize {
        self.inner.len_v4()
    }

    /// Number of IPv6 prefixes loaded.
    #[getter]
    fn len_v6(&self) -> usize {
        self.inner.len_v6()
    }

    /// The snapshots behind the table, one dict per family loaded, with keys
    /// ``family``, ``source``, ``date``, ``file``, ``location`` and ``prefixes``.
    #[getter]
    fn snapshots<'py>(&self, py: Python<'py>) -> PyResult<Vec<Bound<'py, PyDict>>> {
        self.snapshots
            .iter()
            .map(|info| {
                let dict = PyDict::new(py);
                dict.set_item("family", info.family.label())?;
                dict.set_item(
                    "source",
                    match info.source {
                        map_ip_to_asn::output::Source::Caida => "caida",
                        map_ip_to_asn::output::Source::Local => "local",
                    },
                )?;
                dict.set_item("date", info.date.as_deref())?;
                dict.set_item("file", &info.file)?;
                dict.set_item("location", &info.location)?;
                dict.set_item("prefixes", info.prefixes)?;
                Ok(dict)
            })
            .collect()
    }

    fn __len__(&self) -> usize {
        self.inner.len_v4() + self.inner.len_v6()
    }

    fn __repr__(&self) -> String {
        let files: Vec<&str> = self.snapshots.iter().map(|s| s.file.as_str()).collect();
        format!(
            "Table(ipv4_prefixes={}, ipv6_prefixes={}, snapshots={files:?})",
            self.inner.len_v4(),
            self.inner.len_v6()
        )
    }
}

fn default_cache_dir() -> PyResult<PathBuf> {
    if let Some(dir) = std::env::var_os("MAP_IP_TO_ASN_CACHE_DIR") {
        return Ok(PathBuf::from(dir));
    }
    directories::ProjectDirs::from("", "", "map-ip-to-asn")
        .map(|dirs| dirs.cache_dir().to_path_buf())
        .ok_or_else(|| PyRuntimeError::new_err("no cache directory could be determined; pass cache_dir="))
}

#[pymodule]
fn _native(module: &Bound<'_, PyModule>) -> PyResult<()> {
    module.add_class::<Table>()?;
    module.add_class::<Match>()?;
    module.add("__version__", map_ip_to_asn::VERSION)?;
    Ok(())
}
