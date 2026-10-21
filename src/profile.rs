use std::path::Path;

use polars::prelude::*;
use serde::Serialize;

use crate::error::Result;
use crate::load::{self, FileKind};
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

#[derive(Debug, Clone, Serialize)]
pub struct ColumnProfile {
    pub name: String,
    #[serde(rename = "type")]
    pub dtype: InferredType,
    pub missing_pct: f64,
    pub min: Option<String>,
    pub max: Option<String>,
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
    pub sampled: bool,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub sample_limit: Option<usize>,
    pub format: String,
    pub profile: Vec<ColumnProfile>,
}

impl FileProfile {
    pub fn to_json(&self) -> Result<String> {
        Ok(serde_json::to_string_pretty(self)?)
    }
}

/// Profile a data file at `path`.
pub fn profile_path(
    path: &Path,
    columns: Option<&[String]>,
    sample: Option<usize>,
) -> Result<FileProfile> {
    let loaded = load::load(path, columns, sample)?;
    Ok(profile_loaded(path, loaded.df, loaded.kind, sample))
}

pub fn profile_loaded(
    path: &Path,
    df: DataFrame,
    kind: FileKind,
    sample: Option<usize>,
) -> FileProfile {
    let rows = df.height();
    let mut profile = Vec::with_capacity(df.width());
    for col in df.get_columns() {
        profile.push(profile_column(col, rows));
    }
    FileProfile {
        file: path.display().to_string(),
        rows,
        columns: df.width(),
        sampled: sample.is_some(),
        sample_limit: sample,
        format: format_label(kind),
        profile,
    }
}

fn format_label(kind: FileKind) -> String {
    match kind {
        FileKind::Csv { sep: b'\t' } => "tsv".into(),
        FileKind::Csv { .. } => "csv".into(),
        FileKind::Parquet => "parquet".into(),
        FileKind::Jsonl => "jsonl".into(),
    }
}

fn profile_column(col: &Column, rows: usize) -> ColumnProfile {
    let s = col.as_materialized_series();
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

fn min_max(s: &Series) -> (Option<String>, Option<String>) {
    let min = s
        .min_reduce()
        .ok()
        .and_then(|sc| fmt_typed(s.dtype(), sc.value()));
    let max = s
        .max_reduce()
        .ok()
        .and_then(|sc| fmt_typed(s.dtype(), sc.value()));
    (min, max)
}

fn fmt_typed(dtype: &DataType, av: &AnyValue<'_>) -> Option<String> {
    match dtype {
        DataType::Date => {
            let days = match av {
                AnyValue::Date(d) => *d,
                AnyValue::Int32(d) => *d,
                AnyValue::Int64(d) => *d as i32,
                _ => return fmt_any(av),
            };
            Some(format_date(days))
        }
        DataType::Datetime(unit, _) => {
            let v = match av {
                AnyValue::Datetime(v, _, _) => *v,
                AnyValue::Int64(v) => *v,
                _ => return fmt_any(av),
            };
            Some(format_datetime(v, *unit))
        }
        _ => fmt_any(av),
    }
}

fn fmt_any(av: &AnyValue) -> Option<String> {
    match av {
        AnyValue::Null => None,
        AnyValue::String(s) => Some(truncate(s, 48)),
        AnyValue::StringOwned(s) => Some(truncate(s, 48)),
        AnyValue::Boolean(b) => Some(if *b { "true".into() } else { "false".into() }),
        AnyValue::Int8(v) => Some(fmt_int(*v as i64)),
        AnyValue::Int16(v) => Some(fmt_int(*v as i64)),
        AnyValue::Int32(v) => Some(fmt_int(*v as i64)),
        AnyValue::Int64(v) => Some(fmt_int(*v)),
        AnyValue::UInt8(v) => Some(fmt_uint(*v as u64)),
        AnyValue::UInt16(v) => Some(fmt_uint(*v as u64)),
        AnyValue::UInt32(v) => Some(fmt_uint(*v as u64)),
        AnyValue::UInt64(v) => Some(fmt_uint(*v)),
        AnyValue::Float32(v) => Some(fmt_float(*v as f64)),
        AnyValue::Float64(v) => Some(fmt_float(*v)),
        AnyValue::Date(days) => Some(format_date(*days)),
        AnyValue::Datetime(v, unit, _) => Some(format_datetime(*v, *unit)),
        other => {
            let s = other.to_string();
            let s = s.trim_matches('"').to_string();
            if s.is_empty() || s == "null" {
                None
            } else {
                Some(s)
            }
        }
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
        let loaded = crate::load::load(&csv, None, None).unwrap();
        let mut df = loaded.df;
        let pq = dir.path().join("t.parquet");
        {
            let mut f = std::fs::File::create(&pq).unwrap();
            ParquetWriter::new(&mut f).finish(&mut df).unwrap();
        }
        let p = crate::profile_path(&pq, None, None).unwrap();
        assert_eq!(p.format, "parquet");
        assert_eq!(p.rows, 3);
        assert_eq!(p.columns, 2);
        assert_eq!(p.profile[0].dtype, InferredType::Integer);
        assert_eq!(p.profile[1].dtype, InferredType::Decimal);
    }
}
