//! Excel workbooks (.xlsx, .xlsm, .xls, .xlsb) via calamine.
//!
//! The first row of the chosen sheet is the header. Cells become a Polars
//! frame: integers, floats, bools, dates, and datetimes stay typed; a column
//! that mixes those falls back to text; empty cells are null.

use std::collections::HashSet;
use std::path::Path;

use calamine::{open_workbook_auto, Data, Reader};
use chrono::{Datelike, NaiveDate, NaiveDateTime, NaiveTime, Timelike};
use polars::prelude::*;

use crate::error::{Error, Result};
use crate::load::{ParseWarning, ReadOutcome, RowSelect};
use crate::sample;

/// Pick a sheet.
///
/// A value that equals a sheet name exactly wins, even when that name is
/// numeric (`"1"`). Otherwise a value that parses as an integer is a 0-based
/// index (`"01"` is index 1). Unknown sheets error with the names that exist.
pub fn resolve_sheet<'a>(names: &'a [String], sheet: Option<&str>) -> Result<&'a str> {
    if names.is_empty() {
        return Err(Error::new(
            "workbook has no sheets (have: none)".to_string(),
        ));
    }
    let Some(spec) = sheet else {
        return Ok(names[0].as_str());
    };
    if let Some(found) = names.iter().find(|name| name.as_str() == spec) {
        return Ok(found.as_str());
    }
    if let Ok(index) = spec.parse::<usize>() {
        if let Some(found) = names.get(index) {
            return Ok(found.as_str());
        }
    }
    Err(unknown_sheet(spec, names))
}

pub fn unknown_sheet(spec: &str, names: &[String]) -> Error {
    let have = if names.is_empty() {
        "none".to_string()
    } else {
        names.join(", ")
    };
    Error::new(format!("unknown sheet '{spec}' (have: {have})"))
}

pub fn load_excel(
    path: &Path,
    rows: RowSelect,
    sheet: Option<&str>,
) -> Result<(DataFrame, ReadOutcome, String)> {
    let loaded = load_excel_with_policy(path, rows, sheet, None, false)?;
    Ok((loaded.df, loaded.outcome, loaded.sheet))
}

pub(crate) struct ExcelLoaded {
    pub df: DataFrame,
    pub outcome: ReadOutcome,
    pub sheet: String,
    pub warnings: Vec<ParseWarning>,
}

