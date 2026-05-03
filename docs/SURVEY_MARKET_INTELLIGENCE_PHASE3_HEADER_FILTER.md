# Phase 3 前処理 (b): ヘッダー混入レコード WHERE フィルタ設計書

作成日: 2026-05-04
対象: ハローワーク分析システムV2 / `v2_external_population` + `v2_external_foreign_residents`

---

## 1. 背景

2026-05-03 の文字化け確認時に、以下 2 テーブルの 1 行目に「ヘッダー風文字列」がレコードとして混入していることが判明:

| テーブル | 混入行数 | 混入内容例 |
|---------|---------:|-----------|
| `v2_external_population` | 1 | `('都道府県', '市区町村', '2020-10-01')` |
| `v2_external_foreign_residents` | 1 | (同様のヘッダー文字列) |

これは文字化けではなく **データ品質問題** (CSV 1 行目のヘッダーがレコードとして INSERT された)。

ユーザー判断 (2026-05-03):
- **方針**: WHERE フィルタを設計に組み込む (DB 物理書き換えなし)
- **二段構え**: 読取時ガード + 将来投入時 skip
- **理由**: Claude/AI の DB 書込禁止ルールと整合 / 再投入時の自動防御 / 共通ガード化が容易

---

## 2. WHERE フィルタ仕様

### 2.1 最低限版

```sql
WHERE prefecture <> '都道府県'
```

### 2.2 厳密版 (推奨、ユーザー指示)

```sql
WHERE prefecture IS NOT NULL
  AND prefecture <> ''
  AND prefecture <> '都道府県'
  AND municipality <> '市区町村'
```

**Phase 3 実装では厳密版を採用する。** NULL 値・空文字も併せて除外することで、CSV 投入時の予期しない欠損行も同時にガードできる。

注: `v2_external_foreign_residents` は `municipality` カラムがない可能性 (`fetch_foreign_residents` の SELECT に含まれていない)。テーブル別にカラム存在を確認の上、`municipality` 条件を出し入れする。

---

## 3. Rust 実装の影響範囲

### 3.1 SQL を直接書いている関数 (修正対象)

`v2_external_population` の SELECT 文を持つ関数:

| ファイル | 関数 | 行 | SQL ブランチ |
|---------|------|---|--------------|
| `src/handlers/analysis/fetch/subtab5_phase4.rs` | `fetch_population_data` | 164〜194 | 3 ブランチ (muni 指定 / pref のみ / 全国 SUM) |
| `src/handlers/analysis/fetch/subtab5_phase4.rs` | `fetch_population_pyramid` | 196〜231 | 3 ブランチ (muni 指定 / pref のみ / 全国 GROUP BY) |

`v2_external_foreign_residents` の SELECT 文を持つ関数:

| ファイル | 関数 | 行 | SQL ブランチ |
|---------|------|---|--------------|
| `src/handlers/analysis/fetch/subtab5_phase4_7.rs` | `fetch_foreign_residents` | 19〜41 | 2 ブランチ (pref 指定 / 全国 SUM) |

**実装すべき箇所は 3 関数 × 計 8 SQL ブランチ**。これらに WHERE フィルタを追加すれば、呼び出し側全箇所 (下記 §3.2) に効果が及ぶ。

### 3.2 上記関数を呼び出している箇所 (修正不要だが確認必要)

```
src/handlers/insight/fetch.rs:154-155        (fetch_population_data, fetch_population_pyramid)
src/handlers/region/karte.rs:240, 250        (fetch_population_data, fetch_population_pyramid)
src/handlers/analysis/render/subtab5_anomaly.rs:23-24
src/handlers/analysis/render/subtab7.rs:51   (fetch_population_pyramid)
src/handlers/analysis/fetch/subtab7_other.rs:119
src/handlers/survey/granularity.rs:189-215   (両方)
```

**修正不要**。3 関数の内部 SQL 修正で全箇所に伝播する (DRY)。

### 3.3 影響度分析 (各 SQL ブランチ別)

| 関数 | ブランチ | WHERE | ヘッダー混入の影響 | 修正優先度 |
|------|---------|-------|-------------------|-----------|
| `fetch_population_data` | muni 指定 | `WHERE prefecture=?1 AND municipality=?2` | **影響なし** (`?1='都道府県'` になることはない) | 中 (再投入防御として推奨) |
| `fetch_population_data` | pref のみ | `WHERE prefecture=?1` | **影響なし** (同上) | 中 |
| `fetch_population_data` | 全国 SUM | WHERE なし | **★ 影響あり** (混入レコードが SUM に入り 1 行分過剰) | **高** |
| `fetch_population_pyramid` | muni 指定 | `WHERE prefecture=?1 AND municipality=?2` | 影響なし (現時点) | 中 |
| `fetch_population_pyramid` | pref のみ | `WHERE prefecture=?1` | 影響なし (現時点) | 中 |
| `fetch_population_pyramid` | 全国 GROUP BY | WHERE なし | 要検証 (現時点 pyramid に混入確認なし) | 中 |
| `fetch_foreign_residents` | pref 指定 | `WHERE prefecture=?1` | 影響なし | 中 |
| `fetch_foreign_residents` | 全国 SUM | WHERE なし | **★ 影響あり** | **高** |

