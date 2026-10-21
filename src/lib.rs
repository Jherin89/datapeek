//! datapeek: instant profiles of CSV, TSV, Parquet, and JSON Lines files.

pub mod display;
pub mod error;
pub mod load;
pub mod profile;
pub mod sparkline;

pub use error::{Error, Result};
pub use profile::{profile_path, ColumnProfile, FileProfile, InferredType};
