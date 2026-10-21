use std::fs;
use std::path::{Path, PathBuf};
use std::process::Command;

fn bin() -> &'static str {
    env!("CARGO_BIN_EXE_datapeek")
}

fn sample(name: &str) -> PathBuf {
    Path::new(env!("CARGO_MANIFEST_DIR"))
        .join("samples")
        .join(name)
}

fn run(args: &[&str]) -> std::process::Output {
    Command::new(bin())
        .args(args)
        .output()
        .expect("failed to spawn datapeek")
}

#[test]
fn profiles_csv_sample() {
    let out = run(&[sample("orders.csv").to_str().unwrap()]);
    assert!(
        out.status.success(),
        "stderr: {}",
        String::from_utf8_lossy(&out.stderr)
    );
    let stdout = String::from_utf8_lossy(&out.stdout);
    assert!(stdout.contains("datapeek"));
    assert!(stdout.contains("rows"));
    assert!(stdout.contains("order_id"));
    assert!(stdout.contains("integer"));
    assert!(stdout.contains("amount"));
    assert!(stdout.contains("customer"));
}

#[test]
fn json_output_is_valid() {
    let out = run(&["--json", sample("orders.csv").to_str().unwrap()]);
    assert!(
        out.status.success(),
        "stderr: {}",
        String::from_utf8_lossy(&out.stderr)
    );
    let v: serde_json::Value = serde_json::from_slice(&out.stdout).expect("json");
    assert!(v["rows"].as_u64().unwrap() >= 20);
    assert_eq!(v["columns"].as_u64().unwrap(), 7);
    let names: Vec<&str> = v["profile"]
        .as_array()
        .unwrap()
        .iter()
        .map(|c| c["name"].as_str().unwrap())
        .collect();
    assert!(names.contains(&"order_id"));
    assert!(names.contains(&"shipped"));
}

#[test]
fn columns_flag_filters() {
    let out = run(&[
        "--json",
        "--columns",
        "customer,amount",
        sample("orders.csv").to_str().unwrap(),
    ]);
    assert!(
        out.status.success(),
        "stderr: {}",
        String::from_utf8_lossy(&out.stderr)
    );
    let v: serde_json::Value = serde_json::from_slice(&out.stdout).unwrap();
    assert_eq!(v["columns"].as_u64().unwrap(), 2);
    let names: Vec<&str> = v["profile"]
        .as_array()
        .unwrap()
        .iter()
        .map(|c| c["name"].as_str().unwrap())
        .collect();
    assert_eq!(names, vec!["customer", "amount"]);
}

#[test]
fn sample_flag_limits_rows() {
    let out = run(&[
        "--json",
        "--sample",
        "5",
        "--seed",
        "1",
        sample("orders.csv").to_str().unwrap(),
    ]);
    assert!(
        out.status.success(),
        "stderr: {}",
        String::from_utf8_lossy(&out.stderr)
    );
    let v: serde_json::Value = serde_json::from_slice(&out.stdout).unwrap();
    assert_eq!(v["rows"].as_u64().unwrap(), 5);
    assert!(v["sampled"].as_bool().unwrap());
    assert_eq!(v["mode"], "random");
    assert_eq!(v["seed"], 1);
    assert_eq!(v["sample_limit"], 5);
}

#[test]
fn unknown_column_exits_nonzero() {
    let out = run(&["--columns", "nope", sample("orders.csv").to_str().unwrap()]);
    assert!(!out.status.success());
    let err = String::from_utf8_lossy(&out.stderr);
    assert!(err.contains("datapeek:"));
    assert!(err.contains("unknown column"));
}

#[test]
fn missing_file_exits_nonzero() {
    let path = std::env::temp_dir().join("datapeek-does-not-exist-xyz.csv");
    let _ = fs::remove_file(&path);
    let shown = path.to_str().expect("temp path is utf-8");
    let out = run(&[shown]);
    assert!(!out.status.success());
    let err = String::from_utf8_lossy(&out.stderr);
    assert!(err.contains("datapeek:"), "{err}");
    assert!(err.contains("no such file"), "{err}");
    assert!(err.contains(shown), "error should include the path: {err}");
}

