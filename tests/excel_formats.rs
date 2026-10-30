//! Real workbook fixtures exercise the format readers, not renamed XLSX files.

use std::path::Path;
use std::process::Command;

use serde_json::{json, Value};

fn fixture(name: &str) -> std::path::PathBuf {
    Path::new(env!("CARGO_MANIFEST_DIR"))
        .join("tests/fixtures/excel")
        .join(name)
}

fn profile(name: &str, flags: &[&str]) -> Value {
    let out = Command::new(env!("CARGO_BIN_EXE_datapeek"))
        .arg("--json")
        .args(flags)
        .arg(fixture(name))
        .output()
        .expect("spawn datapeek");
    assert!(
        out.status.success(),
        "{name} {flags:?}: {}",
        String::from_utf8_lossy(&out.stderr)
    );
    assert!(out.stderr.is_empty(), "unexpected warning for {name}");
    serde_json::from_slice(&out.stdout).expect("JSON profile")
}

#[test]
fn legacy_binary_workbooks_preserve_values_and_totals() {
    for (name, format) in [("date.xls", "xls"), ("date.xlsb", "xlsb")] {
        let v = profile(name, &[]);
        assert_eq!(v["format"], format);
        assert_eq!(v["sheet"], "Sheet1");
        assert_eq!(v["mode"], "full");
        assert_eq!(v["rows"], 2);
        assert_eq!(v["file_rows"], 2);
        assert_eq!(v["columns"], 2);
        assert_eq!(v["cells"], 4);
        assert_eq!(v["filled_cells"], 4);
        assert_eq!(v["filled_pct"], 100.0);
        // The fixture's first row is data upstream; datapeek uses it as headers.
        assert_eq!(v["profile"][0]["name"], "2021-01-01");
        // A date mixed with an Excel duration falls back to text.
        assert_eq!(v["profile"][0]["type"], "text");
        assert_eq!(v["profile"][0]["max"], "2021-01-02");
        assert_eq!(v["profile"][1]["name"], "15");
        assert_eq!(v["profile"][1]["type"], "integer");
        assert_eq!(v["profile"][1]["min"], 16);
        assert_eq!(v["profile"][1]["max"], 17);
        assert_eq!(v["profile"][1]["missing_pct"], 0.0);
    }
}

#[test]
fn legacy_binary_head_keeps_dates_typed_and_knows_file_rows() {
    for name in ["date.xls", "date.xlsb"] {
        let v = profile(name, &["--head", "1"]);
        assert_eq!(v["rows"], 1);
        assert_eq!(v["file_rows"], 2);
        assert_eq!(v["mode"], "head");
        assert_eq!(v["sample_limit"], 1);
        assert_eq!(v["profile"][0]["type"], "date");
        assert_eq!(v["profile"][0]["min"], "2021-01-02");
        assert_eq!(v["profile"][0]["max"], "2021-01-02");
        assert_eq!(v["profile"][1]["min"], 16);
        assert_eq!(profile(name, &["--head", "2"])["mode"], "full");
    }
}

#[test]
fn legacy_binary_sampling_and_column_selection() {
    let flags = ["--sample", "1", "--seed", "7", "--columns", "15"];
    for name in ["date.xls", "date.xlsb"] {
        let v = profile(name, &flags);
        assert_eq!(v, profile(name, &flags));
        assert_eq!(v["rows"], 1);
        assert_eq!(v["file_rows"], 2);
        assert_eq!(v["columns"], 1);
        assert_eq!(v["mode"], "random");
        assert_eq!(v["seed"], 7);
        assert_eq!(v["profile"][0]["name"], "15");
        assert!([json!(16), json!(17)].contains(&v["profile"][0]["min"]));
        assert_eq!(v["profile"][0]["min"], v["profile"][0]["max"]);
    }
}

#[test]
fn xlsm_preserves_text_and_selects_sheets_by_name_or_index() {
    let v = profile("issue221.xlsm", &[]);
    assert_eq!(v, profile("issue221.xlsm", &["--sheet", "Sheet1"]));
    assert_eq!(v, profile("issue221.xlsm", &["--sheet", "0"]));
    assert_eq!(v["format"], "xlsm");
    assert_eq!(v["sheet"], "Sheet1");
    assert_eq!(v["rows"], 1);
    assert_eq!(v["columns"], 2);
    assert_eq!(v["cells"], 2);
    assert_eq!(v["filled_cells"], 2);
    for (index, column) in ["A", "B"].iter().enumerate() {
        assert_eq!(v["profile"][index]["name"], format!("Cell_{column}1"));
        assert_eq!(v["profile"][index]["type"], "text");
        assert_eq!(v["profile"][index]["min"], format!("Cell_{column}2"));
        assert_eq!(v["profile"][index]["max"], format!("Cell_{column}2"));
    }
}

#[test]
fn legacy_formats_report_unknown_sheets_without_a_profile() {
    for name in ["date.xls", "date.xlsb", "issue221.xlsm"] {
        let out = Command::new(env!("CARGO_BIN_EXE_datapeek"))
            .args(["--json", "--sheet", "Missing"])
            .arg(fixture(name))
            .output()
            .expect("spawn datapeek");
        assert!(!out.status.success(), "{name}");
        assert!(out.stdout.is_empty(), "{name}");
        let err = String::from_utf8_lossy(&out.stderr);
        assert!(err.contains("unknown sheet 'Missing'"), "{name}: {err}");
        assert!(err.contains("Sheet1"), "{name}: {err}");
    }
}
