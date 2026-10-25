use std::path::Path;

use polars::prelude::*;
use serde::Serialize;

use crate::error::Result;
use crate::load::{self, ParseWarning, ReadMode, RowSelect};
use crate::sparkline::{self, BINS};

const TOP_N: usize = 3;

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize)]
#[serde(rename_all = "lowercase")]
pub enum InferredType {
    Integer,
    Decimal,
    Date,
    Bool,
    Text,
}

impl InferredType {
    pub fn as_str(self) -> &'static str {
        match self {
            Self::Integer => "integer",
            Self::Decimal => "decimal",
            Self::Date => "date",
            Self::Bool => "bool",
            Self::Text => "text",
        }
    }
}

impl std::fmt::Display for InferredType {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.write_str(self.as_str())
    }
}

#[derive(Debug, Clone, Serialize)]
pub struct TopValue {
    pub value: String,
    pub count: u64,
    pub pct: f64,
}

/// A column min or max.
///
/// Numeric variants serialize as raw JSON numbers. The terminal table uses
/// [`StatValue::to_display`], which still groups thousands.
#[derive(Debug, Clone, PartialEq, Serialize)]
#[serde(untagged)]
pub enum StatValue {
    Integer(i64),
    Unsigned(u64),
    Float(f64),
    Text(String),
}

impl StatValue {
    pub fn to_display(&self) -> String {
        match self {
            Self::Integer(n) => fmt_int(*n),
            Self::Unsigned(n) => fmt_uint(*n),
            Self::Float(v) => fmt_float(*v),
            Self::Text(s) => s.clone(),
        }
    }
}

#[derive(Debug, Clone, Serialize)]
pub struct ColumnProfile {
    pub name: String,
    #[serde(rename = "type")]
    pub dtype: InferredType,
    pub missing_pct: f64,
    /// Nulls plus empty strings. Omitted from JSON; `missing_pct` stays the
    /// public per-column figure. Dataset totals are this count summed.
    #[serde(skip)]
    pub missing: usize,
    pub min: Option<StatValue>,
    pub max: Option<StatValue>,
    pub distinct: usize,
    pub top_values: Vec<TopValue>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub histogram: Option<Vec<u64>>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub sparkline: Option<String>,
}

#[derive(Debug, Clone, Serialize)]
pub struct FileProfile {
    pub file: String,
    pub rows: usize,
    pub columns: usize,
    /// `rows * columns` over the profiled frame. Zero when either side is zero.
    pub cells: u64,
    /// Cells that are not null and not empty strings.
    pub filled_cells: u64,
    /// `100 * filled_cells / cells`, or `0` when there are no cells.
    pub filled_pct: f64,
    /// Rows in the file when that number was already known. JSON `null` for a
    /// CSV or JSON Lines `--head` that did not reach the end.
    pub file_rows: Option<usize>,
    /// True when the profile covers a strict subset of the file.
    pub sampled: bool,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub sample_limit: Option<usize>,
    /// `full`, `head`, or `random`.
    pub mode: ReadMode,
    /// Seed passed to `--sample`, or null when sampling was unseeded or not used.
    pub seed: Option<u64>,
    pub format: String,
    /// Excel sheet that was profiled.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub sheet: Option<String>,
    pub profile: Vec<ColumnProfile>,
    #[serde(skip_serializing_if = "Vec::is_empty")]
    pub warnings: Vec<ParseWarning>,
}

/// What to profile. The default is the whole file and, for Excel, the first sheet.
#[derive(Debug, Clone, Copy)]
pub struct ProfileOptions<'a> {
    pub columns: Option<&'a [String]>,
    pub rows: RowSelect,
    pub sheet: Option<&'a str>,
    /// Reject parsing errors instead of returning a profile with warnings.
    pub strict: bool,
}

impl Default for ProfileOptions<'static> {
    fn default() -> Self {
        Self {
            columns: None,
            rows: RowSelect::All,
            sheet: None,
            strict: false,
        }
    }
}

impl FileProfile {
    pub fn to_json(&self) -> Result<String> {
        Ok(serde_json::to_string_pretty(self)?)
    }
}

