use std::fs::File;
use std::io::{BufRead, BufReader, Cursor, Read, Seek, SeekFrom};
use std::path::Path;

use polars::prelude::*;
use rand::Rng;
use serde::Serialize;

use crate::error::{Error, Result};
use crate::excel;
use crate::sample;

/// How we will parse a file, after looking at the extension and a small sniff.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum FileKind {
    Csv { sep: u8 },
    Parquet,
    Jsonl,
    Excel(ExcelKind),
}

/// Workbook extensions calamine opens directly.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum ExcelKind {
    Xlsx,
    Xlsm,
    Xls,
    Xlsb,
}

impl ExcelKind {
    pub fn as_str(self) -> &'static str {
        match self {
            Self::Xlsx => "xlsx",
            Self::Xlsm => "xlsm",
            Self::Xls => "xls",
            Self::Xlsb => "xlsb",
        }
    }
}

/// Which rows to profile.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum RowSelect {
    /// Every row.
    All,
    /// The first N rows. Scanning stops once those rows are in hand.
    Head(usize),
    /// A uniform sample of N rows from the whole file.
    Sample { n: usize, seed: Option<u64> },
}

/// How the loaded frame relates to the file, for the header line and JSON.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize)]
#[serde(rename_all = "lowercase")]
pub enum ReadMode {
    Full,
    Head,
    Random,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct ReadOutcome {
    pub mode: ReadMode,
    pub seed: Option<u64>,
    pub limit: Option<usize>,
}

impl ReadOutcome {
    pub fn full() -> Self {
        Self {
            mode: ReadMode::Full,
            seed: None,
            limit: None,
        }
    }

    pub fn head(n: usize) -> Self {
        Self {
            mode: ReadMode::Head,
            seed: None,
            limit: Some(n),
        }
    }

