# datapeek

**Instant data-file profiler.** Point it at a CSV, TSV, or Parquet file and get a
clean column-by-column readout in the time it takes to blink.

I wrote this because I was tired of spinning up a notebook just to answer
*what is in this file?* — types, missingness, ranges, the shape of a numeric
column. `datapeek` is a small Rust CLI for that moment, built to feel at home
on a MacBook terminal.

```bash
datapeek orders.csv
```

![datapeek screenshot](docs/screenshot.png)

> *Placeholder — drop a terminal screenshot or GIF at `docs/screenshot.png`.*

## Install

**From git** (any platform with a Rust toolchain):

```bash
cargo install --git https://github.com/Jherin89/datapeek
```

**Prebuilt macOS Apple Silicon binary** from
[GitHub Releases](https://github.com/Jherin89/datapeek/releases):

```bash
tar -xzf datapeek-aarch64-apple-darwin.tar.gz
install -m 755 datapeek-aarch64-apple-darwin/datapeek /usr/local/bin/datapeek
```

Building from source needs Rust 1.85 or newer.

## Usage

```bash
datapeek orders.csv
datapeek events.tsv
datapeek warehouse/orders.parquet
datapeek --json orders.csv                 # machine-readable
datapeek --columns customer,amount orders.csv
datapeek --sample 100000 huge.csv          # first N rows
```

Exit status is `1` with a clear `datapeek: …` message on missing files,
unreadable paths, unknown columns, or parse failures.

### Example

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

One row per column:

| Field | What you get |
| --- | --- |
| **type** | `integer`, `decimal`, `date`, `bool`, `text` |
| **miss** | percent of rows that are empty / null |
| **min / max** | range (lexicographic for text) |
| **distinct** | number of unique values |
| **top values** | up to three most common values with % share of rows |
| **dist** | 8-bin unicode sparkline on numeric and date columns |

The header is the row × column shape. `--sample N` notes that you only profiled
the first N rows. `--json` emits the same profile as pretty JSON.

Sample files live in [`samples/`](samples/): `orders.csv`, `events.tsv`,
`orders.parquet`, and `metrics.jsonl`.

## Formats

| Format | Notes |
| --- | --- |
| **CSV** | Delimiter auto-detected (`,`, tab, `;`, `\|`) |
| **TSV** | `.tsv` / `.tab`, or a `.csv` that is actually tab-separated |
| **Parquet** | Fast path via a Polars scan |
| **JSON Lines** | `.jsonl` / `.ndjson` / `.json` |

## Performance

Built on [Polars](https://pola.rs). Delimited and Parquet files stream through
a lazy scan; `--sample` stops after N rows. A few million rows on a laptop is
the design point.

On a Linux x86_64 check, **2,000,000 rows × 7 columns** of CSV profiled in
**0.67 seconds**. `--sample 100000` on the same file was **0.17 seconds**.

## How it works

Polars infers a schema from the file (with extra sniffing for string columns
that are really dates or bools). Missingness, min/max, and distinct counts are
vectorized aggregations. Top values use `value_counts`. Numeric histograms are
an 8-bin pass used only to draw the sparkline.

## Development

```bash
cargo test
cargo run --release -- samples/orders.csv
cargo run --example gen_parquet    # regenerate samples/orders.parquet
```

CI runs `fmt`, `clippy`, and `test` on every push and PR. Pushing a `v*` tag
builds an `aarch64-apple-darwin` binary on `macos-14` and attaches it to the
GitHub Release.

## License

MIT © Jordan Herin
