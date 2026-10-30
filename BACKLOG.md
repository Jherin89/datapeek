# Datapeek backlog

Cycle: 2026-10-29. Main: `ebef934`. No open issues or PRs at triage;
latest main CI passed on Ubuntu, macOS, and Windows.

Scores are 1–5: higher value helps more users; higher effort and risk cost more.
Priority = value × 2 − effort − risk. Ties favor the owner's listed gap order.
One improvement per PR; revisit bugs, CI failures, and Jordan's feedback first.

| Rank | Item | Source | Value | Effort | Risk | Priority | Status / first slice |
| --- | --- | --- | --- | --- | --- | --- | --- |
| 1 | Exercise real `.xls`, `.xlsm`, and `.xlsb` files | Known gap | 4 | 1 | 1 | 6 | This PR: local fixtures and CLI regression tests |
| 2 | Bound `--head` lookahead work | Known gap | 3 | 2 | 2 | 2 | Investigate record parsing versus byte lookahead; preserve exact-EOF `mode` semantics |
| 3 | Reduce Parquet sampling row-group work | Known gap | 4 | 4 | 3 | 1 | Benchmark one large row group and investigate existing reader projection/slicing |
| 4 | Support `.ods` | Known gap | 3 | 3 | 3 | 0 | Propose CLI/JSON contract to Jordan before implementing |
| 5 | Signed binaries | Known gap | 4 | 5 | 4 | -1 | Needs Jordan's publishing/signing approval; investigate requirements only |
| 6 | Publish on crates.io | Known gap | 3 | 3 | 4 | -1 | Needs Jordan's publishing approval |

The current fixture slice covers basic values, dates, totals, selection, seeded
sampling, and unknown sheets. It does not establish complete format coverage;
future slices can add error cells, missing cells, and multi-sheet workbooks.