/// Profile a data file at `path`.
pub fn profile_path(path: &Path, opts: &ProfileOptions<'_>) -> Result<FileProfile> {
    let loaded = load::load_with_policy(path, opts.columns, opts.rows, opts.sheet, opts.strict)?;
    Ok(profile_loaded(path, loaded))
}

pub fn profile_loaded(path: &Path, loaded: load::Loaded) -> FileProfile {
    let rows = loaded.df.height();
    let columns = loaded.df.width();
    let mut profile = Vec::with_capacity(columns);
    let mut missing_cells: u64 = 0;
    for col in loaded.df.get_columns() {
        let column = profile_column(col, rows);
        missing_cells += column.missing as u64;
        profile.push(column);
    }
    let cells = (rows as u64).saturating_mul(columns as u64);
    let filled_cells = cells.saturating_sub(missing_cells);
    debug_assert_eq!(filled_cells + missing_cells, cells);
    let filled_pct = if cells == 0 {
        0.0
    } else {
        100.0 * filled_cells as f64 / cells as f64
    };
    let outcome = loaded.outcome;
    // A full read has seen every row. Partial reads keep a count only when the
    // loader already had one (parquet footer, reservoir total, in-memory sheet).
    let file_rows = outcome.file_rows.or(match outcome.mode {
        ReadMode::Full => Some(rows),
        ReadMode::Head | ReadMode::Random => None,
    });
    FileProfile {
        file: path.display().to_string(),
        rows,
        columns,
        cells,
        filled_cells,
        filled_pct,
        file_rows,
        sampled: outcome.mode != ReadMode::Full,
        sample_limit: outcome.limit,
        mode: outcome.mode,
        seed: outcome.seed,
        format: load::format_name(loaded.kind),
        sheet: loaded.sheet,
        profile,
        warnings: loaded.warnings,
    }
}

/// Turn empty strings into nulls so every later stat uses one missing rule.
///
/// Returns `None` when the column is not text or it has no `""` values, which
/// is the common path (CSV empty fields are already null). The caller then
/// profiles the original series. Filled cells are the sum of these missing
/// counts; this does not scan the frame again.
fn nullify_empty_strings(s: &Series) -> Option<Series> {
    let ca = s.str().ok()?;
    let has_empty = ca.iter().any(|value| matches!(value, Some("")));
    if !has_empty {
        return None;
    }
    let values: Vec<Option<&str>> = ca
        .iter()
        .map(|value| match value {
            Some("") | None => None,
            Some(value) => Some(value),
        })
        .collect();
    Some(Series::new(s.name().to_string().into(), values))
}

fn profile_column(col: &Column, rows: usize) -> ColumnProfile {
    let original = col.as_materialized_series();
    let normalized = nullify_empty_strings(original);
    let s = normalized.as_ref().unwrap_or(original);
    let name = s.name().to_string();
    let nulls = s.null_count();
    let missing_pct = if rows == 0 {
        0.0
    } else {
        100.0 * nulls as f64 / rows as f64
    };
    let distinct = s.n_unique().unwrap_or(0);
    let dtype = infer_type(s);
    let (min, max) = min_max(s);
    let non_null = rows.saturating_sub(nulls);
    let top_values = if non_null == 0 || (distinct == non_null && distinct > TOP_N) {
        Vec::new()
    } else {
        top_values(s, rows)
    };
    let histogram = if matches!(
        dtype,
        InferredType::Integer | InferredType::Decimal | InferredType::Date
    ) {
        numeric_histogram(s)
    } else {
        None
    };
    let spark = histogram.as_deref().map(sparkline::sparkline);

    ColumnProfile {
        name,
        dtype,
        missing_pct,
        missing: nulls,
        min,
        max,
        distinct,
        top_values,
        histogram,
        sparkline: spark,
    }
}

pub fn infer_type(s: &Series) -> InferredType {
    match s.dtype() {
        DataType::Boolean => InferredType::Bool,
        DataType::UInt8
        | DataType::UInt16
        | DataType::UInt32
        | DataType::UInt64
        | DataType::Int8
        | DataType::Int16
        | DataType::Int32
        | DataType::Int64 => InferredType::Integer,
        DataType::Float32 | DataType::Float64 => InferredType::Decimal,
        DataType::Date | DataType::Datetime(_, _) | DataType::Time => InferredType::Date,
        DataType::String => refine_string_type(s),
        _ => InferredType::Text,
    }
}