#[test]
fn json_numeric_minmax_are_raw_numbers() {
    let out = run(&["--json", sample("orders.csv").to_str().unwrap()]);
    assert!(
        out.status.success(),
        "stderr: {}",
        String::from_utf8_lossy(&out.stderr)
    );
    let text = String::from_utf8_lossy(&out.stdout);
    let v: serde_json::Value = serde_json::from_str(&text).expect("json");
    let order_id = column(&v, "order_id");
    assert!(order_id["min"].is_number(), "{}", order_id["min"]);
    assert!(order_id["max"].is_number(), "{}", order_id["max"]);
    assert_eq!(order_id["min"].as_i64(), Some(1001));
    assert_eq!(order_id["max"].as_i64(), Some(1024));
    assert!(column(&v, "amount")["min"].is_number());
    assert!(column(&v, "amount")["max"].is_number());
    assert!((column(&v, "amount")["min"].as_f64().unwrap() - 4.5).abs() < 1e-9);
    assert!((column(&v, "amount")["max"].as_f64().unwrap() - 249.0).abs() < 1e-9);
    assert!(column(&v, "customer")["min"].is_string());
    assert!(column(&v, "order_date")["min"].is_string());
    assert!(column(&v, "shipped")["min"].is_string());
    assert!(!text.contains("\"1,001\""), "{text}");
    assert!(!text.contains("\"1,024\""), "{text}");
}

#[test]
fn table_keeps_thousands_separators() {
    let out = run(&[sample("orders.csv").to_str().unwrap()]);
    assert!(
        out.status.success(),
        "stderr: {}",
        String::from_utf8_lossy(&out.stderr)
    );
    let stdout = String::from_utf8_lossy(&out.stdout);
    assert!(stdout.contains("1,001"), "{stdout}");
    assert!(stdout.contains("1,024"), "{stdout}");
}

#[test]
fn crlf_csv_and_spaced_path() {
    let dir = tempfile::tempdir().unwrap();
    let path = dir.path().join("my data.csv");
    fs::write(&path, "id,label\r\n1001,a\r\n1024,b\r\n").unwrap();
    let shown = path.to_str().expect("utf-8 path");
    let out = run(&["--json", shown]);
    assert!(
        out.status.success(),
        "stderr: {}",
        String::from_utf8_lossy(&out.stderr)
    );
    let v: serde_json::Value = serde_json::from_slice(&out.stdout).expect("json");
    assert_eq!(v["rows"].as_u64(), Some(2));
    assert_eq!(v["format"], "csv");
    assert_eq!(v["profile"][0]["min"].as_i64(), Some(1001));
    assert_eq!(v["profile"][0]["max"].as_i64(), Some(1024));
    let file = path.display().to_string();
    assert_eq!(v["file"].as_str(), Some(file.as_str()));
}

fn column<'a>(v: &'a serde_json::Value, name: &str) -> &'a serde_json::Value {
    v["profile"]
        .as_array()
        .unwrap()
        .iter()
        .find(|c| c["name"] == name)
        .unwrap_or_else(|| panic!("missing column {name}"))
}

#[test]
fn profiles_tsv() {
    let out = run(&["--json", sample("events.tsv").to_str().unwrap()]);
    assert!(
        out.status.success(),
        "stderr: {}",
        String::from_utf8_lossy(&out.stderr)
    );
    let v: serde_json::Value = serde_json::from_slice(&out.stdout).unwrap();
    assert_eq!(v["format"], "tsv");
    assert!(v["rows"].as_u64().unwrap() >= 10);
}

#[test]
fn profiles_jsonl() {
    let out = run(&["--json", sample("metrics.jsonl").to_str().unwrap()]);
    assert!(
        out.status.success(),
        "stderr: {}",
        String::from_utf8_lossy(&out.stderr)
    );
    let v: serde_json::Value = serde_json::from_slice(&out.stdout).unwrap();
    assert!(v["rows"].as_u64().unwrap() >= 8);
    let names: Vec<&str> = v["profile"]
        .as_array()
        .unwrap()
        .iter()
        .map(|c| c["name"].as_str().unwrap())
        .collect();
    assert!(names.contains(&"metric"));
}

#[test]
fn profiles_parquet_if_present() {
    let path = sample("orders.parquet");
    if !path.exists() {
        return;
    }
    let out = run(&["--json", path.to_str().unwrap()]);
    assert!(
        out.status.success(),
        "stderr: {}",
        String::from_utf8_lossy(&out.stderr)
    );
    let v: serde_json::Value = serde_json::from_slice(&out.stdout).unwrap();
    assert_eq!(v["format"], "parquet");
    assert!(v["rows"].as_u64().unwrap() >= 20);
}

