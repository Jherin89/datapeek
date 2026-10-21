use std::io::IsTerminal;

use colored::{ColoredString, Colorize};

use crate::load::ReadMode;
use crate::profile::{fmt_uint, ColumnProfile, FileProfile, InferredType, StatValue};

/// Render a colored terminal table, one row per column.
pub fn render_table(profile: &FileProfile) -> String {
    let use_color = std::io::stdout().is_terminal();
    if !use_color {
        colored::control::set_override(false);
    }

    let mut lines = Vec::new();
    lines.push(header(profile));
    lines.push(String::new());

    let rows: Vec<DisplayRow> = profile.profile.iter().map(DisplayRow::from_col).collect();

    let headers = [
        "column",
        "type",
        "miss",
        "min",
        "max",
        "distinct",
        "top values",
        "dist",
    ];
    let mut widths = headers.map(|h| h.chars().count());
    for row in &rows {
        widths[0] = widths[0].max(row.name.chars().count());
        widths[1] = widths[1].max(row.dtype.chars().count());
        widths[2] = widths[2].max(row.miss.chars().count());
        widths[3] = widths[3].max(row.min.chars().count());
        widths[4] = widths[4].max(row.max.chars().count());
        widths[5] = widths[5].max(row.distinct.chars().count());
        widths[6] = widths[6].max(row.top.chars().count());
        widths[7] = widths[7].max(row.hist.chars().count());
    }
    // Keep the table readable on a laptop screen.
    widths[0] = widths[0].min(28);
    widths[3] = widths[3].min(19);
    widths[4] = widths[4].min(19);
    widths[6] = widths[6].min(42);

    let header_line = format!(
        "  {name}  {dtype}  {miss}  {min}  {max}  {distinct}  {top}  {hist}",
        name = pad("column", widths[0]),
        dtype = pad("type", widths[1]),
        miss = pad("miss", widths[2]),
        min = pad("min", widths[3]),
        max = pad("max", widths[4]),
        distinct = pad("distinct", widths[5]),
        top = pad("top values", widths[6]),
        hist = pad("dist", widths[7]),
    );
    lines.push(header_line.dimmed().to_string());

    let rule = format!(
        "  {name}  {dtype}  {miss}  {min}  {max}  {distinct}  {top}  {hist}",
        name = rule(widths[0]),
        dtype = rule(widths[1]),
        miss = rule(widths[2]),
        min = rule(widths[3]),
        max = rule(widths[4]),
        distinct = rule(widths[5]),
        top = rule(widths[6]),
        hist = rule(widths[7].max(1)),
    );
    lines.push(rule.dimmed().to_string());

    for row in &rows {
        let line = format!(
            "  {name}  {dtype}  {miss}  {min}  {max}  {distinct}  {top}  {hist}",
            name = pad(&row.name, widths[0]).bold(),
            dtype = color_type(&pad(&row.dtype, widths[1]), row.dtype_kind),
            miss = color_miss(&pad(&row.miss, widths[2]), row.missing_pct),
            min = pad(&row.min, widths[3]).dimmed(),
            max = pad(&row.max, widths[4]).dimmed(),
            distinct = pad(&row.distinct, widths[5]),
            top = pad(&row.top, widths[6]),
            hist = pad(&row.hist, widths[7]).cyan(),
        );
        lines.push(line);
    }

    lines.push(String::new());
    lines.join("\n")
}

struct DisplayRow {
    name: String,
    dtype: String,
    dtype_kind: InferredType,
    miss: String,
    missing_pct: f64,
    min: String,
    max: String,
    distinct: String,
    top: String,
    hist: String,
}

impl DisplayRow {
    fn from_col(col: &ColumnProfile) -> Self {
        let top = if col.top_values.is_empty() {
            "—".to_string()
        } else {
            col.top_values
                .iter()
                .map(|t| format!("{} {}", t.value, fmt_pct(t.pct)))
                .collect::<Vec<_>>()
                .join(" · ")
        };
        Self {
            name: col.name.clone(),
            dtype: col.dtype.as_str().to_string(),
            dtype_kind: col.dtype,
            miss: fmt_pct(col.missing_pct),
            missing_pct: col.missing_pct,
            min: col
                .min
                .as_ref()
                .map(StatValue::to_display)
                .unwrap_or_else(|| "—".into()),
            max: col
                .max
                .as_ref()
                .map(StatValue::to_display)
                .unwrap_or_else(|| "—".into()),
            distinct: fmt_uint(col.distinct as u64),
            top,
            hist: col.sparkline.clone().unwrap_or_default(),
        }
    }
}

