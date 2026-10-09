# 営業KPI カード内訳 テストログ (2026-10-02)

対象: ブランチ `feat/sales-kpi-card-breakdown-v2`、HEAD 8ec3d1c(修正後)。修正前は 8ec3d1c^ = 1a18fff のテンプレート。
データ: `cargo run --example dump_sales_kpi -- data.json 2026-09-04`(tests/fixtures/sales_kpi の TSV、本物の Sheets は読んでいない)。

## 1. 修正前/後の E2E (--only blank / --only rowteam)

修正前テンプレートは `git show 8ec3d1c^:templates/tabs/sales_kpi.html` を別ファイルに出し、`--template` で差し替えた(作業ツリーは書き換えていない)。

コマンド(修正前): `python tests/e2e/sales_kpi_card_breakdown.py --json data.json --out <shots> --template old_template.html --only {blank|rowteam}`
コマンド(修正後): 同上から `--template` を外す(既定 templates/tabs/sales_kpi.html = HEAD)。

### 修正前 --only blank: NG 13 件(想定どおり 13)
```
NG 13 件
 - 担当なし: 行を押しても取引一覧が開かない
 - [担当なし入力] ③ 商談の予定: 「担当なし」の行を押しても一覧が開かない（同じ表に留まった）
 - [担当なし入力] ③ 商談の予定: 表の合計 38 ≠ 下の一覧の合計 27
 - [担当なし入力] ③ 商談の予定: 表の合計 537 ≠ 下の一覧の合計 526
 - [担当なし入力] ③ 商談の予定: 一覧の行数の合計 526 ≠ パネル見出し 537
 - [担当なし入力] ⑨ 持っているCヨミ: 「担当なし」の行を押しても一覧が開かない（同じ表に留まった）
 - [担当なし入力] ⑨ 持っているCヨミ: 表の合計 11 ≠ 下の一覧の合計 7
 - [担当なし入力] ⑨ 持っているCヨミ: 表の合計 123 ≠ 下の一覧の合計 119
 - [担当なし入力] ⑨ 持っているCヨミ: 一覧の行数の合計 119 ≠ パネル見出し 123
 - [担当なし入力] ⑨ 持っているCヨミ BPOだけ: 「担当なし」の行を押しても一覧が開かない（同じ表に留まった）
 - [担当なし入力] ⑨ 持っているCヨミ BPOだけ: 表の合計 2 ≠ 下の一覧の合計 1
 - [担当なし入力] ⑨ 持っているCヨミ BPOだけ: 表の合計 14 ≠ 下の一覧の合計 13
 - [担当なし入力] ⑨ 持っているCヨミ: BPO だけの一覧の合計 13 ≠ 内BPO 14
exit 1
```
### 修正前 --only rowteam: NG 6 件(想定どおり 6)
```
NG 6 件
 - [チーム=第1チーム] ⑦: カード 1.0 / 一覧 1 ≠ 名簿のチームで絞った 2（差: ['15676657489']）
 - [チーム=第2チーム] ⑦: カード 2.0 / 一覧 2 ≠ 名簿のチームで絞った 1（差: ['15676657489']）
 - [チーム=第2チーム] ⑤: カード 61.0 / 一覧 61 ≠ 名簿のチームで絞った 60（差: ['13222198048']）
 - [チーム=第2チーム] ⑨: カード 11.0 / 一覧 11 ≠ 名簿のチームで絞った 12（差: ['38477434516']）
 - [チーム=第3チーム] ⑨: カード 12.0 / 一覧 12 ≠ 名簿のチームで絞った 11（差: ['38477434516']）
 - [チーム=第4チーム] ⑤: カード 41.0 / 一覧 41 ≠ 名簿のチームで絞った 42（差: ['13222198048']）
exit 1
```
### 修正後(HEAD) --only blank / --only rowteam
```
OK   (blank, exit 0)
OK   (rowteam, exit 0)
```

## 2. 3 段階検証 (HEAD 8ec3d1c)

| 段階 | コマンド | 結果 |
|---|---|---|
| 静的 | `cargo fmt -- --check` | exit 0、差分なし |
| 静的 | `bash C:/dev/cargo-slot.sh cargo clippy --all-targets -j 3` | exit 0、error 0(warning のみ。lib test 489 件) |
| ユニット | `cargo test --lib -j 3` | `test result: ok. 3785 passed; 0 failed; 45 ignored; 0 measured; 0 filtered out` |
| 統合 | `cargo test --tests --no-fail-fast -j 3` | 下記 |
| E2E 全ケース | `python tests/e2e/sales_kpi_card_breakdown.py --json data.json --out <shots>` | `OK: カードの値 == パネル見出し == 一覧の行数の合計（開いたパネル 70 回、数えた一覧の行 9380 行）`、exit 0 |

統合テスト: 22 target 中 21 が ok(passed 数の例: 7, 5, 4, 4, 4, 32, 20, 16, 1。0 passed の target 11 は doc/例など件数 0 の target)。
失敗は 1 target のみ:
`no_forbidden_terms`: `test result: FAILED. 4 passed; 1 failed` (no_forbidden_identifiers_in_src)。
指摘は call_quality の p10_future_actions.rs と prisk_riskboard.rs の `target_count` 5 件で、既存の失敗(今回の変更範囲外・対象外)。
