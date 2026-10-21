# Changelog

All notable changes to datapeek are documented in this file.

The format is based on [Keep a Changelog](https://keepachangelog.com/en/1.1.0/),
and this project adheres to [Semantic Versioning](https://semver.org/spec/v2.0.0.html).

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
