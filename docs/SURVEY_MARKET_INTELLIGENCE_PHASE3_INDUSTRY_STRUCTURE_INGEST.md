# P0-2 前提作業: `v2_external_industry_structure` Local 投入手順

**日付**: 2026-05-10
**目的**: Round 8 P0-2 (地域 × 産業 × 性別 を MI PDF に出す) 着手前に、Local DB に `v2_external_industry_structure` を投入し、`employees_total / employees_male / employees_female` が読めることを確認する。
**性質**: ユーザー手動オペレーション (Claude は DB 書込禁止)
**前提 docs**: `MEDIA_REPORT_P0_FEASIBILITY_CHECK_2026_05_09.md`

---

## 0. 結論サマリ (3 行)

1. fetch スクリプト (`scripts/fetch_industry_structure.py`) は完成済 (e-Stat 経済センサス R3 / statsDataId=0003449718)、出力 CSV カラムに `employees_male / employees_female` あり。
2. **Local 投入用 ingest スクリプトは新規作成** (`scripts/ingest_industry_structure_to_local.py`)。Turso 用の DDL (`scripts/upload_new_external_to_turso.py:106-119`) と同一スキーマを採用。
3. 投入 → 確認 SQL (COUNT / PRAGMA / NULL 率 / サンプル) → P0-2 着手判定の 4 ステップ。

---

## 1. 前提状態 (read-only 確認済)

| 項目 | 状態 | 根拠 |
|---|---|---|
| fetch スクリプト | ✅ 完成 | `scripts/fetch_industry_structure.py` (570 行 / 2026-03-19) |
| データソース | e-Stat 経済センサス R3 | statsDataId=`0003449718` (`fetch_industry_structure.py:122,193`) |
| 出力 CSV カラム | 9 列 | `prefecture_code, city_code, city_name, industry_code, industry_name, establishments, employees_total, employees_male, employees_female` (`fetch_industry_structure.py:11-13, 102-110`) |
| 出力 CSV パス | `scripts/data/industry_structure_by_municipality.csv` | `fetch_industry_structure.py:37` |
| 性別カラムの存在 | ✅ あり | `fetch_industry_structure.py:362-364` で `TAB_EMPLOYEES_MALE / TAB_EMPLOYEES_FEMALE` を取得 |
| 年齢軸 | ❌ なし | 経済センサスは事業所単位で年齢非公開 |
| Turso スキーマ | ✅ 定義済 | `scripts/upload_new_external_to_turso.py:106-119` (city_code + industry_code が PRIMARY KEY) |
| Local 投入スクリプト | ⚠️ **新規作成** | `scripts/ingest_industry_structure_to_local.py` (本書と同時 commit) |
| ローカル DB 現状 | ❌ テーブル不在 | Agent B 監査: `data/hellowork.db` の sqlite_master に `v2_external_industry_structure` なし |

**ローカル DB に投入されていない理由 (推定)**: 既存 `import_external_csv.py` は population/migration/foreign/daytime の 4 種のみ対応で、industry_structure 用の専用 ingest は未整備だった。Turso には投入されている可能性 (Agent B/D 監査では Turso schema 確認、行数は未照会)。

---

## 2. 投入手順 (ユーザー手動オペレーション)

### 2.1 e-Stat APP_ID の確認

`fetch_industry_structure.py` は環境変数 `ESTAT_APP_ID` を要求する (line 122, 193)。既に設定済か確認:

```powershell
echo $env:ESTAT_APP_ID
```

