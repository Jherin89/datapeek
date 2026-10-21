//! One-shot helper: `cargo run --example gen_xlsx`
//!
//! Writes `samples/orders.xlsx` from `samples/orders.csv`, plus a `Returns`
//! sheet and a `Types` sheet so `--sheet` and Excel dtypes can be checked.
//! The generator is deterministic: the same CSV produces the same workbook.

use std::fs;
use std::path::PathBuf;

use rust_xlsxwriter::{ExcelDateTime, Format, Workbook, Worksheet, XlsxError};

fn main() -> Result<(), XlsxError> {
    let root = PathBuf::from(env!("CARGO_MANIFEST_DIR"));
    let csv_path = root.join("samples/orders.csv");
    let out = root.join("samples/orders.xlsx");
    let csv = fs::read_to_string(&csv_path).unwrap_or_else(|err| {
        panic!("read {}: {err}", csv_path.display());
    });

    let date_fmt = Format::new().set_num_format("yyyy-mm-dd");
    let datetime_fmt = Format::new().set_num_format("yyyy-mm-dd hh:mm:ss");

    let mut workbook = Workbook::new();
    write_orders(workbook.add_worksheet(), &csv, &date_fmt)?;
    write_returns(workbook.add_worksheet(), &datetime_fmt)?;
    write_types(workbook.add_worksheet(), &date_fmt, &datetime_fmt)?;
    workbook.save(&out)?;
    println!("wrote {}", out.display());
    Ok(())
}

fn write_orders(sheet: &mut Worksheet, csv: &str, date_fmt: &Format) -> Result<(), XlsxError> {
    sheet.set_name("Orders")?;
    for (row_idx, line) in csv.lines().enumerate() {
        let line = line.trim_end_matches('\r');
        if line.is_empty() {
            continue;
        }
        let fields: Vec<&str> = line.split(',').collect();
        let row = row_idx as u32;
        if row == 0 {
            for (col, field) in fields.iter().enumerate() {
                sheet.write_string(row, col as u16, *field)?;
            }
            continue;
        }
        // order_id, customer, sku, amount, order_date, shipped, note
        write_number_str(sheet, row, 0, fields.first().copied().unwrap_or(""))?;
        sheet.write_string(row, 1, fields.get(1).copied().unwrap_or(""))?;
        sheet.write_string(row, 2, fields.get(2).copied().unwrap_or(""))?;
        let amount = fields.get(3).copied().unwrap_or("");
        if !amount.is_empty() {
            write_number_str(sheet, row, 3, amount)?;
        }
        let date = fields.get(4).copied().unwrap_or("");
        if !date.is_empty() {
            write_date(sheet, row, 4, date, date_fmt)?;
        }
        let shipped = fields.get(5).copied().unwrap_or("");
        if !shipped.is_empty() {
            sheet.write_boolean(row, 5, shipped.eq_ignore_ascii_case("true"))?;
        }
        let note = fields.get(6).copied().unwrap_or("");
        if !note.is_empty() {
            sheet.write_string(row, 6, note)?;
        }
    }
    Ok(())
}

fn write_returns(sheet: &mut Worksheet, datetime_fmt: &Format) -> Result<(), XlsxError> {
    sheet.set_name("Returns")?;
    for (col, name) in ["return_id", "order_id", "reason", "refund", "received_at"]
        .iter()
        .enumerate()
    {
        sheet.write_string(0, col as u16, *name)?;
    }
    let rows = [
        (
            1_f64,
            1002_f64,
            "damaged",
            Some(249_f64),
            (2024, 2, 1, 9, 30, 0),
        ),
        (2_f64, 1011_f64, "wrong item", None, (2024, 2, 2, 15, 4, 0)),
        (3_f64, 1015_f64, "late", Some(10_f64), (2024, 2, 3, 0, 0, 0)),
    ];
    for (idx, (ret, order, reason, refund, (y, m, d, hh, mm, ss))) in rows.iter().enumerate() {
        let row = (idx + 1) as u32;
        sheet.write_number(row, 0, *ret)?;
        sheet.write_number(row, 1, *order)?;
        sheet.write_string(row, 2, *reason)?;
        if let Some(refund) = refund {
            sheet.write_number(row, 3, *refund)?;
        }
        let stamp = excel_stamp(*y, *m, *d, *hh, *mm, *ss)?;
        sheet.write_datetime_with_format(row, 4, &stamp, datetime_fmt)?;
    }
    Ok(())
}

