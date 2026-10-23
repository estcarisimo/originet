//! End-to-end tests of the binary against the fixture snapshots. No network.

use std::fs;
use std::path::PathBuf;

use assert_cmd::Command;
use originet::cache::format_bytes;
use predicates::prelude::*;

fn fixture(name: &str) -> PathBuf {
    PathBuf::from(env!("CARGO_MANIFEST_DIR"))
        .join("tests/fixtures")
        .join(name)
}

fn cmd() -> Command {
    let mut cmd = Command::cargo_bin("originet").unwrap();
    cmd.env_remove("RUST_LOG").env_remove("ORIGINET_CACHE_DIR");
    cmd
}

fn with_fixtures() -> Command {
    let mut cmd = cmd();
    cmd.arg("--pfx2as")
        .arg(fixture("sample-v4.pfx2as.gz"))
        .arg("--pfx2as6")
        .arg(fixture("sample-v6.pfx2as"));
    cmd
}

#[test]
fn json_report_for_mixed_addresses() {
    let output = with_fixtures()
        .args([
            "8.8.8.8",
            "2001:4860::8888",
            "1.1.8.9",
            "203.0.113.1",
            "not-an-ip",
        ])
        .output()
        .unwrap();
    assert!(
        output.status.success(),
        "{}",
        String::from_utf8_lossy(&output.stderr)
    );
    let report: serde_json::Value = serde_json::from_slice(&output.stdout).unwrap();

    assert_eq!(report["tool"]["name"], "originet");
    assert_eq!(report["summary"]["total"], 5);
    assert_eq!(report["summary"]["found"], 3);
    assert_eq!(report["summary"]["not_found"], 1);
    assert_eq!(report["summary"]["invalid"], 1);

    let snapshots = report["snapshots"].as_array().unwrap();
    assert_eq!(snapshots.len(), 2);
    assert_eq!(snapshots[0]["family"], "ipv4");
    assert_eq!(snapshots[0]["source"], "local");
    assert_eq!(snapshots[0]["prefixes"], 9);
    assert_eq!(snapshots[1]["family"], "ipv6");
    assert_eq!(snapshots[1]["prefixes"], 5);

    let results = report["results"].as_array().unwrap();
    assert_eq!(results[0]["ip"], "8.8.8.8");
    assert_eq!(results[0]["prefix"], "8.8.8.0/24");
    assert_eq!(results[0]["asn"], 15169);
    assert_eq!(results[1]["prefix"], "2001:4860::/32");
    assert_eq!(results[1]["asn"], 15169);
    assert_eq!(results[2]["asn"], 149511);
    assert_eq!(results[2]["origins"], "149511_138421");
    assert!(results[3]["asn"].is_null());
    assert!(results[3]["prefix"].is_null());
    assert!(results[3]["error"].is_null());
    assert_eq!(results[4]["ip"], "not-an-ip");
    assert!(results[4]["error"].is_string());

    let stderr = String::from_utf8_lossy(&output.stderr);
    assert!(
        stderr.contains("5 addresses: 3 found, 1 not found, 1 invalid"),
        "{stderr}"
    );
}

#[test]
fn only_the_needed_family_is_loaded() {
    // IPv4 input only: the IPv6 file is not required, even though it is absent.
    let output = cmd()
        .arg("--pfx2as")
        .arg(fixture("sample-v4.pfx2as.gz"))
        .args(["--quiet", "9.9.9.9"])
        .output()
        .unwrap();
    assert!(
        output.status.success(),
        "{}",
        String::from_utf8_lossy(&output.stderr)
    );
    let report: serde_json::Value = serde_json::from_slice(&output.stdout).unwrap();
    assert_eq!(report["snapshots"].as_array().unwrap().len(), 1);
    assert_eq!(report["results"][0]["asn"], 19281);
    assert_eq!(output.stderr, b"");
}

