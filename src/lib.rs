//! Map IP addresses to origin ASNs with CAIDA's RouteViews prefix-to-AS snapshots.
//!
//! The crate is a thin library under the `map-ip-to-asn` binary:
//!
//! - [`pfx2as`] parses the `pfx2as` text format CAIDA publishes;
//! - [`table`] holds a longest-prefix-match table for IPv4 and IPv6;
//! - [`caida`] knows the dataset layout on `publicdata.caida.org` and picks a snapshot by date;
//! - [`fetch`] downloads the index and the snapshots into a local cache;
//! - [`load`] fills a table from a local file or from CAIDA and records where it came from;
//! - [`input`] reads and validates the addresses to look up;
//! - [`output`] serializes results as JSON, JSON Lines, CSV, TSV or Parquet.

pub mod caida;
pub mod fetch;
pub mod input;
pub mod load;
pub mod output;
pub mod pfx2as;
pub mod table;

/// The crate version, as compiled into the binary.
pub const VERSION: &str = env!("CARGO_PKG_VERSION");

/// The `User-Agent` sent to CAIDA, so the data host can tell who is downloading.
pub const USER_AGENT: &str = concat!(
    "map-ip-to-asn/",
    env!("CARGO_PKG_VERSION"),
    " (+https://github.com/estcarisimo/map-ip-to-asn)"
);
