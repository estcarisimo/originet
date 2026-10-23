//! Parser for CAIDA's `pfx2as` format.
//!
//! Each line is `prefix<TAB>length<TAB>origins`, for example `1.0.0.0\t24\t13335`.
//! The origin field is one ASN, several ASNs joined with `_` (a multi-origin
//! prefix, MOAS) or several joined with `,` (an AS set). Blank lines and lines
//! starting with `#` are ignored. Files are plain text or gzip-compressed.

use std::fmt;
use std::fs::File;
use std::io::{self, BufRead, BufReader, Read};
use std::net::IpAddr;
use std::path::Path;

use flate2::read::MultiGzDecoder;
use ipnet::IpNet;

/// The origin ASN(s) announced for a prefix, in CAIDA's notation.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Origins {
    raw: Box<str>,
    primary: u32,
}

impl Origins {
    /// Parses an origin field such as `13335`, `4847_58851_4808` or `64496,64497`.
    pub fn parse(field: &str) -> Result<Self, ParseError> {
        let mut first = None;
        for token in field.split(['_', ',']) {
            let asn: u32 = token.parse().map_err(|_| ParseError::Origins(field.to_owned()))?;
            first.get_or_insert(asn);
        }
        let primary = first.ok_or_else(|| ParseError::Origins(field.to_owned()))?;
        Ok(Self {
            raw: field.into(),
            primary,
        })
    }

    /// The field exactly as CAIDA writes it (`A`, `A_B` or `A,B`).
    pub fn raw(&self) -> &str {
        &self.raw
    }

    /// The first ASN of the field. For a single-origin prefix it is the origin.
    pub fn primary(&self) -> u32 {
        self.primary
    }

    /// Every ASN of the field, in order.
    pub fn asns(&self) -> impl Iterator<Item = u32> + '_ {
        // Validated in `parse`, so each token is a u32.
        self.raw.split(['_', ',']).filter_map(|token| token.parse().ok())
    }

    /// True when several ASes originate the prefix (`A_B`).
    pub fn is_multi_origin(&self) -> bool {
        self.raw.contains('_')
    }

    /// True when the origin is an AS set (`A,B`).
    pub fn is_as_set(&self) -> bool {
        self.raw.contains(',')
    }
}

impl fmt::Display for Origins {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.write_str(&self.raw)
    }
}

/// One line of a `pfx2as` file.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Entry {
    /// The announced prefix.
    pub prefix: IpNet,
    /// Its origin ASN(s).
    pub origins: Origins,
}

/// Why a `pfx2as` line could not be parsed.
#[derive(Debug, thiserror::Error, PartialEq, Eq)]
pub enum ParseError {
    /// The line does not have three tab-separated fields.
    #[error("expected `prefix<TAB>length<TAB>origins`, got {0:?}")]
    Fields(String),
    /// The address field is not an IP address.
    #[error("invalid address {0:?}")]
    Address(String),
    /// The length field is not a valid prefix length for the address family.
    #[error("invalid prefix length {0:?}")]
    Length(String),
    /// The origin field is not ASNs joined with `_` or `,`.
    #[error("invalid origin field {0:?}")]
    Origins(String),
    /// The prefix belongs to the other address family than the one being loaded.
    #[error("expected an {expected} prefix, got {prefix}")]
    Family {
        /// `ipv4` or `ipv6`.
        expected: &'static str,
        /// The prefix found.
        prefix: String,
    },
}

/// Parses one line. Returns `Ok(None)` for blank lines and comments.
pub fn parse_line(line: &str) -> Result<Option<Entry>, ParseError> {
    let line = line.trim_end_matches(['\r', '\n']);
    if line.trim().is_empty() || line.starts_with('#') {
        return Ok(None);
    }
    let mut fields = line.split('\t');
    let (Some(address), Some(length), Some(origins), None) =
        (fields.next(), fields.next(), fields.next(), fields.next())
    else {
        return Err(ParseError::Fields(line.to_owned()));
    };
    let address: IpAddr = address
        .trim()
        .parse()
        .map_err(|_| ParseError::Address(address.to_owned()))?;
    let length: u8 = length
        .trim()
        .parse()
        .map_err(|_| ParseError::Length(length.to_owned()))?;
    let prefix = IpNet::new(address, length).map_err(|_| ParseError::Length(length.to_string()))?;
    let origins = Origins::parse(origins.trim())?;
    Ok(Some(Entry { prefix, origins }))
}

