# Changelog

All notable changes to datapeek are documented in this file.

The format is based on [Keep a Changelog](https://keepachangelog.com/en/1.1.0/),
and this project adheres to [Semantic Versioning](https://semver.org/spec/v2.0.0.html).

## [0.3.0] - 2026-10-24

### Added

- A totals panel at the top of the terminal profile: rows, columns, cells, and filled cells (count and percent). Thousands separators match the table. A partial read labels the panel as the profiled rows and, when the file total was already known, shows that count too. Narrow terminals fall back to stacked lines so the box does not wrap. `NO_COLOR` and piped output stay plain and readable.
- JSON fields `cells`, `filled_cells`, `filled_pct`, and `file_rows`. `file_rows` is the file's row count when a full read, a Parquet footer, a random sample, or an in-memory Excel sheet already had it, and `null` for a CSV or JSON Lines `--head`.
- `--strict` rejects CSV/TSV parsing errors and Excel error cells without writing a profile.
- Parsing errors explain the affected column and value; Excel errors identify the worksheet cell. Strict failures offer a command to profile the same input with tolerated errors. `--verbose` exposes technical CSV parser details.
- Tolerated parsing problems produce stderr warnings and a structured JSON `warnings` field. Excel warnings count error cells per selected column; missingness continues to include them.

### Changed

- Empty strings count as missing everywhere: CSV and TSV fields, JSON Lines `""`, Excel empty cells and empty-string cells, and Parquet empty strings. The miss column and the filled total use that same count. Whitespace-only text is still data.

### Fixed

- CSV/TSV parsing errors that previously became null values silently are now reported.
- `--head` limits CSV value parsing and schema inference to the requested records, so an error in the lookahead record does not affect the profile.

## [0.2.0] - 2026-10-21

### Added

- Excel profiles for `.xlsx`, `.xlsm`, `.xls`, and `.xlsb`. The first sheet is the default. The first row is the header. Cells become integers, floats, bools, dates, and datetimes; a mixed column falls back to text; empty cells are null.
- `--sheet <name|index>`. A value that matches a sheet name exactly wins, even when the name is numeric. Otherwise a value that parses as an integer is a 0-based index. An unknown sheet is an error that lists the sheet names. `--sheet` on a non-Excel file is an error.
- `--head N`, the fast first-N scan that used to be `--sample`.
- `--seed <u64>` for `--sample`. The same seed profiles the same rows. Without `--seed`, each run draws a new sample.
- JSON fields `mode` (`full`, `head`, or `random`) and `seed` (null when unused). Excel profiles include `sheet`.
- `samples/orders.xlsx` (sheets Orders, Returns, Types) and `examples/gen_xlsx.rs` to regenerate it.

### Changed

- `--sample N` is a uniform random sample of N rows from the whole file, not the first N. The header reads `random sample of N rows (seed S) × … columns`. If N covers every row, the profile is a full read (`mode` is `full`, `sampled` is false).
- `--sample` and `--head` cannot be combined. `--seed` requires `--sample`.
- Minimum supported Rust version is 1.89 (was 1.85). `rust-toolchain.toml` tracks the stable channel. The transitive pins that kept the crate building on rustc 1.85 are removed.

## [0.1.0] - 2026-10-21

### Added

- Initial release. Profiles CSV, TSV (and sniffed semicolon or pipe text), Parquet, and JSON Lines: inferred type, missingness, min/max, distinct count, top values, and an 8-bin sparkline.
- Colored terminal table and `--json`.
- `--columns` to keep named columns, and `--sample N` to profile the first N rows.
- Prebuilt archives for macOS Apple Silicon, macOS Intel, Linux x86_64, and Windows x86_64.