### 3.4 即時影響を受けている API/UI

`fetch_population_data` の「全国 SUM」ブランチが使われる箇所 (推定):
- 全国概況 KPI (総人口等)
- ダッシュボード初期表示時の集計

→ 混入レコードの数値カラムは NULL または 0 のため、SUM への加算影響は限定的だが、`COUNT(DISTINCT prefecture)` 系の集計では 1 件過剰になる。

---

## 4. B-1. 読取時ガード設計 (Phase 3 で実装)

### 4.1 全 SQL ブランチに WHERE 追加

3 関数 × 8 ブランチすべてに以下を追加 (二重防御):

```sql
-- 既存
WHERE prefecture = ?1 AND municipality = ?2

-- 追加後
WHERE prefecture = ?1 AND municipality = ?2
  AND prefecture <> '都道府県'
  AND municipality <> '市区町村'
```

```sql
-- 既存 (全国 SUM)
FROM v2_external_population

-- 追加後
FROM v2_external_population
WHERE prefecture IS NOT NULL
  AND prefecture <> ''
  AND prefecture <> '都道府県'
  AND municipality <> '市区町村'
```

### 4.2 共通ヘルパー化案 (推奨)

現状 8 ブランチに同じ WHERE をベタ書きするのは DRY 違反。以下のヘルパーを `src/handlers/analysis/fetch/mod.rs` または新規 `src/handlers/analysis/fetch/external_filter.rs` に追加:

```rust
/// `v2_external_population` 系テーブルの「ヘッダー風文字列」混入レコードを除外する WHERE 句断片。
/// CSV 投入時に1行目ヘッダーがレコード化されたデータ品質問題への防御。
/// 詳細: docs/SURVEY_MARKET_INTELLIGENCE_PHASE3_HEADER_FILTER.md
pub const EXTERNAL_CLEAN_FILTER: &str = "\
    prefecture IS NOT NULL \
    AND prefecture <> '' \
    AND prefecture <> '都道府県' \
    AND municipality <> '市区町村'";

pub const EXTERNAL_CLEAN_FILTER_NO_MUNI: &str = "\
    prefecture IS NOT NULL \
    AND prefecture <> '' \
    AND prefecture <> '都道府県'";
```

使用例 (ベタ書きを置き換え):

```rust
// Before
"FROM v2_external_population WHERE prefecture = ?1".to_string()

// After
format!("FROM v2_external_population WHERE prefecture = ?1 AND {}", EXTERNAL_CLEAN_FILTER)
```

`municipality` カラムがないテーブル (`v2_external_foreign_residents`) は `EXTERNAL_CLEAN_FILTER_NO_MUNI` を使用。

### 4.3 共通ヘルパー設計の利点

| 項目 | 効果 |
|------|------|
| DRY | 8 箇所のベタ書きを 1 つの定数で管理 |
| 将来テーブル追加 | 新 v2_external_* テーブル追加時も同じガードを適用しやすい |
| 文字列定義の単一性 | フィルタ仕様変更時の修正箇所が 1 箇所 |
| 監査容易 | grep `EXTERNAL_CLEAN_FILTER` で全使用箇所を把握可能 |

---

## 5. B-2. 将来投入パイプライン側 skip (将来対応)

### 5.1 投入元スクリプトでの skip

`v2_external_population` と `v2_external_foreign_residents` の投入元スクリプトを特定し、CSV 読み込み時にヘッダー風行を除外する。

#### 該当候補スクリプト

`v2_external_population` 投入元:
- `scripts/upload_to_turso.py` (Turso 用、CREATE TABLE 文に PRIMARY KEY 定義あり)
- 別途 国勢調査 CSV から hellowork.db に投入するスクリプトがあるはず (要 grep / `scripts/fetch_census_demographics.py` 等)

`v2_external_foreign_residents` 投入元:
- `scripts/fetch_foreign_residents.py` (実存確認済)

### 5.2 推奨追加コード

各 fetch スクリプト内 CSV → DB 投入直前で:

