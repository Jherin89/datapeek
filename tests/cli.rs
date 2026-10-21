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
    let out = run(&["/tmp/datapeek-does-not-exist-xyz.csv"]);
    assert!(!out.status.success());
    let err = String::from_utf8_lossy(&out.stderr);
    assert!(err.contains("datapeek:"));
    assert!(err.contains("no such file"));
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
