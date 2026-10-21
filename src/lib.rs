//! datapeek: instant profiles of CSV, TSV, Parquet, JSON Lines, and Excel files.

pub mod display;
pub mod error;
pub mod excel;
pub mod load;
pub mod profile;
pub mod sample;
pub mod sparkline;

pub use error::{Error, Result};
pub use load::{FileKind, ReadMode, RowSelect};
pub use profile::{profile_path, ColumnProfile, FileProfile, InferredType, ProfileOptions};
