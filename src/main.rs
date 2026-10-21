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

    /// Fail on CSV parsing errors or Excel error cells instead of tolerating them
    #[arg(long)]
    strict: bool,

    /// Include technical details for CSV/TSV parsing errors
    #[arg(long)]
    verbose: bool,

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
    let cli = Cli::parse();
    match run(&cli) {
        Ok(()) => ExitCode::SUCCESS,
        Err(err) => {
            eprintln!("datapeek: {err}");
            if cli.strict && err.allows_tolerant_retry() {
                eprintln!("\nTo get a profile anyway, run without --strict. Problem values will count as missing:\n  {}", tolerant_command(std::env::args_os().map(|arg| arg.to_string_lossy().into_owned())));
            }
            if let Some(details) = err.technical_details() {
                if cli.verbose {
                    eprintln!("\nTechnical details:\n{details}");
                } else {
                    eprintln!("\nFor technical details, add --verbose.");
                }
            }
            ExitCode::from(1)
        }
    }
}

fn run(cli: &Cli) -> datapeek::Result<()> {
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
        strict: cli.strict,
    };
    let profile = profile_path(&cli.file, &opts)?;

    for warning in &profile.warnings {
        eprintln!("datapeek: warning: {}", warning.message);
    }

    if cli.json {
        write_stdout(&profile.to_json()?)?;
        write_stdout("\n")?;
    } else {
        write_stdout(&render_table(&profile))?;
    }
    Ok(())
}

fn tolerant_command(args: impl IntoIterator<Item = String>) -> String {
    let mut args = args.into_iter();
    let executable = args.next().unwrap_or_else(|| "datapeek".into());
    let mut command = if cfg!(windows) {
        format!("& {}", shell_quote(&executable))
    } else {
        shell_quote(&executable)
    };
    let mut positional = false;
    for arg in args {
        if !positional && arg == "--strict" {
            continue;
        }
        if arg == "--" {
            positional = true;
        }
        command.push(' ');
        command.push_str(&shell_quote(&arg));
    }
    command
}

fn shell_quote(arg: &str) -> String {
    if !arg.is_empty()
        && arg
            .chars()
            .all(|c| c.is_ascii_alphanumeric() || "-_/.:".contains(c))
    {
        arg.to_string()
    } else if cfg!(windows) {
        format!("'{}'", arg.replace('\'', "''"))
    } else {
        format!("'{}'", arg.replace('\'', "'\\''"))
    }
}

fn write_stdout(s: &str) -> datapeek::Result<()> {
    let mut out = io::stdout().lock();
    match out.write_all(s.as_bytes()) {
        Ok(()) => Ok(()),
        Err(err) if err.kind() == io::ErrorKind::BrokenPipe => Ok(()),
        Err(err) => Err(err.into()),
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn retry_keeps_sampling_options_and_a_positional_strict_filename() {
        let args = [
            "datapeek", "--strict", "--sample", "15", "--seed", "4", "--", "--strict",
        ];
        let command = tolerant_command(args.into_iter().map(str::to_string));
        assert!(command.contains("--sample 15 --seed 4 -- --strict"));
        assert_eq!(command.matches("--strict").count(), 1);
    }
}