fn refine_string_type(s: &Series) -> InferredType {
    let Ok(ca) = s.str() else {
        return InferredType::Text;
    };
    let sample: Vec<&str> = ca.into_iter().flatten().take(200).collect();
    if sample.is_empty() {
        return InferredType::Text;
    }
    if sample.iter().all(|v| is_bool_str(v)) {
        return InferredType::Bool;
    }
    if sample.iter().all(|v| is_int_str(v)) {
        return InferredType::Integer;
    }
    if sample.iter().all(|v| is_float_str(v)) {
        return InferredType::Decimal;
    }
    if sample.iter().all(|v| is_date_str(v)) {
        return InferredType::Date;
    }
    InferredType::Text
}

fn is_bool_str(v: &str) -> bool {
    matches!(
        v.trim().to_ascii_lowercase().as_str(),
        "true" | "false" | "t" | "f" | "yes" | "no" | "y" | "n"
    )
}

fn is_int_str(v: &str) -> bool {
    let v = v.trim();
    if v.is_empty() {
        return false;
    }
    v.parse::<i64>().is_ok()
}

fn is_float_str(v: &str) -> bool {
    let v = v.trim();
    if v.is_empty() {
        return false;
    }
    v.parse::<f64>().is_ok()
}

fn is_date_str(v: &str) -> bool {
    let v = v.trim();
    if v.len() < 8 {
        return false;
    }
    chrono::NaiveDate::parse_from_str(v, "%Y-%m-%d").is_ok()
        || chrono::NaiveDate::parse_from_str(v, "%m/%d/%Y").is_ok()
        || chrono::NaiveDate::parse_from_str(v, "%d/%m/%Y").is_ok()
        || chrono::NaiveDateTime::parse_from_str(v, "%Y-%m-%d %H:%M:%S").is_ok()
        || chrono::NaiveDateTime::parse_from_str(v, "%Y-%m-%dT%H:%M:%S").is_ok()
        || chrono::DateTime::parse_from_rfc3339(v).is_ok()
}

fn min_max(s: &Series) -> (Option<StatValue>, Option<StatValue>) {
    let min = s
        .min_reduce()
        .ok()
        .and_then(|sc| stat_value(s.dtype(), sc.value()));
    let max = s
        .max_reduce()
        .ok()
        .and_then(|sc| stat_value(s.dtype(), sc.value()));
    (min, max)
}

fn fmt_typed(dtype: &DataType, av: &AnyValue<'_>) -> Option<String> {
    stat_value(dtype, av).map(|v| v.to_display())
}

fn stat_value(dtype: &DataType, av: &AnyValue<'_>) -> Option<StatValue> {
    match dtype {
        DataType::Date => {
            let days = match av {
                AnyValue::Date(d) => *d,
                AnyValue::Int32(d) => *d,
                AnyValue::Int64(d) => *d as i32,
                _ => return stat_any(av),
            };
            Some(StatValue::Text(format_date(days)))
        }
        DataType::Datetime(unit, _) => {
            let v = match av {
                AnyValue::Datetime(v, _, _) => *v,
                AnyValue::Int64(v) => *v,
                _ => return stat_any(av),
            };
            Some(StatValue::Text(format_datetime(v, *unit)))
        }
        _ => stat_any(av),
    }
}

fn stat_any(av: &AnyValue) -> Option<StatValue> {
    match av {
        AnyValue::Null => None,
        AnyValue::String(s) => Some(StatValue::Text(truncate(s, 48))),
        AnyValue::StringOwned(s) => Some(StatValue::Text(truncate(s, 48))),
        AnyValue::Boolean(b) => Some(StatValue::Text(if *b {
            "true".into()
        } else {
            "false".into()
        })),
        AnyValue::Int8(v) => Some(StatValue::Integer(*v as i64)),
        AnyValue::Int16(v) => Some(StatValue::Integer(*v as i64)),
        AnyValue::Int32(v) => Some(StatValue::Integer(*v as i64)),
        AnyValue::Int64(v) => Some(StatValue::Integer(*v)),
        AnyValue::UInt8(v) => Some(StatValue::Unsigned(*v as u64)),
        AnyValue::UInt16(v) => Some(StatValue::Unsigned(*v as u64)),
        AnyValue::UInt32(v) => Some(StatValue::Unsigned(*v as u64)),
        AnyValue::UInt64(v) => Some(StatValue::Unsigned(*v)),
        AnyValue::Float32(v) => Some(float_stat(*v as f64)),
        AnyValue::Float64(v) => Some(float_stat(*v)),
        AnyValue::Date(days) => Some(StatValue::Text(format_date(*days))),
        AnyValue::Datetime(v, unit, _) => Some(StatValue::Text(format_datetime(*v, *unit))),
        other => {
            let s = other.to_string();
            let s = s.trim_matches('"').to_string();
            if s.is_empty() || s == "null" {
                None
            } else {
                Some(StatValue::Text(s))
            }
        }
    }
}