#[test]
fn same_seed_is_identical_and_other_seeds_differ() {
    let path = sample("orders.csv");
    let path = path.to_str().unwrap();
    let a = run(&["--json", "--sample", "8", "--seed", "1", path]);
    let b = run(&["--json", "--sample", "8", "--seed", "1", path]);
    assert!(a.status.success() && b.status.success());
    assert_eq!(a.stdout, b.stdout);
    let c = run(&["--json", "--sample", "8", "--seed", "2", path]);
    assert!(c.status.success());
    assert_ne!(a.stdout, c.stdout);
    let va: serde_json::Value = serde_json::from_slice(&a.stdout).unwrap();
    let vc: serde_json::Value = serde_json::from_slice(&c.stdout).unwrap();
    assert_eq!(va["mode"], "random");
    assert_eq!(vc["seed"], 2);
}

#[test]
fn unseeded_samples_are_not_fixed() {
    let dir = tempfile::tempdir().unwrap();
    let path = dir.path().join("sorted.csv");
    let mut body = String::from("n\n");
    for i in 1..=4_000 {
        body.push_str(&format!("{i}\n"));
    }
    fs::write(&path, body).unwrap();
    let shown = path.to_str().unwrap();
    let first = run(&["--json", "--sample", "30", shown]);
    assert!(
        first.status.success(),
        "{}",
        String::from_utf8_lossy(&first.stderr)
    );
    let mut differed = false;
    for _ in 0..5 {
        let next = run(&["--json", "--sample", "30", shown]);
        assert!(next.status.success());
        if next.stdout != first.stdout {
            differed = true;
            break;
        }
    }
    assert!(differed, "five unseeded samples were identical");
}

#[test]
fn sample_and_head_differ_on_sorted_column() {
    let dir = tempfile::tempdir().unwrap();
    let path = dir.path().join("sorted.csv");
    let mut body = String::from("n\n");
    for i in 1..=500 {
        body.push_str(&format!("{i}\n"));
    }
    fs::write(&path, body).unwrap();
    let shown = path.to_str().unwrap();
    let head = run(&["--json", "--head", "10", shown]);
    let sample = run(&["--json", "--sample", "10", "--seed", "1", shown]);
    assert!(
        head.status.success(),
        "{}",
        String::from_utf8_lossy(&head.stderr)
    );
    assert!(
        sample.status.success(),
        "{}",
        String::from_utf8_lossy(&sample.stderr)
    );
    let head: serde_json::Value = serde_json::from_slice(&head.stdout).unwrap();
    let sample: serde_json::Value = serde_json::from_slice(&sample.stdout).unwrap();
    assert_eq!(head["mode"], "head");
    assert_eq!(head["rows"], 10);
    assert!(head["seed"].is_null());
    assert_eq!(head["profile"][0]["min"], 1);
    assert_eq!(head["profile"][0]["max"], 10);
    assert_eq!(sample["mode"], "random");
    assert_eq!(sample["seed"], 1);
    assert_eq!(sample["rows"], 10);
    assert_ne!(sample["profile"][0]["max"], 10);
    let head_table = run(&["--head", "10", shown]);
    let text = String::from_utf8_lossy(&head_table.stdout);
    assert!(text.contains("first 10 rows × 1 columns"), "{text}");
    let sample_table = run(&["--sample", "10", "--seed", "1", shown]);
    let text = String::from_utf8_lossy(&sample_table.stdout);
    assert!(
        text.contains("random sample of 10 rows (seed 1) × 1 columns"),
        "{text}"
    );
}

#[test]
fn sample_larger_than_file_uses_every_row() {
    let path = sample("orders.csv");
    let full = run(&["--json", path.to_str().unwrap()]);
    let sampled = run(&[
        "--json",
        "--sample",
        "100000",
        "--seed",
        "7",
        path.to_str().unwrap(),
    ]);
    let head = run(&["--json", "--head", "100000", path.to_str().unwrap()]);
    assert!(full.status.success() && sampled.status.success() && head.status.success());
    let full: serde_json::Value = serde_json::from_slice(&full.stdout).unwrap();
    let sampled: serde_json::Value = serde_json::from_slice(&sampled.stdout).unwrap();
    let head: serde_json::Value = serde_json::from_slice(&head.stdout).unwrap();
    assert_eq!(full["rows"], 24);
    assert_eq!(sampled["rows"], full["rows"]);
    assert_eq!(head["rows"], full["rows"]);
    assert_eq!(sampled["mode"], "full");
    assert_eq!(head["mode"], "full");
    assert!(sampled["sampled"] == false);
    assert!(sampled["seed"].is_null());
    assert!(sampled.get("sample_limit").is_none() || sampled["sample_limit"].is_null());
    assert_eq!(sampled["profile"], full["profile"]);
    assert_eq!(head["profile"], full["profile"]);
}