    pub fn random(n: usize, seed: Option<u64>) -> Self {
        Self {
            mode: ReadMode::Random,
            seed,
            limit: Some(n),
        }
    }
}

pub struct Loaded {
    pub df: DataFrame,
    pub kind: FileKind,
    pub outcome: ReadOutcome,
    pub sheet: Option<String>,
    pub warnings: Vec<ParseWarning>,
}

/// A problem observed while parsing the profiled input.
#[derive(Debug, Clone, Serialize)]
pub struct ParseWarning {
    pub code: &'static str,
    pub message: String,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub column: Option<String>,
    /// Exact count when the source reader exposes one (Excel cells).
    #[serde(skip_serializing_if = "Option::is_none")]
    pub count: Option<usize>,
}

/// Load a tabular file into a DataFrame.
///
/// `--head` stops after N rows. `--sample` draws a uniform sample: delimited
/// and JSON Lines files use a streaming reservoir, Parquet reads only the row
/// groups that contain the chosen row indexes (and only the requested
/// columns), and Excel picks row indexes out of the sheet calamine already
/// parsed. When N is at least the row count, the whole file is loaded the
/// same way as an unsampled profile.
pub fn load(
    path: &Path,
    columns: Option<&[String]>,
    rows: RowSelect,
    sheet: Option<&str>,
) -> Result<Loaded> {
    load_with_policy(path, columns, rows, sheet, false)
}

pub fn load_with_policy(
    path: &Path,
    columns: Option<&[String]>,
    rows: RowSelect,
    sheet: Option<&str>,
    strict: bool,
) -> Result<Loaded> {
    if matches!(rows, RowSelect::Head(0) | RowSelect::Sample { n: 0, .. }) {
        return Err(Error::new("row limit must be at least 1"));
    }
    if !path.exists() {
        return Err(Error::new(format!("no such file: {}", path.display())));
    }
    if path.is_dir() {
        return Err(Error::new(format!("is a directory: {}", path.display())));
    }
    let meta = std::fs::metadata(path).map_err(|e| Error::io(path, e))?;
    if meta.len() == 0 {
        return Err(Error::new(format!("file is empty: {}", path.display())));
    }

    let kind = sniff_kind(path)?;
    if sheet.is_some() && !matches!(kind, FileKind::Excel(_)) {
        return Err(Error::new(format!(
            "--sheet only applies to Excel files (.xlsx, .xlsm, .xls, .xlsb); '{}' is {}",
            path.display(),
            format_name(kind)
        )));
    }

    let mut warnings = Vec::new();
    let (df, outcome, sheet_name) = match kind {
        FileKind::Csv { sep } => {
            let (df, outcome) = load_csv(path, sep, rows, strict, &mut warnings)?;
            (df, outcome, None)
        }
        FileKind::Parquet => {
            let (df, outcome) = load_parquet(path, columns, rows)?;
            (df, outcome, None)
        }
        FileKind::Jsonl => {
            let (df, outcome) = load_jsonl(path, rows)?;
            (df, outcome, None)
        }
        FileKind::Excel(_) => {
            let loaded = excel::load_excel_with_policy(path, rows, sheet, columns, strict)?;
            warnings = loaded.warnings;
            (loaded.df, loaded.outcome, Some(loaded.sheet))
        }
    };

    if df.width() == 0 {
        return Err(Error::new(format!(
            "no columns found in '{}'",
            path.display()
        )));
    }

    let df = select_columns(df, columns)?;
    Ok(Loaded {
        df,
        kind,
        outcome,
        sheet: sheet_name,
        warnings,
    })
}

pub fn format_name(kind: FileKind) -> String {
    match kind {
        FileKind::Csv { sep: b'\t' } => "tsv".into(),
        FileKind::Csv { .. } => "csv".into(),
        FileKind::Parquet => "parquet".into(),
        FileKind::Jsonl => "jsonl".into(),
        FileKind::Excel(kind) => kind.as_str().into(),
    }
}

fn wrap_parse(path: &Path, err: impl std::fmt::Display) -> Error {
    Error::parse(path, err)
}

pub fn sniff_kind(path: &Path) -> Result<FileKind> {
    let ext = path
        .extension()
        .and_then(|e| e.to_str())
        .unwrap_or("")
        .to_ascii_lowercase();

    match ext.as_str() {
        "parquet" | "pq" => Ok(FileKind::Parquet),
        "tsv" | "tab" => Ok(FileKind::Csv { sep: b'\t' }),
        "csv" => {
            let sep = detect_delimiter_file(path).unwrap_or(b',');
            Ok(FileKind::Csv { sep })
        }
        "jsonl" | "ndjson" | "json" => Ok(FileKind::Jsonl),
        "xlsx" => Ok(FileKind::Excel(ExcelKind::Xlsx)),
        "xlsm" => Ok(FileKind::Excel(ExcelKind::Xlsm)),
        "xls" => Ok(FileKind::Excel(ExcelKind::Xls)),
        "xlsb" => Ok(FileKind::Excel(ExcelKind::Xlsb)),
        "ods" => Err(Error::unsupported(
            path,
            "OpenDocument spreadsheets are not supported",
        )),
        _ => sniff_content(path),
    }
}

fn sniff_content(path: &Path) -> Result<FileKind> {
    let mut f = File::open(path).map_err(|e| Error::io(path, e))?;
    let mut buf = [0u8; 16];
    let n = f.read(&mut buf).map_err(|e| Error::io(path, e))?;
    if n >= 4 && &buf[..4] == b"PAR1" {
        return Ok(FileKind::Parquet);
    }
    f.seek(SeekFrom::Start(0)).map_err(|e| Error::io(path, e))?;
    let mut head = Vec::new();
    f.take(64 * 1024)
        .read_to_end(&mut head)
        .map_err(|e| Error::io(path, e))?;
    if looks_like_jsonl(&head) {
        return Ok(FileKind::Jsonl);
    }
    let sep = detect_delimiter(&head);
    Ok(FileKind::Csv { sep })
}

fn looks_like_jsonl(bytes: &[u8]) -> bool {
    let text = String::from_utf8_lossy(bytes);
    let line = text
        .lines()
        .find(|l| !l.trim().is_empty())
        .unwrap_or("")
        .trim();
    line.starts_with('{') || line.starts_with('[')
}

fn detect_delimiter_file(path: &Path) -> Option<u8> {
    let f = File::open(path).ok()?;
    let mut head = Vec::new();
    f.take(64 * 1024).read_to_end(&mut head).ok()?;
    Some(detect_delimiter(&head))
}

/// Pick the delimiter with the most consistent column count across the first lines.
pub fn detect_delimiter(bytes: &[u8]) -> u8 {
    const CANDIDATES: &[u8] = b",\t;|";
    let text = String::from_utf8_lossy(bytes);
    let lines: Vec<&str> = text
        .lines()
        .filter(|l| !l.trim().is_empty())
        .take(20)
        .collect();
    if lines.is_empty() {
        return b',';
    }

    let mut best = b',';
    let mut best_score = -1i32;
    for &sep in CANDIDATES {
        let counts: Vec<usize> = lines.iter().map(|l| split_count(l, sep)).collect();
        let first = counts[0];
        if first < 2 {
            continue;
        }
        if counts.iter().any(|&c| c != first) {
            continue;
        }
        let score = first as i32;
        if score > best_score {
            best_score = score;
            best = sep;
        }
    }
    best
}

fn split_count(line: &str, sep: u8) -> usize {
    let mut n = 1usize;
    let mut in_quotes = false;
    for c in line.chars() {
        if c == '"' {
            in_quotes = !in_quotes;
        } else if c == sep as char && !in_quotes {
            n += 1;
        }
    }
    n
}

fn load_csv(
    path: &Path,
    sep: u8,
    rows: RowSelect,
    strict: bool,
    warnings: &mut Vec<ParseWarning>,
) -> Result<(DataFrame, ReadOutcome)> {
    match rows {
        RowSelect::All => {
            let df = checked_csv(strict, warnings, |ignore| read_csv(path, sep, ignore))?;
            Ok((df, ReadOutcome::full()))
        }
        RowSelect::Head(n) => head_csv(path, sep, n, strict, warnings),
        RowSelect::Sample { n, seed } => sample_csv(path, sep, n, seed, strict, warnings),
    }
}

fn head_csv(
    path: &Path,
    sep: u8,
    n: usize,
    strict: bool,
    warnings: &mut Vec<ParseWarning>,
) -> Result<(DataFrame, ReadOutcome)> {
    // Bound typed parsing to the requested records. Reader read-ahead and the
    // extra record used to establish the mode must not taint this profile.
    let mut reader = csv::ReaderBuilder::new()
        .delimiter(sep)
        .flexible(true)
        .from_path(path)
        .map_err(|err| Error::io(path, err))?;
    let header = reader
        .byte_headers()
        .map_err(|err| wrap_parse(path, err))?
        .clone();
    let mut buf = Vec::new();
    let mut writer = csv::WriterBuilder::new()
        .delimiter(sep)
        .flexible(true)
        .from_writer(&mut buf);
    writer
        .write_byte_record(&header)
        .map_err(|err| wrap_parse(path, err))?;
    let mut record = csv::ByteRecord::new();
    let mut count = 0;
    while count < n
        && reader
            .read_byte_record(&mut record)
            .map_err(|err| wrap_parse(path, err))?
    {
        writer
            .write_byte_record(&record)
            .map_err(|err| wrap_parse(path, err))?;
        count += 1;
    }
    writer.flush().map_err(|err| wrap_parse(path, err))?;
    drop(writer);
    // A malformed next record also establishes that the input continues;
    // it is outside the requested prefix and is not part of validation.
    let has_more = count == n && !matches!(reader.read_byte_record(&mut record), Ok(false));
    let df = checked_csv(strict, warnings, |ignore| {
        parse_csv_bytes(path, sep, &buf, ignore)
    })?;
    let outcome = if has_more {
        ReadOutcome::head(n)
    } else {
        ReadOutcome::full()
    };
    Ok((df, outcome))
}

fn read_csv(path: &Path, sep: u8, ignore_errors: bool) -> Result<DataFrame> {
    LazyCsvReader::new(path)
        .with_has_header(true)
        .with_separator(sep)
        .with_infer_schema_length(Some(10_000))
        .with_try_parse_dates(true)
        .with_ignore_errors(ignore_errors)
        .finish()
        .and_then(|lf| lf.collect())
        .map_err(|err| Error::csv_parse(path, err))
}

fn checked_csv(
    strict: bool,
    warnings: &mut Vec<ParseWarning>,
    mut read: impl FnMut(bool) -> Result<DataFrame>,
) -> Result<DataFrame> {
    match read(false) {
        Ok(df) => Ok(df),
        Err(err) if strict => Err(err),
        Err(err) => {
            // Only warn after the legacy tolerant reader actually succeeds.
            let df = read(true)?;
            warnings.push(ParseWarning {
                code: "csv_parse_errors",
                message: "Profile created, but some CSV values could not be read and are counted as missing. Run with --strict to see the problem and stop instead.".into(),
                column: err.column().map(str::to_string),
                count: None,
            });
            Ok(df)
        }
    }
}

fn sample_csv(
    path: &Path,
    sep: u8,
    n: usize,
    seed: Option<u64>,
    strict: bool,
    warnings: &mut Vec<ParseWarning>,
) -> Result<(DataFrame, ReadOutcome)> {
    let mut reader = csv::ReaderBuilder::new()
        .delimiter(sep)
        .has_headers(true)
        .flexible(false)
        .from_path(path)
        .map_err(|err| Error::io(path, err))?;
    let header = reader
        .byte_headers()
        .map_err(|err| wrap_parse(path, err))?
        .clone();

    let mut reservoir: Vec<(usize, csv::ByteRecord)> = Vec::with_capacity(n.min(65_536));
    let mut rng = sample::rng(seed);
    let mut total = 0usize;
    let mut record = csv::ByteRecord::new();
    loop {
        let more = reader
            .read_byte_record(&mut record)
            .map_err(|err| wrap_parse(path, err))?;
        if !more {
            break;
        }
        if total < n {
            reservoir.push((total, record.clone()));
        } else {
            let j = rng.gen_range(0..=total);
            if j < n {
                reservoir[j] = (total, record.clone());
            }
        }
        total += 1;
    }

    if total <= n {
        // Same loader as an unsampled profile, so types match exactly.
        let df = checked_csv(strict, warnings, |ignore| read_csv(path, sep, ignore))?;
        return Ok((df, ReadOutcome::full()));
    }

    reservoir.sort_unstable_by_key(|(idx, _)| *idx);
    let mut buf = Vec::new();
    {
        let mut writer = csv::WriterBuilder::new()
            .delimiter(sep)
            .from_writer(&mut buf);
        writer
            .write_byte_record(&header)
            .map_err(|err| wrap_parse(path, err))?;
        for (_, row) in &reservoir {
            writer
                .write_byte_record(row)
                .map_err(|err| wrap_parse(path, err))?;
        }
        writer.flush().map_err(|err| wrap_parse(path, err))?;
    }

    let df = checked_csv(strict, warnings, |ignore| {
        parse_csv_bytes(path, sep, &buf, ignore)
    })?;
    Ok((df, ReadOutcome::random(n, seed)))
}

fn parse_csv_bytes(path: &Path, sep: u8, buf: &[u8], ignore_errors: bool) -> Result<DataFrame> {
    CsvReadOptions::default()
        .with_has_header(true)
        .with_infer_schema_length(Some(10_000))
        .with_ignore_errors(ignore_errors)
        .with_raise_if_empty(false)
        .map_parse_options(|opts| opts.with_separator(sep).with_try_parse_dates(true))
        .into_reader_with_file_handle(Cursor::new(buf))
        .finish()
        .map_err(|err| Error::csv_parse(path, err))
}

fn load_parquet(
    path: &Path,
    columns: Option<&[String]>,
    rows: RowSelect,
) -> Result<(DataFrame, ReadOutcome)> {
    match rows {
        RowSelect::All => {
            let df = scan_parquet(path, None, None)?;
            Ok((df, ReadOutcome::full()))
        }
        RowSelect::Head(n) => {
            let df = scan_parquet(path, None, Some(n + 1))?;
            Ok(trim_head(df, n))
        }
        RowSelect::Sample { n, seed } => sample_parquet(path, columns, n, seed),
    }
}

fn scan_parquet(
    path: &Path,
    columns: Option<&[String]>,
    n_rows: Option<usize>,
) -> Result<DataFrame> {
    // The full-file path stays a lazy scan with no column projection pushed
    // down, matching v0.1. Sampling projects columns before the read.
    if columns.is_none() {
        let args = ScanArgsParquet {
            n_rows,
            ..Default::default()
        };
        return LazyFrame::scan_parquet(path, args)
            .and_then(|lf| lf.collect())
            .map_err(|err| wrap_parse(path, err));
    }

    let file = File::open(path).map_err(|err| Error::io(path, err))?;
    let mut reader = ParquetReader::new(file);
    if let Some(n) = n_rows {
        reader = reader.with_slice(Some((0, n)));
    }
    if let Some(cols) = columns {
        reader = reader.with_columns(Some(cols.to_vec()));
    }
    reader.finish().map_err(|err| wrap_parse(path, err))
}

fn sample_parquet(
    path: &Path,
    columns: Option<&[String]>,
    n: usize,
    seed: Option<u64>,
) -> Result<(DataFrame, ReadOutcome)> {
    let (total, spans) = parquet_layout(path)?;
    if n >= total {
        let df = scan_parquet(path, columns, None)?;
        return Ok((df, ReadOutcome::full()));
    }
    if let Some(wanted) = columns {
        let names = parquet_column_names(path)?;
        validate_columns(&names, wanted)?;
    }

    let indices = sample::sample_indices(total, n, seed);
    let mut parts = Vec::new();
    let mut cursor = 0usize;
    for (start, len) in spans {
        if len == 0 {
            continue;
        }
        let end = start + len;
        let mut local = Vec::new();
        while cursor < indices.len() && indices[cursor] < end {
            if indices[cursor] >= start {
                local.push(indices[cursor] - start);
            }
            cursor += 1;
        }
        if local.is_empty() {
            continue;
        }
        let file = File::open(path).map_err(|err| Error::io(path, err))?;
        let mut reader = ParquetReader::new(file).with_slice(Some((start, len)));
        if let Some(cols) = columns {
            reader = reader.with_columns(Some(cols.to_vec()));
        }
        let group = reader.finish().map_err(|err| wrap_parse(path, err))?;
        let taken = take_rows(&group, &local).map_err(|err| wrap_parse(path, err))?;
        parts.push(taken);
    }
    if cursor != indices.len() {
        return Err(Error::new(format!(
            "failed to parse '{}': parquet row groups do not cover every row",
            path.display()
        )));
    }
    let df = vstack_all(parts).map_err(|err| wrap_parse(path, err))?;
    Ok((df, ReadOutcome::random(n, seed)))
}

fn parquet_layout(path: &Path) -> Result<(usize, Vec<(usize, usize)>)> {
    let file = File::open(path).map_err(|err| Error::io(path, err))?;
    let mut reader = ParquetReader::new(file);
    let metadata = reader
        .get_metadata()
        .map_err(|err| wrap_parse(path, err))?
        .clone();
    let mut spans = Vec::with_capacity(metadata.row_groups.len());
    let mut start = 0usize;
    for group in &metadata.row_groups {
        let len = group.num_rows();
        spans.push((start, len));
        start += len;
    }
    if start != metadata.num_rows {
        return Err(Error::new(format!(
            "failed to parse '{}': parquet row count does not match its row groups",
            path.display()
        )));
    }
    Ok((metadata.num_rows, spans))
}

fn parquet_column_names(path: &Path) -> Result<Vec<String>> {
    let mut lf = LazyFrame::scan_parquet(
        path,
        ScanArgsParquet {
            ..Default::default()
        },
    )
    .map_err(|err| wrap_parse(path, err))?;
    let schema = lf.collect_schema().map_err(|err| wrap_parse(path, err))?;
    Ok(schema.iter_names().map(|name| name.to_string()).collect())
}

fn take_rows(df: &DataFrame, indices: &[usize]) -> PolarsResult<DataFrame> {
    let values: Vec<u64> = indices.iter().copied().map(|idx| idx as u64).collect();
    let series = Series::new("idx".into(), values).cast(&IDX_DTYPE)?;
    df.take(series.idx()?)
}

fn vstack_all(parts: Vec<DataFrame>) -> PolarsResult<DataFrame> {
    let mut parts = parts.into_iter();
    let Some(mut acc) = parts.next() else {
        polars_bail!(ComputeError: "sample selected no rows");
    };
    for df in parts {
        acc.vstack_mut(&df)?;
    }
    Ok(acc)
}

fn load_jsonl(path: &Path, rows: RowSelect) -> Result<(DataFrame, ReadOutcome)> {
    match rows {
        RowSelect::All => {
            let df = LazyJsonLineReader::new(path)
                .finish()
                .and_then(|lf| lf.collect())
                .map_err(|err| wrap_parse(path, err))?;
            Ok((df, ReadOutcome::full()))
        }
        RowSelect::Head(n) => {
            let prefix = read_line_prefix(path, n + 1)?;
            if prefix.len() <= n {
                let df = LazyJsonLineReader::new(path)
                    .finish()
                    .and_then(|lf| lf.collect())
                    .map_err(|err| wrap_parse(path, err))?;
                Ok((df, ReadOutcome::full()))
            } else {
                let df = dataframe_from_jsonl(path, &prefix[..n])?;
                Ok((df, ReadOutcome::head(n)))
            }
        }
        RowSelect::Sample { n, seed } => sample_jsonl(path, n, seed),
    }
}

fn sample_jsonl(path: &Path, n: usize, seed: Option<u64>) -> Result<(DataFrame, ReadOutcome)> {
    let file = File::open(path).map_err(|err| Error::io(path, err))?;
    let reader = BufReader::new(file);
    let mut reservoir: Vec<(usize, String)> = Vec::with_capacity(n.min(65_536));
    let mut rng = sample::rng(seed);
    let mut total = 0usize;
    for line in reader.lines() {
        let mut line = line.map_err(|err| Error::io(path, err))?;
        if line.ends_with('\r') {
            line.pop();
        }
        if total < n {
            reservoir.push((total, line));
        } else {
            let j = rng.gen_range(0..=total);
            if j < n {
                reservoir[j] = (total, line);
            }
        }
        total += 1;
    }
    if total <= n {
        let df = LazyJsonLineReader::new(path)
            .finish()
            .and_then(|lf| lf.collect())
            .map_err(|err| wrap_parse(path, err))?;
        return Ok((df, ReadOutcome::full()));
    }
    reservoir.sort_unstable_by_key(|(idx, _)| *idx);
    let lines: Vec<String> = reservoir.into_iter().map(|(_, line)| line).collect();
    let df = dataframe_from_jsonl(path, &lines)?;
    Ok((df, ReadOutcome::random(n, seed)))
}

fn read_line_prefix(path: &Path, max_lines: usize) -> Result<Vec<String>> {
    let file = File::open(path).map_err(|err| Error::io(path, err))?;
    let reader = BufReader::new(file);
    let mut lines = Vec::new();
    for line in reader.lines() {
        let mut line = line.map_err(|err| Error::io(path, err))?;
        if line.ends_with('\r') {
            line.pop();
        }
        lines.push(line);
        if lines.len() >= max_lines {
            break;
        }
    }
    Ok(lines)
}

fn dataframe_from_jsonl(path: &Path, lines: &[String]) -> Result<DataFrame> {
    let mut text = lines.join("\n");
    text.push('\n');
    JsonReader::new(Cursor::new(text.into_bytes()))
        .with_json_format(JsonFormat::JsonLines)
        .finish()
        .map_err(|err| wrap_parse(path, err))
}

fn trim_head(df: DataFrame, n: usize) -> (DataFrame, ReadOutcome) {
    if df.height() > n {
        (df.slice(0, n), ReadOutcome::head(n))
    } else {
        (df, ReadOutcome::full())
    }
}

fn validate_columns(available: &[String], wanted: &[String]) -> Result<()> {
    for name in wanted {
        if !available.iter().any(|have| have == name) {
            return Err(Error::new(format!(
                "unknown column '{name}' (have: {})",
                available.join(", ")
            )));
        }
    }
    Ok(())
}

fn select_columns(df: DataFrame, columns: Option<&[String]>) -> Result<DataFrame> {
    let Some(wanted) = columns else {
        return Ok(df);
    };
    if wanted.is_empty() {
        return Ok(df);
    }
    let available: Vec<String> = df
        .get_column_names()
        .iter()
        .map(|s| s.to_string())
        .collect();
    validate_columns(&available, wanted)?;
    df.select(wanted)
        .map_err(|e| Error::new(format!("could not select columns: {e}")))
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn detects_comma() {
        let sample = b"a,b,c\n1,2,3\n4,5,6\n";
        assert_eq!(detect_delimiter(sample), b',');
    }

    #[test]
    fn detects_comma_with_crlf() {
        let sample = b"a,b,c\r\n1,2,3\r\n4,5,6\r\n";
        assert_eq!(detect_delimiter(sample), b',');
    }

    #[test]
    fn detects_tab() {
        let sample = b"a\tb\tc\n1\t2\t3\n4\t5\t6\n";
        assert_eq!(detect_delimiter(sample), b'\t');
    }

    #[test]
    fn detects_pipe() {
        let sample = b"a|b|c\n1|2|3\n";
        assert_eq!(detect_delimiter(sample), b'|');
    }

    #[test]
    fn quoted_commas_do_not_break_csv() {
        let sample = b"name,city\n\"Smith, Jane\",Boston\n\"Lee, Ann\",Austin\n";
        assert_eq!(detect_delimiter(sample), b',');
    }

    #[test]
    fn sniffs_excel_extensions() {
        assert_eq!(
            sniff_kind(Path::new("book.xlsx")).unwrap(),
            FileKind::Excel(ExcelKind::Xlsx)
        );
        assert_eq!(
            sniff_kind(Path::new("book.XLSM")).unwrap(),
            FileKind::Excel(ExcelKind::Xlsm)
        );
        assert_eq!(
            sniff_kind(Path::new("book.xls")).unwrap(),
            FileKind::Excel(ExcelKind::Xls)
        );
        assert_eq!(
            sniff_kind(Path::new("book.xlsb")).unwrap(),
            FileKind::Excel(ExcelKind::Xlsb)
        );
        let err = sniff_kind(Path::new("book.ods")).unwrap_err();
        assert!(err.to_string().contains("OpenDocument"), "{err}");
    }

    #[test]
    fn sheet_flag_rejected_for_csv() {
        let dir = tempfile::tempdir().unwrap();
        let csv = dir.path().join("t.csv");
        std::fs::write(&csv, "a,b\n1,2\n").unwrap();
        let err = match load(&csv, None, RowSelect::All, Some("Sheet1")) {
            Ok(_) => panic!("--sheet on a csv should fail"),
            Err(err) => err,
        };
        let msg = err.to_string();
        assert!(msg.contains("--sheet"), "{msg}");
        assert!(msg.contains("Excel"), "{msg}");
    }
}