fn float_stat(v: f64) -> StatValue {
    if v.is_finite() {
        StatValue::Float(v)
    } else {
        StatValue::Text(v.to_string())
    }
}

fn format_date(days: i32) -> String {
    chrono::NaiveDate::from_ymd_opt(1970, 1, 1)
        .and_then(|d| d.checked_add_signed(chrono::TimeDelta::days(days as i64)))
        .map(|d| d.format("%Y-%m-%d").to_string())
        .unwrap_or_else(|| days.to_string())
}

fn format_datetime(v: i64, unit: TimeUnit) -> String {
    let millis = match unit {
        TimeUnit::Milliseconds => v,
        TimeUnit::Microseconds => v / 1_000,
        TimeUnit::Nanoseconds => v / 1_000_000,
    };
    let secs = millis.div_euclid(1_000);
    let nsecs = (millis.rem_euclid(1_000) * 1_000_000) as u32;
    chrono::DateTime::from_timestamp(secs, nsecs)
        .map(|dt| dt.format("%Y-%m-%d %H:%M:%S").to_string())
        .unwrap_or_else(|| v.to_string())
}

fn fmt_float(v: f64) -> String {
    if !v.is_finite() {
        return v.to_string();
    }
    if v.fract() == 0.0 && v.abs() < 1e15 {
        return fmt_int(v as i64);
    }
    if v.abs() >= 1e12 || (v.abs() > 0.0 && v.abs() < 1e-4) {
        return format!("{v:.4e}");
    }
    let s = format!("{v:.6}");
    s.trim_end_matches('0').trim_end_matches('.').to_string()
}

pub fn fmt_int(n: i64) -> String {
    let neg = n < 0;
    let s = n.unsigned_abs().to_string();
    let mut out = String::new();
    for (i, c) in s.chars().rev().enumerate() {
        if i > 0 && i % 3 == 0 {
            out.push(',');
        }
        out.push(c);
    }
    let mut out: String = out.chars().rev().collect();
    if neg {
        out.insert(0, '-');
    }
    out
}

pub fn fmt_uint(n: u64) -> String {
    let s = n.to_string();
    let mut out = String::new();
    for (i, c) in s.chars().rev().enumerate() {
        if i > 0 && i % 3 == 0 {
            out.push(',');
        }
        out.push(c);
    }
    out.chars().rev().collect()
}

fn truncate(s: &str, max_chars: usize) -> String {
    let n = s.chars().count();
    if n <= max_chars {
        s.to_string()
    } else {
        let take = max_chars.saturating_sub(1);
        let mut out: String = s.chars().take(take).collect();
        out.push('…');
        out
    }
}

fn top_values(s: &Series, rows: usize) -> Vec<TopValue> {
    let dropped = s.drop_nulls();
    let Ok(vc) = dropped.value_counts(true, true, "count".into(), false) else {
        return Vec::new();
    };
    let n = vc.height().min(TOP_N);
    if n == 0 {
        return Vec::new();
    }
    let mut out = Vec::with_capacity(n);
    let value_col = &vc.get_columns()[0];
    let count_col = match vc.column("count") {
        Ok(c) => c,
        Err(_) => return Vec::new(),
    };
    for i in 0..n {
        let Ok(val) = value_col.get(i) else { continue };
        let Some(label) = fmt_typed(s.dtype(), &val) else {
            continue;
        };
        let Ok(count_av) = count_col.get(i) else {
            continue;
        };
        let count = any_to_u64(&count_av);
        let pct = if rows == 0 {
            0.0
        } else {
            100.0 * count as f64 / rows as f64
        };
        out.push(TopValue {
            value: truncate(&label, 24),
            count,
            pct,
        });
    }
    out
}