#[test]
fn sample_and_head_conflict_and_seed_needs_sample() {
    let path = sample("orders.csv");
    let both = run(&["--sample", "5", "--head", "5", path.to_str().unwrap()]);
    assert_eq!(
        both.status.code(),
        Some(2),
        "{}",
        String::from_utf8_lossy(&both.stderr)
    );
    let seed_only = run(&["--seed", "1", path.to_str().unwrap()]);
    assert_eq!(seed_only.status.code(), Some(2));
    let zero = run(&["--sample", "0", path.to_str().unwrap()]);
    assert!(!zero.status.success());
    assert!(String::from_utf8_lossy(&zero.stderr).contains("at least 1"));
}

#[test]
fn profiles_excel_default_sheet() {
    let path = sample("orders.xlsx");
    let out = run(&["--json", path.to_str().unwrap()]);
    assert!(
        out.status.success(),
        "stderr: {}",
        String::from_utf8_lossy(&out.stderr)
    );
    let v: serde_json::Value = serde_json::from_slice(&out.stdout).unwrap();
    assert_eq!(v["format"], "xlsx");
    assert_eq!(v["sheet"], "Orders");
    assert_eq!(v["rows"], 24);
    assert_eq!(v["columns"], 7);
    assert_eq!(v["mode"], "full");
    assert_eq!(column(&v, "order_id")["type"], "integer");
    assert_eq!(column(&v, "order_id")["min"], 1001);
    assert_eq!(column(&v, "order_id")["max"], 1024);
    assert_eq!(column(&v, "amount")["type"], "decimal");
    assert!(column(&v, "amount")["missing_pct"].as_f64().unwrap() > 0.0);
    assert_eq!(column(&v, "shipped")["type"], "bool");
    assert_eq!(column(&v, "order_date")["type"], "date");
    assert_eq!(column(&v, "order_date")["min"], "2024-01-15");
    assert_eq!(column(&v, "order_date")["max"], "2024-01-27");
    assert_eq!(column(&v, "customer")["type"], "text");
    assert!(column(&v, "note")["missing_pct"].as_f64().unwrap() > 50.0);
    let table_out = run(&[path.to_str().unwrap()]);
    let table = String::from_utf8_lossy(&table_out.stdout);
    assert!(table.contains("sheet Orders"), "{table}");
    assert!(table.contains("24 rows × 7 columns"), "{table}");
}

#[test]
fn excel_sheet_by_name_and_index() {
    let path = sample("orders.xlsx");
    let shown = path.to_str().unwrap();
    let by_name = run(&["--json", "--sheet", "Returns", shown]);
    let by_index = run(&["--json", "--sheet", "1", shown]);
    let orders = run(&["--json", "--sheet", "Orders", shown]);
    let by_zero = run(&["--json", "--sheet", "0", shown]);
    assert!(
        by_name.status.success(),
        "{}",
        String::from_utf8_lossy(&by_name.stderr)
    );
    assert!(
        by_index.status.success(),
        "{}",
        String::from_utf8_lossy(&by_index.stderr)
    );
    assert!(orders.status.success() && by_zero.status.success());
    let name: serde_json::Value = serde_json::from_slice(&by_name.stdout).unwrap();
    let index: serde_json::Value = serde_json::from_slice(&by_index.stdout).unwrap();
    assert_eq!(name["sheet"], "Returns");
    assert_eq!(index["sheet"], "Returns");
    assert_eq!(name["profile"], index["profile"]);
    assert!(name["rows"].as_u64().unwrap() < 24);
    assert_eq!(column(&name, "return_id")["type"], "integer");
    let orders: serde_json::Value = serde_json::from_slice(&orders.stdout).unwrap();
    let zero: serde_json::Value = serde_json::from_slice(&by_zero.stdout).unwrap();
    assert_eq!(orders["profile"], zero["profile"]);
    assert_eq!(orders["sheet"], "Orders");
}

#[test]
fn excel_unknown_sheet_lists_names() {
    let path = sample("orders.xlsx");
    let out = run(&["--sheet", "Missing", path.to_str().unwrap()]);
    assert!(!out.status.success());
    let err = String::from_utf8_lossy(&out.stderr);
    assert!(err.contains("unknown sheet 'Missing'"), "{err}");
    assert!(err.contains("Orders"), "{err}");
    assert!(err.contains("Returns"), "{err}");
    assert!(err.contains("Types"), "{err}");
}

