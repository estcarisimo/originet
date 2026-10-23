# Rust library

The binary is a thin layer over the `originet` crate, so the same pieces are available
to Rust programs: `cargo add originet` and read the API documentation on
[docs.rs](https://docs.rs/originet).

```rust
use originet::caida::Family;
use originet::fetch::Fetcher;
use originet::load;
use originet::table::Table;

fn main() -> anyhow::Result<()> {
    let fetcher = Fetcher::new("/tmp/pfx2as-cache".into());
    let mut table = Table::new();
    let info = load::load_caida(&mut table, &fetcher, Family::V4, None)?;
    println!("{} prefixes from {}", info.prefixes, info.file);

    if let Some(hit) = table.lookup("8.8.8.8".parse()?) {
        println!("{} -> {} (origins {})", hit.prefix, hit.origins.primary(), hit.origins);
    }
    Ok(())
}
```

`Fetcher::offline`, `refresh_index` and `progress` are builder methods; the last takes
an implementation of `fetch::Progress`, which is told when a snapshot starts
downloading (with its size, when known), how many bytes arrived, and when it starts and
finishes loading. The binary's progress bars are one such implementation; a library
user may log, or pass nothing and hear nothing. `load::fetch_snapshot` resolves the
snapshot for a date and puts it in the cache without loading it (what `--fetch` does).

Modules:

| Module | What it does |
| --- | --- |
| `pfx2as` | Parses the `pfx2as` format line by line, gzip or plain; `Origins` keeps the field verbatim and knows whether it is multi-origin or an AS set |
| `table` | `Table` holds one longest-prefix-match trie per family; `lookup` returns the prefix and a reference to its origins |
| `caida` | Dataset URLs, `Index::parse` of the rolling `pfx2as-creation.log`, `parse_listing` of a month's directory page, `Index::on_or_before` |
| `fetch` | `Fetcher`: cached index (six-hour freshness), cached snapshots, atomic downloads, offline mode |
| `cache` | `Inventory::scan` lists the cached snapshots with their sizes; `Retention` (`keep_days`, `keep_last`) and `prune` remove the old ones and the directories they empty, with a dry run; `format_bytes` |
| `load` | `select_snapshot` resolves a date through the index and the monthly listings; `load_local` and `load_caida` fill a table and return a `SnapshotInfo` with the provenance |
| `input` | `Item::parse` and `read_items` for addresses from text |
| `output` | `Record`, `Report`, `Summary`, and `write` for every format |

The crate has one default feature, `parquet`, which pulls in `arrow` and `parquet`.
Disable it with `default-features = false` when you do not need Parquet output.