fn header(profile: &FileProfile) -> String {
    let file = profile.file.as_str().bold();
    format!(
        "  {}  {}\n  {}",
        "datapeek".bold().cyan(),
        file,
        shape_line(profile).dimmed()
    )
}

fn shape_line(profile: &FileProfile) -> String {
    let rows = fmt_uint(profile.rows as u64);
    let cols = fmt_uint(profile.columns as u64);
    let body = match profile.mode {
        ReadMode::Full => format!("{rows} rows × {cols} columns"),
        ReadMode::Head => format!("first {rows} rows × {cols} columns"),
        ReadMode::Random => match profile.seed {
            Some(seed) => format!("random sample of {rows} rows (seed {seed}) × {cols} columns"),
            None => format!("random sample of {rows} rows × {cols} columns"),
        },
    };
    match &profile.sheet {
        Some(sheet) => format!("sheet {sheet} · {body}"),
        None => body,
    }
}

fn pad(s: &str, width: usize) -> String {
    let n = s.chars().count();
    if n > width {
        let take = width.saturating_sub(1);
        let mut out: String = s.chars().take(take).collect();
        out.push('…');
        out
    } else {
        let mut out = s.to_string();
        out.push_str(&" ".repeat(width - n));
        out
    }
}

fn rule(width: usize) -> String {
    "─".repeat(width)
}

fn color_type(s: &str, kind: InferredType) -> ColoredString {
    match kind {
        InferredType::Integer => s.cyan(),
        InferredType::Decimal => s.blue(),
        InferredType::Date => s.magenta(),
        InferredType::Bool => s.green(),
        InferredType::Text => s.yellow(),
    }
}

fn color_miss(s: &str, pct: f64) -> ColoredString {
    if pct >= 20.0 {
        s.red()
    } else if pct >= 5.0 {
        s.yellow()
    } else {
        s.dimmed()
    }
}

pub fn fmt_pct(p: f64) -> String {
    if p <= 0.0 {
        "0%".into()
    } else if p < 0.1 {
        "<0.1%".into()
    } else if p >= 99.95 {
        "100%".into()
    } else {
        format!("{p:.1}%")
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::load::ReadMode;

    fn bare(mode: ReadMode, seed: Option<u64>, sheet: Option<String>, rows: usize) -> FileProfile {
        FileProfile {
            file: "t.csv".into(),
            rows,
            columns: 7,
            sampled: mode != ReadMode::Full,
            sample_limit: if mode == ReadMode::Full {
                None
            } else {
                Some(rows)
            },
            mode,
            seed,
            format: "csv".into(),
            sheet,
            profile: Vec::new(),
        }
    }

    #[test]
    fn shape_lines_name_the_read_mode() {
        let full = render_table(&bare(ReadMode::Full, None, None, 24));
        assert!(full.contains("24 rows × 7 columns"), "{full}");
        let head = render_table(&bare(ReadMode::Head, None, None, 100));
        assert!(head.contains("first 100 rows × 7 columns"), "{head}");
        let random = render_table(&bare(ReadMode::Random, Some(1), None, 100));
        assert!(
            random.contains("random sample of 100 rows (seed 1) × 7 columns"),
            "{random}"
        );
        let unseeded = render_table(&bare(ReadMode::Random, None, None, 100));
        assert!(
            unseeded.contains("random sample of 100 rows × 7 columns"),
            "{unseeded}"
        );
        assert!(!unseeded.contains("seed"), "{unseeded}");
        let sheet = render_table(&bare(ReadMode::Full, None, Some("Orders".into()), 24));
        assert!(
            sheet.contains("sheet Orders · 24 rows × 7 columns"),
            "{sheet}"
        );
    }
}
