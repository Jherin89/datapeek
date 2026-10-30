# Excel regression fixtures

These are real workbook files, copied unchanged from
[calamine](https://github.com/tafia/calamine/tree/f2b5654205082aab8552679110e345d5093fe6d2/tests)
at commit `f2b5654205082aab8552679110e345d5093fe6d2`.
The upstream MIT copyright and permission notice is in [LICENSE-MIT.md](LICENSE-MIT.md).

| File | Content used by datapeek's CLI tests | SHA-256 |
| --- | --- | --- |
| `date.xls` | BIFF workbook: date and duration cells, with integers 15–17 | `e3eb3391a371cc1e0b85d94b9f1dd94bbcedf19733ed0ca449e2d269af758e9e` |
| `date.xlsb` | Binary workbook with the same cells; duration precision differs | `68d36adf4d4de3890209e786eeb89a98778d25158628f83e96587e652fbb4eca` |
| `issue221.xlsm` | Macro-enabled workbook format, two rows of text in `Sheet1`; no VBA project | `4757c73be0ad3e0cb789dcf6c170df7ddf10160618986af41614278dc8ce6498` |

The date workbooks have no dedicated header row. Datapeek uses their first row
as headers (`2021-01-01` and `15`), leaving two data rows. A full profile mixes a
date and a duration in the first column, so that column becomes text. `--head 1`
profiles just the date and keeps it typed as a date.

Tests read these local files and need no network access, Excel installation, or
macro execution. To refresh a fixture, download its original name from the
pinned source above, verify its SHA-256, and run `cargo test --test excel_formats`.
Do not replace these with renamed `.xlsx` files: that would stop exercising the
BIFF and XLSB readers.