#[test]
fn reads_a_file_and_stdin() {
    let dir = tempfile::tempdir().unwrap();
    let list = dir.path().join("ips.txt");
    fs::write(&list, "# comment\n8.8.8.8\n\n1.1.1.1 # trailing comment\n").unwrap();

    with_fixtures()
        .arg("--file")
        .arg(&list)
        .args(["-F", "jsonl", "-q"])
        .assert()
        .success()
        .stdout(
            predicate::str::contains("\"ip\":\"8.8.8.8\"")
                .and(predicate::str::contains("\"ip\":\"1.1.1.1\"")),
        );

    with_fixtures()
        .args(["-f", "-", "-F", "tsv", "-q"])
        .write_stdin("64.1.2.3\n64.9.9.9\n")
        .assert()
        .success()
        .stdout("ip\tprefix\tasn\torigins\tsnapshot\terror\n64.1.2.3\t64.1.0.0/16\t64498\t64498\t\t\n64.9.9.9\t64.0.0.0/8\t64496\t64496,64497\t\t\n");
}

#[test]
fn table_format_and_the_piped_default() {
    // Standard output is a pipe here, so the default stays JSON; `-F table` is explicit.
    with_fixtures()
        .args(["8.8.8.8", "64.9.9.9", "10.0.0.1", "bogus", "-F", "table", "-q"])
        .assert()
        .success()
        .stdout(
            "\
ip        prefix                 asn  snapshot  error
8.8.8.8   8.8.8.0/24           15169  -
64.9.9.9  64.0.0.0/8  {64496, 64497}  -
10.0.0.1  -                        -  -
bogus     -                        -  -         invalid IP address syntax
",
        );
    let output = with_fixtures().args(["8.8.8.8", "-q"]).output().unwrap();
    assert!(output.stdout.starts_with(b"{\n"), "piped default is JSON");
}

#[test]
fn color_is_off_in_a_pipe_unless_asked() {
    // Not a terminal: `auto` and `never` write plain text, `always` writes escapes.
    for when in ["auto", "never"] {
        with_fixtures()
            .args(["8.8.8.8", "bogus", "-F", "table", "--color", when])
            .assert()
            .success()
            .stdout(predicate::str::contains("\x1b[").not())
            .stderr(predicate::str::contains("\x1b[").not());
    }
    let output = with_fixtures()
        .args(["8.8.8.8", "bogus", "-F", "table", "--color", "always"])
        .output()
        .unwrap();
    let stdout = String::from_utf8(output.stdout).unwrap();
    assert!(stdout.starts_with("\x1b[1mip\x1b[0m"), "{stdout:?}");
    assert!(stdout.contains("\x1b[31mbogus\x1b[0m"), "{stdout:?}");
    let stderr = String::from_utf8(output.stderr).unwrap();
    assert!(
        stderr.contains("\x1b[1m\x1b[32m1\x1b[0m found") && stderr.contains("\x1b[31m1\x1b[0m invalid"),
        "{stderr:?}"
    );
    // Other formats are never colored.
    with_fixtures()
        .args(["8.8.8.8", "-F", "csv", "-q", "--color", "always"])
        .assert()
        .success()
        .stdout(predicate::str::contains("\x1b[").not());
}

#[test]
fn csv_to_a_file() {
    let dir = tempfile::tempdir().unwrap();
    let out = dir.path().join("results.csv");
    with_fixtures()
        .args(["157.92.49.99", "-F", "csv", "-q", "-o"])
        .arg(&out)
        .assert()
        .success()
        .stdout("");
    let text = fs::read_to_string(&out).unwrap();
    assert_eq!(
        text,
        "ip,prefix,asn,origins,snapshot,error\n157.92.49.99,157.92.0.0/16,3597,3597,,\n"
    );
}

