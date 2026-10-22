//! Reading and validating the addresses to look up.

use std::io::{self, BufRead};
use std::net::IpAddr;

/// One line of input: a parsed address or the text that failed to parse.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum Item {
    /// A valid IP address.
    Address(IpAddr),
    /// Text that is not an IP address, kept so the output can report it.
    Invalid {
        /// The text as given, trimmed.
        text: String,
        /// Why it was rejected.
        reason: String,
    },
}

impl Item {
    /// Parses one token. Surrounding whitespace is ignored; a `/32` or `/128` host
    /// prefix is accepted, and an IPv4-mapped IPv6 address is looked up as IPv4.
    pub fn parse(text: &str) -> Self {
        let trimmed = text.trim();
        let host = trimmed
            .strip_suffix("/32")
            .or_else(|| trimmed.strip_suffix("/128"))
            .unwrap_or(trimmed);
        match host.parse::<IpAddr>() {
            Ok(IpAddr::V6(v6)) if trimmed.ends_with("/32") => Item::Invalid {
                text: trimmed.to_owned(),
                reason: format!("{v6} is IPv6 but has a /32 host suffix"),
            },
            Ok(IpAddr::V4(v4)) if trimmed.ends_with("/128") => Item::Invalid {
                text: trimmed.to_owned(),
                reason: format!("{v4} is IPv4 but has a /128 host suffix"),
            },
            Ok(IpAddr::V6(v6)) => match v6.to_ipv4_mapped() {
                Some(v4) => Item::Address(IpAddr::V4(v4)),
                None => Item::Address(IpAddr::V6(v6)),
            },
            Ok(address) => Item::Address(address),
            Err(error) => Item::Invalid {
                text: trimmed.to_owned(),
                reason: error.to_string(),
            },
        }
    }

    /// The address when valid.
    pub fn address(&self) -> Option<IpAddr> {
        match self {
            Item::Address(address) => Some(*address),
            Item::Invalid { .. } => None,
        }
    }

    /// The input text as the user wrote it, for reporting.
    pub fn text(&self) -> String {
        match self {
            Item::Address(address) => address.to_string(),
            Item::Invalid { text, .. } => text.clone(),
        }
    }
}

/// Reads one address per line. Blank lines and `#` comments are skipped; a trailing
/// `# comment` on a line is ignored too.
pub fn read_items<R: BufRead>(reader: R) -> io::Result<Vec<Item>> {
    let mut items = Vec::new();
    for line in reader.lines() {
        let line = line?;
        let token = line.split('#').next().unwrap_or("").trim();
        if token.is_empty() {
            continue;
        }
        items.push(Item::parse(token));
    }
    Ok(items)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn parses_addresses_and_host_prefixes() {
        assert_eq!(
            Item::parse(" 8.8.8.8 ").address(),
            Some("8.8.8.8".parse().unwrap())
        );
        assert_eq!(
            Item::parse("8.8.8.8/32").address(),
            Some("8.8.8.8".parse().unwrap())
        );
        assert_eq!(
            Item::parse("2001:4860::8888/128").address(),
            Some("2001:4860::8888".parse().unwrap())
        );
        assert_eq!(
            Item::parse("::ffff:8.8.8.8").address(),
            Some("8.8.8.8".parse().unwrap())
        );
    }

    #[test]
    fn rejects_non_addresses() {
        for text in [
            "8.8.8",
            "example.com",
            "8.8.8.0/24",
            "2001:db8::/32",
            "8.8.8.8/128",
            "2001:db8::1/32",
            "",
        ] {
            assert!(
                Item::parse(text).address().is_none(),
                "{text:?} should be invalid"
            );
        }
        match Item::parse("not-an-ip") {
            Item::Invalid { text, reason } => {
                assert_eq!(text, "not-an-ip");
                assert!(!reason.is_empty());
            }
            Item::Address(_) => panic!(),
        }
    }

    #[test]
    fn reads_lines_skipping_comments() {
        let text = "# header\n8.8.8.8\n\n  1.1.1.1  # cloudflare\nbogus\n";
        let items = read_items(text.as_bytes()).unwrap();
        assert_eq!(items.len(), 3);
        assert_eq!(items[0].text(), "8.8.8.8");
        assert_eq!(items[1].text(), "1.1.1.1");
        assert!(items[2].address().is_none());
    }
}
