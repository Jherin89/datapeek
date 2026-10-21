use std::io::{self, Write};
use std::path::PathBuf;
use std::process::ExitCode;

use clap::Parser;
use datapeek::display::render_table;
use datapeek::profile::profile_path;

/// Instant data-file profiler.
///
/// Prints one row per column: inferred type, % missing, min/max, distinct
/// count, top values, and a unicode sparkline for numeric columns.
#[derive(Debug, Parser)]
#[command(
    name = "datapeek",
    version,
    about = "Instant data-file profiler",
    long_about = None,
    after_help = "Formats: CSV/TSV (delimiter auto-detected), Parquet, JSON Lines"
)]
struct Cli {
    /// Data file to profile
    file: PathBuf,

    /// Print a machine-readable JSON profile
    #[arg(long)]
    json: bool,

    /// Comma-separated column names to include
    #[arg(short, long, value_delimiter = ',', value_name = "COLS")]
    columns: Option<Vec<String>>,

    /// Profile only the first N rows
    #[arg(long, value_name = "N")]
    sample: Option<usize>,
}

fn main() -> ExitCode {
    match run() {
        Ok(()) => ExitCode::SUCCESS,
        Err(err) => {
            eprintln!("datapeek: {err}");
            ExitCode::from(1)
        }
    }
}

fn run() -> datapeek::Result<()> {
    let cli = Cli::parse();
    let columns = cli.columns.as_deref();
    let profile = profile_path(&cli.file, columns, cli.sample)?;

    if cli.json {
        write_stdout(&profile.to_json()?)?;
        write_stdout("\n")?;
    } else {
        write_stdout(&render_table(&profile))?;
    }
    Ok(())
}

fn write_stdout(s: &str) -> datapeek::Result<()> {
    let mut out = io::stdout().lock();
    match out.write_all(s.as_bytes()) {
        Ok(()) => Ok(()),
        Err(err) if err.kind() == io::ErrorKind::BrokenPipe => Ok(()),
        Err(err) => Err(err.into()),
    }
}