fn any_to_u64(av: &AnyValue) -> u64 {
    match av {
        AnyValue::UInt32(v) => *v as u64,
        AnyValue::UInt64(v) => *v,
        AnyValue::Int32(v) => *v.max(&0) as u64,
        AnyValue::Int64(v) => *v.max(&0) as u64,
        AnyValue::UInt16(v) => *v as u64,
        AnyValue::UInt8(v) => *v as u64,
        _ => av.extract::<u64>().unwrap_or(0),
    }
}

fn numeric_histogram(s: &Series) -> Option<Vec<u64>> {
    let nums = as_f64_series(s)?;
    let ca = nums.f64().ok()?;
    let min = ca.min()?;
    let max = ca.max()?;
    if !min.is_finite() || !max.is_finite() {
        return None;
    }
    let mut counts = vec![0u64; BINS];
    let non_null = ca.len() as u64 - ca.null_count() as u64;
    if non_null == 0 {
        return None;
    }
    if (max - min).abs() < f64::EPSILON {
        counts[0] = non_null;
        return Some(counts);
    }
    let width = (max - min) / BINS as f64;
    for v in ca.into_iter().flatten() {
        if !v.is_finite() {
            continue;
        }
        let mut idx = ((v - min) / width).floor() as i64;
        if idx < 0 {
            idx = 0;
        }
        if idx as usize >= BINS {
            idx = (BINS - 1) as i64;
        }
        counts[idx as usize] += 1;
    }
    Some(counts)
}

