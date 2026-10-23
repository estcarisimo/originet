# Output formats

Every format carries the same six columns per input, in input order:

| Column | Meaning |
| --- | --- |
| `ip` | The address, normalized (`::ffff:8.8.8.8` becomes `8.8.8.8`; invalid inputs are kept as typed) |
| `prefix` | The longest matching prefix, or null when none covers the address |
| `asn` | The origin ASN: the first ASN of the `origins` field, or null |
| `origins` | All origins in CAIDA's notation: `15169`, `149511_138421` (multi-origin), `64496,64497` (AS set) |
| `snapshot` | The date of the snapshot that answered (`YYYY-MM-DD`), when known |
| `error` | Why the input could not be looked up; only set for inputs that are not IP addresses |

Null is `null` in JSON, an empty field in CSV/TSV, `-` in the table and a real null in
Parquet.

Without `-F/--format`, the `table` is used when standard output is a terminal and
`json` when it is a pipe or `--output` names a file.

## `table`

Aligned columns for reading at the prompt, results only (the snapshots and totals go
to standard error as the summary line). A missing value is `-`; the `error` column is
empty unless the input was invalid.

```text
$ originet 8.8.8.8 2001:4860::8888 1.1.8.9 10.0.0.1 not-an-ip
ip               prefix             asn  origins        snapshot    error
8.8.8.8          8.8.8.0/24       15169  15169          2026-10-21
2001:4860::8888  2001:4860::/32   15169  15169          2026-10-21
1.1.8.9          1.1.8.0/24      149511  149511_138421  2026-10-21
10.0.0.1         -                    -  -              2026-10-21
not-an-ip        -                    -  -              -           invalid IP address syntax
5 addresses: 3 found, 1 not found, 1 invalid (ipv4 2026-10-21, ipv6 2026-10-21; 1.2s)
```

## `json`

One document with the tool version, the snapshots used, the totals and the results; the
default for pipes and files.

```json
{
  "tool": { "name": "originet", "version": "1.0.0" },
  "generated_at": "2026-10-22T22:55:08Z",
  "snapshots": [
    {
      "family": "ipv4",
      "source": "caida",
      "date": "2026-10-21",
      "file": "routeviews-rv2-20261021-0800.pfx2as.gz",
      "location": "https://publicdata.caida.org/datasets/routing/routeviews-prefix2as/2026/10/routeviews-rv2-20261021-0800.pfx2as.gz",
      "prefixes": 1129523
    }
  ],
  "summary": { "total": 2, "found": 1, "not_found": 1, "invalid": 0 },
  "results": [
    { "ip": "8.8.8.8", "prefix": "8.8.8.0/24", "asn": 15169, "origins": "15169", "snapshot": "2026-10-21", "error": null },
    { "ip": "10.0.0.1", "prefix": null, "asn": null, "origins": null, "snapshot": "2026-10-21", "error": null }
  ]
}
```

`source` is `caida` for a downloaded (or cached) snapshot and `local` for a `--pfx2as`
file; `location` is then the URL or the path.

## `jsonl`

One result object per line, nothing else. The natural format for pipes and for very
large batches.

```text
{"ip":"8.8.8.8","prefix":"8.8.8.0/24","asn":15169,"origins":"15169","snapshot":"2026-10-21","error":null}
```

## `csv` and `tsv`

A header line and one row per result; minimal quoting.

```text
ip,prefix,asn,origins,snapshot,error
8.8.8.8,8.8.8.0/24,15169,15169,2026-10-21,
10.0.0.1,,,,2026-10-21,
bogus,,,,,invalid IP address syntax
```

## `parquet`

The same six columns (`ip`, `prefix`, `origins`, `snapshot`, `error` as UTF-8 strings,
`asn` as nullable `UInt32`) with Snappy compression; the file's `created_by` names the
tool version. The writer refuses to stream Parquet to a terminal: pass `--output` or
redirect standard output. Parquet is a default Cargo feature; build with
`--no-default-features` to drop it (and the arrow dependency).

## Exit status and standard error

| Status | Meaning |
| --- | --- |
| 0 | Done (invalid inputs are reported per row, not as a failure) |
| 1 | An error: no input, a file that could not be read, a snapshot that could not be fetched, `--strict` with invalid inputs |
| 2 | Usage error (clap) |

Standard error gets a one-line summary unless `--quiet`, warnings (for example a
snapshot date that had to be moved earlier), and with `-v` what is fetched or read from
the cache. `RUST_LOG` is honored too. On a terminal it also shows a progress bar while a
snapshot downloads and a spinner while it loads, cleared when done; `--quiet` turns them
off.
