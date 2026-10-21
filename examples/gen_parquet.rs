//! One-shot helper: `cargo run --example gen_parquet`
//! Writes `samples/orders.parquet` from `samples/orders.csv`.

use std::fs::File;
use std::path::PathBuf;

use polars::prelude::*;

fn main() -> PolarsResult<()> {
    let root = PathBuf::from(env!("CARGO_MANIFEST_DIR"));
    let csv = root.join("samples/orders.csv");
    let out = root.join("samples/orders.parquet");
    let mut df = LazyCsvReader::new(&csv)
        .with_has_header(true)
        .with_try_parse_dates(true)
        .finish()?
        .collect()?;
    let mut file = File::create(&out)?;
    ParquetWriter::new(&mut file).finish(&mut df)?;
    println!("wrote {} ({} rows)", out.display(), df.height());
    Ok(())
}