```python
# CSV ヘッダー混入防御
df = df[df['prefecture'].notna()]
df = df[df['prefecture'].str.strip() != '']
df = df[df['prefecture'] != '都道府県']
if 'municipality' in df.columns:
    df = df[df['municipality'] != '市区町村']
```

または (sqlite3 直叩きの場合):

```python
SKIP_HEADER_VALUES = {'都道府県', '', None}
for row in csv_rows:
    if row.get('prefecture') in SKIP_HEADER_VALUES:
        continue
    if row.get('municipality') == '市区町村':
        continue
    cur.execute(insert_sql, row.values())
```

### 5.3 将来投入時 skip の優先度

**Phase 3 では設計のみ、実装は将来の再投入タイミング**:
- ユーザーが Task A の手順書に従って 6 テーブル投入を行う際は、現状スクリプトのままで OK (混入が新規発生したら Task A の検証 SQL で検出)
- 国勢調査・在留外国人の **再投入が発生する場合** (年度更新等) に skip ロジックを追加する

---

## 6. 検証 SQL

### 6.1 フィルタ適用前後の COUNT 比較

```sql
-- 適用前 (現状)
SELECT COUNT(*) FROM v2_external_population;
-- 期待: 1742

-- 厳密フィルタ適用後
SELECT COUNT(*) FROM v2_external_population
WHERE prefecture IS NOT NULL
  AND prefecture <> ''
  AND prefecture <> '都道府県'
  AND municipality <> '市区町村';
-- 期待: 1741 (1 件減 = ヘッダー除外)
```

```sql
-- 適用前
SELECT COUNT(*) FROM v2_external_foreign_residents;
-- 期待: 1742

-- フィルタ適用後
SELECT COUNT(*) FROM v2_external_foreign_residents
WHERE prefecture IS NOT NULL
  AND prefecture <> ''
  AND prefecture <> '都道府県';
-- 期待: 1741
```

### 6.2 Phase 3 着手前の最終検証 (他テーブル含む)

Task A の Step 6.2 検証 SQL と組み合わせ、新規投入したテーブルにヘッダー混入が**発生していない**ことを確認:

```sql
-- 新規 6 テーブルの混入チェック
SELECT 'household_spending', COUNT(*) FROM v2_external_household_spending WHERE prefecture = '都道府県'
UNION ALL SELECT 'labor_stats', COUNT(*) FROM v2_external_labor_stats WHERE prefecture = '都道府県'
UNION ALL SELECT 'establishments', COUNT(*) FROM v2_external_establishments WHERE prefecture = '都道府県'
UNION ALL SELECT 'land_price', COUNT(*) FROM v2_external_land_price WHERE prefecture = '都道府県'
UNION ALL SELECT 'industry_structure', COUNT(*) FROM v2_external_industry_structure WHERE prefecture_code = '都道府県';
-- 期待: すべて 0 件
```

混入が新規発生していた場合は、本書の WHERE フィルタ対象テーブルを追加更新すること。

---

## 7. Rust テスト追加案 (Phase 3 で実装)

```rust
#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_fetch_population_data_excludes_header_record() {
        // セットアップ: テストDBに「都道府県/市区町村」レコードを1件挿入
        let db = setup_test_db_with_header_record();
        let rows = fetch_population_data(&db, None, "", "");
        // 全国集計でヘッダー行が除外されていること
        assert!(rows.iter().all(|r| r.get("prefecture") != Some("都道府県")));
    }
}
```

---

## 8. 実装チェックリスト (Phase 3 着手時)

- [ ] `EXTERNAL_CLEAN_FILTER` 定数を `src/handlers/analysis/fetch/mod.rs` または新規モジュールに追加
- [ ] `fetch_population_data` の 3 ブランチに WHERE フィルタ適用
- [ ] `fetch_population_pyramid` の 3 ブランチに WHERE フィルタ適用
- [ ] `fetch_foreign_residents` の 2 ブランチに WHERE フィルタ適用 (`municipality` 列なしバリアント使用)
- [ ] 検証 SQL (§6.1) で混入レコード除外を確認
- [ ] Rust ユニットテスト追加 (§7)
- [ ] cargo test --lib で既存テスト regression なし
- [ ] 全国集計 API レスポンスの数値が変化していないこと (混入レコードの数値カラムが 0/NULL なら変化ゼロ)

---

## 9. 完了条件

Task B は **設計書の作成のみ** で完了。Rust 実装は Phase 3 のスコープ。

- [x] 影響範囲が grep 結果で全列挙
- [x] WHERE フィルタ仕様確定 (最低限版 + 厳密版)
- [x] 共通ヘルパー設計案あり
- [x] 将来投入時 skip の実装方針あり
- [x] 検証 SQL あり
- [x] 実装チェックリスト Phase 3 着手時用にあり
