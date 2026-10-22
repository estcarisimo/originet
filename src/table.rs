//! Longest-prefix-match table built from `pfx2as` entries.

use std::io::BufRead;
use std::net::IpAddr;
use std::path::Path;

use ipnet::{IpNet, Ipv4Net, Ipv6Net};
use prefix_trie::PrefixMap;

use crate::caida::Family;
use crate::pfx2as::{self, Entry, Origins, ParseError, ReadError};

/// A prefix-to-origin table for IPv4 and IPv6.
#[derive(Debug, Default)]
pub struct Table {
    v4: PrefixMap<Ipv4Net, Origins>,
    v6: PrefixMap<Ipv6Net, Origins>,
}

/// The longest matching prefix for an address and its origins.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Match<'a> {
    /// The matched prefix.
    pub prefix: IpNet,
    /// Its origin ASN(s).
    pub origins: &'a Origins,
}

impl Table {
    /// An empty table.
    pub fn new() -> Self {
        Self::default()
    }

    /// Adds one entry; a repeated prefix keeps the last origins seen.
    pub fn insert(&mut self, entry: Entry) {
        match entry.prefix {
            IpNet::V4(prefix) => {
                self.v4.insert(prefix, entry.origins);
            }
            IpNet::V6(prefix) => {
                self.v6.insert(prefix, entry.origins);
            }
        }
    }

    /// Loads every entry of a `pfx2as` stream into the table.
    pub fn load<R: BufRead>(&mut self, reader: R) -> Result<usize, ReadError> {
        let mut count = 0;
        for entry in pfx2as::entries(reader) {
            self.insert(entry?);
            count += 1;
        }
        Ok(count)
    }

    /// Loads a `pfx2as` file (plain or gzip) into the table and returns how many
    /// entries it had.
    pub fn load_path(&mut self, path: &Path) -> Result<usize, ReadError> {
        let reader = pfx2as::open(path)?;
        self.load(reader)
    }

    /// Loads a stream that must contain only `family` prefixes; an entry of the other
    /// family is an error naming its line. Returns how many entries were loaded.
    pub fn load_family<R: BufRead>(&mut self, reader: R, family: Family) -> Result<usize, ReadError> {
        let mut count = 0;
        for item in pfx2as::numbered_entries(reader) {
            let (line, entry) = item?;
            let matches = match (family, entry.prefix) {
                (Family::V4, IpNet::V4(_)) | (Family::V6, IpNet::V6(_)) => true,
                (Family::V4 | Family::V6, _) => false,
            };
            if !matches {
                return Err(ReadError::Parse {
                    line,
                    source: ParseError::Family {
                        expected: family.label(),
                        prefix: entry.prefix.to_string(),
                    },
                });
            }
            self.insert(entry);
            count += 1;
        }
        Ok(count)
    }

    /// Loads a `pfx2as` file (plain or gzip) that must contain only `family` prefixes.
    pub fn load_path_family(&mut self, path: &Path, family: Family) -> Result<usize, ReadError> {
        let reader = pfx2as::open(path)?;
        self.load_family(reader, family)
    }

    /// Number of IPv4 prefixes in the table.
    pub fn len_v4(&self) -> usize {
        self.v4.len()
    }

    /// Number of IPv6 prefixes in the table.
    pub fn len_v6(&self) -> usize {
        self.v6.len()
    }

    /// Longest-prefix match for an address.
    pub fn lookup(&self, address: IpAddr) -> Option<Match<'_>> {
        match address {
            IpAddr::V4(address) => {
                let key = Ipv4Net::new(address, 32).expect("32 is a valid IPv4 prefix length");
                self.v4.get_lpm(&key).map(|(prefix, origins)| Match {
                    prefix: IpNet::V4(prefix),
                    origins,
                })
            }
            IpAddr::V6(address) => {
                let key = Ipv6Net::new(address, 128).expect("128 is a valid IPv6 prefix length");
                self.v6.get_lpm(&key).map(|(prefix, origins)| Match {
                    prefix: IpNet::V6(prefix),
                    origins,
                })
            }
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn table() -> Table {
        let mut table = Table::new();
        table
            .load(
                "1.0.0.0\t24\t13335\n1.0.0.0\t16\t64496\n1.1.8.0\t24\t149511_138421\n2001::\t32\t6939\n2001:4:112::\t48\t112\n"
                    .as_bytes(),
            )
            .unwrap();
        table
    }

    #[test]
    fn longest_prefix_wins() {
        let table = table();
        let hit = table.lookup("1.0.0.9".parse().unwrap()).unwrap();
        assert_eq!(hit.prefix.to_string(), "1.0.0.0/24");
        assert_eq!(hit.origins.primary(), 13335);

        let covering = table.lookup("1.0.200.1".parse().unwrap()).unwrap();
        assert_eq!(covering.prefix.to_string(), "1.0.0.0/16");
        assert_eq!(covering.origins.primary(), 64496);
    }

    #[test]
    fn ipv6_and_misses() {
        let table = table();
        let hit = table.lookup("2001:4:112::53".parse().unwrap()).unwrap();
        assert_eq!(hit.prefix.to_string(), "2001:4:112::/48");
        assert_eq!(hit.origins.primary(), 112);
        assert_eq!(
            table
                .lookup("2001:0:1::1".parse().unwrap())
                .unwrap()
                .origins
                .primary(),
            6939
        );
        assert!(table.lookup("9.9.9.9".parse().unwrap()).is_none());
        assert!(table.lookup("2a00::1".parse().unwrap()).is_none());
        assert_eq!(table.len_v4(), 3);
        assert_eq!(table.len_v6(), 2);
    }

    #[test]
    fn load_family_rejects_the_other_family() {
        let mut table = Table::new();
        let loaded = table
            .load_family("8.8.8.0\t24\t15169\n".as_bytes(), Family::V4)
            .unwrap();
        assert_eq!(loaded, 1);
        let error = table
            .load_family(
                "8.8.8.0\t24\t15169\n2001:db8::\t32\t64496\n".as_bytes(),
                Family::V4,
            )
            .unwrap_err();
        assert_eq!(
            error.to_string(),
            "line 2: expected an ipv4 prefix, got 2001:db8::/32"
        );
        assert_eq!(table.len_v6(), 0, "nothing of the other family is inserted");
        assert!(
            table
                .load_family("8.8.8.0\t24\t15169\n".as_bytes(), Family::V6)
                .is_err()
        );
    }

    #[test]
    fn multi_origin_is_preserved() {
        let table = table();
        let hit = table.lookup("1.1.8.1".parse().unwrap()).unwrap();
        assert_eq!(hit.origins.raw(), "149511_138421");
        assert!(hit.origins.is_multi_origin());
    }
}
