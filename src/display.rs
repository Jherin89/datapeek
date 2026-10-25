use std::io::IsTerminal;

use colored::{ColoredString, Colorize};

use crate::load::ReadMode;
use crate::profile::{fmt_uint, ColumnProfile, FileProfile, InferredType, StatValue};

/// Render a colored terminal table, one row per column.
pub fn render_table(profile: &FileProfile) -> String {
    render_table_with_width(profile, terminal_width())
}

fn render_table_with_width(profile: &FileProfile, width: usize) -> String {
    // Piped output stays plain. On a terminal, `colored` still honors NO_COLOR.
    if !std::io::stdout().is_terminal() {
        colored::control::set_override(false);
    }

    let mut lines = Vec::new();
    lines.push(header(profile, width));
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

#[derive(Clone, Copy, PartialEq, Eq)]
enum StatKind {
    Rows,
    Columns,
    Cells,
    Filled,
}

struct Card {
    kind: StatKind,
    label: &'static str,
    value: String,
    detail: String,
}

fn terminal_width() -> usize {
    if std::io::stdout().is_terminal() {
        if let Some((terminal_size::Width(width), _)) = terminal_size::terminal_size() {
            if width >= 1 {
                return usize::from(width);
            }
        }
    }
    if let Ok(value) = std::env::var("COLUMNS") {
        if let Ok(width) = value.parse::<usize>() {
            if width >= 1 {
                return width;
            }
        }
    }
    80
}

/// Scope of a partial read. Empty for a full file.
fn mode_phrase(profile: &FileProfile) -> String {
    let rows = fmt_uint(profile.rows as u64);
    match profile.mode {
        ReadMode::Full => String::new(),
        ReadMode::Head => match profile.file_rows {
            Some(total) => format!("first {rows} of {} rows", fmt_uint(total as u64)),
            None => format!("first {rows} rows"),
        },
        ReadMode::Random => {
            let mut text = match profile.seed {
                Some(seed) => format!("random sample of {rows} rows (seed {seed})"),
                None => format!("random sample of {rows} rows"),
            };
            if let Some(total) = profile.file_rows {
                text.push_str(" · of ");
                text.push_str(&fmt_uint(total as u64));
                text.push_str(" rows");
            }
            text
        }
    }
}

fn stat_cards(profile: &FileProfile) -> [Card; 4] {
    let rows_detail = match profile.mode {
        ReadMode::Full => String::new(),
        ReadMode::Head | ReadMode::Random => match profile.file_rows {
            Some(total) => format!("of {}", fmt_uint(total as u64)),
            None => String::new(),
        },
    };
    let rows_label = match profile.mode {
        ReadMode::Full => "Rows",
        ReadMode::Head => "First rows",
        ReadMode::Random => "Sampled rows",
    };
    [
        Card {
            kind: StatKind::Rows,
            label: rows_label,
            value: fmt_uint(profile.rows as u64),
            detail: rows_detail,
        },
        Card {
            kind: StatKind::Columns,
            label: "Columns",
            value: fmt_uint(profile.columns as u64),
            detail: String::new(),
        },
        Card {
            kind: StatKind::Cells,
            label: "Cells",
            value: fmt_uint(profile.cells),
            detail: String::new(),
        },
        Card {
            kind: StatKind::Filled,
            label: "Filled",
            value: fmt_uint(profile.filled_cells),
            detail: fmt_pct(profile.filled_pct),
        },
    ]
}

fn card_inner(card: &Card) -> usize {
    let label = card.label.chars().count() + 4;
    let value = card.value.chars().count() + 2;
    let detail = card.detail.chars().count() + 2;
    label.max(value).max(detail).max(12)
}

fn cards_line_width(inner: usize) -> usize {
    // Two spaces of indent, four boxes, three single-space gaps.
    2 + 4 * (inner + 2) + 3
}

fn header(profile: &FileProfile, width: usize) -> String {
    let cards = stat_cards(profile);
    let inner = cards.iter().map(card_inner).max().unwrap_or(12);
    let phrase = mode_phrase(profile);
    let scope = if phrase.is_empty() {
        String::new()
    } else {
        format!("Profiled · {phrase}")
    };
    let scope_width = if scope.is_empty() {
        0
    } else {
        2 + scope.chars().count()
    };
    let wide = cards_line_width(inner).max(scope_width) <= width;
    let panel = if wide {
        wide_panel(&cards, inner, &scope, profile)
    } else {
        narrow_panel(&cards, &scope, profile)
    };

    let mut lines = vec![panel];
    lines.push(String::new());
    let file = profile.file.as_str().bold();
    lines.push(format!("  {}  {file}", "datapeek".bold().cyan()));
    let info = info_line(profile, &phrase);
    if !info.is_empty() {
        lines.push(format!("  {}", info.dimmed()));
    }
    lines.join("\n")
}

fn info_line(profile: &FileProfile, phrase: &str) -> String {
    let mut parts = Vec::new();
    if let Some(sheet) = &profile.sheet {
        parts.push(format!("sheet {sheet}"));
    }
    if !phrase.is_empty() {
        parts.push(phrase.to_string());
    }
    parts.join(" · ")
}

fn wide_panel(cards: &[Card; 4], inner: usize, scope: &str, profile: &FileProfile) -> String {
    let mut rows: [String; 4] = [String::new(), String::new(), String::new(), String::new()];
    for (index, card) in cards.iter().enumerate() {
        if index > 0 {
            for row in &mut rows {
                row.push(' ');
            }
        }
        let built = wide_card(card, inner, profile);
        for (row, part) in rows.iter_mut().zip(built) {
            row.push_str(&part);
        }
    }
    let mut lines = Vec::new();
    if !scope.is_empty() {
        lines.push(format!("  {}", scope.bold().cyan()));
    }
    for row in rows {
        lines.push(format!("  {row}"));
    }
    lines.join("\n")
}

fn wide_card(card: &Card, inner: usize, profile: &FileProfile) -> [String; 4] {
    let label_n = card.label.chars().count();
    let dashes = inner.saturating_sub(label_n + 3);
    let top = format!(
        "{}{}{}{}{}",
        "┌─ ".dimmed(),
        card.label.bold(),
        " ".dimmed(),
        "─".repeat(dashes).dimmed(),
        "┐".dimmed(),
    );
    let value = color_stat(&center(&card.value, inner), card.kind, profile);
    let middle = format!("{}{value}{}", "│".dimmed(), "│".dimmed());
    let detail = color_detail(&center(&card.detail, inner), card.kind, profile);
    let detail_line = format!("{}{detail}{}", "│".dimmed(), "│".dimmed());
    let bottom = format!(
        "{}{}{}",
        "└".dimmed(),
        "─".repeat(inner).dimmed(),
        "┘".dimmed()
    );
    [top, middle, detail_line, bottom]
}

fn narrow_panel(cards: &[Card; 4], scope: &str, profile: &FileProfile) -> String {
    let label_w = cards
        .iter()
        .map(|card| card.label.chars().count())
        .max()
        .unwrap_or(0);
    let mut lines = Vec::new();
    if !scope.is_empty() {
        lines.push(format!("  {}", scope.bold().cyan()));
    }
    for card in cards {
        let label = pad(card.label, label_w);
        let value = color_stat(&card.value, card.kind, profile);
        let line = if card.detail.is_empty() {
            format!("  {label}  {value}")
        } else if card.detail.ends_with('%') {
            let pct = color_detail(&card.detail, card.kind, profile);
            format!("  {label}  {value} ({pct})")
        } else {
            let detail = card.detail.dimmed();
            format!("  {label}  {value}  {detail}")
        };
        lines.push(line);
    }
    lines.join("\n")
}

fn color_stat(text: &str, kind: StatKind, profile: &FileProfile) -> ColoredString {
    match kind {
        StatKind::Rows | StatKind::Columns => text.bold().cyan(),
        StatKind::Cells => text.bold(),
        StatKind::Filled => {
            if profile.cells == 0 {
                text.bold().dimmed()
            } else {
                text.bold().color(fill_color(profile.filled_pct))
            }
        }
    }
}

fn color_detail(text: &str, kind: StatKind, profile: &FileProfile) -> ColoredString {
    if kind == StatKind::Filled && profile.cells > 0 && text.contains('%') {
        text.color(fill_color(profile.filled_pct))
    } else {
        text.dimmed()
    }
}

fn fill_color(pct: f64) -> colored::Color {
    // Mirror the miss scale: under 5% missing is green, under 20% is yellow.
    if pct >= 95.0 {
        colored::Color::Green
    } else if pct >= 80.0 {
        colored::Color::Yellow
    } else {
        colored::Color::Red
    }
}

fn center(text: &str, width: usize) -> String {
    let n = text.chars().count();
    if n >= width {
        return text.chars().take(width).collect();
    }
    let left = (width - n) / 2;
    let right = width - n - left;
    format!("{}{text}{}", " ".repeat(left), " ".repeat(right))
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
        profile_at(mode, seed, sheet, rows, None, rows as u64 * 7)
    }

    fn profile_at(
        mode: ReadMode,
        seed: Option<u64>,
        sheet: Option<String>,
        rows: usize,
        file_rows: Option<usize>,
        filled: u64,
    ) -> FileProfile {
        let cells = (rows as u64).saturating_mul(7);
        let filled_pct = if cells == 0 {
            0.0
        } else {
            100.0 * filled as f64 / cells as f64
        };
        FileProfile {
            file: "t.csv".into(),
            rows,
            columns: 7,
            cells,
            filled_cells: filled,
            filled_pct,
            file_rows,
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
            warnings: Vec::new(),
        }
    }

    fn plain(profile: &FileProfile, width: usize) -> String {
        render_table_with_width(profile, width)
    }

    #[test]
    fn panel_names_rows_columns_cells_and_filled() {
        let text = plain(&bare(ReadMode::Full, None, None, 24), 100);
        assert!(text.contains("Rows"), "{text}");
        assert!(text.contains("Columns"), "{text}");
        assert!(text.contains("Cells"), "{text}");
        assert!(text.contains("Filled"), "{text}");
        assert!(text.contains("168"), "{text}");
        assert!(text.contains("100%"), "{text}");
        assert!(text.contains('┌'), "{text}");
        let rows_at = text.find("Rows").unwrap();
        let file_at = text.find("t.csv").unwrap();
        let table_at = text.find("column").unwrap();
        assert!(rows_at < file_at && file_at < table_at, "{text}");
        for line in text.lines() {
            if line.contains('┌') || line.contains('│') || line.contains('└') {
                assert!(
                    line.chars().count() <= 100,
                    "{} chars: {line}",
                    line.chars().count()
                );
            }
        }
        assert!(!text.contains('\u{1b}'), "{text}");
    }

    #[test]
    fn narrow_terminals_drop_the_box() {
        let text = plain(&bare(ReadMode::Full, None, None, 24), 36);
        assert!(!text.contains('┌'), "{text}");
        assert!(!text.contains('│'), "{text}");
        assert!(!text.contains('└'), "{text}");
        assert!(text.contains("Rows"), "{text}");
        assert!(text.contains("24"), "{text}");
        assert!(text.contains("168"), "{text}");
        assert!(text.contains("100%"), "{text}");
        assert!(!text.contains('\u{1b}'), "{text}");
    }

    #[test]
    fn panel_groups_thousands_and_partial_reads() {
        let wide = profile_at(
            ReadMode::Full,
            None,
            None,
            2_000_000,
            Some(2_000_000),
            14_000_000,
        );
        let text = plain(&wide, 120);
        assert!(text.contains("2,000,000"), "{text}");
        assert!(text.contains("14,000,000"), "{text}");

        let head = profile_at(ReadMode::Head, None, None, 100, None, 700);
        let text = plain(&head, 120);
        assert!(text.contains("Profiled · first 100 rows"), "{text}");
        assert!(text.contains("First rows"), "{text}");
        assert!(!text.contains("of "), "{text}");

        let head_known = profile_at(
            ReadMode::Head,
            None,
            None,
            100_000,
            Some(2_000_000),
            690_000,
        );
        let text = plain(&head_known, 120);
        assert!(text.contains("first 100,000 of 2,000,000 rows"), "{text}");
        assert!(text.contains("of 2,000,000"), "{text}");

        let random = profile_at(ReadMode::Random, Some(1), None, 100, Some(2_000), 690);
        let text = plain(&random, 140);
        assert!(
            text.contains("random sample of 100 rows (seed 1)"),
            "{text}"
        );
        assert!(text.contains("of 2,000"), "{text}");
        assert!(text.contains("Sampled rows"), "{text}");

        let unseeded = profile_at(ReadMode::Random, None, None, 100, Some(2_000), 690);
        let text = plain(&unseeded, 140);
        assert!(text.contains("random sample of 100 rows"), "{text}");
        assert!(!text.contains("seed"), "{text}");

        let sheet = profile_at(
            ReadMode::Full,
            None,
            Some("Orders".into()),
            24,
            Some(24),
            160,
        );
        let text = plain(&sheet, 100);
        assert!(text.contains("sheet Orders"), "{text}");
        let rows_at = text.find("Rows").unwrap();
        assert!(rows_at < text.find("sheet Orders").unwrap(), "{text}");
    }

    #[test]
    fn narrow_partial_read_stays_plain() {
        let head = profile_at(
            ReadMode::Head,
            None,
            None,
            100_000,
            Some(2_000_000),
            690_000,
        );
        let text = plain(&head, 40);
        assert!(!text.contains('┌'), "{text}");
        assert!(text.contains("first 100,000 of 2,000,000 rows"), "{text}");
        assert!(text.contains("690,000"), "{text}");
        assert!(text.contains("of 2,000,000"), "{text}");
    }

    #[test]
    fn empty_profile_shows_zero_without_a_nan() {
        let empty = profile_at(ReadMode::Full, None, None, 0, Some(0), 0);
        let text = plain(&empty, 100);
        assert!(text.contains("0%"), "{text}");
        assert!(!text.to_ascii_lowercase().contains("nan"), "{text}");
    }
}
