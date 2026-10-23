//! `originet`: map IP addresses to origin ASNs with CAIDA's RouteViews
//! prefix-to-AS snapshots.

use std::fs::File;
use std::io::{self, BufReader, BufWriter, IsTerminal, Write};
use std::net::IpAddr;
use std::path::PathBuf;
use std::process::ExitCode;
use std::time::Instant;

use anyhow::{Context, bail};
use clap::{ArgAction, Parser};
use jiff::civil::Date;

use originet::caida::Family;
use originet::fetch::Fetcher;
use originet::input::{self, Item};
use originet::load;
use originet::output::{self, Format, Record, Report, SnapshotInfo, Summary, Tool};
use originet::table::Table;

mod progress;

/// Map IP addresses to origin ASNs with CAIDA's RouteViews prefix-to-AS snapshots.
///
/// Addresses come from the command line, from a file (`--file`, one per line) or from
/// standard input (`--file -`). The matching snapshot is downloaded from CAIDA on first
/// use and cached; `--date` picks the newest snapshot on or before a day, and
/// `--pfx2as`/`--pfx2as6` use local files instead of downloading. `--fetch` only fills
/// the cache.
#[derive(Debug, Parser)]
#[command(name = "originet", version, about, long_about, after_help = EXAMPLES)]
struct Args {
    /// IP addresses to look up (IPv4 or IPv6).
    #[arg(value_name = "IP")]
    addresses: Vec<String>,

    /// Read addresses from a file, one per line (`-` for standard input).
    #[arg(short, long, value_name = "PATH")]
    file: Option<PathBuf>,

    /// Use the newest snapshot taken on or before this day (YYYY-MM-DD). Default: the newest.
    #[arg(short, long, value_name = "YYYY-MM-DD", value_parser = parse_date)]
    date: Option<Date>,

    /// Local IPv4 pfx2as file (plain or gzip) instead of downloading from CAIDA.
    #[arg(long, value_name = "PATH")]
    pfx2as: Option<PathBuf>,

    /// Local IPv6 pfx2as file (plain or gzip) instead of downloading from CAIDA.
    #[arg(long, value_name = "PATH")]
    pfx2as6: Option<PathBuf>,

    /// Output format. Default: `table` on a terminal, `json` when piped or with `--output`.
    #[arg(short = 'F', long, value_enum, value_name = "FORMAT")]
    format: Option<Format>,

    /// Write the output to this file instead of standard output.
    #[arg(short, long, value_name = "PATH")]
    output: Option<PathBuf>,

    /// Directory for downloaded indexes and snapshots.
    #[arg(long, value_name = "DIR", env = "ORIGINET_CACHE_DIR")]
    cache_dir: Option<PathBuf>,

    /// Never download; fail unless the index and snapshot are already cached.
    #[arg(long)]
    offline: bool,

    /// Re-download the snapshot index even if the cached copy is fresh.
    #[arg(long, conflicts_with = "offline")]
    refresh: bool,

    /// Download the IPv4 and IPv6 snapshots for the day (`--date`, default the newest)
    /// into the cache and exit without looking anything up; print one line per snapshot
    /// with its date, whether it was already cached, and its path. With `--offline` it
    /// only checks that they are cached.
    #[arg(
        long,
        conflicts_with_all = ["addresses", "file", "pfx2as", "pfx2as6", "format", "output", "strict"]
    )]
    fetch: bool,

    /// Exit with status 1 when any input is not an IP address.
    #[arg(long)]
    strict: bool,

    /// Do not print the summary to standard error.
    #[arg(short, long)]
    quiet: bool,

    /// More detail on standard error (`-v` what is fetched, `-vv` debugging).
    #[arg(short, long, action = ArgAction::Count)]
    verbose: u8,
}

const EXAMPLES: &str = "\
Examples:
  originet 8.8.8.8 2001:4860::8888
  originet --format json 8.8.8.8 2001:4860::8888
  originet --file ips.txt --format csv --output results.csv
  originet --date 2019-06-01 --file ips.txt --format parquet -o june-2019.parquet
  originet --pfx2as routeviews-rv2-20261021-0800.pfx2as.gz --file ips.txt
  cat ips.txt | originet -f - -F jsonl
  originet --fetch --date 2019-06-01";

