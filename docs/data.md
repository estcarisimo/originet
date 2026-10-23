# The data

## Where the snapshots come from

CAIDA derives a prefix-to-origin-AS mapping from RouteViews BGP tables once a day and
publishes it as the *Routeviews Prefix to AS mappings Dataset (pfx2as)*:

| Family | Directory on `publicdata.caida.org` | File names |
| --- | --- | --- |
| IPv4 | `datasets/routing/routeviews-prefix2as/YYYY/MM/` | `routeviews-rv2-YYYYMMDD-HHMM.pfx2as.gz` |
| IPv6 | `datasets/routing/routeviews6-prefix2as/YYYY/MM/` | `routeviews-rv6-YYYYMMDD-HHMM.pfx2as.gz` |

Each dataset directory also has an index, `pfx2as-creation.log`, listing the files
published in roughly the last thirty days (`seqnum`, Unix timestamp, path). For older
days originet reads the monthly directory listings (`YYYY/MM/`), which exist back
to May 2005 for IPv4 and 2007 for IPv6 (the oldest files are named `routeviews-oix-...`).

## Snapshot selection

- No `--date`: the newest snapshot in the index.
- `--date YYYY-MM-DD` inside the index window: the newest snapshot taken **on or before**
  that day. If the day itself has no snapshot the closest earlier one is used and a
  warning names it.
- `--date` older than the index: the listing of that month, then the previous months,
  up to twelve, until one has a snapshot on or before the day. A date before 2005 is an
  error that says so; a gap longer than twelve months is an error that says how far back
  it looked.

The index is cached and reused for six hours, then fetched again (`--refresh` forces
it). A month's listing is final once it was fetched after that month ended (UTC) and is
then kept forever; one fetched while the month was still running follows the six-hour
rule. Snapshot files never change once published, so they are cached forever.

## The `pfx2as` format

One line per announced prefix, tab-separated:

```text
1.0.0.0	24	13335
1.1.8.0	24	149511_138421
64.0.0.0	8	64496,64497
2001:4:112::	48	112
```

The third field is one origin ASN, several joined with `_` (a **multi-origin** prefix,
MOAS: several ASes announce it) or several joined with `,` (an **AS set**). originet
keeps the field verbatim in `origins` and reports the first ASN as `asn`. Nothing in the
dataset ranks the origins of a MOAS prefix, so treat `asn` as "one of them" and look at
`origins` (or `Match.asns()` in Python) when it matters.

## Longest-prefix match

Every lookup returns the most specific prefix in the snapshot that covers the address,
the same rule routers apply. `prefix` in the output is that prefix. An address that no
prefix covers (private space, unrouted space, or space not seen by RouteViews that day)
gets `prefix` and `asn` set to null.

An IPv4-mapped IPv6 address (`::ffff:8.8.8.8`) is looked up as IPv4. A `/32` or `/128`
host suffix is accepted; any other prefix length is rejected, because the tool maps
addresses, not prefixes.

## Cache

| Platform | Default directory |
| --- | --- |
| Linux | `~/.cache/originet/` |
| macOS | `~/Library/Caches/originet/` |
| Windows | `%LOCALAPPDATA%\originet\cache\` |
| Docker image | `/cache` |

Override it with `--cache-dir` or `ORIGINET_CACHE_DIR`. Inside, files mirror
CAIDA's layout: `routeviews-prefix2as/pfx2as-creation.log`,
`routeviews-prefix2as/2026/10/routeviews-rv2-20261021-0800.pfx2as.gz`, a parsed
listing per month visited (`routeviews-prefix2as/2015/01/.listing`), and the same
under `routeviews6-prefix2as/`. A snapshot is about 4 MB (IPv4) and 1 MB (IPv6)
compressed. Downloads go to a temporary file and are renamed into place, so an
interrupted run never leaves a truncated snapshot behind.

Nothing expires on its own: every day you ask for stays, about 5 MB per day for both
families, and the same day is never downloaded twice. `--cache list` and `--cache size`
show what is there; `--cache prune --keep-days N` or `--keep-last N` (with `--dry-run`
to look first) drops the old snapshots and the month and year directories they leave
empty, and never touches the indexes or anything else (see
[The cache](cli.md#the-cache)). Deleting by hand works too: a month directory
(`routeviews-prefix2as/2019/06/`) drops its snapshots, the whole cache directory starts
over. `--fetch --offline --date DAY` tells whether a day is cached without downloading.

## Terms of use

The dataset is CAIDA's; its
[terms of use](https://www.caida.org/catalog/datasets/routeviews-prefix2as/) apply to
anything you derive from it, and CAIDA asks to be cited (see [Citing](citing.md)).
