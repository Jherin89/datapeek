use std::io::{self, Write};
use std::path::PathBuf;
use std::process::ExitCode;

use clap::Parser;
use datapeek::display::render_table;
use datapeek::profile::{profile_path, ProfileOptions};
use datapeek::RowSelect;

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
    after_help = "Formats: CSV/TSV (delimiter auto-detected), Parquet, JSON Lines, Excel (.xlsx, .xlsm, .xls, .xlsb)\n\n--sample draws a uniform random sample of N rows from the whole file.\n--head reads the first N rows and stops. The two flags cannot be combined.\nOn an Excel file, --sheet accepts a sheet name or a 0-based index. A value\nthat matches a sheet name exactly wins, even when the name is numeric."
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

    /// Uniform random sample of N rows from the whole file
    #[arg(long, value_name = "N", conflicts_with = "head")]
    sample: Option<usize>,

    /// Profile the first N rows and stop scanning
    #[arg(long, value_name = "N", conflicts_with = "sample")]
    head: Option<usize>,

    /// RNG seed for --sample. The same seed profiles the same rows
    #[arg(long, value_name = "SEED", requires = "sample")]
    seed: Option<u64>,

    /// Excel sheet name, or a 0-based index when no sheet has that exact name
    #[arg(long, value_name = "NAME|INDEX")]
    sheet: Option<String>,
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
    let rows = match (cli.sample, cli.head) {
        (Some(n), None) => RowSelect::Sample { n, seed: cli.seed },
        (None, Some(n)) => RowSelect::Head(n),
        (None, None) => RowSelect::All,
        (Some(_), Some(_)) => unreachable!("clap rejects --sample with --head"),
    };
    let opts = ProfileOptions {
        columns: cli.columns.as_deref(),
        rows,
        sheet: cli.sheet.as_deref(),
    };
    let profile = profile_path(&cli.file, &opts)?;

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
