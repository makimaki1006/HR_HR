---
name: audit-numeric-anomaly
description: 数値表示の異常 (100倍ずれ、桁違い、想定外の値、+N%が異常値) を観測したら起動。データ層 / 計算層 / 表示層の 3 層を必ず全部 grep し、仮説立てる前に実証データで真因を特定する。視覚レビュー、PDF レビュー、グラフ数値確認 等の依頼時にも先に呼ぶ。
---

# audit-numeric-anomaly

数値の表示異常を観測したら、**仮説を立てる前に** 全層を grep して真因を特定するための監査ルーチン。2026-05-14 の表示層 ×100 バグ (30分浪費) の再発防止策。

## 起動条件 (必須)

ユーザーが以下の依頼を出した時、または以下を観測した時:

- **数値の異常**: 100倍ずれ、桁違い、想定外の符号、`+\d{3,}%`、桁不一致
- **視覚レビュー / PDF レビュー / グラフ確認**: 表示数値の妥当性を確認する作業
- **「データがおかしい」「値がずれてる」「桁違い」「100倍」「100分の1」**

## 3 層監査チェックリスト (省略禁止)

> ⚠️ 仮説を立てる前にこのチェックを **必ず全て実施** する。1つでも飛ばすと再発する。

### 層 1: データ層 (DB / ETL)

- [ ] DB スキーマ確認: 該当カラムの単位は? (例: `%`単位 / 比率 / 円 / 万円)
- [ ] ETL 投入スクリプトを grep: `grep -rn "<col_name>" scripts/`
- [ ] ETL での換算有無を確認 (`* 100`, `/ 100`, `astype` 等)
- [ ] DB の実値 sample を SQL で取得: `SELECT <col> FROM <table> LIMIT 10`
- [ ] **SQL alias 出力名と DB スキーマ列名が異なる可能性を確認** (例: `SELECT ... as labor_force_participation_rate, ...` だが Rust 側 `get_f64(r, "labor_force_ratio")` で読んでいる ← name mismatch で常に 0/None)

### 層 2: 計算層 (fetch / aggregator / filter)

- [ ] Rust 側全 grep: `grep -rn "<col_name>" src/handlers/`
- [ ] フィルタ条件で単位が一致しているか確認 (例: `delta > 10.0` は %単位前提、`delta > 0.10` は比率前提)
- [ ] 中間集計 / 平均 / sort で単位変換していないか
- [ ] 構造体メンバー名と内部利用箇所の対応を全列挙
- [ ] **fetch 側 SQL の alias 出力名 と read 側 `get_f64(row, "...")` の文字列を厳密一致確認**

### 層 3: 表示層 (format! / template / ECharts option)

- [ ] **`grep -rn "<col_name>\s*\*\s*100"`** — ×100 換算の全箇所列挙
- [ ] **`grep -rn "<col_name>\s*/\s*100"`** — ÷100 換算の全箇所列挙
- [ ] format! 文字列内の単位指定 (`{:+.1}%` / `{:.2}` 等)
- [ ] ECharts option `formatter` での補正有無
- [ ] HTML template の単位ラベル ("万円" / "%" / "件" 等) と実値の整合性
- [ ] **同種機能の複数 variant 経路を全部監査** (例: Section 07 最低賃金は `hw_enrichment.rs` (Full variant) と `navy_report.rs:render_navy_section_07_lifestyle` (MI variant) で別実装。片方だけ修正すると variant 切替時に再発)

### 層 0: デプロイ反映確認 (層 1-3 の検証前に **必須**)

> ⚠️ 「コード修正したから直ったはず」は理論的推論。**実測で直っていることを確認するまで完了主張禁止**。

- [ ] `git log --oneline -3` で最新 commit を確認
- [ ] curl で health endpoint を叩いて `cache_entries` 値を観測
- [ ] 修正コードに含めた **build marker** (HTML コメント等) を curl で grep
  - marker が見えなければ → デプロイ未完了。**待つ** (Render 平均 5-10 分)
  - marker が見えれば → 検証へ進む
- [ ] 検証は **必ず新規セッション** で行う (古い session_id は古いバイナリ + 古いデータで生成された可能性あり)

## 既知の単位混在 / 名前不一致事故変数 (このプロジェクト固有)

