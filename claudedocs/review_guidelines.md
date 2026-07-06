# AI レビューガイドライン: 事故クラスと検出方法の対応表

このドキュメントは AI レビュー担当者が「自由探索」ではなく、
**既知の事故クラスをチェックリスト形式でカバーする**ことを目的にしている。

レビューはここに示す表の上から順に確認し、
各クラスが指定する検出方法を実行してから次に進むこと。

---

## 1. 統計表現の誤用 (相関→因果 / 断定語)

| サブクラス | 具体例 | 検出方法 |
|-----------|-------|---------|
| 効果約束 (causal claim) | 「ことで応募率が向上します」 | **CI lint** `scripts/lint_statistical_claims.py` |
| 絶対表現 | 「必ず改善されます」 | 同上 |
| 誇張語 | 「劇的な向上」「完璧な解決」 | 同上 |
| 断定ラベル再導入 | 「離職多発」「流出継続」 | 同上 |

実行コマンド:

```bash
python scripts/lint_statistical_claims.py
```

終了コード 0 = OK、1 = 違反あり (ファイル:行:マッチ を出力)。

免責文脈でどうしても使う場合は同一行に `// lint-allow: statistical-claim` を付ける。
ただし、許可リスト乱用は禁止 — 根本的に表現を修正することを優先する。

---

## 2. 列・単位の契約違反

| サブクラス | 過去事故 | 検出方法 |
|-----------|---------|---------|
| 単位不一致 (% vs 比率) | 2026-04-30: `employee_delta_1y` 100 倍ずれ | **列契約テスト**: `cargo test -- unit_consistency` |
| DB カラム追加時の表示層漏れ | 2026-05-14: `navy_report.rs:2729` 表示層 ×100 | 3 層 grep: `grep -n 'employee_delta_1y' src/**/*.rs` で DATA/CALC/DISPLAY 全確認 |
| `workers_count_tenfold` 忘れ | 職種カルテ ×10 表示漏れ | 変数名 grep + 表示層の ×10 乗算確認 |

---

## 3. dedup キー漏れ (雇用形態・dedupルール)

| サブクラス | 過去事故 | 検出方法 |
|-----------|---------|---------|
| 雇用形態をdedupキーに含めない | 2026-02-24: 介護職 25,452 件消失 | **コード目視**: `drop_duplicates` / `DISTINCT` の `subset` に `employment_type` があるか確認 |

確認コマンド例:

```bash
grep -n "drop_duplicates\|DISTINCT" python_scripts/*.py | grep -v employment_type
# → 出力があったら要注意
```

---

## 4. 決定性テスト (乱数・外部依存なし)

| サブクラス | リスク | 検出方法 |
|-----------|-------|---------|
| テストが乱数・外部 API に依存 | flaky test / CI 不安定 | **決定性テスト**: `cargo test -- --test-threads=1` を 2 回実行し diff が出ないことを確認 |
| assert が「存在確認」のみ | データ内容の問題を見逃す | 具体値またはドメイン不変条件 (失業率 < 100% 等) で検証 |

---

## 5. 視覚的検証 (ECharts / 印刷 PDF)

| サブクラス | 過去事故 | 検出方法 |
|-----------|---------|---------|
| ECharts 初期化失敗 (canvas 存在のみ確認) | 2026-04-08: 19/24 チャートブランク | **E2E 視覚テスト**: `waitForFunction(() => echartsInstance != null)` |
| 印刷 CSS 崩れ | 2026-04-30: 本文幅縮小 | `@media print` CSS を全 Read → `python gen_survey_pdf.py` → PyMuPDF でフッター除外測定 |
| PDF ページ数だけで判定 | 2026-05-10: 誤判定 2 回 | ページ数 + 固有文言/class grep + 更新時刻確認の 3 点セット |

---

## 6. デプロイ・ビルド境界

| サブクラス | 過去事故 | 検出方法 |
|-----------|---------|---------|
| OneDrive 配下で cargo build | 2026-06-10: リンカー大量エラー | ビルドは必ず `CARGO_TARGET_DIR` を OneDrive 外に設定して実行 |
| 稼働中 exe のロック | 2026-05-11: silent failure | build ログで `os error 5` / `アクセスが拒否` を grep |
| 部分コミットの依存漏れ | 2026-04-22: Render deploy 失敗 | コミット前 `include_str!/pub mod` 依存チェーン grep |

---

## レビュー実施手順 (まとめ)

1. `python scripts/lint_statistical_claims.py` を実行 → exit 0 を確認
2. 差分に `drop_duplicates` / `DISTINCT` があれば 3 節を手動確認
3. 差分に単位系 (%, rate, tenfold 等) があれば 2 節の 3 層 grep
4. チャート・印刷関連の変更があれば 5 節の E2E / PDF 測定
5. `cargo test --lib` が全 PASS であることを確認
6. 以上がすべて green なら「品質完了」と報告してよい

---

*最終更新: 2026-07-07 (統計表現 lint CI 化対応)*