#[test]
fn excel_types_and_missing_values() {
    let path = sample("orders.xlsx");
    let out = run(&["--json", "--sheet", "Types", path.to_str().unwrap()]);
    assert!(
        out.status.success(),
        "stderr: {}",
        String::from_utf8_lossy(&out.stderr)
    );
    let v: serde_json::Value = serde_json::from_slice(&out.stdout).unwrap();
    assert_eq!(v["rows"], 3);
    assert_eq!(column(&v, "qty")["type"], "integer");
    assert!(column(&v, "qty")["missing_pct"].as_f64().unwrap() > 0.0);
    assert_eq!(column(&v, "price")["type"], "decimal");
    assert_eq!(column(&v, "flag")["type"], "bool");
    assert_eq!(column(&v, "day")["type"], "date");
    assert_eq!(column(&v, "day")["min"], "2024-01-15");
    assert!(!column(&v, "day")["min"].as_str().unwrap().contains(':'));
    assert_eq!(column(&v, "at")["type"], "date");
    let at_min = column(&v, "at")["min"].as_str().unwrap();
    assert!(at_min.contains(':'), "{at_min}");
    assert_eq!(column(&v, "mixed")["type"], "text");
    assert_eq!(column(&v, "label")["type"], "text");
    assert_eq!(column(&v, "empty")["missing_pct"], 100.0);
}

#[test]
fn excel_columns_sample_head_and_seed() {
    let path = sample("orders.xlsx");
    let shown = path.to_str().unwrap();
    let cols = run(&["--json", "--columns", "customer,amount", shown]);
    assert!(
        cols.status.success(),
        "{}",
        String::from_utf8_lossy(&cols.stderr)
    );
    let cols: serde_json::Value = serde_json::from_slice(&cols.stdout).unwrap();
    assert_eq!(cols["columns"], 2);
    assert_eq!(cols["profile"][0]["name"], "customer");
    assert_eq!(cols["profile"][1]["name"], "amount");

    let head = run(&["--json", "--head", "5", shown]);
    assert!(
        head.status.success(),
        "{}",
        String::from_utf8_lossy(&head.stderr)
    );
    let head: serde_json::Value = serde_json::from_slice(&head.stdout).unwrap();
    assert_eq!(head["mode"], "head");
    assert_eq!(head["rows"], 5);
    assert_eq!(head["profile"][0]["max"], 1005);

    let sample = run(&[
        "--json", "--sample", "5", "--seed", "1", "--sheet", "Orders", shown,
    ]);
    let again = run(&[
        "--json", "--sample", "5", "--seed", "1", "--sheet", "Orders", shown,
    ]);
    assert!(sample.status.success() && again.status.success());
    assert_eq!(sample.stdout, again.stdout);
    let sample: serde_json::Value = serde_json::from_slice(&sample.stdout).unwrap();
    assert_eq!(sample["mode"], "random");
    assert_eq!(sample["seed"], 1);
    assert_eq!(sample["rows"], 5);
    assert_eq!(sample["sheet"], "Orders");
    assert_ne!(sample["profile"][0]["max"], 1005);
}

#[test]
fn sheet_flag_on_csv_errors() {
    let out = run(&["--sheet", "Orders", sample("orders.csv").to_str().unwrap()]);
    assert!(!out.status.success());
    let err = String::from_utf8_lossy(&out.stderr);
    assert!(err.contains("--sheet"), "{err}");
    assert!(err.contains("Excel"), "{err}");
}

#[test]
fn empty_file_exits_nonzero() {
    let dir = tempfile::tempdir().unwrap();
    let path = dir.path().join("empty.csv");
    fs::write(&path, "").unwrap();
    let out = run(&[path.to_str().unwrap()]);
    assert!(!out.status.success());
    let err = String::from_utf8_lossy(&out.stderr);
    assert!(err.contains("empty"));
}

// The anomaly must be outside the 10,000-row CSV schema inference window.
fn late_bad_csv(path: &Path) {
    let mut body = String::from("amount\n");
    for _ in 0..10_000 {
        body.push_str("42\n");
    }
    body.push_str("oops\n");
    fs::write(path, body).unwrap();
}

