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
fn empty_file_exits_nonzero() {
    let dir = tempfile::tempdir().unwrap();
    let path = dir.path().join("empty.csv");
    fs::write(&path, "").unwrap();
    let out = run(&[path.to_str().unwrap()]);
    assert!(!out.status.success());
    let err = String::from_utf8_lossy(&out.stderr);
    assert!(err.contains("empty"));
}
