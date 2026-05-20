---
name: audit-i18n-silent-fallback
description: 「英語カラム名が HTML/PDF に残る」「label が翻訳されない」「未マップキーがそのまま表示」等の silent fallback 系不具合を観測したら起動。Rust match arm の `_ => key` のような default 経路で未登録キーが silent に通過する構造を、SQL/コード全 grep × match arm diff で MECE 検出する。
---

# audit-i18n-silent-fallback

`label_for_column` のような **match arm + silent default** 構造で起きる「未マップキーがそのまま英語で表示される」漏れの根絶手順。2026-05-20 表 6-E 労働力統計詳細での英語ラベル残 (30+件の漏れ) 事故の再発防止策。

## 起動条件 (必須)

ユーザーが以下の依頼を出した時、または以下を観測した時:

- 「英語のラベルが残っている」「翻訳されていない」「日本語化漏れ」
- 「`<th>` や label に snake_case の英語がそのまま出ている」
- `label_for_column` / `translate` / `i18n_*` / `to_jp` 等の **match arm + default** 関数を修正する時
- 完了主張前に「未マップが他にもないか網羅確認したい」

## 根本問題: silent fallback 構造

```rust
fn label_for_column(key: &str) -> &str {
    match key {
        "prefecture" => "都道府県",
        "year" => "年",
        ...
        _ => key,  // ← 未登録は英語のまま、コンパイルもテストも fail しない
    }
}
```

**特徴**:
- 未登録キーでも **コンパイル通過 / 単体テスト通過 / 静的解析通過**
- **本番目視で初めて発覚** (= 後追い対応の連鎖を生む)
- session 毎に表示列が変わるため、固定 session の verify で全件検出不可
- ローカル DB と本番 Turso でカラム集合が異なるため、ローカル DB の `PRAGMA table_info()` だけでは網羅できない

## 5 ステップ監査チェックリスト (省略禁止)

> ⚠️ 1 件追加で commit せず、必ず全 5 ステップ実施して網羅追加を 1 コミットにまとめる。

### Step 1: silent fallback 経路を特定

- [ ] `match arm + _ => key` パターンを全 grep:
  ```bash
  Grep multiline=true pattern="match\s+\w+\s*\{[\s\S]*?_\s*=>\s*\w+,?\s*\}"
  ```
- [ ] 該当関数の入出力を把握 (どんなキーが来て、どんな表示になるか)
- [ ] 呼び出し元を grep (`grep -rn "label_for_column"`)

### Step 2: 全参照キーを抽出

silent fallback 関数が受け取り得る「全キー集合」を、コードベースから網羅抽出する。

- [ ] **SQL 由来**: `SELECT ... FROM v2_external_*` 等の SELECT 句のカラム名 (alias 含む)
  ```bash
  Grep multiline=true pattern="SELECT[\s\S]{0,500}FROM\s+<target_table>" output_mode=content
  ```
- [ ] **コード由来**: `get_str(row, "...")`, `get_f64(row, "...")`, `row["..."]` 等の参照キー
  ```bash
  grep -rhoE 'get_(str|f64|i64|opt_f64|opt_i64)\(row[^,]*,\s*"[a-z_][a-z_0-9]+"' src/ | grep -oE '"[a-z_][a-z_0-9]+"' | tr -d '"' | sort -u
  ```
- [ ] **HashMap 由来**: `row.keys()` で全キーが渡る場合は SQL の SELECT 句が唯一の真実

### Step 3: match arm 登録キーを抽出

- [ ] match arm の登録キーを grep:
  ```bash
  grep -oE '"[a-z_][a-z_0-9]+"\s*(\||=>)' <path-to-match-fn> | grep -oE '"[a-z_][a-z_0-9]+"' | tr -d '"' | sort -u
  ```

### Step 4: diff で未マップキーを抽出

- [ ] Bash + `comm -23` で diff:
  ```bash
  comm -23 /tmp/code_cols.txt /tmp/labeled_cols.txt
  ```
- [ ] 未マップキーを (a) 表示対象 / (b) 内部計算のみ で分類
- [ ] (a) の優先度高で一括追加 commit

### Step 5: 監査スクリプトを repo に常駐 + 完了 marker

- [ ] `scripts/audit_columns.py` (または相当スクリプト) を作成 / 更新
- [ ] CI に組み込み可能な形にする (exit 1 if unmapped > 0)
- [ ] 完了 marker touch:
  ```bash
  echo "$(date -u +%Y-%m-%dT%H:%M:%SZ) <target_fn> <unmapped_count>" > .claude/.audit_i18n_done
  ```

## diff 取得の最短コマンド (本プロジェクト用)

```bash
# Step 2 + 3 + 4 を 1 行で
cd <repo-root>
grep -rhoE 'get_(str|f64|i64|opt_f64|opt_i64)\(row[^,]*,\s*"[a-z_][a-z_0-9]+"' src/ \
  | grep -oE '"[a-z_][a-z_0-9]+"' | tr -d '"' | sort -u > /tmp/code_cols.txt

grep -oE '"[a-z_][a-z_0-9]+"\s*(\||=>)' src/handlers/survey/report_html/navy_report.rs \
  | grep -oE '"[a-z_][a-z_0-9]+"' | tr -d '"' | sort -u > /tmp/labeled_cols.txt

comm -23 /tmp/code_cols.txt /tmp/labeled_cols.txt
```

ただし `get_*(row, ...)` 経由 **以外** のキー (HashMap.keys() 経路の `build_navy_auto_table`) は SQL 抽出が必須:

```bash
# SQL の SELECT 句から alias 含めて抽出 (multiline)
Grep multiline=true pattern="SELECT[\s\S]{0,500}FROM\s+v2_external_" output_mode=content
```

これは Bash の grep だと改行制約で漏れるので、**Claude Code の Grep ツール (multiline: true)** を使う。

## ↓ 起動時の最短実行例 (Claude が自動で続ける)

1. silent fallback 関数を確定 (例: `label_for_column`)
2. agent (general-purpose) に「Rust ソース全 SELECT 句抽出 → match arm diff」を委譲
3. 出力された未マップキーを一括 Edit で追加
4. cargo build → commit → push → deploy → verify
5. `.claude/.audit_i18n_done` を touch して完了主張可能に

## 事故記録

- **2026-05-20 表 6-E 労働力統計詳細** (発端):
  - 監査前: 4 件英語残検出 (monthly_salary_male/female, part_time_wage_female, turnover_rate)
  - 1 件追加 commit (応急) → 別 session で elderly_single_households 検出 → さらに 1 件追加 commit
  - ユーザー指摘「なぜこういった漏らしがあるの？普通に確認したら分からないのかな？」
  - agent 委譲で MECE 抽出 → 30 件一括追加 (commit `56c47f8`)
  - 教訓: **「1 件追加で終わらせず、必ず Step 1-5 を実施」**

## 関連 hook / memory

- hook: `.claude/hooks/check_i18n_label_completeness.py` (本 skill 完了主張時に marker をチェック)
- memory: `feedback_silent_fallback_audit.md` (汎用ルール: match arm の `_ => default` を見たら全参照元を grep)