fn error_workbook(path: &Path) {
    use rust_xlsxwriter::{Formula, Workbook};
    let mut workbook = Workbook::new();
    let sheet = workbook.add_worksheet();
    sheet.write_string(0, 0, "amount").unwrap();
    sheet.write_string(0, 1, "id").unwrap();
    sheet.write_number(1, 0, 42).unwrap();
    sheet.write_number(1, 1, 1).unwrap();
    sheet
        .write_formula(2, 0, Formula::new("=1/0").set_result("#DIV/0!"))
        .unwrap();
    sheet.write_number(2, 1, 2).unwrap();
    // An id keeps the blank amount row inside the sheet's used range.
    sheet.write_number(3, 1, 3).unwrap();
    workbook.save(path).unwrap();
}

#[test]
fn csv_parse_errors_are_reported() {
    let dir = tempfile::tempdir().unwrap();
    let path = dir.path().join("late-bad.csv");
    late_bad_csv(&path);
    use polars::prelude::*;
    let strict_read = LazyCsvReader::new(&path)
        .with_has_header(true)
        .with_infer_schema_length(Some(10_000))
        .with_ignore_errors(false)
        .finish()
        .and_then(|lf| lf.collect());
    assert!(
        strict_read.is_err(),
        "strict reader must detect the anomaly"
    );
    let out = run(&["--json", path.to_str().unwrap()]);
    assert!(out.status.success());
    let v: serde_json::Value = serde_json::from_slice(&out.stdout).unwrap();
    assert_eq!(v["rows"], 10_001);
    assert!(column(&v, "amount")["missing_pct"].as_f64().unwrap() > 0.0);
    assert_eq!(v["warnings"][0]["code"], "csv_parse_errors", "{v}");
    assert!(String::from_utf8_lossy(&out.stderr).contains("--strict"));
    let table = run(&[path.to_str().unwrap()]);
    assert!(table.status.success());
    assert!(String::from_utf8_lossy(&table.stdout).contains("amount"));
    assert!(String::from_utf8_lossy(&table.stderr)
        .contains("Profile created, but some CSV values could not be read"));
}

#[test]
fn excel_cell_errors_are_reported_separately_from_blanks() {
    let dir = tempfile::tempdir().unwrap();
    let path = dir.path().join("errors.xlsx");
    error_workbook(&path);
    use calamine::{open_workbook_auto, Data, Reader};
    let mut book = open_workbook_auto(&path).unwrap();
    let range = book.worksheet_range("Sheet1").unwrap();
    assert!(matches!(range.get((2, 0)), Some(Data::Error(_))));
    assert!(matches!(range.get((3, 0)), Some(Data::Empty)));
    let out = run(&["--json", path.to_str().unwrap()]);
    assert!(out.status.success());
    let v: serde_json::Value = serde_json::from_slice(&out.stdout).unwrap();
    assert_eq!(v["rows"], 3);
    assert!((column(&v, "amount")["missing_pct"].as_f64().unwrap() - 200.0 / 3.0).abs() < 1e-9);
    assert_eq!(v["warnings"][0]["code"], "excel_cell_errors", "{v}");
    assert_eq!(v["warnings"][0]["column"], "amount");
    assert_eq!(v["warnings"][0]["count"], 1);
    assert!(String::from_utf8_lossy(&out.stderr).contains("1 Excel error cell"));
}

#[test]
fn strict_csv_rejects_tolerated_errors_without_a_profile() {
    let dir = tempfile::tempdir().unwrap();
    let path = dir.path().join("late-bad.csv");
    late_bad_csv(&path);
    for extra in [vec![], vec!["--head", "10001"], vec!["--sample", "20000"]] {
        let mut args = vec!["--json", "--strict", path.to_str().unwrap()];
        args.extend(extra);
        let out = run(&args);
        assert_eq!(out.status.code(), Some(1));
        assert!(out.stdout.is_empty());
        let err = String::from_utf8_lossy(&out.stderr);
        assert!(
            err.contains("Column: amount") && err.contains("oops"),
            "{err}"
        );
    }
}

#[test]
fn head_csv_does_not_report_errors_outside_the_profile() {
    let dir = tempfile::tempdir().unwrap();
    let path = dir.path().join("late-bad.csv");
    late_bad_csv(&path);
    for extra in [vec![], vec!["--strict"]] {
        let mut args = vec!["--json", "--head", "10000", path.to_str().unwrap()];
        args.extend(extra);
        let out = run(&args);
        assert!(
            out.status.success(),
            "{}",
            String::from_utf8_lossy(&out.stderr)
        );
        assert!(out.stderr.is_empty());
        let v: serde_json::Value = serde_json::from_slice(&out.stdout).unwrap();
        assert_eq!(v["rows"], 10_000);
        assert_eq!(v["mode"], "head");
        assert!(v.get("warnings").is_none());
    }
}