fn parse_date(text: &str) -> Result<Date, String> {
    text.parse::<Date>()
        .map_err(|error| format!("expected YYYY-MM-DD: {error}"))
}

fn main() -> ExitCode {
    let args = Args::parse();
    init_logging(args.verbose);
    match run(&args) {
        Ok(code) => code,
        Err(error) => {
            eprintln!("error: {}", render(&error));
            ExitCode::FAILURE
        }
    }
}

/// The error and its causes, oldest last, without repeating a cause whose text the
/// previous message already carries (the crate's errors embed their source's message).
fn render(error: &anyhow::Error) -> String {
    let mut text = error.to_string();
    for cause in error.chain().skip(1) {
        let cause = cause.to_string();
        if !text.contains(&cause) {
            text.push_str(": ");
            text.push_str(&cause);
        }
    }
    text
}

fn init_logging(verbosity: u8) {
    let level = match verbosity {
        0 => log::LevelFilter::Warn,
        1 => log::LevelFilter::Info,
        _ => log::LevelFilter::Debug,
    };
    env_logger::Builder::from_env(env_logger::Env::default().default_filter_or(level.as_str()))
        .format_timestamp(None)
        .format_target(false)
        .init();
}

fn run(args: &Args) -> anyhow::Result<ExitCode> {
    let started = Instant::now();
    let mut fetcher = Fetcher::new(cache_dir(args.cache_dir.clone())?)
        .offline(args.offline)
        .refresh_index(args.refresh);
    if !args.quiet && io::stderr().is_terminal() {
        fetcher = fetcher.progress(progress::Bars::default());
    }
    if args.fetch {
        return fetch_snapshots(args, &fetcher, started);
    }

    let items = collect_items(args)?;
    if items.is_empty() {
        bail!("no addresses given; pass them as arguments or with --file (see --help)");
    }
    let needs = Needs::of(&items);

    let mut table = Table::new();
    let mut snapshots = Vec::new();
    for (family, local) in [(Family::V4, &args.pfx2as), (Family::V6, &args.pfx2as6)] {
        if !needs.has(family) {
            continue;
        }
        let info = match local {
            Some(path) => {
                if let Some(date) = args.date {
                    log::warn!(
                        "--date {date} does not apply to {family}: it is read from {}",
                        path.display()
                    );
                }
                load::load_local(&mut table, family, path)?
            }
            None => load::load_caida(&mut table, &fetcher, family, args.date)?,
        };
        log::info!("{}: {} prefixes from {}", family, info.prefixes, info.file);
        snapshots.push(info);
    }

    let records = lookup_all(&table, &items, &snapshots);
    let summary = Summary::of(&records);
    let report = Report {
        tool: Tool {
            name: "originet",
            version: originet::VERSION,
        },
        generated_at: jiff::Timestamp::now().round(jiff::Unit::Second)?.to_string(),
        snapshots,
        summary,
        results: records,
    };

    write_report(args, &report)?;

    if !args.quiet {
        let used: Vec<String> = report
            .snapshots
            .iter()
            .map(|s| format!("{} {}", s.family, s.date.as_deref().unwrap_or(&s.file)))
            .collect();
        eprintln!(
            "{} addresses: {} found, {} not found, {} invalid ({}; {:.1?})",
            summary.total,
            summary.found,
            summary.not_found,
            summary.invalid,
            if used.is_empty() {
                "no snapshot".to_owned()
            } else {
                used.join(", ")
            },
            started.elapsed()
        );
    }
    if args.strict && summary.invalid > 0 {
        bail!("{} input(s) were not IP addresses (--strict)", summary.invalid);
    }
    Ok(ExitCode::SUCCESS)
}

/// `--fetch`: put both families' snapshots for the day in the cache and report them.
fn fetch_snapshots(args: &Args, fetcher: &Fetcher, started: Instant) -> anyhow::Result<ExitCode> {
    let mut used = Vec::new();
    for family in [Family::V4, Family::V6] {
        let ready = load::fetch_snapshot(fetcher, family, args.date)?;
        let status = if ready.cached { "cached" } else { "downloaded" };
        println!(
            "{family}  {}  {status:<10}  {}",
            ready.snapshot.date,
            ready.path.display()
        );
        used.push(format!("{family} {} {status}", ready.snapshot.date));
    }
    if !args.quiet {
        eprintln!(
            "{} snapshots ready in {} ({}; {:.1?})",
            used.len(),
            fetcher.cache_dir().display(),
            used.join(", "),
            started.elapsed()
        );
    }
    Ok(ExitCode::SUCCESS)
}

