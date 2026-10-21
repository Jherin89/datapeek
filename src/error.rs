use std::fmt;
use std::io;
use std::path::Path;

/// CLI-facing error. Displayed as `datapeek: {message}` on stderr.
#[derive(Debug)]
pub struct Error {
    message: String,
}

pub type Result<T> = std::result::Result<T, Error>;

impl Error {
    pub fn new(message: impl Into<String>) -> Self {
        Self {
            message: message.into(),
        }
    }

    pub fn io(path: &Path, err: impl fmt::Display) -> Self {
        Self::new(format!("cannot open '{}': {err}", path.display()))
    }

    pub fn parse(path: &Path, err: impl fmt::Display) -> Self {
        Self::new(format!("failed to parse '{}': {err}", path.display()))
    }

    pub fn unsupported(path: &Path, detail: impl fmt::Display) -> Self {
        Self::new(format!(
            "unsupported file '{}': {detail} (try CSV, TSV, Parquet, or JSON Lines)",
            path.display()
        ))
    }
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