fn as_f64_series(s: &Series) -> Option<Series> {
    match s.dtype() {
        DataType::Float64 => Some(s.clone()),
        DataType::Date => s.cast(&DataType::Int32).ok()?.cast(&DataType::Float64).ok(),
        DataType::Datetime(_, _) => s.cast(&DataType::Int64).ok()?.cast(&DataType::Float64).ok(),
        dt if dt.is_primitive_numeric() => s.cast(&DataType::Float64).ok(),
        _ => None,
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn fmt_int_groups_thousands() {
        assert_eq!(fmt_int(1234567), "1,234,567");
        assert_eq!(fmt_int(-42), "-42");
        assert_eq!(fmt_uint(0), "0");
    }

    #[test]
    fn json_numeric_minmax_are_raw_numbers() {
        assert_eq!(
            serde_json::to_string(&StatValue::Integer(1001)).unwrap(),
            "1001"
        );
        assert_eq!(StatValue::Integer(1001).to_display(), "1,001");
        assert_eq!(
            serde_json::to_string(&StatValue::Integer(-1001)).unwrap(),
            "-1001"
        );
        assert_eq!(StatValue::Integer(-1001).to_display(), "-1,001");
        assert_eq!(
            serde_json::to_string(&StatValue::Float(4.5)).unwrap(),
            "4.5"
        );
        assert_eq!(
            serde_json::to_string(&StatValue::Text("Acme".into())).unwrap(),
            "\"Acme\""
        );

        let dir = tempfile::tempdir().unwrap();
        let csv = dir.path().join("nums.csv");
        std::fs::write(
            &csv,
            "id,amount,name\n1001,4.5,Acme\n1002,5.5,Acme\n1024,249,Umbrella\n1003,6,Acme\n1004,7,Acme\n",
        )
        .unwrap();
        let profile = profile_path(&csv, &ProfileOptions::default()).unwrap();
        let id = &profile.profile[0];
        assert_eq!(id.min.as_ref().unwrap().to_display(), "1,001");
        assert_eq!(id.max.as_ref().unwrap().to_display(), "1,024");
        let json = profile.to_json().unwrap();
        let v: serde_json::Value = serde_json::from_str(&json).unwrap();
        assert_eq!(v["profile"][0]["min"], serde_json::json!(1001));
        assert_eq!(v["profile"][0]["max"], serde_json::json!(1024));
        assert!(v["profile"][1]["min"].is_number());
        assert!(v["profile"][1]["max"].is_number());
        assert_eq!(v["profile"][2]["min"], "Acme");
        assert_eq!(v["profile"][2]["max"], "Umbrella");
        assert!(json.contains("\"min\": 1001"));
        assert!(!json.contains("\"min\": \"1,001\""));
        assert!(!json.contains("1,001"));
    }

    #[test]
    fn infers_bool_and_int() {
        let s = Series::new("flag".into(), &[true, false, true]);
        assert_eq!(infer_type(&s), InferredType::Bool);
        let s = Series::new("id".into(), &[1i64, 2, 3]);
        assert_eq!(infer_type(&s), InferredType::Integer);
    }

    #[test]
    fn profiles_parquet_roundtrip() {
        let dir = tempfile::tempdir().unwrap();
        let csv = dir.path().join("t.csv");
        std::fs::write(&csv, "id,amount\n1,2.5\n2,3.5\n3,2.5\n").unwrap();
        let loaded = crate::load::load(&csv, None, RowSelect::All, None).unwrap();
        let mut df = loaded.df;
        let pq = dir.path().join("t.parquet");
        {
            let mut f = std::fs::File::create(&pq).unwrap();
            ParquetWriter::new(&mut f).finish(&mut df).unwrap();
        }
        let p = crate::profile_path(&pq, &ProfileOptions::default()).unwrap();
        assert_eq!(p.format, "parquet");
        assert_eq!(p.rows, 3);
        assert_eq!(p.columns, 2);
        assert_eq!(p.profile[0].dtype, InferredType::Integer);
        assert_eq!(p.profile[1].dtype, InferredType::Decimal);
    }

    #[test]
    fn parquet_and_jsonl_samples_match_their_seeds() {
        let dir = tempfile::tempdir().unwrap();
        let mut body = String::new();
        for i in 0..300 {
            body.push_str(&format!("{{\"n\":{i}}}\n"));
        }
        let jsonl = dir.path().join("n.jsonl");
        std::fs::write(&jsonl, &body).unwrap();

        let sample = ProfileOptions {
            columns: None,
            rows: RowSelect::Sample {
                n: 20,
                seed: Some(4),
            },
            sheet: None,
            strict: false,
        };
        let a = profile_path(&jsonl, &sample).unwrap();
        let b = profile_path(&jsonl, &sample).unwrap();
        assert_eq!(a.mode, ReadMode::Random);
        assert_eq!(a.seed, Some(4));
        assert_eq!(a.rows, 20);
        assert_eq!(a.to_json().unwrap(), b.to_json().unwrap());
        let head = profile_path(
            &jsonl,
            &ProfileOptions {
                columns: None,
                rows: RowSelect::Head(20),
                sheet: None,
                strict: false,
            },
        )
        .unwrap();
        assert_eq!(head.mode, ReadMode::Head);
        assert_eq!(head.profile[0].min, Some(StatValue::Integer(0)));
        assert_eq!(head.profile[0].max, Some(StatValue::Integer(19)));
        assert_ne!(a.profile[0].max, head.profile[0].max);

        let csv = dir.path().join("n.csv");
        let mut csv_body = String::from("n\n");
        for i in 0..300 {
            csv_body.push_str(&format!("{i}\n"));
        }
        std::fs::write(&csv, csv_body).unwrap();
        let loaded = crate::load::load(&csv, None, RowSelect::All, None).unwrap();
        let mut frame = loaded.df;
        let pq = dir.path().join("n.parquet");
        {
            let mut file = std::fs::File::create(&pq).unwrap();
            ParquetWriter::new(&mut file).finish(&mut frame).unwrap();
        }
        let cols = ["n".to_string()];
        let projected = ProfileOptions {
            columns: Some(&cols),
            rows: RowSelect::Sample {
                n: 15,
                seed: Some(8),
            },
            sheet: None,
            strict: false,
        };
        let once = profile_path(&pq, &projected).unwrap();
        let twice = profile_path(&pq, &projected).unwrap();
        assert_eq!(once.format, "parquet");
        assert_eq!(once.rows, 15);
        assert_eq!(once.columns, 1);
        assert_eq!(once.to_json().unwrap(), twice.to_json().unwrap());
        let head = profile_path(
            &pq,
            &ProfileOptions {
                columns: Some(&cols),
                rows: RowSelect::Head(15),
                sheet: None,
                strict: false,
            },
        )
        .unwrap();
        assert_eq!(head.profile[0].max, Some(StatValue::Integer(14)));
        assert_ne!(once.profile[0].max, head.profile[0].max);
    }

    fn totals_agree(profile: &FileProfile) {
        let missing: u64 = profile.profile.iter().map(|col| col.missing as u64).sum();
        assert_eq!(profile.cells, profile.rows as u64 * profile.columns as u64);
        assert_eq!(profile.filled_cells + missing, profile.cells);
        if profile.cells == 0 {
            assert_eq!(profile.filled_pct, 0.0);
        } else {
            let expected = 100.0 * profile.filled_cells as f64 / profile.cells as f64;
            assert!((profile.filled_pct - expected).abs() < 1e-9);
        }
        let json: serde_json::Value = serde_json::from_str(&profile.to_json().unwrap()).unwrap();
        assert_eq!(json["cells"].as_u64(), Some(profile.cells));
        assert_eq!(json["filled_cells"].as_u64(), Some(profile.filled_cells));
        assert!(json.get("missing").is_none() || json["profile"][0].get("missing").is_none());
        match profile.file_rows {
            Some(rows) => assert_eq!(json["file_rows"].as_u64(), Some(rows as u64)),
            None => assert!(json["file_rows"].is_null()),
        }
    }

    #[test]
    fn empty_strings_are_missing_and_totals_match() {
        let frame = DataFrame::new(vec![
            Series::new(
                "s".into(),
                &[Some("a"), Some(""), None, Some(" "), Some("a")],
            )
            .into(),
            Series::new("n".into(), &[Some(1i64), Some(2), None, Some(4), Some(5)]).into(),
        ])
        .unwrap();
        let profile = profile_loaded(
            Path::new("mem.csv"),
            load::Loaded {
                df: frame,
                kind: crate::load::FileKind::Csv { sep: b',' },
                outcome: crate::load::ReadOutcome::full(),
                sheet: None,
                warnings: Vec::new(),
            },
        );
        let text = &profile.profile[0];
        assert_eq!(
            text.missing, 2,
            "null and \"\" are missing; a space is data"
        );
        assert!((text.missing_pct - 40.0).abs() < 1e-9);
        assert_eq!(text.min.as_ref().unwrap().to_display(), " ");
        assert!(text.top_values.iter().all(|value| !value.value.is_empty()));
        assert_eq!(text.distinct, 3);
        assert_eq!(profile.profile[1].missing, 1);
        assert_eq!(profile.rows, 5);
        assert_eq!(profile.columns, 2);
        assert_eq!(profile.cells, 10);
        assert_eq!(profile.filled_cells, 7);
        assert_eq!(profile.file_rows, Some(5));
        totals_agree(&profile);
    }

    #[test]
    fn header_only_csv_has_zero_cells() {
        let dir = tempfile::tempdir().unwrap();
        let csv = dir.path().join("header.csv");
        std::fs::write(&csv, "a,b,c\n").unwrap();
        let profile = profile_path(&csv, &ProfileOptions::default()).unwrap();
        assert_eq!(profile.rows, 0);
        assert_eq!(profile.columns, 3);
        assert_eq!(profile.cells, 0);
        assert_eq!(profile.filled_cells, 0);
        assert_eq!(profile.filled_pct, 0.0);
        assert_eq!(profile.file_rows, Some(0));
        assert!(profile.profile.iter().all(|col| col.missing == 0));
        totals_agree(&profile);
    }

    #[test]
    fn quoted_csv_and_tsv_empty_strings_are_missing() {
        let dir = tempfile::tempdir().unwrap();
        let csv = dir.path().join("q.csv");
        // Quoted "", a missing field, and a whitespace cell.
        std::fs::write(&csv, "s,n\na,1\n\"\",\n,3\n\" \",4\n").unwrap();
        let profile = profile_path(&csv, &ProfileOptions::default()).unwrap();
        assert_eq!(profile.profile[0].missing, 2);
        assert_eq!(profile.profile[0].min.as_ref().unwrap().to_display(), " ");
        assert!(profile.profile[0]
            .top_values
            .iter()
            .all(|value| !value.value.is_empty()));
        assert_eq!(profile.profile[1].missing, 1);
        totals_agree(&profile);

        let tsv = dir.path().join("q.tsv");
        std::fs::write(&tsv, "note\nhello\n\"\"\n").unwrap();
        let profile = profile_path(&tsv, &ProfileOptions::default()).unwrap();
        let note = profile
            .profile
            .iter()
            .find(|col| col.name == "note")
            .unwrap();
        assert_eq!(note.missing, 1);
        totals_agree(&profile);
    }

    #[test]
    fn file_rows_follow_the_loader() {
        let dir = tempfile::tempdir().unwrap();
        let mut body = String::from("n,label\n");
        for i in 0..40 {
            let label = if i % 2 == 0 { "" } else { "x" };
            body.push_str(&format!("{i},{label}\n"));
        }
        let csv = dir.path().join("n.csv");
        std::fs::write(&csv, &body).unwrap();

        let head = profile_path(
            &csv,
            &ProfileOptions {
                columns: None,
                rows: RowSelect::Head(10),
                sheet: None,
                strict: false,
            },
        )
        .unwrap();
        assert_eq!(head.mode, ReadMode::Head);
        assert_eq!(head.rows, 10);
        assert_eq!(head.file_rows, None);
        assert_eq!(head.cells, 20);
        totals_agree(&head);

        let sample = profile_path(
            &csv,
            &ProfileOptions {
                columns: None,
                rows: RowSelect::Sample {
                    n: 10,
                    seed: Some(1),
                },
                sheet: None,
                strict: false,
            },
        )
        .unwrap();
        assert_eq!(sample.mode, ReadMode::Random);
        assert_eq!(sample.rows, 10);
        assert_eq!(sample.file_rows, Some(40));
        totals_agree(&sample);

        let jsonl = dir.path().join("n.jsonl");
        let mut lines = String::new();
        for i in 0..40 {
            let label = if i % 3 == 0 { "" } else { "x" };
            lines.push_str(&format!("{{\"n\":{i},\"label\":\"{label}\"}}\n"));
        }
        std::fs::write(&jsonl, lines).unwrap();
        let json_sample = profile_path(
            &jsonl,
            &ProfileOptions {
                columns: None,
                rows: RowSelect::Sample {
                    n: 8,
                    seed: Some(2),
                },
                sheet: None,
                strict: false,
            },
        )
        .unwrap();
        assert_eq!(json_sample.file_rows, Some(40));
        totals_agree(&json_sample);
        let json_full = profile_path(&jsonl, &ProfileOptions::default()).unwrap();
        let label = json_full
            .profile
            .iter()
            .find(|col| col.name == "label")
            .unwrap();
        assert!(label.missing > 0, "JSONL \"\" counts as missing");
        assert!(label.top_values.iter().all(|value| !value.value.is_empty()));
        totals_agree(&json_full);

        let loaded = crate::load::load(&csv, None, RowSelect::All, None).unwrap();
        let mut frame = loaded.df;
        let pq = dir.path().join("n.parquet");
        {
            let mut file = std::fs::File::create(&pq).unwrap();
            ParquetWriter::new(&mut file).finish(&mut frame).unwrap();
        }
        // Quoted-empty equivalent: a real empty string stored in Parquet.
        let mut stored = DataFrame::new(vec![Series::new(
            "s".into(),
            &[Some("a"), Some(""), None, Some("b")],
        )
        .into()])
        .unwrap();
        let empty_pq = dir.path().join("empty.parquet");
        {
            let mut file = std::fs::File::create(&empty_pq).unwrap();
            ParquetWriter::new(&mut file).finish(&mut stored).unwrap();
        }
        let parquet_empty = profile_path(&empty_pq, &ProfileOptions::default()).unwrap();
        assert_eq!(parquet_empty.profile[0].missing, 2);
        assert_ne!(
            parquet_empty.profile[0].min.as_ref().unwrap().to_display(),
            ""
        );
        totals_agree(&parquet_empty);

        let parquet_head = profile_path(
            &pq,
            &ProfileOptions {
                columns: None,
                rows: RowSelect::Head(7),
                sheet: None,
                strict: false,
            },
        )
        .unwrap();
        assert_eq!(parquet_head.mode, ReadMode::Head);
        assert_eq!(parquet_head.rows, 7);
        assert_eq!(parquet_head.file_rows, Some(40));
        totals_agree(&parquet_head);
    }
}