#[cfg(feature = "parquet")]
#[test]
fn parquet_to_a_file() {
    let dir = tempfile::tempdir().unwrap();
    let out = dir.path().join("results.parquet");
    with_fixtures()
        .args(["8.8.4.4", "-F", "parquet", "-q", "-o"])
        .arg(&out)
        .assert()
        .success();
    let bytes = fs::read(&out).unwrap();
    assert!(
        bytes.starts_with(b"PAR1") && bytes.ends_with(b"PAR1"),
        "not a parquet file"
    );
}

#[test]
fn snapshot_date_comes_from_the_file_name() {
    let dir = tempfile::tempdir().unwrap();
    let named = dir.path().join("routeviews-rv2-20240102-1200.pfx2as");
    fs::copy(fixture("sample-v4.pfx2as"), &named).unwrap();
    let output = cmd()
        .arg("--pfx2as")
        .arg(&named)
        .args(["8.8.8.8", "-F", "jsonl", "-q"])
        .output()
        .unwrap();
    assert!(output.status.success());
    let record: serde_json::Value = serde_json::from_slice(&output.stdout).unwrap();
    assert_eq!(record["snapshot"], "2024-01-02");
}

#[test]
fn strict_fails_on_invalid_input() {
    with_fixtures()
        .args(["--strict", "-q", "8.8.8.8", "bogus"])
        .assert()
        .code(1)
        .stderr(predicate::str::contains("1 input(s) were not IP addresses"));
}

#[test]
fn no_input_is_an_error() {
    with_fixtures()
        .assert()
        .code(1)
        .stderr(predicate::str::contains("no addresses given"));
}

#[test]
fn offline_without_cache_fails_clearly() {
    let dir = tempfile::tempdir().unwrap();
    cmd()
        .arg("--cache-dir")
        .arg(dir.path())
        .args(["--offline", "8.8.8.8"])
        .assert()
        .code(1)
        .stderr(predicate::str::contains(
            "offline and the ipv4 index is not cached",
        ));
}

#[test]
fn offline_with_a_cached_snapshot_works() {
    // Seed the cache exactly as the fetcher lays it out and resolve a date through the index.
    let dir = tempfile::tempdir().unwrap();
    let dataset = dir.path().join("routeviews-prefix2as");
    fs::create_dir_all(dataset.join("2026/10")).unwrap();
    fs::copy(
        fixture("pfx2as-creation.log"),
        dataset.join("pfx2as-creation.log"),
    )
    .unwrap();
    fs::copy(
        fixture("sample-v4.pfx2as.gz"),
        dataset.join("2026/10/routeviews-rv2-20261020-1000.pfx2as.gz"),
    )
    .unwrap();

    let output = cmd()
        .arg("--cache-dir")
        .arg(dir.path())
        .args(["--offline", "--date", "2026-10-20", "8.8.8.8"])
        .output()
        .unwrap();
    assert!(
        output.status.success(),
        "{}",
        String::from_utf8_lossy(&output.stderr)
    );
    let report: serde_json::Value = serde_json::from_slice(&output.stdout).unwrap();
    assert_eq!(report["snapshots"][0]["source"], "caida");
    assert_eq!(report["snapshots"][0]["date"], "2026-10-20");
    assert_eq!(
        report["snapshots"][0]["file"],
        "routeviews-rv2-20261020-1000.pfx2as.gz"
    );
    assert_eq!(report["results"][0]["snapshot"], "2026-10-20");
    assert_eq!(report["results"][0]["asn"], 15169);

    // The newest snapshot (2026-10-21) is listed but not cached: offline must say so.
    cmd()
        .arg("--cache-dir")
        .arg(dir.path())
        .args(["--offline", "8.8.8.8"])
        .assert()
        .code(1)
        .stderr(predicate::str::contains(
            "snapshot routeviews-rv2-20261021-0800.pfx2as.gz is not cached",
        ));
}

