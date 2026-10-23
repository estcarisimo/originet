//! Result records and the output formats.

use std::fmt::Write as _;
use std::io::Write;

use anstyle::{AnsiColor, Style};
use serde::Serialize;

use crate::caida::Family;

/// Where a table came from.
#[derive(Debug, Clone, PartialEq, Eq, Serialize)]
#[serde(rename_all = "lowercase")]
pub enum Source {
    /// Downloaded from (or cached from) CAIDA.
    Caida,
    /// A file the user supplied.
    Local,
}

/// The snapshot behind one address family's answers.
#[derive(Debug, Clone, PartialEq, Eq, Serialize)]
pub struct SnapshotInfo {
    /// `ipv4` or `ipv6`.
    pub family: Family,
    /// `caida` or `local`.
    pub source: Source,
    /// The snapshot date (`YYYY-MM-DD`) when it is known from the file name.
    pub date: Option<String>,
    /// File name of the snapshot.
    pub file: String,
    /// Download URL (CAIDA) or local path.
    pub location: String,
    /// Number of prefixes loaded.
    pub prefixes: usize,
}

/// One answer. Flat on purpose so every format carries the same columns.
#[derive(Debug, Clone, PartialEq, Eq, Serialize)]
pub struct Record {
    /// The address, normalized (`8.8.8.8/32` and `::ffff:8.8.8.8` become `8.8.8.8`); an
    /// invalid input is kept as typed.
    pub ip: String,
    /// The longest matching prefix, or null when none covers the address.
    pub prefix: Option<String>,
    /// The origin ASN: the first ASN of a multi-origin or AS-set field.
    pub asn: Option<u32>,
    /// All origins in CAIDA's notation (`A`, `A_B` for multi-origin, `A,B` for an AS set).
    pub origins: Option<String>,
    /// Date of the snapshot that answered, when known.
    pub snapshot: Option<String>,
    /// Why the input could not be looked up, for invalid lines.
    pub error: Option<String>,
}

/// Counts over all records.
#[derive(Debug, Clone, Copy, Default, PartialEq, Eq, Serialize)]
pub struct Summary {
    /// Records in total.
    pub total: usize,
    /// Addresses with a matching prefix.
    pub found: usize,
    /// Valid addresses without a matching prefix.
    pub not_found: usize,
    /// Inputs that were not IP addresses.
    pub invalid: usize,
}

impl Summary {
    /// Tallies the records.
    pub fn of(records: &[Record]) -> Self {
        let mut summary = Self::default();
        for record in records {
            summary.total += 1;
            if record.error.is_some() {
                summary.invalid += 1;
            } else if record.asn.is_some() {
                summary.found += 1;
            } else {
                summary.not_found += 1;
            }
        }
        summary
    }
}

/// The complete output of a run.
#[derive(Debug, Clone, Serialize)]
pub struct Report {
    /// Name and version of this tool.
    pub tool: Tool,
    /// When the report was produced (RFC 3339, UTC).
    pub generated_at: String,
    /// The snapshots used, one per address family that was needed.
    pub snapshots: Vec<SnapshotInfo>,
    /// Totals.
    pub summary: Summary,
    /// One record per input, in input order.
    pub results: Vec<Record>,
}

/// Tool identification embedded in the JSON report.
#[derive(Debug, Clone, Serialize)]
pub struct Tool {
    /// `originet`.
    pub name: &'static str,
    /// Crate version.
    pub version: &'static str,
}

/// The output formats.
#[derive(Debug, Clone, Copy, PartialEq, Eq, clap::ValueEnum)]
pub enum Format {
    /// Aligned columns for reading in a terminal, results only.
    Table,
    /// One JSON document with metadata, summary and results.
    Json,
    /// One JSON object per line, results only.
    Jsonl,
    /// Comma-separated values with a header, results only.
    Csv,
    /// Tab-separated values with a header, results only.
    Tsv,
    /// Apache Parquet, results only.
    #[cfg(feature = "parquet")]
    Parquet,
}

