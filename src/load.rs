use std::fs::File;
use std::io::{Read, Seek, SeekFrom};
use std::path::Path;

use polars::prelude::*;

use crate::error::{Error, Result};

/// How we will parse a file, after looking at the extension and a small sniff.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum FileKind {
    Csv { sep: u8 },
    Parquet,
    Jsonl,
}

pub struct Loaded {
    pub df: DataFrame,
    pub kind: FileKind,
}

/// Load a tabular file into a DataFrame.
///
/// `sample` takes the first N rows (cheap for delimited and Parquet scans).
/// Column filtering happens after the schema is known so missing names error clearly.
pub fn load(path: &Path, columns: Option<&[String]>, sample: Option<usize>) -> Result<Loaded> {
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
    let df = match kind {
        FileKind::Csv { sep } => load_csv(path, sep, sample).map_err(|e| wrap_parse(path, e))?,
        FileKind::Parquet => load_parquet(path, sample).map_err(|e| wrap_parse(path, e))?,
        FileKind::Jsonl => load_jsonl(path, sample).map_err(|e| wrap_parse(path, e))?,
    };

    if df.width() == 0 {
        return Err(Error::new(format!(
            "no columns found in '{}'",
            path.display()
        )));
    }

    let df = select_columns(df, columns)?;
    Ok(Loaded { df, kind })
}

fn wrap_parse(path: &Path, err: polars::prelude::PolarsError) -> Error {
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
        "xlsx" | "xlsm" | "xls" | "ods" | "xlsb" => Err(Error::new(format!(
            "Excel files are not supported in v1: {}",
            path.display()
        ))),
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

fn load_csv(path: &Path, sep: u8, sample: Option<usize>) -> PolarsResult<DataFrame> {
    let mut reader = LazyCsvReader::new(path)
        .with_has_header(true)
        .with_separator(sep)
        .with_infer_schema_length(Some(10_000))
        .with_try_parse_dates(true)
        .with_ignore_errors(true);
    if let Some(n) = sample {
        reader = reader.with_n_rows(Some(n));
    }
    reader.finish()?.collect()
}

fn load_parquet(path: &Path, sample: Option<usize>) -> PolarsResult<DataFrame> {
    let args = ScanArgsParquet {
        n_rows: sample,
        ..Default::default()
    };
    LazyFrame::scan_parquet(path, args)?.collect()
}

fn load_jsonl(path: &Path, sample: Option<usize>) -> PolarsResult<DataFrame> {
    let mut df = LazyJsonLineReader::new(path).finish()?.collect()?;
    if let Some(n) = sample {
        df = df.slice(0, n);
    }
    Ok(df)
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
    for name in wanted {
        if !available.iter().any(|a| a == name) {
            return Err(Error::new(format!(
                "unknown column '{name}' (have: {})",
                available.join(", ")
            )));
        }
    }
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
}