pub(crate) fn load_excel_with_policy(
    path: &Path,
    rows: RowSelect,
    sheet: Option<&str>,
    selected_columns: Option<&[String]>,
    strict: bool,
) -> Result<ExcelLoaded> {
    let mut workbook = open_workbook_auto(path).map_err(|err| Error::parse(path, err))?;
    let names: Vec<String> = workbook
        .sheet_names()
        .into_iter()
        .map(|name| name.to_string())
        .collect();
    let sheet_name = resolve_sheet(&names, sheet)
        .map_err(|err| {
            // `resolve_sheet` already lists names. Attach the path for the
            // empty-workbook case, which has no spec to quote.
            if sheet.is_none() {
                Error::new(format!("{err}: {}", path.display()))
            } else {
                err
            }
        })?
        .to_string();

    let range = workbook.worksheet_range(&sheet_name).map_err(|err| {
        let msg = err.to_string();
        if msg.to_ascii_lowercase().contains("empty") {
            Error::new(format!("sheet '{sheet_name}' is empty"))
        } else {
            Error::parse(path, msg)
        }
    })?;

    let grid: Vec<Vec<Data>> = range.rows().map(|row| row.to_vec()).collect();
    if grid.is_empty() || grid[0].is_empty() {
        return Err(Error::new(format!("sheet '{sheet_name}' is empty")));
    }

    let width = grid.iter().map(Vec::len).max().unwrap_or(0);
    if width == 0 {
        return Err(Error::new(format!("sheet '{sheet_name}' is empty")));
    }

    let mut used = HashSet::new();
    let headers: Vec<String> = (0..width)
        .map(|col| {
            let cell = grid[0].get(col).unwrap_or(&Data::Empty);
            unique_name(header_text(cell, col), &mut used)
        })
        .collect();

    let n_data = grid.len().saturating_sub(1);
    let (indexes, outcome) = select_rows(n_data, rows);
    let data_rows: Vec<&[Data]> = indexes
        .iter()
        .map(|&idx| grid[idx + 1].as_slice())
        .collect();

    let mut columns = Vec::with_capacity(width);
    let mut warnings = Vec::new();
    for (col, name) in headers.iter().enumerate() {
        let cells: Vec<&Data> = data_rows
            .iter()
            .map(|row| row.get(col).unwrap_or(&Data::Empty))
            .collect();
        let selected = selected_columns.is_none_or(|names| names.contains(name));
        let count = cells
            .iter()
            .filter(|cell| matches!(cell, Data::Error(_)))
            .count();
        if selected && count > 0 {
            let noun = if count == 1 { "cell" } else { "cells" };
            let (position, error) = cells
                .iter()
                .enumerate()
                .find_map(|(i, cell)| {
                    if let Data::Error(error) = cell {
                        Some((i, error))
                    } else {
                        None
                    }
                })
                .expect("count > 0 establishes an error cell");
            let (start_row, start_col) = range.start().unwrap_or((0, 0));
            let address = cell_address(
                start_row as usize + indexes[position] + 1,
                start_col as usize + col,
            );
            if strict {
                return Err(Error::new(format!(
                    "found {count} Excel error {noun}.\n\n  File: {}\n  Sheet: {}\n  Column: {}\n  Cell: {address} ({error})\n\nNo profile was created. Fix the cell in Excel, then save the workbook and try again.",
                    path.display(), sheet_name.escape_debug(), name.escape_debug()
                )).with_tolerant_retry());
            }
            warnings.push(ParseWarning {
                code: "excel_cell_errors",
                message: format!(
                    "Profile created with {count} Excel error {noun} counted as missing in column '{}' (sheet '{}', first at {address}: {error}). Fix the cell in Excel, then save the workbook and try again.",
                    name.escape_debug(), sheet_name.escape_debug()
                ),
                column: Some(name.clone()),
                count: Some(count),
            });
        }
        columns.push(column_from_cells(name, &cells)?);
    }

    let df = DataFrame::new(columns)
        .map_err(|err| Error::new(format!("could not build sheet '{sheet_name}': {err}")))?;
    Ok(ExcelLoaded {
        df,
        outcome,
        sheet: sheet_name,
        warnings,
    })
}

fn cell_address(row: usize, column: usize) -> String {
    let mut column = column + 1;
    let mut letters = Vec::new();
    while column > 0 {
        letters.push((b'A' + ((column - 1) % 26) as u8) as char);
        column = (column - 1) / 26;
    }
    format!(
        "{}{row}",
        letters.into_iter().rev().collect::<String>(),
        row = row + 1
    )
}

fn select_rows(n_data: usize, rows: RowSelect) -> (Vec<usize>, ReadOutcome) {
    // The sheet is already in memory, so the file row count is free.
    let (indexes, outcome) = match rows {
        RowSelect::All => ((0..n_data).collect(), ReadOutcome::full()),
        RowSelect::Head(n) if n >= n_data => ((0..n_data).collect(), ReadOutcome::full()),
        RowSelect::Head(n) => ((0..n).collect(), ReadOutcome::head(n)),
        RowSelect::Sample { n, .. } if n >= n_data => ((0..n_data).collect(), ReadOutcome::full()),
        RowSelect::Sample { n, seed } => (
            sample::sample_indices(n_data, n, seed),
            ReadOutcome::random(n, seed),
        ),
    };
    (indexes, outcome.with_file_rows(n_data))
}

#[derive(Clone, Copy, PartialEq, Eq)]
enum Kind {
    Empty,
    Int,
    Float,
    Bool,
    Date,
    Datetime,
    Text,
}