#[test]
fn fetch_reports_cached_snapshots_and_takes_no_addresses() {
    let dir = tempfile::tempdir().unwrap();
    for (dataset, name) in [
        ("routeviews-prefix2as", "routeviews-rv2-20261020-1000.pfx2as.gz"),
        ("routeviews6-prefix2as", "routeviews-rv6-20261020-1200.pfx2as.gz"),
    ] {
        let dataset = dir.path().join(dataset);
        fs::create_dir_all(dataset.join("2026/10")).unwrap();
        fs::write(
            dataset.join("pfx2as-creation.log"),
            format!("6776\t1792599333\t2026/10/{name}\n"),
        )
        .unwrap();
        fs::copy(fixture("sample-v4.pfx2as.gz"), dataset.join("2026/10").join(name)).unwrap();
    }
    // Joined component by component: the binary prints the platform's separators.
    let v4 = [
        "routeviews-prefix2as",
        "2026",
        "10",
        "routeviews-rv2-20261020-1000.pfx2as.gz",
    ]
    .iter()
    .fold(dir.path().to_path_buf(), |path, part| path.join(part));
    let output = cmd()
        .arg("--cache-dir")
        .arg(dir.path())
        .args(["--fetch", "--offline"])
        .output()
        .unwrap();
    assert!(
        output.status.success(),
        "{}",
        String::from_utf8_lossy(&output.stderr)
    );
    let stdout = String::from_utf8_lossy(&output.stdout);
    let lines: Vec<_> = stdout.lines().collect();
    assert_eq!(lines.len(), 2, "{stdout}");
    assert_eq!(
        lines[0],
        format!("ipv4  2026-10-20  cached      {}", v4.display())
    );
    assert!(lines[1].starts_with("ipv6  2026-10-20  cached      "), "{stdout}");
    let stderr = String::from_utf8_lossy(&output.stderr);
    assert!(
        stderr.contains("2 snapshots ready in") && stderr.contains("ipv6 2026-10-20 cached"),
        "{stderr}"
    );

    // A snapshot the index lists but the cache lacks is an error offline.
    fs::remove_file(&v4).unwrap();
    cmd()
        .arg("--cache-dir")
        .arg(dir.path())
        .args(["--fetch", "--offline"])
        .assert()
        .code(1)
        .stderr(predicate::str::contains(
            "snapshot routeviews-rv2-20261020-1000.pfx2as.gz is not cached",
        ));

    // Fetching and looking up are different runs.
    cmd()
        .args(["--fetch", "8.8.8.8"])
        .assert()
        .code(2)
        .stderr(predicate::str::contains("cannot be used with"));
    cmd().args(["--fetch", "--format", "csv"]).assert().code(2);
}

#[test]
fn date_with_a_local_file_warns() {
    with_fixtures()
        .args(["--date", "1999-01-01", "8.8.8.8", "-F", "csv", "-q"])
        .assert()
        .success()
        .stderr(predicate::str::contains(
            "--date 1999-01-01 does not apply to ipv4",
        ));
}

#[test]
fn errors_are_printed_once() {
    let dir = tempfile::tempdir().unwrap();
    let output = cmd()
        .arg("--pfx2as")
        .arg(fixture("sample-v6.pfx2as"))
        .arg("--cache-dir")
        .arg(dir.path())
        .arg("8.8.8.8")
        .output()
        .unwrap();
    let stderr = String::from_utf8_lossy(&output.stderr);
    assert_eq!(stderr.matches("expected an ipv4 prefix").count(), 1, "{stderr}");
}

#[test]
fn rejects_a_bad_date() {
    with_fixtures()
        .args(["--date", "2026-13-01", "8.8.8.8"])
        .assert()
        .code(2)
        .stderr(predicate::str::contains("expected YYYY-MM-DD"));
}

#[test]
fn wrong_family_file_is_rejected() {
    cmd()
        .arg("--pfx2as")
        .arg(fixture("sample-v6.pfx2as"))
        .arg("8.8.8.8")
        .assert()
        .code(1)
        .stderr(predicate::str::contains(
            "line 1: expected an ipv4 prefix, got 2001::/32",
        ));
}