未設定なら e-Stat 開発者登録ページ (https://www.e-stat.go.jp/api/) で APP_ID を取得し、以下で設定:

```powershell
$env:ESTAT_APP_ID = "<your_app_id>"
```

### 2.2 fetch 実行 (CSV 生成、所要時間 30-60 分の見込み)

```powershell
cd C:\Users\fuji1\AppData\Local\Temp\hellowork-deploy
python scripts\fetch_industry_structure.py
```

**注意点**:
- 1,917 自治体 × 19 産業 × 4 集計 = 約 14 万 API 呼び出し相当 (実際は city 単位でまとめて取得するため呼び出し回数は少ない)
- 進捗ファイル (`scripts/data/industry_structure_progress.txt`) で再開可能
- 出力先 `scripts/data/industry_structure_by_municipality.csv` (UTF-8 BOM)
- 1 city 失敗で全停止しない (ログ出力のみ)

完了後の確認:

```powershell
Get-Item scripts\data\industry_structure_by_municipality.csv | Select-Object Length, LastWriteTime
Get-Content scripts\data\industry_structure_by_municipality.csv -TotalCount 3
```

期待: サイズ 数 MB / 1,917 cities × 19 industries = 約 36,400 行。

### 2.3 投入計画の dry-run

```powershell
python scripts\ingest_industry_structure_to_local.py --dry-run
```

**期待出力**:
```
[csv] total rows  : 36,400 程度
[csv] unique cities    : 1,917
[csv] unique industries: 19 (['A', 'B', ..., 'S', 'AS'])
[csv] expected (cities x industries) = 36,423
[csv] ✅ row count MATCH (or ⚠️ 一部 city 取得失敗で diff)
[null] establishments  : ?% (e-Stat の "-" / "…" 由来)
[null] employees_male  : ?%
[null] employees_female: ?%
[sample] first 3 rows: { ... }
```

**判断ポイント**:
- 全行 NULL なら fetch 結果が空 (e-Stat API キーの権限不足等) → fetch 再実行
- `employees_male / employees_female` の NULL 率 > 30% なら、性別データが取得できていない自治体が多い → P0-2 のスコープ縮小判断

### 2.4 投入適用

```powershell
python scripts\ingest_industry_structure_to_local.py --apply
```

**期待出力**:
```
[apply] before    : 0
[apply] inserted  : 36,400 程度
[apply] after     : 36,400 程度
[apply] delta     : +36,400
```

スクリプトは `INSERT OR REPLACE` で冪等。再実行しても二重計上しない (PRIMARY KEY: `city_code + industry_code`)。

### 2.5 投入後の検証

```powershell
python scripts\ingest_industry_structure_to_local.py --verify-only
```

**期待出力**:
- `[count] total rows : 36,000-37,000 程度`
- `[count] unique city_code : 1,917`
- `[count] unique industry_code : 19`
- `[null]` 4 カラムの NULL 率
- `[sample]` 新宿区 (13104) と 千代田区 (13101) 各 5 行

**新宿区 (13104) の期待値** (e-Stat R3 経済センサスの実値依存):
- A 〜 S の 19 産業 × 1 行 = 5 行サンプル
- `employees_total > 0`、`employees_male + employees_female ≈ employees_total`
- NULL 率: 産業 D (鉱業) など極小規模で NULL の可能性

---

## 3. 確認 SQL (検証スクリプト以外で個別確認したい場合)

```python
import sqlite3
con = sqlite3.connect('data/hellowork.db')
cur = con.cursor()

# 1. 行数
cur.execute("SELECT COUNT(*) FROM v2_external_industry_structure")
print(cur.fetchone())

# 2. スキーマ
for r in cur.execute("PRAGMA table_info(v2_external_industry_structure)"):
    print(r)

# 3. 新宿区 産業別
for r in cur.execute("""
    SELECT industry_code, industry_name, employees_total, employees_male, employees_female
    FROM v2_external_industry_structure
    WHERE city_code='13104'
    ORDER BY employees_total DESC NULLS LAST
"""):
    print(r)

# 4. NULL 率
for col in ['employees_total', 'employees_male', 'employees_female']:
    cur.execute(f"""
        SELECT
            SUM(CASE WHEN {col} IS NULL THEN 1 ELSE 0 END) AS n_null,
            COUNT(*) AS total
        FROM v2_external_industry_structure
    """)
    n_null, total = cur.fetchone()
    print(f"{col}: {n_null}/{total} = {100.0*n_null/total:.1f}%")
```

---

## 4. P0-2 着手のトリガ条件

以下 5 条件すべて満たしたら P0-2 (実装) 着手判断:

| # | 条件 | 確認方法 |
|---|---|---|
| 1 | テーブル存在 | `--verify-only` で schema 出力あり |
| 2 | 行数 ≥ 30,000 | `--verify-only` の total rows |
| 3 | unique city_code ≥ 1,800 | `--verify-only` |
| 4 | `employees_male` NULL 率 < 30% | `--verify-only` |
| 5 | `employees_female` NULL 率 < 30% | `--verify-only` |

5 件すべて満たさない場合は **P0-2 着手前にデータ品質判断** を行う (NULL 率が高い場合は性別比表示が成立しない自治体が多すぎるため)。

---

## 5. P0-2 実装の概略 (条件達成後の参考)

トリガ達成後の P0-2 実装:

| ステップ | 内容 | 備考 |
|---|---|---|
| (a) fetch 関数追加 | `fetch_industry_structure_with_gender(db, turso, target_municipalities) -> Vec<Row>` を `src/handlers/analysis/fetch/market_intelligence.rs` に追加 | `WHERE city_code IN (...)` でフィルタ |
| (b) DTO 追加 | `pub struct IndustryGenderCellDto` (city_code, industry_code/name, employees_total/male/female) | |
| (c) `SurveyMarketIntelligenceData` にフィールド追加 | `pub industry_gender_cells: Vec<IndustryGenderCellDto>` | |
| (d) build に組込 | `build_market_intelligence_data` に `fetch_industry_structure_with_gender` を呼ぶ行を追加 | |
| (e) render 関数 | `render_mi_industry_gender_summary(html, cells)` を `market_intelligence.rs` に追加 | 自治体ごとに産業 Top 10、女性比、採用示唆 (例: 女性比 ≥60% → 女性中心) |
| (f) call site | `render_section_market_intelligence` 内に組込 (Round 8 P0-1 の `render_mi_occupation_segment_summary` 直後など、論理的に近い位置) | |
| (g) ローカル + 本番検証 | Round 8 P0-1 と同手順で PDF 実物確認 | |

---

## 6. 既存 docs との関係

| docs | 関係 |
|---|---|
| `SURVEY_MARKET_INTELLIGENCE_PHASE3_TABLE_INGEST.md` | Phase 3 の 6 テーブル投入手順書。`v2_external_industry_structure` は ④ に含まれているが Turso 投入のみ記載。**Local 投入は本書 (PHASE3_INDUSTRY_STRUCTURE_INGEST.md) で補完** |
| `SURVEY_MARKET_INTELLIGENCE_PHASE3_LOCAL_INGEST_VALIDATION.md` | Local 投入検証 docs (Phase 3 既存) |
| `OPEN_DATA_UTILIZATION_TOTAL_AUDIT_2026_05_09.md` §3 | Local 未投入を確認した親監査 |
| `MEDIA_REPORT_P0_FEASIBILITY_CHECK_2026_05_09.md` §1.A | `employees_male/female` 列の存在を確認した実証 docs |
| `ROUND8_P0_1_COMPLETION_2026_05_10.md` | Round 8 P0-1 完了報告。本 P0-2 はその直後の作業 |

---

## 7. 監査メタデータ

- 作成: 2026-05-10
- 性質: 投入手順書 + ingest スクリプト仕様
- Claude が実行する作業: ingest スクリプト作成、本書作成
- ユーザーが実行する作業: APP_ID 設定、fetch 実行、ingest 実行、検証
- DB 書込: ユーザー側のみ (Claude は禁止)
- 完了条件: §4 トリガ 5 条件達成 → P0-2 実装着手判断