#[test]
fn random_csv_sample_reports_and_rejects_its_parse_errors() {
    let dir = tempfile::tempdir().unwrap();
    let path = dir.path().join("sample-bad.csv");
    let body = format!("amount\n{}oops\n", "42\n".repeat(20_000));
    fs::write(&path, body).unwrap();
    // Pin a seed that includes the final anomaly after the inference window.
    let seed = (0..100)
        .find(|seed| {
            datapeek::sample::sample_indices(20_001, 15_000, Some(*seed)).contains(&20_000)
        })
        .unwrap()
        .to_string();
    let args = [
        "--json",
        "--sample",
        "15000",
        "--seed",
        &seed,
        path.to_str().unwrap(),
    ];
    let tolerant = run(&args);
    assert!(tolerant.status.success());
    let v: serde_json::Value = serde_json::from_slice(&tolerant.stdout).unwrap();
    assert_eq!(v["mode"], "random");
    assert_eq!(v["warnings"][0]["code"], "csv_parse_errors");
    assert!(v["warnings"][0].get("count").is_none());
    let mut strict_args = args.to_vec();
    strict_args.push("--strict");
    let strict = run(&strict_args);
    assert_eq!(strict.status.code(), Some(1));
    assert!(strict.stdout.is_empty());
    assert!(String::from_utf8_lossy(&strict.stderr).contains("oops"));
}

#[test]
fn excel_strict_and_warnings_respect_row_and_column_selection() {
    let dir = tempfile::tempdir().unwrap();
    let path = dir.path().join("errors.xlsx");
    error_workbook(&path);
    let file = path.to_str().unwrap();
    let out = run(&["--strict", "--json", file]);
    assert_eq!(out.status.code(), Some(1));
    assert!(out.stdout.is_empty());
    assert!(String::from_utf8_lossy(&out.stderr).contains("Column: amount"));
    for selection in [vec!["--columns", "id"], vec!["--head", "1"]] {
        for strict in [false, true] {
            let mut args = vec!["--json", file];
            args.extend_from_slice(&selection);
            if strict {
                args.push("--strict");
            }
            let out = run(&args);
            assert!(
                out.status.success(),
                "{}",
                String::from_utf8_lossy(&out.stderr)
            );
            assert!(out.stderr.is_empty());
            let v: serde_json::Value = serde_json::from_slice(&out.stdout).unwrap();
            assert!(v.get("warnings").is_none());
        }
    }
    // Random sample diagnostics count only chosen cells, rather than the sheet.
    for seed in ["0", "1", "2", "3"] {
        let out = run(&["--json", "--sample", "1", "--seed", seed, file]);
        assert!(out.status.success());
        let v: serde_json::Value = serde_json::from_slice(&out.stdout).unwrap();
        let index = datapeek::sample::sample_indices(3, 1, Some(seed.parse().unwrap()))[0];
        if index == 1 {
            assert_eq!(v["warnings"][0]["count"], 1);
        } else {
            assert!(v.get("warnings").is_none());
        }
        let strict = run(&["--strict", "--json", "--sample", "1", "--seed", seed, file]);
        assert_eq!(strict.status.success(), index != 1);
        if index == 1 {
            assert!(strict.stdout.is_empty());
            assert!(String::from_utf8_lossy(&strict.stderr).contains("Excel error cell"));
        }
    }
}

#[test]
fn clean_inputs_and_true_blanks_pass_strict_mode() {
    for name in [
        "orders.csv",
        "events.tsv",
        "metrics.jsonl",
        "orders.parquet",
        "orders.xlsx",
    ] {
        let path = sample(name);
        let out = run(&["--strict", "--json", path.to_str().unwrap()]);
        assert!(
            out.status.success(),
            "{name}: {}",
            String::from_utf8_lossy(&out.stderr)
        );
        assert!(out.stderr.is_empty());
        let v: serde_json::Value = serde_json::from_slice(&out.stdout).unwrap();
        assert!(v.get("warnings").is_none());
    }
}