fn column_from_cells(name: &str, cells: &[&Data]) -> Result<Column> {
    let mut kind = Kind::Empty;
    for cell in cells {
        kind = absorb(kind, classify(cell));
    }
    let series = match kind {
        Kind::Empty | Kind::Text => {
            let values: Vec<Option<String>> =
                cells.iter().map(|cell| cell_to_string(cell)).collect();
            Series::new(name.into(), values)
        }
        Kind::Int => {
            let values: Vec<Option<i64>> = cells.iter().map(|cell| cell_to_int(cell)).collect();
            Series::new(name.into(), values)
        }
        Kind::Float => {
            let values: Vec<Option<f64>> = cells.iter().map(|cell| cell_to_float(cell)).collect();
            Series::new(name.into(), values)
        }
        Kind::Bool => {
            let values: Vec<Option<bool>> = cells.iter().map(|cell| cell_to_bool(cell)).collect();
            Series::new(name.into(), values)
        }
        Kind::Date => {
            let values: Vec<Option<i32>> = cells.iter().map(|cell| cell_to_days(cell)).collect();
            Series::new(name.into(), values)
                .cast(&DataType::Date)
                .map_err(|err| Error::new(format!("could not build column '{name}': {err}")))?
        }
        Kind::Datetime => {
            let values: Vec<Option<i64>> = cells.iter().map(|cell| cell_to_millis(cell)).collect();
            Series::new(name.into(), values)
                .cast(&DataType::Datetime(TimeUnit::Milliseconds, None))
                .map_err(|err| Error::new(format!("could not build column '{name}': {err}")))?
        }
    };
    Ok(series.into())
}

fn absorb(kind: Kind, next: Kind) -> Kind {
    match (kind, next) {
        (_, Kind::Empty) => kind,
        (Kind::Empty, next) => next,
        (Kind::Text, _) | (_, Kind::Text) => Kind::Text,
        (left, right) if left == right => left,
        (Kind::Int, Kind::Float) | (Kind::Float, Kind::Int) => Kind::Float,
        (Kind::Date, Kind::Datetime) | (Kind::Datetime, Kind::Date) => Kind::Datetime,
        _ => Kind::Text,
    }
}

fn classify(cell: &Data) -> Kind {
    match cell {
        Data::Empty => Kind::Empty,
        Data::Error(_) => Kind::Empty,
        Data::Int(_) => Kind::Int,
        Data::Float(value) => match float_as_int(*value) {
            Some(_) => Kind::Int,
            None if value.is_finite() => Kind::Float,
            None => Kind::Empty,
        },
        Data::Bool(_) => Kind::Bool,
        Data::String(value) => {
            if value.is_empty() {
                Kind::Empty
            } else {
                Kind::Text
            }
        }
        Data::DateTime(dt) => {
            if dt.is_duration() {
                Kind::Text
            } else {
                match dt.as_datetime() {
                    Some(value) if is_midnight(value) => Kind::Date,
                    Some(_) => Kind::Datetime,
                    None => Kind::Empty,
                }
            }
        }
        Data::DateTimeIso(value) => match parse_iso(value) {
            Some(dt) if is_midnight(dt) => Kind::Date,
            Some(_) => Kind::Datetime,
            None if value.is_empty() => Kind::Empty,
            None => Kind::Text,
        },
        Data::DurationIso(value) => {
            if value.is_empty() {
                Kind::Empty
            } else {
                Kind::Text
            }
        }
    }
}

fn header_text(cell: &Data, index: usize) -> String {
    match cell_to_string(cell) {
        Some(value) if !value.trim().is_empty() => value.trim().to_string(),
        _ => format!("column_{}", index + 1),
    }
}

fn unique_name(raw: String, used: &mut HashSet<String>) -> String {
    if used.insert(raw.clone()) {
        return raw;
    }
    let mut n = 2usize;
    loop {
        let candidate = format!("{raw}_{n}");
        if used.insert(candidate.clone()) {
            return candidate;
        }
        n += 1;
    }
}