impl Format {
    /// Whether the format is binary (and should not be written to a terminal).
    pub fn is_binary(self) -> bool {
        #[cfg(feature = "parquet")]
        {
            self == Format::Parquet
        }
        #[cfg(not(feature = "parquet"))]
        {
            false
        }
    }
}

/// Writes the report in the chosen format.
pub fn write(format: Format, report: &Report, writer: &mut dyn Write) -> anyhow::Result<()> {
    match format {
        Format::Table => write_table(report, writer, &Theme::plain())?,
        Format::Json => {
            serde_json::to_writer_pretty(&mut *writer, report)?;
            writer.write_all(b"\n")?;
        }
        Format::Jsonl => {
            for record in &report.results {
                serde_json::to_writer(&mut *writer, record)?;
                writer.write_all(b"\n")?;
            }
        }
        Format::Csv => write_delimited(b',', report, writer)?,
        Format::Tsv => write_delimited(b'\t', report, writer)?,
        #[cfg(feature = "parquet")]
        Format::Parquet => parquet_output::write(report, writer)?,
    }
    writer.flush()?;
    Ok(())
}

/// How the `table` format is styled. [`Theme::plain`] renders no escape codes at all;
/// [`Theme::colored`] is what the binary uses on a terminal.
#[derive(Debug, Clone, Copy, Default, PartialEq, Eq)]
pub struct Theme {
    /// The header line.
    pub header: Style,
    /// The `ip` of a valid input.
    pub ip: Style,
    /// The matched `prefix`.
    pub prefix: Style,
    /// The origin `asn`.
    pub asn: Style,
    /// `origins` when it is a single ASN (the same as `asn`).
    pub origins: Style,
    /// `origins` when several ASes originate the prefix (`A_B`) or it is an AS set (`A,B`).
    pub multiple_origins: Style,
    /// The `snapshot` date.
    pub snapshot: Style,
    /// The `-` of a missing value.
    pub missing: Style,
    /// An invalid input: its `ip` as typed and the `error`.
    pub error: Style,
}

impl Theme {
    /// No styling.
    pub fn plain() -> Self {
        Self::default()
    }

    /// Bold header, cyan prefix, bold green ASN, yellow multi-origin fields, dim
    /// snapshot dates and dashes, red invalid inputs.
    pub fn colored() -> Self {
        Self {
            header: Style::new().bold(),
            ip: Style::new(),
            prefix: AnsiColor::Cyan.on_default(),
            asn: AnsiColor::Green.on_default().bold(),
            origins: Style::new().dimmed(),
            multiple_origins: AnsiColor::Yellow.on_default(),
            snapshot: Style::new().dimmed(),
            missing: Style::new().dimmed(),
            error: AnsiColor::Red.on_default(),
        }
    }
}

/// The `table` format with a [`Theme`]: the six columns, padded to the widest value of
/// each. A missing value is `-`, except in the `error` column, which is empty unless
/// the input was invalid; the `asn` column is right-aligned, the last column is not
/// padded. Padding is never inside a styled span.
pub fn write_table(report: &Report, writer: &mut dyn Write, theme: &Theme) -> anyhow::Result<()> {
    const HEADER: [&str; 6] = ["ip", "prefix", "asn", "origins", "snapshot", "error"];
    const ASN: usize = 2;
    let present = |value: Option<&str>, style: Style| match value {
        Some(value) => (value.to_owned(), style),
        None => ("-".to_owned(), theme.missing),
    };
    let rows: Vec<[(String, Style); 6]> = report
        .results
        .iter()
        .map(|record| {
            let invalid = record.error.is_some();
            let origins_style = match &record.origins {
                Some(origins) if origins.contains(['_', ',']) => theme.multiple_origins,
                _ => theme.origins,
            };
            [
                (record.ip.clone(), if invalid { theme.error } else { theme.ip }),
                present(record.prefix.as_deref(), theme.prefix),
                present(record.asn.map(|asn| asn.to_string()).as_deref(), theme.asn),
                present(record.origins.as_deref(), origins_style),
                present(record.snapshot.as_deref(), theme.snapshot),
                (record.error.clone().unwrap_or_default(), theme.error),
            ]
        })
        .collect();
    let mut widths = HEADER.map(str::len);
    for row in &rows {
        for (width, (cell, _)) in widths.iter_mut().zip(row) {
            *width = (*width).max(cell.chars().count());
        }
    }
    let header = HEADER.map(|name| (name.to_owned(), theme.header));
    for row in std::iter::once(&header).chain(&rows) {
        let mut line = String::new();
        for (index, ((cell, style), width)) in row.iter().zip(widths).enumerate() {
            if index > 0 {
                line.push_str("  ");
            }
            let padding = width - cell.chars().count();
            if index == ASN {
                line.extend(std::iter::repeat_n(' ', padding));
            }
            if !cell.is_empty() {
                write!(line, "{style}{cell}{style:#}")?;
            }
            if index != ASN && index + 1 < row.len() {
                line.extend(std::iter::repeat_n(' ', padding));
            }
        }
        writeln!(writer, "{}", line.trim_end())?;
    }
    Ok(())
}

