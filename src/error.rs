use std::fmt;
use std::io;
use std::path::Path;

/// CLI-facing error. Displayed as `datapeek: {message}` on stderr.
#[derive(Debug)]
pub struct Error {
    message: String,
    technical_details: Option<String>,
    tolerant_retry: bool,
    column: Option<String>,
}

pub type Result<T> = std::result::Result<T, Error>;

impl Error {
    pub fn new(message: impl Into<String>) -> Self {
        Self {
            message: message.into(),
            technical_details: None,
            tolerant_retry: false,
            column: None,
        }
    }

    pub fn io(path: &Path, err: impl fmt::Display) -> Self {
        Self::new(format!("cannot open '{}': {err}", path.display()))
    }

    pub fn parse(path: &Path, err: impl fmt::Display) -> Self {
        Self::new(format!("failed to parse '{}': {err}", path.display()))
    }

    /// Present the pinned CSV reader's diagnostic in terms of the user's data.
    /// Unrecognized diagnostics retain their full text for verbose output.
    pub fn csv_parse(path: &Path, err: impl fmt::Display) -> Self {
        let details = err.to_string();
        let mut error = if let Some((value, expected, column)) = csv_value_problem(&details) {
            let expected = match expected {
                "i8" | "i16" | "i32" | "i64" | "u8" | "u16" | "u32" | "u64" => "whole numbers",
                "f32" | "f64" => "numbers",
                "bool" => "true/false values",
                dtype if dtype.starts_with("date") => "dates or times",
                _ => "values matching the inferred column type",
            };
            let mut error = Self::new(format!(
                "couldn't profile this file because a value doesn't match its column.\n\n  File: {}\n  Column: {}\n  Expected: {expected}\n  Found: {:?}\n\nNo profile was created. Fix the value in this column.",
                path.display(), column.escape_debug(), preview(value)
            ));
            error.column = Some(column.to_string());
            error.tolerant_retry = true;
            error
        } else {
            Self::new(format!(
                "couldn't read '{}' as a CSV/TSV file.\n\nCheck that the file can be opened and is a valid delimited table with the same fields as the header.\nNo profile was created.",
                path.display()
            ))
        };
        error.technical_details = Some(details);
        error
    }

    pub fn technical_details(&self) -> Option<&str> {
        self.technical_details.as_deref()
    }

    pub fn allows_tolerant_retry(&self) -> bool {
        self.tolerant_retry
    }

    pub(crate) fn with_tolerant_retry(mut self) -> Self {
        self.tolerant_retry = true;
        self
    }

    pub(crate) fn column(&self) -> Option<&str> {
        self.column.as_deref()
    }

    pub fn unsupported(path: &Path, detail: impl fmt::Display) -> Self {
        Self::new(format!(
            "unsupported file '{}': {detail} (try CSV, TSV, Parquet, JSON Lines, or Excel)",
            path.display()
        ))
    }
}

fn csv_value_problem(details: &str) -> Option<(&str, &str, &str)> {
    // This is a presentation adapter for Polars 0.46, not a parsing decision.
    // Match the complete known shape; use a generic message if it changes.
    let rest = details.strip_prefix("could not parse `")?;
    let (value, rest) = rest.rsplit_once("` as dtype `")?;
    let (expected, rest) = rest.split_once("` at column '")?;
    let (column, _) = rest.split_once("\n").unwrap_or((rest, ""));
    let (column, number) = column.rsplit_once("' (column number ")?;
    number.strip_suffix(')')?.parse::<usize>().ok()?;
    Some((value, expected, column))
}

fn preview(value: &str) -> String {
    let mut chars = value.chars();
    let mut preview: String = chars.by_ref().take(80).collect();
    if chars.next().is_some() {
        preview.push('…');
    }
    preview
}

impl fmt::Display for Error {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.write_str(&self.message)
    }
}

impl std::error::Error for Error {}

impl From<io::Error> for Error {
    fn from(err: io::Error) -> Self {
        Self::new(err.to_string())
    }
}

impl From<polars::prelude::PolarsError> for Error {
    fn from(err: polars::prelude::PolarsError) -> Self {
        Self::new(format!("failed to read data: {err}"))
    }
}

impl From<serde_json::Error> for Error {
    fn from(err: serde_json::Error) -> Self {
        Self::new(format!("failed to write JSON: {err}"))
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn unknown_csv_diagnostics_keep_details_without_inventing_a_location() {
        let raw = "reader changed its diagnostic format\ninternal settings";
        let err = Error::csv_parse(Path::new("input.csv"), raw);
        assert!(err.to_string().contains("couldn't read"));
        assert!(!err.to_string().contains("internal settings"));
        assert_eq!(err.technical_details(), Some(raw));
        assert_eq!(err.column(), None);
        assert!(!err.allows_tolerant_retry());
    }

    #[test]
    fn csv_value_previews_escape_controls_and_limit_long_values() {
        let value = format!("\x1b[31m\n{}", "x".repeat(1000));
        let raw = format!("could not parse `{value}` as dtype `f64` at column 'total' (column number 1)\nparser details");
        let err = Error::csv_parse(Path::new("input.csv"), &raw);
        let display = err.to_string();
        assert!(display.contains("Expected: numbers"));
        assert!(display.contains("Column: total"));
        assert!(!display.contains('\x1b'));
        assert!(!display.contains(&"x".repeat(81)));
        assert!(display.contains('…'));
        assert_eq!(err.technical_details(), Some(raw.as_str()));
    }
}
