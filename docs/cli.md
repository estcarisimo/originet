# Command line

```text
$ originet --help
Map IP addresses to origin ASNs with CAIDA's RouteViews prefix-to-AS snapshots.

Addresses come from the command line, from a file (`--file`, one per line) or from standard input
(`--file -`). The matching snapshot is downloaded from CAIDA on first use and cached; `--date` picks
the newest snapshot on or before a day, and `--pfx2as`/`--pfx2as6` use local files instead of
downloading. `--fetch` only fills the cache.

Usage: originet [OPTIONS] [IP]...

Arguments:
  [IP]...
          IP addresses to look up (IPv4 or IPv6)

Options:
  -f, --file <PATH>
          Read addresses from a file, one per line (`-` for standard input)

  -d, --date <YYYY-MM-DD>
          Use the newest snapshot taken on or before this day (YYYY-MM-DD). Default: the newest

      --pfx2as <PATH>
          Local IPv4 pfx2as file (plain or gzip) instead of downloading from CAIDA

      --pfx2as6 <PATH>
          Local IPv6 pfx2as file (plain or gzip) instead of downloading from CAIDA

  -F, --format <FORMAT>
          Output format. Default: `table` on a terminal, `json` when piped or with `--output`

          Possible values:
          - table:   Aligned columns for reading in a terminal, results only
          - json:    One JSON document with metadata, summary and results
          - jsonl:   One JSON object per line, results only
          - csv:     Comma-separated values with a header, results only
          - tsv:     Tab-separated values with a header, results only
          - parquet: Apache Parquet, results only

  -o, --output <PATH>
          Write the output to this file instead of standard output

      --color <WHEN>
          Color the table, the summary and the progress bar: `auto` only on a terminal (and not with
          `NO_COLOR`), `always` even into a pipe or file, `never` for plain text
          
          [default: auto]
          [possible values: auto, always, never]

      --cache-dir <DIR>
          Directory for downloaded indexes and snapshots
          
          [env: ORIGINET_CACHE_DIR=]

      --offline
          Never download; fail unless the index and snapshot are already cached

      --refresh
          Re-download the snapshot index even if the cached copy is fresh

      --fetch
          Download the IPv4 and IPv6 snapshots for the day (`--date`, default the newest) into the
          cache and exit without looking anything up; print one line per snapshot with its date,
          whether it was already cached, and its path. With `--offline` it only checks that they are
          cached

      --strict
          Exit with status 1 when any input is not an IP address

  -q, --quiet
          Do not print the summary to standard error

  -v, --verbose...
          More detail on standard error (`-v` what is fetched, `-vv` debugging)

  -h, --help
          Print help (see a summary with '-h')

  -V, --version
          Print version

Examples:
  originet 8.8.8.8 2001:4860::8888
  originet --format json 8.8.8.8 2001:4860::8888
  originet --file ips.txt --format csv --output results.csv
  originet --date 2019-06-01 --file ips.txt --format parquet -o june-2019.parquet
  originet --pfx2as routeviews-rv2-20261021-0800.pfx2as.gz --file ips.txt
  cat ips.txt | originet -f - -F jsonl
  originet --fetch --date 2019-06-01
```

## Inputs

Addresses can be given as arguments and, at the same time, from `--file` (one per line;
blank lines and `#` comments are ignored, also at the end of a line; `-` reads standard
input). Each one is an IPv4 or IPv6 address, optionally with a `/32` or `/128` host
suffix; an IPv4-mapped IPv6 address (`::ffff:8.8.8.8`) is looked up as IPv4. Anything
else is kept in the output as a row with an `error` and counted as *invalid*; `--strict`
turns that into exit status 1.

Order and duplicates are preserved: the output has exactly one row per input, in the
same order.

## Which snapshot

| Option | Snapshot used |
| --- | --- |
| none | the newest one CAIDA has published |
| `--date 2024-02-29` | the newest one taken on or before 2024-02-29 (a warning names it when the day itself has none) |
| `--pfx2as FILE` | that file for IPv4 (plain or gzip); the date comes from the file name when it follows CAIDA's naming |
| `--pfx2as6 FILE` | that file for IPv6 |
| `--fetch` | downloads the day's IPv4 and IPv6 snapshots into the cache and exits, looking nothing up |

Only the families present in the input are loaded: an IPv4-only input never fetches the
IPv6 snapshot. `--date` applies to the families fetched from CAIDA; a family read from
`--pfx2as` or `--pfx2as6` keeps the file's own date, and the tool warns when both are
given. See [The data](data.md) for how dates are resolved and cached.

`--fetch` fills the cache without a lookup: one line per snapshot on standard output
with its date, `downloaded` or `cached`, and its path. Run it once (or in a cron job, an
image build, a CI step) and later lookups for the same day need no network; with
`--offline` it only checks that the snapshots are there. Addresses, `--file`, `--pfx2as`,
`--format`, `--output` and `--strict` do not combine with it.

```text
$ originet --fetch --date 2026-10-19 --cache-dir ./cache
ipv4  2026-10-19  downloaded  ./cache/routeviews-prefix2as/2026/10/routeviews-rv2-20261019-1200.pfx2as.gz
ipv6  2026-10-19  downloaded  ./cache/routeviews6-prefix2as/2026/10/routeviews-rv6-20261019-1200.pfx2as.gz
2 snapshots ready in ./cache (ipv4 2026-10-19 downloaded, ipv6 2026-10-19 downloaded; 6.1s)
```

## Outputs

`-F/--format` is `table`, `json`, `jsonl`, `csv`, `tsv` or `parquet`. Without it, a
terminal gets the `table` and a pipe or `-o/--output` gets `json`, so `originet 8.8.8.8`
reads well at the prompt and `originet 8.8.8.8 | jq` keeps working; pass `-F` to choose
regardless. Parquet is refused on a terminal. The columns are the same in every format;
see [Output formats](output.md).

On a terminal the table is colored (bold header, cyan prefix, bold green ASN, yellow
multi-origin fields, dim dates and dashes, red invalid inputs), as are the counts of the
summary line. `--color never` gives plain text, `--color always` keeps the escapes even
into a pipe or `--output`, and `auto` (the default) also honors `NO_COLOR`,
`CLICOLOR_FORCE` and `TERM=dumb`. Only the table is ever colored.

Standard error gets a one-line summary, warnings, and with `-v` the downloads and cache
hits. When it is a terminal, a progress bar shows a snapshot downloading (size, speed,
time left) and a spinner shows it loading; `-q` silences the summary and the bars.

## Exit status

`0` done (invalid inputs are rows, not failures); `1` an error (no input, unreadable
file, snapshot not available, `--strict` with invalid inputs); `2` a usage error.

## Examples

```bash
# Enrich a log: JSON Lines in, JSON Lines out, joined on ip
cut -d' ' -f1 access.log | sort -u | originet -f - -F jsonl -q > ip2asn.jsonl

# Compare today's and 2019's origin for the same addresses
originet -F csv -f ips.txt -o now.csv
originet -F csv -f ips.txt --date 2019-06-01 -o 2019.csv

# Keep a cache in the project and never download during a run
originet --cache-dir ./cache --fetch                        # warms the cache
originet --cache-dir ./cache --offline -f ips.txt -F parquet -o out.parquet
```