fn write_delimited(delimiter: u8, report: &Report, writer: &mut dyn Write) -> anyhow::Result<()> {
    let mut csv = csv::WriterBuilder::new().delimiter(delimiter).from_writer(writer);
    for record in &report.results {
        csv.serialize(record)?;
    }
    csv.flush()?;
    Ok(())
}

#[cfg(feature = "parquet")]
mod parquet_output {
    use std::io::Write;
    use std::sync::Arc;

    use arrow::array::{ArrayRef, RecordBatch, StringArray, UInt32Array};
    use arrow::datatypes::{DataType, Field, Schema};
    use parquet::arrow::ArrowWriter;
    use parquet::basic::Compression;
    use parquet::file::properties::WriterProperties;

    use super::Report;

    pub fn write(report: &Report, writer: &mut dyn Write) -> anyhow::Result<()> {
        let schema = Arc::new(Schema::new(vec![
            Field::new("ip", DataType::Utf8, false),
            Field::new("prefix", DataType::Utf8, true),
            Field::new("asn", DataType::UInt32, true),
            Field::new("origins", DataType::Utf8, true),
            Field::new("snapshot", DataType::Utf8, true),
            Field::new("error", DataType::Utf8, true),
        ]));
        let results = &report.results;
        let columns: Vec<ArrayRef> = vec![
            Arc::new(StringArray::from(
                results.iter().map(|r| r.ip.as_str()).collect::<Vec<_>>(),
            )),
            Arc::new(StringArray::from(
                results.iter().map(|r| r.prefix.as_deref()).collect::<Vec<_>>(),
            )),
            Arc::new(UInt32Array::from(
                results.iter().map(|r| r.asn).collect::<Vec<_>>(),
            )),
            Arc::new(StringArray::from(
                results.iter().map(|r| r.origins.as_deref()).collect::<Vec<_>>(),
            )),
            Arc::new(StringArray::from(
                results.iter().map(|r| r.snapshot.as_deref()).collect::<Vec<_>>(),
            )),
            Arc::new(StringArray::from(
                results.iter().map(|r| r.error.as_deref()).collect::<Vec<_>>(),
            )),
        ];
        let batch = RecordBatch::try_new(schema.clone(), columns)?;
        let properties = WriterProperties::builder()
            .set_compression(Compression::SNAPPY)
            .set_created_by(format!("originet {}", crate::VERSION))
            .build();
        let mut buffer = Vec::new();
        let mut parquet = ArrowWriter::try_new(&mut buffer, schema, Some(properties))?;
        parquet.write(&batch)?;
        parquet.close()?;
        writer.write_all(&buffer)?;
        Ok(())
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn report() -> Report {
        Report {
            tool: Tool {
                name: "originet",
                version: "0.0.0",
            },
            generated_at: "2026-10-22T00:00:00Z".into(),
            snapshots: vec![],
            summary: Summary::of(&records()),
            results: records(),
        }
    }

    fn records() -> Vec<Record> {
        vec![
            Record {
                ip: "8.8.8.8".into(),
                prefix: Some("8.8.8.0/24".into()),
                asn: Some(15169),
                origins: Some("15169".into()),
                snapshot: Some("2026-10-21".into()),
                error: None,
            },
            Record {
                ip: "10.0.0.1".into(),
                prefix: None,
                asn: None,
                origins: None,
                snapshot: Some("2026-10-21".into()),
                error: None,
            },
            Record {
                ip: "bogus".into(),
                prefix: None,
                asn: None,
                origins: None,
                snapshot: None,
                error: Some("invalid IP address syntax".into()),
            },
        ]
    }

    #[test]
    fn summary_counts() {
        let summary = Summary::of(&records());
        assert_eq!(
            summary,
            Summary {
                total: 3,
                found: 1,
                not_found: 1,
                invalid: 1
            }
        );
    }

    #[test]
    fn csv_and_tsv_have_a_header_and_empty_nulls() {
        let mut out = Vec::new();
        write(Format::Csv, &report(), &mut out).unwrap();
        let text = String::from_utf8(out).unwrap();
        let lines: Vec<_> = text.lines().collect();
        assert_eq!(lines[0], "ip,prefix,asn,origins,snapshot,error");
        assert_eq!(lines[1], "8.8.8.8,8.8.8.0/24,15169,15169,2026-10-21,");
        assert_eq!(lines[2], "10.0.0.1,,,,2026-10-21,");
        assert_eq!(lines[3], "bogus,,,,,invalid IP address syntax");

        let mut out = Vec::new();
        write(Format::Tsv, &report(), &mut out).unwrap();
        assert!(
            String::from_utf8(out)
                .unwrap()
                .starts_with("ip\tprefix\tasn\torigins\tsnapshot\terror\n")
        );
    }

    #[test]
    fn table_is_aligned_with_dashes_for_nulls() {
        let mut out = Vec::new();
        write(Format::Table, &report(), &mut out).unwrap();
        let text = String::from_utf8(out).unwrap();
        assert_eq!(
            text,
            "\
ip        prefix        asn  origins  snapshot    error
8.8.8.8   8.8.8.0/24  15169  15169    2026-10-21
10.0.0.1  -               -  -        2026-10-21
bogus     -               -  -        -           invalid IP address syntax
"
        );
        assert!(!Format::Table.is_binary());

        let mut colored = Vec::new();
        write_table(&report(), &mut colored, &Theme::colored()).unwrap();
        let colored = String::from_utf8(colored).unwrap();
        assert!(colored.starts_with("\x1b[1mip\x1b[0m"), "{colored:?}");
        assert!(colored.contains("\x1b[1m\x1b[32m15169\x1b[0m"), "{colored:?}");
        assert!(colored.contains("\x1b[31mbogus\x1b[0m"), "{colored:?}");
        assert_eq!(anstream::adapter::strip_str(&colored).to_string(), text);
    }

    #[test]
    fn json_and_jsonl() {
        let mut out = Vec::new();
        write(Format::Json, &report(), &mut out).unwrap();
        let value: serde_json::Value = serde_json::from_slice(&out).unwrap();
        assert_eq!(value["summary"]["found"], 1);
        assert_eq!(value["results"][0]["asn"], 15169);
        assert!(value["results"][1]["asn"].is_null());

        let mut out = Vec::new();
        write(Format::Jsonl, &report(), &mut out).unwrap();
        let text = String::from_utf8(out).unwrap();
        assert_eq!(text.lines().count(), 3);
        let first: serde_json::Value = serde_json::from_str(text.lines().next().unwrap()).unwrap();
        assert_eq!(first["prefix"], "8.8.8.0/24");
    }

    #[cfg(feature = "parquet")]
    #[test]
    fn parquet_round_trips() {
        use arrow::array::RecordBatch;
        use parquet::arrow::arrow_reader::ParquetRecordBatchReaderBuilder;
        let mut out = Vec::new();
        write(Format::Parquet, &report(), &mut out).unwrap();
        let reader = ParquetRecordBatchReaderBuilder::try_new(bytes::Bytes::from(out))
            .unwrap()
            .build()
            .unwrap();
        let batches: Vec<_> = reader.map(Result::unwrap).collect();
        assert_eq!(batches.iter().map(RecordBatch::num_rows).sum::<usize>(), 3);
        assert_eq!(batches[0].schema().fields().len(), 6);
    }
}
