# originet (Python)

Fast IP-to-ASN lookups on CAIDA's RouteViews prefix-to-AS snapshots. The table is a
Rust longest-prefix-match trie; bulk lookups release the GIL.

```python
from originet import Table

table = Table.from_caida("2024-01-01")  # newest snapshot on or before that day, cached
table.lookup("8.8.8.8")  # Match(ip='8.8.8.8', prefix='8.8.8.0/24', asn=15169, origins='15169')
table.lookup_asns(["1.1.1.1", "2001:4860::8888"])  # [13335, 15169]
```

Full documentation: https://estcarisimo.github.io/originet/python/