#[test]
fn help_and_version() {
    cmd()
        .arg("--help")
        .assert()
        .success()
        .stdout(predicate::str::contains("--pfx2as6"));
    cmd()
        .arg("--version")
        .assert()
        .success()
        .stdout(predicate::str::starts_with("originet "));
}

/// Seeds a cache as the fetcher lays it out: two IPv4 months (one of them with a
/// listing), one IPv6 month, both indexes. Returns the paths of the snapshots, IPv4
/// first, oldest first, as the binary prints them (joined component by component: the
/// binary prints the platform's separators).
fn seed_cache(root: &std::path::Path) -> Vec<PathBuf> {
    let mut paths = Vec::new();
    for (dataset, year, month, name) in [
        (
            "routeviews-prefix2as",
            "2015",
            "01",
            "routeviews-rv2-20150101-1200.pfx2as.gz",
        ),
        (
            "routeviews-prefix2as",
            "2026",
            "10",
            "routeviews-rv2-20261020-1000.pfx2as.gz",
        ),
        (
            "routeviews-prefix2as",
            "2026",
            "10",
            "routeviews-rv2-20261021-0800.pfx2as.gz",
        ),
        (
            "routeviews6-prefix2as",
            "2026",
            "10",
            "routeviews-rv6-20261021-1200.pfx2as.gz",
        ),
    ] {
        let dir = root.join(dataset).join(year).join(month);
        fs::create_dir_all(&dir).unwrap();
        let path = dir.join(name);
        fs::copy(fixture("sample-v4.pfx2as.gz"), &path).unwrap();
        paths.push(path);
    }
    // Written, not copied from a fixture: git may check a text fixture out with CRLF
    // line endings on Windows, and the sizes below must be the same on every platform.
    for dataset in ["routeviews-prefix2as", "routeviews6-prefix2as"] {
        fs::write(root.join(dataset).join("pfx2as-creation.log"), SEED_INDEX).unwrap();
    }
    fs::write(root.join("routeviews-prefix2as/2015/01/.listing"), SEED_LISTING).unwrap();
    paths
}

const SEED_INDEX: &str = "6777\t1792685687\t2026/10/routeviews-rv2-20261021-0800.pfx2as.gz\n";
const SEED_LISTING: &str = "routeviews-rv2-20150101-1200.pfx2as.gz\n";

/// The sizes `--cache` reports for the seeded cache: one snapshot, the IPv4 dataset
/// (three snapshots, the listing, the index) and the IPv6 dataset (one snapshot, the index).
fn seeded_bytes() -> (u64, u64, u64) {
    let snapshot = fs::metadata(fixture("sample-v4.pfx2as.gz")).unwrap().len();
    let index = u64::try_from(SEED_INDEX.len()).unwrap();
    let listing = u64::try_from(SEED_LISTING.len()).unwrap();
    (snapshot, 3 * snapshot + listing + index, snapshot + index)
}