fn cell_to_string(cell: &Data) -> Option<String> {
    match cell {
        Data::Empty | Data::Error(_) => None,
        Data::Int(value) => Some(value.to_string()),
        Data::Float(value) => {
            if !value.is_finite() {
                None
            } else if let Some(int) = float_as_int(*value) {
                Some(int.to_string())
            } else {
                Some(format!("{value}"))
            }
        }
        Data::Bool(value) => Some(if *value {
            "true".to_string()
        } else {
            "false".to_string()
        }),
        Data::String(value) => {
            if value.is_empty() {
                None
            } else {
                Some(value.clone())
            }
        }
        Data::DateTime(dt) => {
            if dt.is_duration() {
                Some(dt.as_f64().to_string())
            } else {
                dt.as_datetime().map(format_datetime_value)
            }
        }
        Data::DateTimeIso(value) | Data::DurationIso(value) => {
            if value.is_empty() {
                None
            } else {
                Some(value.clone())
            }
        }
    }
}

fn cell_to_int(cell: &Data) -> Option<i64> {
    match cell {
        Data::Int(value) => Some(*value),
        Data::Float(value) => float_as_int(*value),
        _ => None,
    }
}

fn cell_to_float(cell: &Data) -> Option<f64> {
    match cell {
        Data::Int(value) => Some(*value as f64),
        Data::Float(value) if value.is_finite() => Some(*value),
        _ => None,
    }
}

fn cell_to_bool(cell: &Data) -> Option<bool> {
    match cell {
        Data::Bool(value) => Some(*value),
        _ => None,
    }
}

fn cell_to_days(cell: &Data) -> Option<i32> {
    naive_datetime(cell).map(|dt| date_to_days(dt.date()))
}

fn cell_to_millis(cell: &Data) -> Option<i64> {
    naive_datetime(cell).map(datetime_to_millis)
}

fn naive_datetime(cell: &Data) -> Option<NaiveDateTime> {
    match cell {
        Data::DateTime(dt) if !dt.is_duration() => dt.as_datetime(),
        Data::DateTimeIso(value) => parse_iso(value),
        _ => None,
    }
}

fn float_as_int(value: f64) -> Option<i64> {
    if !value.is_finite() || value.fract() != 0.0 {
        return None;
    }
    if value < i64::MIN as f64 || value > i64::MAX as f64 {
        return None;
    }
    Some(value as i64)
}

fn is_midnight(value: NaiveDateTime) -> bool {
    value.time() == NaiveTime::MIN
}

fn parse_iso(value: &str) -> Option<NaiveDateTime> {
    if let Ok(dt) = NaiveDateTime::parse_from_str(value, "%Y-%m-%dT%H:%M:%S%.f") {
        return Some(dt);
    }
    if let Ok(dt) = NaiveDateTime::parse_from_str(value, "%Y-%m-%d %H:%M:%S%.f") {
        return Some(dt);
    }
    if let Ok(date) = NaiveDate::parse_from_str(value, "%Y-%m-%d") {
        return date.and_hms_opt(0, 0, 0);
    }
    chrono::DateTime::parse_from_rfc3339(value)
        .ok()
        .map(|dt| dt.naive_utc())
}

fn format_datetime_value(value: NaiveDateTime) -> String {
    if is_midnight(value) {
        format!(
            "{:04}-{:02}-{:02}",
            value.year(),
            value.month(),
            value.day()
        )
    } else {
        format!(
            "{:04}-{:02}-{:02} {:02}:{:02}:{:02}",
            value.year(),
            value.month(),
            value.day(),
            value.hour(),
            value.minute(),
            value.second()
        )
    }
}

fn date_to_days(date: NaiveDate) -> i32 {
    let epoch = NaiveDate::from_ymd_opt(1970, 1, 1).expect("unix epoch");
    (date - epoch).num_days() as i32
}

fn datetime_to_millis(value: NaiveDateTime) -> i64 {
    value.and_utc().timestamp_millis()
}

#[cfg(test)]
mod tests {
    use super::*;
    use calamine::{ExcelDateTime, ExcelDateTimeType};

    fn excel_datetime(year: i32, month: u32, day: u32, hour: u32, min: u32, sec: u32) -> Data {
        let date = NaiveDate::from_ymd_opt(year, month, day).unwrap();
        let epoch = NaiveDate::from_ymd_opt(1899, 12, 30).unwrap();
        let days = (date - epoch).num_days() as f64;
        let seconds = hour * 3600 + min * 60 + sec;
        let serial = days + (seconds as f64) / 86_400.0;
        Data::DateTime(ExcelDateTime::new(
            serial,
            ExcelDateTimeType::DateTime,
            false,
        ))
    }