fn write_types(
    sheet: &mut Worksheet,
    date_fmt: &Format,
    datetime_fmt: &Format,
) -> Result<(), XlsxError> {
    sheet.set_name("Types")?;
    for (col, name) in [
        "qty", "price", "flag", "day", "at", "label", "mixed", "empty",
    ]
    .iter()
    .enumerate()
    {
        sheet.write_string(0, col as u16, *name)?;
    }

    sheet.write_number(1, 0, 1)?;
    sheet.write_number(2, 0, 2)?;
    // row 3 qty left empty

    sheet.write_number(1, 1, 1.5)?;
    sheet.write_number(2, 1, 2.25)?;
    sheet.write_number(3, 1, 3)?;

    sheet.write_boolean(1, 2, true)?;
    sheet.write_boolean(2, 2, false)?;
    sheet.write_boolean(3, 2, true)?;

    for (row, (y, m, d)) in [(1, (2024, 1, 15)), (2, (2024, 1, 16)), (3, (2024, 1, 17))] {
        write_ymd(sheet, row, 3, y, m, d, date_fmt)?;
    }

    let stamps = [
        (1, 2024, 1, 15, 13, 45, 0),
        (2, 2024, 1, 16, 8, 0, 0),
        (3, 2024, 1, 17, 0, 0, 0),
    ];
    for (row, y, m, d, hh, mm, ss) in stamps {
        let stamp = excel_stamp(y, m, d, hh, mm, ss)?;
        sheet.write_datetime_with_format(row, 4, &stamp, datetime_fmt)?;
    }

    sheet.write_string(1, 5, "alpha")?;
    sheet.write_string(2, 5, "beta")?;
    sheet.write_string(3, 5, "gamma")?;

    sheet.write_number(1, 6, 1)?;
    sheet.write_string(2, 6, "foo")?;
    sheet.write_number(3, 6, 3.5)?;
    // empty column stays blank
    Ok(())
}

fn excel_stamp(
    year: u16,
    month: u8,
    day: u8,
    hour: u16,
    min: u8,
    sec: u8,
) -> Result<ExcelDateTime, XlsxError> {
    ExcelDateTime::from_ymd(year, month, day)?.and_hms(hour, min, sec)
}

fn write_number_str(
    sheet: &mut Worksheet,
    row: u32,
    col: u16,
    text: &str,
) -> Result<(), XlsxError> {
    let value: f64 = text.parse().unwrap_or_else(|_| panic!("number '{text}'"));
    sheet.write_number(row, col, value)?;
    Ok(())
}

fn write_date(
    sheet: &mut Worksheet,
    row: u32,
    col: u16,
    text: &str,
    fmt: &Format,
) -> Result<(), XlsxError> {
    let mut parts = text.split('-');
    let year: u16 = parts.next().unwrap().parse().expect("year");
    let month: u8 = parts.next().unwrap().parse().expect("month");
    let day: u8 = parts.next().unwrap().parse().expect("day");
    write_ymd(sheet, row, col, year, month, day, fmt)
}

fn write_ymd(
    sheet: &mut Worksheet,
    row: u32,
    col: u16,
    year: u16,
    month: u8,
    day: u8,
    fmt: &Format,
) -> Result<(), XlsxError> {
    let date = ExcelDateTime::from_ymd(year, month, day)?;
    sheet.write_datetime_with_format(row, col, &date, fmt)?;
    Ok(())
}