#[test]
fn cache_list_and_size() {
    let dir = tempfile::tempdir().unwrap();
    let paths = seed_cache(dir.path());
    let (_, v4, v6) = seeded_bytes();
    // The fixture is 200 bytes; the indexes are 272 each and the listing 39.
    let output = cmd()
        .arg("--cache-dir")
        .arg(dir.path())
        .args(["--cache", "list"])
        .output()
        .unwrap();
    assert!(
        output.status.success(),
        "{}",
        String::from_utf8_lossy(&output.stderr)
    );
    let stdout = String::from_utf8_lossy(&output.stdout);
    assert_eq!(
        stdout,
        format!(
            "ipv4  2015-01-01     200 B  {}\nipv4  2026-10-20     200 B  {}\nipv4  2026-10-21     200 B  {}\nipv6  2026-10-21     200 B  {}\n",
            paths[0].display(),
            paths[1].display(),
            paths[2].display(),
            paths[3].display()
        )
    );
    let stderr = String::from_utf8_lossy(&output.stderr);
    assert!(
        stderr.contains(&format!("4 snapshots, {} in", format_bytes(v4 + v6)))
            && stderr.contains(&format!(
                "(ipv4 3 snapshots {}, ipv6 1 snapshot {};",
                format_bytes(v4),
                format_bytes(v6)
            )),
        "{stderr}"
    );

    cmd()
        .arg("--cache-dir")
        .arg(dir.path())
        .args(["--cache", "size", "-q"])
        .assert()
        .success()
        .stdout(format!(
            "{:<5}  {:>14}  {:>8}\n{:<5}  {:>14}  {:>8}\n{:<5}  {:>14}  {:>8}  {}\n",
            "ipv4",
            "3 snapshots",
            format_bytes(v4),
            "ipv6",
            "1 snapshot",
            format_bytes(v6),
            "total",
            "4 snapshots",
            format_bytes(v4 + v6),
            dir.path().display()
        ))
        .stderr("");

    // An empty (even missing) cache is not an error.
    cmd()
        .arg("--cache-dir")
        .arg(dir.path().join("missing"))
        .args(["--cache", "list"])
        .assert()
        .success()
        .stdout("")
        .stderr(predicate::str::starts_with("0 snapshots, 0 B in"));
}

#[test]
fn cache_prune_keep_last_with_a_dry_run_first() {
    let dir = tempfile::tempdir().unwrap();
    let paths = seed_cache(dir.path());
    let (snapshot, v4_bytes, v6_bytes) = seeded_bytes();
    let v4 = dir.path().join("routeviews-prefix2as");

    // The dry run names what would go and touches nothing.
    let output = cmd()
        .arg("--cache-dir")
        .arg(dir.path())
        .args(["--cache", "prune", "--keep-last", "1", "--dry-run"])
        .output()
        .unwrap();
    assert!(
        output.status.success(),
        "{}",
        String::from_utf8_lossy(&output.stderr)
    );
    assert_eq!(
        String::from_utf8_lossy(&output.stdout),
        format!(
            "ipv4  2015-01-01     200 B  would remove  {}\nipv4  2026-10-20     200 B  would remove  {}\n",
            paths[0].display(),
            paths[1].display()
        )
    );
    let stderr = String::from_utf8_lossy(&output.stderr);
    assert!(
        stderr.starts_with(&format!(
            "2 snapshots would be removed ({}), 2 kept ({}) in",
            format_bytes(2 * snapshot),
            // "kept" is the kept snapshots' own size, not indexes or listings.
            format_bytes(2 * snapshot)
        )),
        "{stderr}"
    );
    assert!(paths.iter().all(|path| path.is_file()));
    assert!(v4.join("2015/01/.listing").is_file());

    let output = cmd()
        .arg("--cache-dir")
        .arg(dir.path())
        .args(["--cache", "prune", "--keep-last", "1"])
        .output()
        .unwrap();
    assert!(
        output.status.success(),
        "{}",
        String::from_utf8_lossy(&output.stderr)
    );
    assert_eq!(
        String::from_utf8_lossy(&output.stdout),
        format!(
            "ipv4  2015-01-01     200 B  removed       {}\nipv4  2026-10-20     200 B  removed       {}\n",
            paths[0].display(),
            paths[1].display()
        )
    );
    let stderr = String::from_utf8_lossy(&output.stderr);
    assert!(
        stderr.starts_with(&format!(
            "2 snapshots removed ({}), 2 kept ({}) in",
            format_bytes(2 * snapshot),
            format_bytes(v4_bytes + v6_bytes - 2 * snapshot)
        )) && stderr.contains(&format!(
            "(ipv4 1 snapshot {v6}, ipv6 1 snapshot {v6};",
            v6 = format_bytes(v6_bytes)
        )),
        "{stderr}"
    );
    // The emptied month went with its listing, then the emptied year; the month that
    // kept a snapshot, both indexes and the other family are untouched.
    assert!(!v4.join("2015").exists());
    assert!(!paths[1].exists());
    assert!(paths[2].is_file() && paths[3].is_file());
    assert!(v4.join("pfx2as-creation.log").is_file());
    assert!(
        dir.path()
            .join("routeviews6-prefix2as/pfx2as-creation.log")
            .is_file()
    );

    // Nothing left to prune: no lines, a summary.
    cmd()
        .arg("--cache-dir")
        .arg(dir.path())
        .args(["--cache", "prune", "--keep-last", "1"])
        .assert()
        .success()
        .stdout("")
        .stderr(predicate::str::starts_with("0 snapshots removed (0 B), 2 kept"));
}

