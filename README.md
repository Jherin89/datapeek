# datapeek

**See what's in a data file before you load it.**

Point datapeek at a CSV, TSV, Parquet, or JSON Lines file and get one row per
column: inferred type, missingness, range, distinct count, top values, and a
sparkline.

[![CI](https://github.com/Jherin89/datapeek/actions/workflows/ci.yml/badge.svg)](https://github.com/Jherin89/datapeek/actions/workflows/ci.yml)
[![Release](https://img.shields.io/github/v/release/Jherin89/datapeek)](https://github.com/Jherin89/datapeek/releases/latest)
[![License: MIT](https://img.shields.io/badge/license-MIT-blue.svg)](LICENSE)
[![Rust 1.85+](https://img.shields.io/badge/rust-1.85%2B-DEA584?logo=rust)](https://www.rust-lang.org)

![datapeek profiling samples/orders.csv](docs/screenshot.png)

![demo: a profile, --columns, and --json piped to jq](docs/demo.gif)

## Features

- A column profile in one command, with no notebook and no Python environment.
- CSV, TSV, semicolon- and pipe-delimited text, Parquet, and JSON Lines.
- A colored terminal table, plus `--json` for scripts.
- `--columns` to focus a profile, and `--sample` to read only the first N rows.
- Built on [Polars](https://pola.rs), so a few million rows stay practical on a laptop.
- Prebuilt binaries for macOS (Apple Silicon and Intel), Linux x86_64, and Windows x86_64.

## Why datapeek

A file shows up from a partner, a pipeline dump, or a Parquet landing in a
bucket, and the question is small: what is in here? Types, holes, ranges, and
whether a numeric column is a spike or a flat line. Opening a notebook to
answer that costs more than the question.

I wrote datapeek for that moment. It is a small Rust CLI that prints the
shape of the file and gets out of the way. Use it before a warehouse load,
before you trust a schema, or whenever you want a second look without
standing up pandas.

## Getting started

v0.1.0 binaries are on the
[GitHub release](https://github.com/Jherin89/datapeek/releases/tag/v0.1.0).
Pick the archive that matches your machine. Each one unpacks to a directory
of the same name, with the `datapeek` binary, `LICENSE`, and `README.md`.

| Machine | Asset |
| --- | --- |
| macOS Apple Silicon | `datapeek-v0.1.0-aarch64-apple-darwin.tar.gz` |
| macOS Intel | `datapeek-v0.1.0-x86_64-apple-darwin.tar.gz` |
| Linux x86_64 | `datapeek-v0.1.0-x86_64-unknown-linux-gnu.tar.gz` |
| Windows x86_64 | `datapeek-v0.1.0-x86_64-pc-windows-msvc.zip` |

`datapeek --version` prints `datapeek 0.1.0`.

### macOS (Apple Silicon)

1. Download the Apple Silicon archive.

   ```bash
   curl -fL -o datapeek-v0.1.0-aarch64-apple-darwin.tar.gz \
     https://github.com/Jherin89/datapeek/releases/download/v0.1.0/datapeek-v0.1.0-aarch64-apple-darwin.tar.gz
   ```

2. Extract it.

   ```bash
   tar -xzf datapeek-v0.1.0-aarch64-apple-darwin.tar.gz
   ```

3. Clear the Gatekeeper quarantine attribute. macOS adds
   `com.apple.quarantine` to files downloaded from the internet, and an
   unsigned binary then fails with “datapeek cannot be opened because the
   developer cannot be verified.” Remove it from the binary before the first
   run:

   ```bash
   xattr -d com.apple.quarantine datapeek-v0.1.0-aarch64-apple-darwin/datapeek
   ```

   If `xattr` reports `No such xattr: com.apple.quarantine`, the file was not
   quarantined and you can continue. If macOS still blocks it, clear the
   attribute on the whole folder:

   ```bash
   xattr -dr com.apple.quarantine datapeek-v0.1.0-aarch64-apple-darwin
   ```

4. Put the binary on `PATH`. `/usr/local/bin` is already on the default PATH:

   ```bash
   sudo install -m 755 datapeek-v0.1.0-aarch64-apple-darwin/datapeek /usr/local/bin/datapeek
   ```

   Without `sudo`, install into a user directory and add it to `PATH` (zsh is
   the default shell on current macOS):

   ```bash
   mkdir -p "$HOME/.local/bin"
   install -m 755 datapeek-v0.1.0-aarch64-apple-darwin/datapeek "$HOME/.local/bin/datapeek"
   echo 'export PATH="$HOME/.local/bin:$PATH"' >> "$HOME/.zshrc"
   source "$HOME/.zshrc"
   ```

5. Check the install.

   ```bash
   datapeek --version
   ```

### macOS (Intel)

Same steps, with the Intel archive `datapeek-v0.1.0-x86_64-apple-darwin.tar.gz`
and the folder `datapeek-v0.1.0-x86_64-apple-darwin`.

```bash
curl -fL -o datapeek-v0.1.0-x86_64-apple-darwin.tar.gz \
  https://github.com/Jherin89/datapeek/releases/download/v0.1.0/datapeek-v0.1.0-x86_64-apple-darwin.tar.gz
tar -xzf datapeek-v0.1.0-x86_64-apple-darwin.tar.gz
xattr -d com.apple.quarantine datapeek-v0.1.0-x86_64-apple-darwin/datapeek
sudo install -m 755 datapeek-v0.1.0-x86_64-apple-darwin/datapeek /usr/local/bin/datapeek
datapeek --version
```

### Linux (x86_64)

The Linux asset is a gzip-compressed tar. There is no quarantine attribute.

```bash
curl -fL -o datapeek-v0.1.0-x86_64-unknown-linux-gnu.tar.gz \
  https://github.com/Jherin89/datapeek/releases/download/v0.1.0/datapeek-v0.1.0-x86_64-unknown-linux-gnu.tar.gz
tar -xzf datapeek-v0.1.0-x86_64-unknown-linux-gnu.tar.gz
sudo install -m 755 datapeek-v0.1.0-x86_64-unknown-linux-gnu/datapeek /usr/local/bin/datapeek
datapeek --version
```

For a user install instead of `/usr/local/bin`:

```bash
mkdir -p "$HOME/.local/bin"
install -m 755 datapeek-v0.1.0-x86_64-unknown-linux-gnu/datapeek "$HOME/.local/bin/datapeek"
export PATH="$HOME/.local/bin:$PATH"
datapeek --version
```

Add that `export` to your shell profile if you want it in new terminals.
`~/.local/bin` is not always on `PATH` by default.

### Windows (x86_64)

The Windows asset is a real zip (a Deflate archive with a `PK` header).
Extract it with `Expand-Archive`. `unzip` works too. Do not extract it with
`tar`.

1. Download the zip in PowerShell.

   ```powershell
   Invoke-WebRequest `
     -Uri https://github.com/Jherin89/datapeek/releases/download/v0.1.0/datapeek-v0.1.0-x86_64-pc-windows-msvc.zip `
     -OutFile datapeek-v0.1.0-x86_64-pc-windows-msvc.zip
   ```

2. Extract it. This creates `datapeek-v0.1.0-x86_64-pc-windows-msvc\`
   containing `datapeek.exe`, `LICENSE`, and `README.md`.

   ```powershell
   Expand-Archive -Path datapeek-v0.1.0-x86_64-pc-windows-msvc.zip -DestinationPath .
   ```

3. Confirm the binary before changing `PATH`.

   ```powershell
   .\datapeek-v0.1.0-x86_64-pc-windows-msvc\datapeek.exe --version
   ```

4. Add that folder to your user `Path`, then open a new terminal so the
   change is picked up.

   ```powershell
   $dir = (Resolve-Path .\datapeek-v0.1.0-x86_64-pc-windows-msvc).Path
   $userPath = [Environment]::GetEnvironmentVariable('Path', 'User')
   if ($userPath -notlike "*$dir*") {
     [Environment]::SetEnvironmentVariable('Path', "$userPath;$dir", 'User')
   }
   ```

5. In the new terminal:

   ```powershell
   datapeek --version
   ```

### From source with cargo

This needs Rust 1.85 or newer. The repo pins 1.85.1 in `rust-toolchain.toml`,
so a rustup-managed toolchain will download that compiler when you build.

Install Rust from <https://rustup.rs> if you do not already have it.

macOS or Linux:

```bash
curl --proto '=https' --tlsv1.2 -sSf https://sh.rustup.rs | sh
source "$HOME/.cargo/env"
rustc --version
```

Windows: download `rustup-init.exe` from <https://rustup.rs> and run it, or
`winget install Rustlang.Rustup`. Open a new terminal afterward so `cargo` is
on `PATH`.

Then install datapeek from this repository:

```bash
cargo install --git https://github.com/Jherin89/datapeek
datapeek --version
```

`cargo install` puts the binary in `~/.cargo/bin` (or `%USERPROFILE%\.cargo\bin`
on Windows), which rustup adds to `PATH`.

## Usage

### First run

The repo includes small files in [`samples/`](samples/): `orders.csv`,
`events.tsv`, `orders.parquet`, and `metrics.jsonl`.

```bash
datapeek samples/orders.csv
```

On a terminal, the table is colored: types are tinted, missingness turns
yellow at 5% and red at 20%, and sparklines are cyan. The text form of that
profile is:

```
  datapeek  samples/orders.csv
  24 rows × 7 columns

  column      type     miss   min         max         distinct  top values                                  dist
  ──────────  ───────  ─────  ──────────  ──────────  ────────  ──────────────────────────────────────────  ────────
  order_id    integer  0%     1,001       1,024       24        —                                           ████████
  customer    text     0%     Acme        Umbrella    6         Acme 29.2% · Globex 20.8% · Initech 16.7%
  sku         text     0%     GADGET-2    WIDGET-7    6         WIDGET-1 33.3% · WIDGET-2 16.7% · GADGET-…
  amount      decimal  8.3%   4.5         249         7         19.99 33.3% · 249 12.5% · 9.99 12.5%        █▁▃▁▂▁▁▃
  order_date  date     0%     2024-01-15  2024-01-27  13        2024-01-16 8.3% · 2024-01-17 8.3% · 2024-…  ▆▅█▅█▅█▆
  shipped     bool     0%     false       true        2         true 75.0% · false 25.0%
  note        text     75.0%  gift wrap   wait        5         rush 8.3% · gift wrap 8.3% · hold 4.2%
```

The header is the file path and the row × column shape. `--sample` changes
the shape line to `sampled first N rows × … columns`.

The other samples:

```bash
datapeek samples/events.tsv
datapeek samples/orders.parquet
datapeek samples/metrics.jsonl
```

### Reading the table

| Column | Meaning |
| --- | --- |
| **column** | Field name from the header or Parquet/JSON schema. |
| **type** | `integer`, `decimal`, `date`, `bool`, or `text`. |
| **miss** | Percent of rows that are null or empty. `0%` when nothing is missing, `<0.1%` for a tiny share, one decimal otherwise. |
| **min / max** | Range. Text is lexicographic. The table prints thousands separators (`1,001`). `--json` emits numeric bounds as JSON numbers (`1001`). |
| **distinct** | Number of unique values, including a single null bucket when nulls are present. |
| **top values** | Up to three most common values, with each value's share of all rows. `—` when the column is empty or every non-null value is unique and there are more than three of them. Long values are truncated with `…`. |
| **dist** | An 8-bin sparkline for `integer`, `decimal`, and `date` columns. Text and bool columns leave this blank. |

### Flags

```
datapeek [OPTIONS] <FILE>
```

| Flag | What it does |
| --- | --- |
| `<FILE>` | Data file to profile. Required. |
| `--json` | Pretty-printed JSON profile instead of the table. |
| `-c, --columns <COLS>` | Comma-separated column names to keep, in the order you list them. |
| `--sample <N>` | Profile only the first N rows. |
| `-h, --help` | Print help. |
| `-V, --version` | Print `datapeek 0.1.0`. |

**`--json`.** Same profile as the table. Numeric min/max are raw JSON numbers.
`histogram` and `sparkline` are present only for numeric and date columns.
`sample_limit` is present only when you passed `--sample`.

```bash
datapeek --json samples/orders.csv
```

Full pretty-printed output of that command:

<details>
<summary><code>datapeek --json samples/orders.csv</code></summary>

```json
{
  "file": "samples/orders.csv",
  "rows": 24,
  "columns": 7,
  "sampled": false,
  "format": "csv",
  "profile": [
    {
      "name": "order_id",
      "type": "integer",
      "missing_pct": 0.0,
      "min": 1001,
      "max": 1024,
      "distinct": 24,
      "top_values": [],
      "histogram": [
        3,
        3,
        3,
        3,
        3,
        3,
        3,
        3
      ],
      "sparkline": "████████"
    },
    {
      "name": "customer",
      "type": "text",
      "missing_pct": 0.0,
      "min": "Acme",
      "max": "Umbrella",
      "distinct": 6,
      "top_values": [
        {
          "value": "Acme",
          "count": 7,
          "pct": 29.166666666666668
        },
        {
          "value": "Globex",
          "count": 5,
          "pct": 20.833333333333332
        },
        {
          "value": "Initech",
          "count": 4,
          "pct": 16.666666666666668
        }
      ]
    },
    {
      "name": "sku",
      "type": "text",
      "missing_pct": 0.0,
      "min": "GADGET-2",
      "max": "WIDGET-7",
      "distinct": 6,
      "top_values": [
        {
          "value": "WIDGET-1",
          "count": 8,
          "pct": 33.333333333333336
        },
        {
          "value": "WIDGET-2",
          "count": 4,
          "pct": 16.666666666666668
        },
        {
          "value": "GADGET-9",
          "count": 4,
          "pct": 16.666666666666668
        }
      ]
    },
    {
      "name": "amount",
      "type": "decimal",
      "missing_pct": 8.333333333333334,
      "min": 4.5,
      "max": 249.0,
      "distinct": 7,
      "top_values": [
        {
          "value": "19.99",
          "count": 8,
          "pct": 33.333333333333336
        },
        {
          "value": "249",
          "count": 3,
          "pct": 12.5
        },
        {
          "value": "9.99",
          "count": 3,
          "pct": 12.5
        }
      ],
      "histogram": [
        14,
        0,
        3,
        0,
        2,
        0,
        0,
        3
      ],
      "sparkline": "█▁▃▁▂▁▁▃"
    },
    {
      "name": "order_date",
      "type": "date",
      "missing_pct": 0.0,
      "min": "2024-01-15",
      "max": "2024-01-27",
      "distinct": 13,
      "top_values": [
        {
          "value": "2024-01-16",
          "count": 2,
          "pct": 8.333333333333334
        },
        {
          "value": "2024-01-17",
          "count": 2,
          "pct": 8.333333333333334
        },
        {
          "value": "2024-01-18",
          "count": 2,
          "pct": 8.333333333333334
        }
      ],
      "histogram": [
        3,
        2,
        4,
        2,
        4,
        2,
        4,
        3
      ],
      "sparkline": "▆▅█▅█▅█▆"
    },
    {
      "name": "shipped",
      "type": "bool",
      "missing_pct": 0.0,
      "min": "false",
      "max": "true",
      "distinct": 2,
      "top_values": [
        {
          "value": "true",
          "count": 18,
          "pct": 75.0
        },
        {
          "value": "false",
          "count": 6,
          "pct": 25.0
        }
      ]
    },
    {
      "name": "note",
      "type": "text",
      "missing_pct": 75.0,
      "min": "gift wrap",
      "max": "wait",
      "distinct": 5,
      "top_values": [
        {
          "value": "rush",
          "count": 2,
          "pct": 8.333333333333334
        },
        {
          "value": "gift wrap",
          "count": 2,
          "pct": 8.333333333333334
        },
        {
          "value": "hold",
          "count": 1,
          "pct": 4.166666666666667
        }
      ]
    }
  ]
}
```

</details>

`amount` in that file is `"min": 4.5` and `"max": 249.0`. Text, dates, and
bools stay strings (`"min": "Acme"`, `"min": "2024-01-15"`, `"min": "false"`).

**`--columns`.** Names that are not in the file are an error. Order follows
the flag, not the file.

```bash
datapeek --columns customer,amount samples/orders.csv
```

```
  datapeek  samples/orders.csv
  24 rows × 2 columns

  column    type     miss  min   max       distinct  top values                                 dist
  ────────  ───────  ────  ────  ────────  ────────  ─────────────────────────────────────────  ────────
  customer  text     0%    Acme  Umbrella  6         Acme 29.2% · Globex 20.8% · Initech 16.7%
  amount    decimal  8.3%  4.5   249       7         19.99 33.3% · 249 12.5% · 9.99 12.5%       █▁▃▁▂▁▁▃
```

**`--sample`.** Delimited files and Parquet stop after N rows. JSON Lines is
read and then sliced to N rows.

```bash
datapeek --sample 5 samples/orders.csv
```

```
  datapeek  samples/orders.csv
  sampled first 5 rows × 7 columns

  column      type     miss   min         max         distinct  top values                                  dist
  ──────────  ───────  ─────  ──────────  ──────────  ────────  ──────────────────────────────────────────  ────────
  order_id    integer  0%     1,001       1,005       5         —                                           █▁█▁█▁██
  customer    text     0%     Acme        Umbrella    4         Acme 40.0% · Globex 20.0% · Initech 20.0%
  sku         text     0%     GADGET-9    WIDGET-7    4         WIDGET-1 40.0% · WIDGET-2 20.0% · GADGET-…
  amount      decimal  20.0%  9.99        249         4         —                                           █▁▁▁▁▁▁▃
  order_date  date     0%     2024-01-15  2024-01-17  3         2024-01-16 40.0% · 2024-01-17 40.0% · 202…  ▅▁▁▁█▁▁█
  shipped     bool     0%     false       true        2         true 80.0% · false 20.0%
  note        text     60.0%  gift wrap   rush        3         rush 20.0% · gift wrap 20.0%
```

### Formats

| Format | How datapeek recognizes it |
| --- | --- |
| **CSV** | `.csv`. The delimiter is sniffed from the first lines: comma, tab, semicolon, or pipe. The separator with a stable column count wins. |
| **TSV** | `.tsv` or `.tab` is always tab-separated. A `.csv` that is actually tabs is sniffed as TSV. |
| **Parquet** | `.parquet` or `.pq`, or a file whose first bytes are the `PAR1` magic. |
| **JSON Lines** | `.jsonl`, `.ndjson`, or `.json`. A file with no extension is treated as JSON Lines when the first non-empty line starts with `{` or `[`. |
| **Extensionless text** | Sniffed as Parquet, then JSON Lines, then a delimited table. |

Excel workbooks (`.xlsx`, `.xlsm`, `.xls`, `.ods`, `.xlsb`) are rejected.
Dates in delimited files are parsed when they look like `YYYY-MM-DD`,
`MM/DD/YYYY`, `DD/MM/YYYY`, or an ISO datetime. A string column whose sampled
values are all integers, decimals, or booleans is reported as that type.

A file that is not actually the format it claims to be exits `1` with
`datapeek: failed to parse …`. A delimited row with more fields than the
header is a parse error too.

### Exit codes and errors

| Exit | When |
| --- | --- |
| `0` | The profile was written. Closing the pipe early, as in `datapeek file \| head`, is also success. |
| `1` | A datapeek error. The message is on stderr and starts with `datapeek: `. |
| `2` | The arguments are wrong (missing file, or `--sample` is not a number). This comes from the argument parser. |

```text
datapeek: no such file: /tmp/no-such-datapeek.csv
datapeek: is a directory: samples
datapeek: file is empty: /tmp/empty-datapeek.csv
datapeek: unknown column 'nope' (have: order_id, customer, sku, amount, order_date, shipped, note)
datapeek: Excel files are not supported in v1: /tmp/book.xlsx
datapeek: failed to parse '/tmp/bad.parquet': parquet: File out of specification: The file must end with PAR1: 'parquet scan'
```

A missing `<FILE>` exits 2:

```text
error: the following required arguments were not provided:
  <FILE>
```

### Recipes

**Check a CSV before a warehouse load.** Look for columns that are mostly
empty, types you did not expect (an id that arrived as text, a date that
stayed text), and a distinct count of 1 on a column you thought varied.

```bash
datapeek --json samples/orders.csv \
  | jq '[.profile[] | select(.missing_pct > 0) | {name, type, missing_pct}]'
```

```json
[
  {
    "name": "amount",
    "type": "decimal",
    "missing_pct": 8.333333333333334
  },
  {
    "name": "note",
    "type": "text",
    "missing_pct": 75.0
  }
]
```

**Script a profile with jq.** `--json` is pretty-printed. `jq -c` gives one
object per column, which is easy to grep or load elsewhere.

```bash
datapeek --json samples/orders.csv \
  | jq -c '.profile[] | {name, type, missing_pct, min, max}'
```

```json
{"name":"order_id","type":"integer","missing_pct":0.0,"min":1001,"max":1024}
{"name":"customer","type":"text","missing_pct":0.0,"min":"Acme","max":"Umbrella"}
{"name":"sku","type":"text","missing_pct":0.0,"min":"GADGET-2","max":"WIDGET-7"}
{"name":"amount","type":"decimal","missing_pct":8.333333333333334,"min":4.5,"max":249.0}
{"name":"order_date","type":"date","missing_pct":0.0,"min":"2024-01-15","max":"2024-01-27"}
{"name":"shipped","type":"bool","missing_pct":0.0,"min":"false","max":"true"}
{"name":"note","type":"text","missing_pct":75.0,"min":"gift wrap","max":"wait"}
```

**Peek at a large file.** Profile the first 100,000 rows while you decide
whether to scan the rest.

```bash
datapeek --sample 100000 huge.csv
```

**Profile one measure.**

```bash
datapeek --columns amount,order_date samples/orders.csv
```

## Performance

Delimited and Parquet files go through a Polars lazy scan. `--sample` stops
that scan after N rows. A few million rows on a laptop is the design point.

On a Linux x86_64 check, **2,000,000 rows × 7 columns** of CSV profiled in
**0.67 seconds**. `--sample 100000` on the same file was **0.17 seconds**.

## How it works

Polars infers a schema from the file. String columns get an extra pass so
dates and bools are not left as text. Missingness, min/max, and distinct
counts are vectorized aggregations. Top values come from `value_counts`.
Numeric and date histograms are an 8-bin pass used only to draw the sparkline.

## Development

```bash
cargo test
cargo run --release -- samples/orders.csv
cargo run --example gen_parquet    # regenerate samples/orders.parquet
```

CI runs `fmt` and `clippy` on Ubuntu, and `cargo test` on Ubuntu, macOS, and
Windows, for every push and pull request. Pushing a `v*` tag builds release
archives and attaches them to the GitHub Release:

- `datapeek-<tag>-aarch64-apple-darwin.tar.gz`
- `datapeek-<tag>-x86_64-apple-darwin.tar.gz`
- `datapeek-<tag>-x86_64-unknown-linux-gnu.tar.gz`
- `datapeek-<tag>-x86_64-pc-windows-msvc.zip` — a real Deflate zip, built with 7-Zip and checked for a `PK` header. Extract it with `Expand-Archive` (or `unzip`). It contains `datapeek.exe`, `LICENSE`, and `README.md`.

The GIF at the top is `docs/demo.tape`, recorded with
[vhs](https://github.com/charmbracelet/vhs). From the repo root, with `jq`
and a release build on `PATH`:

```bash
export PATH="$PWD/target/release:$PATH"
vhs docs/demo.tape
```

## License

MIT © Jordan Herin
