# Rust library

The binary is a thin layer over the `map_ip_to_asn` crate, so the same pieces are
available to Rust programs. The API documentation is on
[docs.rs](https://docs.rs/map-ip-to-asn) once the crate is published; until then,
`cargo doc --open` builds it locally.

```rust
use map_ip_to_asn::caida::Family;
use map_ip_to_asn::fetch::Fetcher;
use map_ip_to_asn::load;
use map_ip_to_asn::table::Table;

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

Modules:

| Module | What it does |
| --- | --- |
| `pfx2as` | Parses the `pfx2as` format line by line, gzip or plain; `Origins` keeps the field verbatim and knows whether it is multi-origin or an AS set |
| `table` | `Table` holds one longest-prefix-match trie per family; `lookup` returns the prefix and a reference to its origins |
| `caida` | Dataset URLs, `Index::parse` of `pfx2as-creation.log`, `Index::select` by date |
| `fetch` | `Fetcher`: cached index (six-hour freshness), cached snapshots, atomic downloads, offline mode |
| `load` | `load_local` and `load_caida` fill a table and return a `SnapshotInfo` with the provenance |
| `input` | `Item::parse` and `read_items` for addresses from text |
| `output` | `Record`, `Report`, `Summary`, and `write` for every format |

The crate has one default feature, `parquet`, which pulls in `arrow` and `parquet`.
Disable it with `default-features = false` when you do not need Parquet output.