fn collect_items(args: &Args) -> anyhow::Result<Vec<Item>> {
    let mut items: Vec<Item> = args.addresses.iter().map(|text| Item::parse(text)).collect();
    if let Some(path) = &args.file {
        let from_file = if path.as_os_str() == "-" {
            input::read_items(io::stdin().lock()).context("reading standard input")?
        } else {
            let file = File::open(path).with_context(|| format!("opening {}", path.display()))?;
            input::read_items(BufReader::new(file)).with_context(|| format!("reading {}", path.display()))?
        };
        items.extend(from_file);
    }
    Ok(items)
}

/// Which address families the input needs.
#[derive(Debug, Clone, Copy, Default)]
struct Needs {
    v4: bool,
    v6: bool,
}

impl Needs {
    fn of(items: &[Item]) -> Self {
        let mut needs = Self::default();
        for address in items.iter().filter_map(Item::address) {
            match address {
                IpAddr::V4(_) => needs.v4 = true,
                IpAddr::V6(_) => needs.v6 = true,
            }
        }
        needs
    }

    fn has(self, family: Family) -> bool {
        match family {
            Family::V4 => self.v4,
            Family::V6 => self.v6,
        }
    }
}

fn cache_dir(explicit: Option<PathBuf>) -> anyhow::Result<PathBuf> {
    if let Some(dir) = explicit {
        return Ok(dir);
    }
    directories::ProjectDirs::from("", "", "originet")
        .map(|dirs| dirs.cache_dir().to_path_buf())
        .context("no cache directory could be determined; pass --cache-dir")
}

fn lookup_all(table: &Table, items: &[Item], snapshots: &[SnapshotInfo]) -> Vec<Record> {
    let snapshot_date = |family: Family| {
        snapshots
            .iter()
            .find(|s| s.family == family)
            .and_then(|s| s.date.clone())
    };
    let dates = (snapshot_date(Family::V4), snapshot_date(Family::V6));
    items
        .iter()
        .map(|item| match item {
            Item::Invalid { text, reason } => Record {
                ip: text.clone(),
                prefix: None,
                asn: None,
                origins: None,
                snapshot: None,
                error: Some(reason.clone()),
            },
            Item::Address(address) => {
                let hit = table.lookup(*address);
                Record {
                    ip: address.to_string(),
                    prefix: hit.as_ref().map(|m| m.prefix.to_string()),
                    asn: hit.as_ref().map(|m| m.origins.primary()),
                    origins: hit.as_ref().map(|m| m.origins.raw().to_owned()),
                    snapshot: match address {
                        IpAddr::V4(_) => dates.0.clone(),
                        IpAddr::V6(_) => dates.1.clone(),
                    },
                    error: None,
                }
            }
        })
        .collect()
}

/// The format asked for, else `table` for a person at a terminal and `json` for a file
/// or a pipe.
fn format(args: &Args) -> Format {
    args.format.unwrap_or_else(|| {
        if args.output.is_none() && io::stdout().is_terminal() {
            Format::Table
        } else {
            Format::Json
        }
    })
}

fn write_report(args: &Args, report: &Report) -> anyhow::Result<()> {
    let format = format(args);
    if let Some(path) = &args.output {
        let file = File::create(path).with_context(|| format!("creating {}", path.display()))?;
        let mut writer = BufWriter::new(file);
        output::write(format, report, &mut writer).with_context(|| format!("writing {}", path.display()))?;
        writer.flush()?;
        return Ok(());
    }
    if format.is_binary() && io::stdout().is_terminal() {
        bail!("refusing to write {format:?} output to a terminal; use --output or redirect stdout");
    }
    let stdout = io::stdout();
    let mut writer = BufWriter::new(stdout.lock());
    output::write(format, report, &mut writer).context("writing standard output")
}