    #[test]
    fn sheet_name_beats_numeric_index() {
        let names = vec!["Orders".to_string(), "1".to_string(), "Returns".to_string()];
        assert_eq!(resolve_sheet(&names, Some("1")).unwrap(), "1");
        assert_eq!(resolve_sheet(&names, Some("0")).unwrap(), "Orders");
        assert_eq!(resolve_sheet(&names, Some("2")).unwrap(), "Returns");
        assert_eq!(resolve_sheet(&names, Some("Orders")).unwrap(), "Orders");
        assert_eq!(resolve_sheet(&names, None).unwrap(), "Orders");
    }

    #[test]
    fn unknown_sheet_lists_names() {
        let names = vec!["Orders".to_string(), "Returns".to_string()];
        let err = resolve_sheet(&names, Some("Missing")).unwrap_err();
        let msg = err.to_string();
        assert!(msg.contains("unknown sheet 'Missing'"), "{msg}");
        assert!(msg.contains("Orders"), "{msg}");
        assert!(msg.contains("Returns"), "{msg}");
        let err = resolve_sheet(&names, Some("9")).unwrap_err();
        assert!(err.to_string().contains("have: Orders, Returns"));
    }

    #[test]
    fn types_nulls_and_mixed_columns() {
        let rows = [
            vec![
                Data::Int(1),
                Data::Float(1.5),
                Data::Bool(true),
                excel_datetime(2024, 1, 15, 0, 0, 0),
                excel_datetime(2024, 1, 15, 13, 45, 0),
                Data::Int(1),
                Data::Empty,
            ],
            vec![
                Data::Float(2.0),
                Data::Float(2.25),
                Data::Bool(false),
                excel_datetime(2024, 1, 16, 0, 0, 0),
                excel_datetime(2024, 1, 16, 8, 0, 0),
                Data::String("x".into()),
                Data::String(String::new()),
            ],
            vec![
                Data::Empty,
                Data::Int(3),
                Data::Empty,
                excel_datetime(2024, 1, 17, 0, 0, 0),
                excel_datetime(2024, 1, 17, 0, 0, 0),
                Data::Float(3.5),
                Data::Empty,
            ],
        ];
        let names = ["qty", "price", "flag", "day", "at", "mixed", "empty"];
        let mut columns = Vec::new();
        for (col, name) in names.iter().enumerate() {
            let cells: Vec<&Data> = rows.iter().map(|row| &row[col]).collect();
            columns.push(column_from_cells(name, &cells).unwrap());
        }
        let df = DataFrame::new(columns).unwrap();
        assert_eq!(df.height(), 3);
        assert_eq!(df.column("qty").unwrap().dtype(), &DataType::Int64);
        assert_eq!(df.column("qty").unwrap().null_count(), 1);
        assert_eq!(df.column("price").unwrap().dtype(), &DataType::Float64);
        assert_eq!(df.column("flag").unwrap().dtype(), &DataType::Boolean);
        assert_eq!(df.column("flag").unwrap().null_count(), 1);
        assert_eq!(df.column("day").unwrap().dtype(), &DataType::Date);
        assert_eq!(
            df.column("at").unwrap().dtype(),
            &DataType::Datetime(TimeUnit::Milliseconds, None)
        );
        assert_eq!(df.column("mixed").unwrap().dtype(), &DataType::String);
        assert_eq!(df.column("empty").unwrap().dtype(), &DataType::String);
        assert_eq!(df.column("empty").unwrap().null_count(), 3);

        let mixed = df
            .column("mixed")
            .unwrap()
            .as_materialized_series()
            .str()
            .unwrap();
        assert_eq!(mixed.get(0), Some("1"));
        assert_eq!(mixed.get(1), Some("x"));
        assert_eq!(mixed.get(2), Some("3.5"));
    }
}