#[test]
fn cache_prune_keep_days_counts_from_today() {
    // One snapshot dated today (UTC) and one from 2015: `--keep-days 30` keeps today's.
    let dir = tempfile::tempdir().unwrap();
    let today = jiff::Timestamp::now().to_zoned(jiff::tz::TimeZone::UTC).date();
    let month_dir = dir
        .path()
        .join("routeviews-prefix2as")
        .join(format!("{:04}", today.year()))
        .join(format!("{:02}", today.month()));
    fs::create_dir_all(&month_dir).unwrap();
    let recent = month_dir.join(format!(
        "routeviews-rv2-{:04}{:02}{:02}-1200.pfx2as.gz",
        today.year(),
        today.month(),
        today.day()
    ));
    fs::copy(fixture("sample-v4.pfx2as.gz"), &recent).unwrap();
    let old_dir = dir.path().join("routeviews-prefix2as").join("2015").join("01");
    fs::create_dir_all(&old_dir).unwrap();
    let old = old_dir.join("routeviews-rv2-20150101-1200.pfx2as.gz");
    fs::copy(fixture("sample-v4.pfx2as.gz"), &old).unwrap();

    cmd()
        .arg("--cache-dir")
        .arg(dir.path())
        .args(["--cache", "prune", "--keep-days", "30", "-q"])
        .assert()
        .success()
        .stdout(format!(
            "ipv4  2015-01-01     200 B  removed       {}\n",
            old.display()
        ))
        .stderr("");
    assert!(recent.is_file());
    assert!(!old_dir.exists());

    // Both rules bound the result: the newest one, but only if it is from today.
    cmd()
        .arg("--cache-dir")
        .arg(dir.path())
        .args(["--cache", "prune", "--keep-days", "0", "--keep-last", "1", "-q"])
        .assert()
        .success()
        .stdout("");
    assert!(recent.is_file());
}

#[test]
fn cache_flags_are_usage_checked() {
    cmd()
        .args(["--cache", "prune"])
        .assert()
        .code(2)
        .stderr(predicate::str::contains(
            "'--cache prune' requires '--keep-days <N>' and/or '--keep-last <N>'",
        ));
    for flag in [["--keep-days", "3"], ["--keep-last", "3"]] {
        cmd()
            .args(flag)
            .assert()
            .code(2)
            .stderr(predicate::str::contains("--cache <ACTION>"));
        cmd()
            .args(["--cache", "list"])
            .args(flag)
            .assert()
            .code(2)
            .stderr(predicate::str::contains("can only be used with '--cache prune'"));
    }
    cmd()
        .args(["--cache", "size", "--dry-run"])
        .assert()
        .code(2)
        .stderr(predicate::str::contains(
            "'--dry-run' can only be used with '--cache prune'",
        ));
    cmd()
        .args(["--cache", "list", "8.8.8.8"])
        .assert()
        .code(2)
        .stderr(predicate::str::contains("cannot be used with"));
    cmd().args(["--cache", "list", "--fetch"]).assert().code(2);
    cmd().args(["--cache", "wipe"]).assert().code(2);
}