| 変数 | DB の単位 / 名前 | 過去事故 |
|------|----------------|---------|
| `employee_delta_1y` | **% 単位** (5.0 = +5%) | 2026-04-30 salesnow.rs で `*100` 誤り、2026-05-14 navy_report.rs:2729 で再発 |
| `employee_delta_3m` | **% 単位** (同上) | 同上、同種パターン |
| `salary_min` / `salary_max` | **円** (250000 = 25万円) | 表示時 `/10000` で「万円」表示するか確認 |
| `unemployment_rate` (推定) | 不明 — 投入前確認必須 | 2026-04-27 不変条件で 380% 検出 |
| `labor_force_participation_rate` | **% 単位** (60.0 = 60%) | 2026-05-14 SQL alias `as labor_force_participation_rate` vs Rust `get_f64(r, "labor_force_ratio")` の name mismatch で「— %」(常に 0 値 → filter で除外)。**SQL alias と read 側カラム名は文字列一致必須** |

新規にこれらを使う時は **必ず全層 grep** してから着手。

## DIAG dump テンプレ (中間値観察)

`Vec<Struct>` の実値を HTML コメントや log で観察するためのテンプレ:

```rust
// HTML コメント版 (本番デプロイ可、診断後 revert)
let diag_vals: Vec<String> = vec_of_structs
    .iter()
    .map(|c| format!("{}={:+.1}", c.id_field, c.numeric_field))
    .collect();
html.push_str(&format!(
    "<!-- DIAG <name>.len()={} vals=[{}] -->\n",
    vec_of_structs.len(),
    diag_vals.join(",")
));
```

```rust
// tracing log 版 (恒久残置可)
tracing::debug!(
    name = "<name>",
    count = vec_of_structs.len(),
    sample = ?vec_of_structs.iter().take(5).map(|c| c.numeric_field).collect::<Vec<_>>()
);
```

**個社識別子 (corporate_number / 個人名等) を HTML に dump する時は診断後必ず revert する。**

## 実行手順

1. **異常値特定** — どの画面/PDFの/どの数値が異常か明示。期待値も併記。
2. **変数名特定** — その表示の元になっている Rust の構造体メンバー or DB カラム名
3. **3 層 grep** — 上記チェックリスト全実施。発見した全箇所を列挙
4. **仮説立案** — 全層 grep の結果を踏まえて初めて仮説を立てる
5. **DIAG dump (必要時)** — 中間値を観察できない場合のみ。観察したら revert
6. **修正実装** — 全 variant 経路を漏れなく修正
7. **🔴 デプロイ反映確認** — 修正に含めた build marker を curl で grep して反映確認。**markers 未確認のまま PDF 検証に進まない**
8. **検証** — 新規セッションで PDF/HTML 再生成し、期待値が出ているかを実測
9. **完了マーカー書込** — `.claude/.audit_numeric_done` を touch (hook 連動)

## 完了マーカー

監査完了したら以下を実行:

```bash
echo "$(date -u +%Y-%m-%dT%H:%M:%SZ) <variable_name> <root_cause_layer>" > .claude/.audit_numeric_done
```

この marker ファイルの存在 + mtime を `check_numeric_review_skill_used.py` hook が検査する。
30分以内に touch されていない状態で「視覚レビュー完了」「数値確認OK」等を主張すると hook が block する。

## アンチパターン (やってはいけない)

❌ 「データ側の異常値」と即断して fetch.rs にフィルタだけ入れる (3層検証なし)
❌ 「Render ビルドが古い」と仮説して時間を浪費 (DIAG dump で先に実証する)
❌ コメントの単位記述だけ信用する (`// %単位` と書いてあっても実装が違うことがある)
❌ ユーザーから「データ側の不具合では」と言われた時、データ層だけ確認する (両層 grep する)
❌ テスト pass = 数値正しい、と判断する (テストが想定単位で書かれていない可能性あり)
❌ **git push 直後に「デプロイ反映済み」と推論で判定** (Render 平均 5-10 分かかる。`curl /health` の cache_entries や build marker で実測確認するまで完了主張しない)
❌ **同種機能の variant 分岐を片方だけ修正** (例: Full / MarketIntelligence で別ファイルに同種コードがある。grep で同種パターンを全部洗い出してから修正)
❌ **SQL alias と Rust read 側のカラム名を目視確認せず修正完了とする** (SQL を直しても read 側が違う名前なら無意味)

## 参照ルール

- `feedback_three_layer_audit.md` — 3 層監査ルール本体 (2026-05-14)
- `feedback_unit_consistency_audit.md` — 単位統一監査 (2026-04-30)
- `feedback_code_first_test_second.md` — コード目視優先 (2026-05-13)
- `feedback_reverse_proof_tests.md` — 不変条件で逆証明 (2026-04-27)