#[test]
fn head_csv_preserves_quotes_multiline_fields_and_short_rows() {
    let dir = tempfile::tempdir().unwrap();
    let path = dir.path().join("quoted.csv");
    fs::write(
        &path,
        "id,note,amount\r\n1,\"hello,\r\nworld\",42\r\n2,short\r\n3,end,7\r\n",
    )
    .unwrap();
    for limit in ["2", "3", "4"] {
        let out = run(&[
            "--strict",
            "--json",
            "--head",
            limit,
            path.to_str().unwrap(),
        ]);
        assert!(
            out.status.success(),
            "{}",
            String::from_utf8_lossy(&out.stderr)
        );
        assert!(out.stderr.is_empty());
        let v: serde_json::Value = serde_json::from_slice(&out.stdout).unwrap();
        assert_eq!(v["rows"], limit.parse::<u64>().unwrap().min(3));
        assert!(column(&v, "note")["top_values"]
            .as_array()
            .unwrap()
            .iter()
            .any(|t| t["value"] == "hello,\r\nworld"));
        assert_eq!(v["mode"], if limit == "2" { "head" } else { "full" });
    }
}

#[test]
fn strict_errors_explain_the_problem_and_offer_a_usable_next_step() {
    let dir = tempfile::tempdir().unwrap();
    let csv = dir.path().join("bad data's.csv");
    late_bad_csv(&csv);
    let out = run(&["--strict", "--json", csv.to_str().unwrap()]);
    assert_eq!(out.status.code(), Some(1));
    assert!(out.stdout.is_empty());
    let err = String::from_utf8_lossy(&out.stderr);
    assert!(err.contains("Column: amount"), "{err}");
    assert!(err.contains("Expected: whole numbers"), "{err}");
    assert!(err.contains("Found: \"oops\""), "{err}");
    assert!(err.contains("No profile was created."), "{err}");
    assert!(err.contains("To get a profile anyway"), "{err}");
    assert!(
        !err.contains("dtype") && !err.contains("infer_schema_length") && !err.contains("offset"),
        "{err}"
    );
    let retry = err
        .split("Problem values will count as missing:\n  ")
        .nth(1)
        .unwrap()
        .lines()
        .next()
        .unwrap();
    assert!(!retry.contains("--strict"));
    // Execute the displayed command, including the filename's spaces and apostrophe.
    let mut shell = if cfg!(windows) {
        Command::new("powershell.exe")
    } else {
        Command::new("sh")
    };
    if cfg!(windows) {
        shell.args(["-NoProfile", "-NonInteractive", "-Command"]);
    } else {
        shell.arg("-c");
    }
    let retried = shell.arg(retry).output().unwrap();
    assert!(
        retried.status.success(),
        "{}",
        String::from_utf8_lossy(&retried.stderr)
    );
    let v: serde_json::Value = serde_json::from_slice(&retried.stdout).unwrap();
    assert_eq!(v["warnings"][0]["code"], "csv_parse_errors");
    let verbose = run(&["--strict", "--verbose", "--json", csv.to_str().unwrap()]);
    assert_eq!(verbose.status.code(), Some(1));
    assert!(verbose.stdout.is_empty());
    let detail = String::from_utf8_lossy(&verbose.stderr);
    assert!(detail.contains("Expected: whole numbers"));
    assert!(detail.contains("Technical details:") && detail.contains("dtype `i64`"));

    let excel = dir.path().join("errors.xlsx");
    error_workbook(&excel);
    let out = run(&["--strict", excel.to_str().unwrap()]);
    assert_eq!(out.status.code(), Some(1));
    let err = String::from_utf8_lossy(&out.stderr);
    assert!(err.contains("Cell: A3 (#DIV/0!)"), "{err}");
    assert!(err.contains("Fix the cell in Excel"), "{err}");
    assert!(!err.contains("included in missingness"), "{err}");
}

#[test]
fn excel_errors_show_absolute_cell_addresses_for_offset_tables() {
    use rust_xlsxwriter::{Formula, Workbook};
    let dir = tempfile::tempdir().unwrap();
    let path = dir.path().join("offset.xlsx");
    let mut workbook = Workbook::new();
    let sheet = workbook.add_worksheet();
    sheet.write_string(9, 29, "amount").unwrap(); // AD10
    sheet.write_number(10, 29, 42).unwrap();
    sheet
        .write_formula(11, 29, Formula::new("=1/0").set_result("#DIV/0!"))
        .unwrap();
    workbook.save(&path).unwrap();
    let out = run(&["--strict", path.to_str().unwrap()]);
    assert_eq!(out.status.code(), Some(1));
    assert!(String::from_utf8_lossy(&out.stderr).contains("Cell: AD12 (#DIV/0!)"));
}