/// A line that failed to parse, with its 1-based line number.
#[derive(Debug, thiserror::Error)]
pub enum ReadError {
    /// An I/O error while reading.
    #[error(transparent)]
    Io(#[from] io::Error),
    /// A malformed line.
    #[error("line {line}: {source}")]
    Parse {
        /// 1-based line number.
        line: usize,
        /// What was wrong with it.
        #[source]
        source: ParseError,
    },
}

/// Iterates over the entries of a `pfx2as` stream with their 1-based line numbers.
pub fn numbered_entries<R: BufRead>(reader: R) -> impl Iterator<Item = Result<(usize, Entry), ReadError>> {
    reader.lines().enumerate().filter_map(|(index, line)| match line {
        Err(error) => Some(Err(ReadError::Io(error))),
        Ok(line) => match parse_line(&line) {
            Ok(None) => None,
            Ok(Some(entry)) => Some(Ok((index + 1, entry))),
            Err(source) => Some(Err(ReadError::Parse {
                line: index + 1,
                source,
            })),
        },
    })
}

/// Iterates over the entries of a `pfx2as` stream.
pub fn entries<R: BufRead>(reader: R) -> impl Iterator<Item = Result<Entry, ReadError>> {
    numbered_entries(reader).map(|item| item.map(|(_, entry)| entry))
}

/// Opens a `pfx2as` file, transparently decompressing gzip (detected by its magic bytes,
/// so the file name does not matter).
pub fn open(path: &Path) -> io::Result<Box<dyn BufRead>> {
    let file = File::open(path)?;
    decode(BufReader::new(file))
}

/// Wraps a reader so gzip content is decompressed, detected by its magic bytes.
pub fn decode<R: BufRead + 'static>(mut reader: R) -> io::Result<Box<dyn BufRead>> {
    let magic = reader.fill_buf()?;
    if magic.starts_with(&[0x1f, 0x8b]) {
        Ok(Box::new(BufReader::new(MultiGzDecoder::new(reader))))
    } else {
        Ok(Box::new(reader))
    }
}

/// Reads an entire gzip-or-plain file into a string. Used for small text files.
pub fn read_to_string(path: &Path) -> io::Result<String> {
    let mut text = String::new();
    open(path)?.read_to_string(&mut text)?;
    Ok(text)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn parses_single_origin() {
        let entry = parse_line("1.0.0.0\t24\t13335\n").unwrap().unwrap();
        assert_eq!(entry.prefix, "1.0.0.0/24".parse::<IpNet>().unwrap());
        assert_eq!(entry.origins.primary(), 13335);
        assert_eq!(entry.origins.raw(), "13335");
        assert!(!entry.origins.is_multi_origin());
        assert!(!entry.origins.is_as_set());
    }

    #[test]
    fn parses_multi_origin_and_sets() {
        let moas = parse_line("1.119.178.0\t23\t4847_58851_4808").unwrap().unwrap();
        assert_eq!(moas.origins.primary(), 4847);
        assert_eq!(moas.origins.asns().collect::<Vec<_>>(), [4847, 58851, 4808]);
        assert!(moas.origins.is_multi_origin());

        let set = parse_line("2001:db8::\t32\t64496,64497").unwrap().unwrap();
        assert_eq!(set.origins.primary(), 64496);
        assert!(set.origins.is_as_set());
        assert_eq!(set.prefix.to_string(), "2001:db8::/32");
    }

    #[test]
    fn skips_blank_and_comment_lines() {
        assert_eq!(parse_line("").unwrap(), None);
        assert_eq!(parse_line("   \r\n").unwrap(), None);
        assert_eq!(parse_line("# comment").unwrap(), None);
    }

    #[test]
    fn rejects_malformed_lines() {
        assert!(matches!(
            parse_line("1.0.0.0 24 13335"),
            Err(ParseError::Fields(_))
        ));
        assert!(matches!(parse_line("1.0.0.0\t24"), Err(ParseError::Fields(_))));
        assert!(matches!(
            parse_line("1.0.0.0\t24\t1\textra"),
            Err(ParseError::Fields(_))
        ));
        assert!(matches!(
            parse_line("1.0.0\t24\t13335"),
            Err(ParseError::Address(_))
        ));
        assert!(matches!(
            parse_line("1.0.0.0\t33\t13335"),
            Err(ParseError::Length(_))
        ));
        assert!(matches!(
            parse_line("1.0.0.0\tx\t13335"),
            Err(ParseError::Length(_))
        ));
        assert!(matches!(parse_line("1.0.0.0\t24\t"), Err(ParseError::Origins(_))));
        assert!(matches!(
            parse_line("1.0.0.0\t24\tAS13335"),
            Err(ParseError::Origins(_))
        ));
        assert!(matches!(
            parse_line("1.0.0.0\t24\t1__2"),
            Err(ParseError::Origins(_))
        ));
    }

    #[test]
    fn iterates_entries_with_line_numbers() {
        let text = "# header\n1.0.0.0\t24\t13335\n\nbad line\n";
        let items: Vec<_> = entries(text.as_bytes()).collect();
        assert_eq!(items.len(), 2);
        assert!(items[0].is_ok());
        match &items[1] {
            Err(ReadError::Parse { line, .. }) => assert_eq!(*line, 4),
            other => panic!("expected a parse error, got {other:?}"),
        }
    }

    #[test]
    fn decodes_gzip_by_magic_bytes() {
        use flate2::write::GzEncoder;
        use std::io::Write;
        let mut encoder = GzEncoder::new(Vec::new(), flate2::Compression::fast());
        encoder.write_all(b"8.8.8.0\t24\t15169\n").unwrap();
        let compressed = encoder.finish().unwrap();
        let reader = decode(io::Cursor::new(compressed)).unwrap();
        let entries: Vec<_> = entries(reader).map(Result::unwrap).collect();
        assert_eq!(entries.len(), 1);
        assert_eq!(entries[0].origins.primary(), 15169);

        let plain = decode(io::Cursor::new(b"8.8.8.0\t24\t15169\n".to_vec())).unwrap();
        assert_eq!(entries_count(plain), 1);
    }

    fn entries_count(reader: Box<dyn BufRead>) -> usize {
        entries(reader).count()
    }
}
