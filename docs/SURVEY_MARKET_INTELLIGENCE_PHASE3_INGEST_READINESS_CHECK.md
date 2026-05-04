# Phase 3/4 データ投入前チェック (Ingest Readiness Check)

**作成日**: 2026-05-04
**作成者**: Worker C3
**対象**: Phase 3 (`v2_external_industry_structure`) + Phase 4 (`occupation_industry_weight`)
**位置付け**: 投入前チェック + ユーザー手動投入手順書（DB 書き込みは Claude 禁止）
**関連 docs**:
- `docs/SURVEY_MARKET_INTELLIGENCE_PHASE3_F2_IMPLEMENTATION_PLAN.md` (§1.1, §1.2)
- Worker A2 推奨: 最終テーブル名 `v2_municipality_target_thickness`
- Worker B 出力: `data/generated/occupation_industry_weight.csv` (231 行)

---

## 0. サマリー

| 項目 | Phase 3 | Phase 4 |
|------|:-------:|:-------:|
| 対象テーブル | `v2_external_industry_structure` | `occupation_industry_weight` |
| CSV 存在 | ✅ `scripts/data/industry_structure_by_municipality.csv` (36,099 行) | ✅ `data/generated/occupation_industry_weight.csv` (231 行) |
| ローカル DB 投入状態 | ❌ 未投入 | ❌ 未投入 |
| DDL 既定義 | ✅ `scripts/upload_new_external_to_turso.py:128-141` | ❌ 新規 (本書 §2.2 で提示) |
| 既存スクリプト対応 | ⚠️ Turso 用のみ存在、ローカル投入機能なし | ❌ なし |
| 新規スクリプト要否 | 🟡 不要 (sqlite3 .import で代替可) | 🟡 不要 (sqlite3 .import で代替可) |
| ユーザー手動工程数 | **3 ステップ** (DDL → import → 検証) | **3 ステップ** (DDL → import → 検証) |
| 投入順序 | Phase 3 → Phase 4 → Phase 5 (本書 §6) | (上に同じ) |

**ボトルネック**: なし。両テーブルとも CSV は完成済みで、CREATE TABLE と `.import` のみで投入可能。新規スクリプト作成は不要 (sqlite3 CLI で完結)。

**weight_source 運用要点**:
- Phase 4 投入時は `weight_source='hypothesis_v1'` を全 231 行に保持
- PRIMARY KEY に `weight_source` を含めて将来 `estat_R2_xxx` を並列保管可能に設計
- UI 表示は `hypothesis_v1` 単独の間「**検証済み推定 β**」、置換完了後「**検証済み推定**」(β 削除)

---

## 1. Phase 3: `v2_external_industry_structure` 投入準備

### 1.1 既存資産チェック結果

| 項目 | 結果 | 備考 |
|------|:----:|------|
| CSV 元データ存在 | ✅ | `scripts/data/industry_structure_by_municipality.csv` |
| CSV 行数 | ✅ | 36,099 行 (ヘッダー除く、`wc -l` 結果 36,100) |
| CSV エンコーディング | ✅ | UTF-8 with BOM (`utf-8-sig` 必須) |
| CSV カラム | ⚠️ 計画書と差分あり | **実 CSV ヘッダー**: `prefecture_code, city_code, city_name, industry_code, industry_name, establishments, employees_total, employees_male, employees_female` (9 カラム)<br>**計画書 §1.2.1 案**: `prefecture, municipality, jsic_code, ..., reference_year` ⇒ **不一致**。実装は実 CSV に合わせる |
| DDL 既存定義 | ✅ | `scripts/upload_new_external_to_turso.py:128-141` で定義済 (9 カラム + PRIMARY KEY (city_code, industry_code)) |
| 都道府県数 | ✅ | 47 (`prefecture_code` 一意数 = 47) |
| 市区町村数 | ✅ | 1,719 (`city_code` 一意数 = 1,719) |
| 産業コード数 | ✅ | 21 種 (AB, AR, AS, C, CR, D, E, F, G, H, I, J, K, L, M, N, O, P, Q, R, S)<br>**注**: AS=全産業, AR=全産業（公務除く）, AB=農林漁業計, CR=鉱業＋建設＋製造業計 と推測される集計値が混在。F2 計算時の集計値除外に留意 |
| 各産業×市区町村行数 | ✅ | 各産業 1,719 行ずつ均等 (= 47 県 × 平均 36.6 市区町村) |
| ローカル DB 不在確認 | ✅ | `data/hellowork.db` に `v2_external_industry_structure` テーブルなし (未投入) |

### 1.2 採用 DDL (実 CSV カラムに整合)

`scripts/upload_new_external_to_turso.py:128-141` の既存定義をそのまま採用する。計画書 §1.2.1 のスキーマ案 (`prefecture, municipality, jsic_code, ...`) は CSV と不一致のため**採用しない**。

```sql
CREATE TABLE IF NOT EXISTS v2_external_industry_structure (
    prefecture_code  TEXT NOT NULL,
    city_code        TEXT NOT NULL,
    city_name        TEXT,
    industry_code    TEXT NOT NULL,
    industry_name    TEXT,
    establishments   INTEGER,
    employees_total  INTEGER,
    employees_male   INTEGER,
    employees_female INTEGER,
    PRIMARY KEY (city_code, industry_code)
);
CREATE INDEX IF NOT EXISTS idx_v2_external_industry_structure_prefecture
    ON v2_external_industry_structure (prefecture_code);
CREATE INDEX IF NOT EXISTS idx_v2_external_industry_structure_industry
    ON v2_external_industry_structure (industry_code);
```

**設計判断**:
- 計画書 §1.2.1 案にあった `survey_year` (R3 デフォルト) と `source` (estat_economic_census_r3) は CSV に列がないため非採用。今後再取得時に列追加するなら DDL マイグレーションを別途行う。
- `prefecture` (47 都道府県名) も CSV に列がないため非採用。`prefecture_code` から JOIN で取得する設計を維持。
- F2 計算で `industry_code IN ('D','E','F','G','H','I','J','K','L','M','N','O','P','Q','R','S','T','X')` 等の単一産業に絞れば、AS/AR/AB/CR の集計値は自然に除外される。

### 1.3 ユーザー手動投入手順

**前提**: 作業ディレクトリ = `C:/Users/fuji1/AppData/Local/Temp/hellowork-deploy`

```bash
# Step 1: 一時 DDL ファイル作成 (本書 §1.2 の SQL を保存)
#         (ユーザーが手元のエディタで /tmp/v2_external_industry_structure_ddl.sql に保存)

# Step 2: ローカル DB に DDL 適用
sqlite3 data/hellowork.db < /tmp/v2_external_industry_structure_ddl.sql

# Step 3: CSV 投入 (sqlite3 CLI .import を使用)
#  - BOM 付き UTF-8 のため、最初の 1 行 (BOM 含むヘッダー) をスキップする必要あり
#  - 一時テーブル経由で型変換してから本テーブルへ挿入
sqlite3 data/hellowork.db <<'EOF'
.mode csv
.import scripts/data/industry_structure_by_municipality.csv tmp_industry_structure
-- BOM 対策: tmp_industry_structure の 1 行目に BOM 残留の可能性あり、
-- prefecture_code が "01" など 2 桁数字以外なら除外
INSERT OR REPLACE INTO v2_external_industry_structure
    (prefecture_code, city_code, city_name, industry_code, industry_name,
     establishments, employees_total, employees_male, employees_female)
SELECT prefecture_code, city_code, city_name, industry_code, industry_name,
       CAST(establishments   AS INTEGER),
       CAST(employees_total  AS INTEGER),
       CAST(employees_male   AS INTEGER),
       CAST(employees_female AS INTEGER)
FROM tmp_industry_structure
WHERE LENGTH(prefecture_code) = 2
  AND prefecture_code GLOB '[0-9][0-9]';
DROP TABLE tmp_industry_structure;
EOF
```

**代替案 (Python ワンライナー)**: BOM 対策が確実。

```bash
python -c "
import csv, sqlite3
with sqlite3.connect('data/hellowork.db') as conn, \
     open('scripts/data/industry_structure_by_municipality.csv', encoding='utf-8-sig') as f:
    rows = list(csv.DictReader(f))
    conn.executemany('''
        INSERT OR REPLACE INTO v2_external_industry_structure
            (prefecture_code, city_code, city_name, industry_code, industry_name,
             establishments, employees_total, employees_male, employees_female)
        VALUES (?, ?, ?, ?, ?, ?, ?, ?, ?)
    ''', [(
        r['prefecture_code'], r['city_code'], r['city_name'],
        r['industry_code'], r['industry_name'],
        int(r['establishments'])   if r['establishments']   else None,
        int(r['employees_total'])  if r['employees_total']  else None,
        int(r['employees_male'])   if r['employees_male']   else None,
        int(r['employees_female']) if r['employees_female'] else None,
    ) for r in rows])
    conn.commit()
    print(f'inserted {len(rows)} rows')
"
```

### 1.4 投入後検証 SQL (Phase 3)

```bash
sqlite3 data/hellowork.db <<'EOF'
-- 期待: 36,099 行
SELECT COUNT(*) AS total_rows FROM v2_external_industry_structure;

-- 期待: 1,719
SELECT COUNT(DISTINCT city_code) AS unique_cities FROM v2_external_industry_structure;

-- 期待: 47
SELECT COUNT(DISTINCT prefecture_code) AS unique_prefectures FROM v2_external_industry_structure;

-- 期待: 21 産業、各 1,719 行
SELECT industry_code, COUNT(*) AS rows FROM v2_external_industry_structure
GROUP BY industry_code ORDER BY industry_code;

-- 期待: AS (全産業) と AR (公務除く) は集計値、employees_total > 0 がほぼ全行
SELECT industry_code,
       MIN(employees_total) AS min_emp,
       MAX(employees_total) AS max_emp,
       AVG(employees_total) AS avg_emp
FROM v2_external_industry_structure
WHERE employees_total > 0
GROUP BY industry_code
ORDER BY industry_code;

-- 期待: 札幌市 (01100) の AS=920986
SELECT * FROM v2_external_industry_structure
WHERE city_code = '01100' AND industry_code = 'AS';
EOF
```

**整合性アサーション** (検証 SQL で確認すべき条件):

| 条件 | 期待値 |
|------|--------|
| 総行数 | 36,099 |
| 都道府県数 | 47 |
| 市区町村数 | 1,719 |
| 産業コード数 | 21 |
| 札幌市 (01100) AS の employees_total | 920,986 |
| NULL 行 (prefecture_code IS NULL) | 0 |

---

## 2. Phase 4: `occupation_industry_weight` 投入準備

### 2.1 既存資産チェック結果

| 項目 | 結果 | 備考 |
|------|:----:|------|
| CSV 存在 | ✅ | `data/generated/occupation_industry_weight.csv` |
| CSV 行数 | ✅ | 231 行 (= 21 産業 × 11 職業) |
| CSV カラム | ✅ | `industry_code, industry_name, occupation_code, occupation_name, weight, source, note` (7 カラム) |
| weight_source 値 | ✅ | 全 231 行が `source='hypothesis_v1'` |
| 産業数 | ✅ | 21 (A〜T, X) |
| 職業数 | ✅ | 11 (`01_管理` 〜 `11_運搬清掃`) |
| 各産業 weight 合計 | ✅ | 全 21 産業で **1.0000 ± 0.0001** (検証完了) |
| ローカル DB 不在確認 | ✅ | `occupation_industry_weight` テーブル不在 |
| 既存 DDL 定義 | ❌ | `upload_new_external_to_turso.py` には未定義 (新規) |

### 2.2 採用 DDL

```sql
CREATE TABLE IF NOT EXISTS occupation_industry_weight (
    industry_code   TEXT NOT NULL,
    industry_name   TEXT NOT NULL,
    occupation_code TEXT NOT NULL,
    occupation_name TEXT NOT NULL,
    weight          REAL NOT NULL CHECK (weight >= 0.0 AND weight <= 1.0),
    weight_source   TEXT NOT NULL DEFAULT 'hypothesis_v1',
    note            TEXT,
    created_at      TEXT NOT NULL DEFAULT (datetime('now')),
    PRIMARY KEY (industry_code, occupation_code, weight_source)
);

CREATE INDEX IF NOT EXISTS idx_occ_industry_weight_industry
    ON occupation_industry_weight (industry_code);
CREATE INDEX IF NOT EXISTS idx_occ_industry_weight_occupation
    ON occupation_industry_weight (occupation_code);
CREATE INDEX IF NOT EXISTS idx_occ_industry_weight_source
    ON occupation_industry_weight (weight_source);
```

**PRIMARY KEY 設計の意図**:

| 観点 | 説明 |
|-----|------|
| `industry_code + occupation_code + weight_source` の三項複合 PK | 同一 (産業, 職業) ペアに対し複数の `weight_source` を並列保管可能 (例: `hypothesis_v1` と `estat_R2_0003450543` が共存) |
| 並列保管の用途 | UI で「推定根拠」切替、A/B 比較、Phase 4 移行時の段階的置換、ロールバック容易性 |
| 計画書 §1.2.3 案との差異 | 計画書 PK = `(jsic_code, occupation_code)` だが、本書では `weight_source` を含めることで将来の e-Stat 実測値版と並列保管可能に拡張 |

**計画書との差分理由**:
- 計画書 §1.2.3 のカラム名 `jsic_code` は CSV ヘッダー `industry_code` と不一致 → 実 CSV に合わせて `industry_code` を採用
- 計画書では `notes`、CSV では `note` → CSV に合わせて `note`

### 2.3 ユーザー手動投入手順

```bash
# Step 1: DDL 適用
sqlite3 data/hellowork.db < /tmp/occupation_industry_weight_ddl.sql

# Step 2: CSV 投入 ("source" → "weight_source" にリネーム)
sqlite3 data/hellowork.db <<'EOF'
.mode csv
.import data/generated/occupation_industry_weight.csv tmp_occ_weight

INSERT OR REPLACE INTO occupation_industry_weight
    (industry_code, industry_name, occupation_code, occupation_name,
     weight, weight_source, note)
SELECT industry_code, industry_name, occupation_code, occupation_name,
       CAST(weight AS REAL), source, note
FROM tmp_occ_weight
WHERE industry_code != 'industry_code';  -- ヘッダー行除外

DROP TABLE tmp_occ_weight;
EOF
```

**代替案 (Python ワンライナー)**:

```bash
python -c "
import csv, sqlite3
with sqlite3.connect('data/hellowork.db') as conn, \
     open('data/generated/occupation_industry_weight.csv', encoding='utf-8') as f:
    rows = list(csv.DictReader(f))
    # weight_source 必須アサート
    assert all(r['source'] == 'hypothesis_v1' for r in rows), 'hypothesis_v1 以外混入'
    assert len(rows) == 231, f'expected 231 rows, got {len(rows)}'
    conn.executemany('''
        INSERT OR REPLACE INTO occupation_industry_weight
            (industry_code, industry_name, occupation_code, occupation_name,
             weight, weight_source, note)
        VALUES (?, ?, ?, ?, ?, ?, ?)
    ''', [(
        r['industry_code'], r['industry_name'],
        r['occupation_code'], r['occupation_name'],
        float(r['weight']), r['source'], r.get('note', ''),
    ) for r in rows])
    conn.commit()
    print(f'inserted {len(rows)} rows (weight_source=hypothesis_v1)')
"
```

### 2.4 投入後検証 SQL (Phase 4)

```bash
sqlite3 data/hellowork.db <<'EOF'
-- 期待: 231
SELECT COUNT(*) AS total_rows FROM occupation_industry_weight;

-- 期待: 全 21 産業で 1.0000 ± 0.0001
SELECT industry_code, ROUND(SUM(weight), 4) AS total_weight
FROM occupation_industry_weight
WHERE weight_source = 'hypothesis_v1'
GROUP BY industry_code
ORDER BY industry_code;

-- 期待: ['hypothesis_v1'] のみ
SELECT DISTINCT weight_source FROM occupation_industry_weight;

-- 期待: 21 産業
SELECT COUNT(DISTINCT industry_code) AS industries FROM occupation_industry_weight;

-- 期待: 11 職業
SELECT COUNT(DISTINCT occupation_code) AS occupations FROM occupation_industry_weight;

-- 期待: weight 範囲 [0.0, 1.0]
SELECT MIN(weight) AS min_w, MAX(weight) AS max_w FROM occupation_industry_weight;

-- 期待: 各 (産業, 職業) ペアは hypothesis_v1 で 1 行ずつ
SELECT COUNT(*) FROM (
    SELECT industry_code, occupation_code, COUNT(*) AS c
    FROM occupation_industry_weight
    WHERE weight_source = 'hypothesis_v1'
    GROUP BY industry_code, occupation_code
    HAVING c > 1
);  -- 期待: 0
EOF
```

**整合性アサーション**:

| 条件 | 期待値 |
|------|--------|
| 総行数 | 231 |
| 産業数 | 21 |
| 職業数 | 11 |
| 各産業の weight 合計 | 1.0000 ± 0.0001 (全 21 産業) |
| weight_source 一意値 | `hypothesis_v1` のみ |
| weight 範囲 | [0.0, 1.0] (CHECK 制約で担保) |

---

## 3. weight_source 運用ルール

### 3.1 値の意味

| weight_source 値 | 意味 | UI 表示 | 検証状態 |
|----------------|------|---------|---------|
| `hypothesis_v1` | Worker B が産業常識ベースで策定した仮重み | **検証済み推定 β** | プロト段階 |
| `estat_R2_0003450543` (将来) | e-Stat 国勢調査 (就業状態等基本集計) 産業×職業実測値 | **検証済み推定** (β 削除) | 公式統計準拠 |
| `estat_xxx_v2` (将来) | 別の e-Stat 表からの実測値 (差分検証用) | 並列保管 (UI 非表示の比較用) | 検証用 |

### 3.2 UI 表示ルール (Worker C2 docs §1〜§5 連動)

```
DB に weight_source = 'hypothesis_v1' のみ存在 → UI = 「検証済み推定 β」
DB に weight_source = 'estat_R2_xxx' が併存       → UI = 「検証済み推定」(β 削除)
                                                    (build_municipality_target_thickness.py 内で
                                                     estat_xxx を優先選択する)
```

### 3.3 build スクリプトでの選択ロジック

`build_municipality_target_thickness.py` (Worker B2 後続実装) での重み読込:

```python
def load_occupation_industry_weight(conn, prefer="estat_R2_0003450543"):
    """
    優先順位で weight_source を選択する。
    estat_xxx 系が DB に存在すれば優先、無ければ hypothesis_v1 を fallback。
    """
    cur = conn.execute(
        "SELECT DISTINCT weight_source FROM occupation_industry_weight"
    )
    sources = [r[0] for r in cur.fetchall()]
    if prefer in sources:
        chosen = prefer
    elif "hypothesis_v1" in sources:
        chosen = "hypothesis_v1"
    else:
        raise RuntimeError(f"利用可能な weight_source なし: {sources}")

    df = pd.read_sql_query(
        "SELECT * FROM occupation_industry_weight WHERE weight_source = ?",
        conn, params=(chosen,))
    print(f"[INFO] weight_source = {chosen}, {len(df)} 行")
    return df, chosen
```

### 3.4 並列投入時のオペレーション

将来 e-Stat 実測値を投入する場合:

```bash
# (a) 新しい weight_source で追加投入 (既存の hypothesis_v1 行は残る)
INSERT OR REPLACE INTO occupation_industry_weight
    (industry_code, industry_name, occupation_code, occupation_name,
     weight, weight_source, note)
SELECT ..., 'estat_R2_0003450543' AS weight_source, ...
FROM <e-Stat 取得結果>;

# (b) 検証: 両 weight_source の合計が各産業で 1.0000 になることを確認
SELECT weight_source, industry_code, ROUND(SUM(weight), 4)
FROM occupation_industry_weight
GROUP BY weight_source, industry_code;

# (c) UI 表示切替: build_municipality_target_thickness.py に prefer='estat_R2_0003450543'
#     を渡せば、UI 上「検証済み推定 β」→「検証済み推定」に自動移行
```

---

## 4. 投入後の整合性検証 SQL (両テーブル統合)

Phase 3 + Phase 4 投入完了後、F2 計算が成立するために必要な結合チェック:

```bash
sqlite3 data/hellowork.db <<'EOF'
-- 4.1 産業コードの整合性チェック
-- 期待: occupation_industry_weight の industry_code は v2_external_industry_structure に
--       (集計コード AS/AR/AB/CR を除く) 全て存在する
SELECT DISTINCT oiw.industry_code
FROM occupation_industry_weight oiw
LEFT JOIN v2_external_industry_structure ind
       ON oiw.industry_code = ind.industry_code
WHERE ind.industry_code IS NULL
ORDER BY oiw.industry_code;
-- 期待結果: 'T' および 'X' のみ
-- (CSV 集計値の AS/AR/AB/CR は片側のみ、T/X は重み側のみ。これらは F2 計算で個別ハンドリング)

-- 4.2 weight_source 状態の確認 (hypothesis_v1 期は β 表示)
SELECT weight_source, COUNT(*) AS rows,
       COUNT(DISTINCT industry_code)   AS industries,
       COUNT(DISTINCT occupation_code) AS occupations
FROM occupation_industry_weight
GROUP BY weight_source;

-- 4.3 F2 計算で実際に使うサブセット (重み側 21 産業から集計値除外で 21 産業すべて使用、
--     構造側からは AS/AR/AB/CR を除外した個別産業のみ)
SELECT ind.industry_code,
       COUNT(DISTINCT ind.city_code) AS cities,
       COUNT(DISTINCT oiw.occupation_code) AS occupations_per_industry,
       ROUND(SUM(oiw.weight), 4) AS weight_total
FROM v2_external_industry_structure ind
JOIN occupation_industry_weight oiw
  ON ind.industry_code = oiw.industry_code
WHERE oiw.weight_source = 'hypothesis_v1'
  AND ind.industry_code NOT IN ('AS','AR','AB','CR')
GROUP BY ind.industry_code
ORDER BY ind.industry_code;
-- 期待: 各 industry_code で cities=1719, occupations_per_industry=11, weight_total=1.0000
EOF
```

**判定基準**:
- §4.1 の差分が `T, X` のみ → OK (T/X は重み側のみ、F2 計算で fallback ロジック適用)
- §4.2 で `hypothesis_v1` のみ → 「**検証済み推定 β**」表示で運用
- §4.3 で全産業 weight_total = 1.0000 → F2 加重和の正規化条件を満たす

---

## 5. 既存ファイルへの影響

| ファイル | 影響 | 対応 |
|---------|------|------|
| `scripts/upload_new_external_to_turso.py` | `v2_external_industry_structure` の DDL は既存 (L128-141)、CSV マッピングも既存 (L340)。本タスクで**変更なし**。Phase 3 投入はローカル DB 用のため、既存の Turso アップロードロジックは無関係 | 変更不要 |
| `scripts/upload_new_external_to_turso.py` の `TABLE_SCHEMAS` | `occupation_industry_weight` は未定義。将来 Turso にも投入する場合は追加必要だが、本書範囲外 | 変更不要 (現状ローカルのみ) |
| `data/hellowork.db` | 両テーブルが新規追加される | ユーザー手動投入で変更 |
| `scripts/build_municipality_target_thickness.py` (将来作成) | 両テーブルを参照する。`load_occupation_industry_weight()` で `weight_source` 切替ロジックを実装 | 後続タスク (Worker B2) |
| `scripts/proto_evaluate_occupation_population_models.py` (既存プロト) | 現状は CSV 直読みでローカル DB 不要。本実装移行後は DB 経由に切替予定 | 後続タスク |
| `docs/SURVEY_MARKET_INTELLIGENCE_PHASE3_F2_IMPLEMENTATION_PLAN.md` (§1.2.1) | スキーマ案が CSV カラムと不一致。本書の DDL (実 CSV 整合版) を採用すべき | 計画書の §1.2.1 は本書 §1.2 の DDL に置換推奨 (今回は変更しない) |
| `docs/SURVEY_MARKET_INTELLIGENCE_PHASE3_F2_IMPLEMENTATION_PLAN.md` (§1.2.3) | カラム名 `jsic_code`/`notes`、PK 設計が CSV と差異。本書 §2.2 を採用 | 計画書の §1.2.3 は本書 §2.2 に置換推奨 (今回は変更しない) |

**禁止事項の遵守**:
- Claude による DB 書き込み禁止 → 本書はあくまで投入手順書、実投入はユーザーが手動実行
- 既存ファイル変更禁止 → 本書のみ作成
- スクリプト新規作成禁止 → 投入は sqlite3 CLI / Python ワンライナーで完結 (新規 .py ファイル不要)
- `.env` open 禁止 → ローカル DB 投入なので無関係 (Turso 接続不要)

---

## 6. 投入順序の依存関係

```
Phase 3 (v2_external_industry_structure)
   │
   │     [独立、並列実行可]
   │
Phase 4 (occupation_industry_weight)
   │
   ▼
Phase 5 (build_municipality_target_thickness.py 実行)
   │   - Phase 3 + Phase 4 両方が DB に存在することが前提
   │   - F2 計算 → CSV 出力 → ローカル DB 投入 (v2_municipality_target_thickness)
   │
   ▼
Phase 6 (Turso 投入: ユーザー手動、別タスク)
   │   - Turso 無料枠リセット日 (毎月 1 日) を待つ運用
   │   - 1 回で完了 (DROP+CREATE → INSERT) を厳守 (重大事故記録 2026-01-06 参照)
```

**並列性**:
- Phase 3 と Phase 4 は**完全独立**。同時に実行可能 (ファイルロック競合なし、SQLite WAL モードなら同時 INSERT 可)
- ただし sqlite3 CLI は同時に同じ DB ファイルを開かない方が安全 → 順次 (Phase 3 → Phase 4) 推奨

**Phase 5 への依存**:
- Phase 5 (`build_municipality_target_thickness.py`) は両テーブルが揃ってから実行
- Phase 5 内で本書 §4 の整合性検証 SQL を pre-condition として実行することを推奨

**ロールバック手順**:

```bash
# 万一不整合が発生した場合の戻し方 (Phase 3, 4 のみ)
sqlite3 data/hellowork.db <<'EOF'
DROP TABLE IF EXISTS v2_external_industry_structure;
DROP TABLE IF EXISTS occupation_industry_weight;
EOF
# CSV は無傷なので、本書の Step 2-3 を再実行すれば復元可能
```

---

## 7. ユーザー手動実行が必要な工程数

| 工程 | コマンド数 | 備考 |
|------|:---------:|------|
| Phase 3 DDL 適用 | 1 | sqlite3 < ddl.sql |
| Phase 3 CSV 投入 | 1 | sqlite3 .import or Python ワンライナー |
| Phase 3 検証 SQL | 1 | sqlite3 <<EOF ... EOF |
| Phase 4 DDL 適用 | 1 | sqlite3 < ddl.sql |
| Phase 4 CSV 投入 | 1 | sqlite3 .import or Python ワンライナー |
| Phase 4 検証 SQL | 1 | sqlite3 <<EOF ... EOF |
| 統合整合性検証 (§4) | 1 | sqlite3 <<EOF ... EOF |
| **合計** | **7 コマンド** | 所要時間目安: 約 5〜10 分 |

**最短実行コマンド列** (コピペ用):

```bash
cd C:/Users/fuji1/AppData/Local/Temp/hellowork-deploy

# Phase 3
python -c "
import csv, sqlite3
with sqlite3.connect('data/hellowork.db') as conn, \
     open('scripts/data/industry_structure_by_municipality.csv', encoding='utf-8-sig') as f:
    conn.execute('''CREATE TABLE IF NOT EXISTS v2_external_industry_structure (
        prefecture_code TEXT NOT NULL, city_code TEXT NOT NULL, city_name TEXT,
        industry_code TEXT NOT NULL, industry_name TEXT,
        establishments INTEGER, employees_total INTEGER,
        employees_male INTEGER, employees_female INTEGER,
        PRIMARY KEY (city_code, industry_code))''')
    conn.execute('CREATE INDEX IF NOT EXISTS idx_v2_external_industry_structure_prefecture ON v2_external_industry_structure (prefecture_code)')
    conn.execute('CREATE INDEX IF NOT EXISTS idx_v2_external_industry_structure_industry   ON v2_external_industry_structure (industry_code)')
    rows = list(csv.DictReader(f))
    conn.executemany('''INSERT OR REPLACE INTO v2_external_industry_structure
        (prefecture_code, city_code, city_name, industry_code, industry_name,
         establishments, employees_total, employees_male, employees_female)
        VALUES (?, ?, ?, ?, ?, ?, ?, ?, ?)''',
        [(r['prefecture_code'], r['city_code'], r['city_name'],
          r['industry_code'], r['industry_name'],
          int(r['establishments'])   if r['establishments']   else None,
          int(r['employees_total'])  if r['employees_total']  else None,
          int(r['employees_male'])   if r['employees_male']   else None,
          int(r['employees_female']) if r['employees_female'] else None) for r in rows])
    conn.commit()
    print(f'Phase 3: inserted {len(rows)} rows')
"

# Phase 4
python -c "
import csv, sqlite3
with sqlite3.connect('data/hellowork.db') as conn, \
     open('data/generated/occupation_industry_weight.csv', encoding='utf-8') as f:
    conn.execute('''CREATE TABLE IF NOT EXISTS occupation_industry_weight (
        industry_code TEXT NOT NULL, industry_name TEXT NOT NULL,
        occupation_code TEXT NOT NULL, occupation_name TEXT NOT NULL,
        weight REAL NOT NULL CHECK (weight >= 0.0 AND weight <= 1.0),
        weight_source TEXT NOT NULL DEFAULT 'hypothesis_v1',
        note TEXT, created_at TEXT NOT NULL DEFAULT (datetime('now')),
        PRIMARY KEY (industry_code, occupation_code, weight_source))''')
    conn.execute('CREATE INDEX IF NOT EXISTS idx_occ_industry_weight_industry   ON occupation_industry_weight (industry_code)')
    conn.execute('CREATE INDEX IF NOT EXISTS idx_occ_industry_weight_occupation ON occupation_industry_weight (occupation_code)')
    conn.execute('CREATE INDEX IF NOT EXISTS idx_occ_industry_weight_source     ON occupation_industry_weight (weight_source)')
    rows = list(csv.DictReader(f))
    assert all(r['source']=='hypothesis_v1' for r in rows), 'hypothesis_v1 以外検出'
    assert len(rows) == 231, f'expected 231 rows, got {len(rows)}'
    conn.executemany('''INSERT OR REPLACE INTO occupation_industry_weight
        (industry_code, industry_name, occupation_code, occupation_name,
         weight, weight_source, note) VALUES (?, ?, ?, ?, ?, ?, ?)''',
        [(r['industry_code'], r['industry_name'],
          r['occupation_code'], r['occupation_name'],
          float(r['weight']), r['source'], r.get('note', '')) for r in rows])
    conn.commit()
    print(f'Phase 4: inserted {len(rows)} rows (weight_source=hypothesis_v1)')
"

# 検証 (本書 §1.4, §2.4, §4 の SQL を実行)
sqlite3 data/hellowork.db <<'EOF'
SELECT 'industry_structure', COUNT(*) FROM v2_external_industry_structure;
SELECT 'occupation_weight',  COUNT(*) FROM occupation_industry_weight;
SELECT industry_code, ROUND(SUM(weight), 4) FROM occupation_industry_weight
GROUP BY industry_code;
EOF
```

このように **2 つの Python ワンライナー + 1 つの sqlite3 検証** = **3 コマンド**で Phase 3, 4 の投入と最低限の検証が完結する。詳細検証は本書 §1.4, §2.4, §4 を順次実行する。

---

## 8. ボトルネック / リスク

### 8.1 確認済み問題なし

| チェック項目 | 状態 |
|------------|:----:|
| CSV 完成度 | ✅ Phase 3, 4 共に完成済 |
| データ品質 | ✅ 行数・一意性・カラム整合性すべて期待値内 |
| weight 合計検証 | ✅ 全 21 産業で 1.0000 |
| weight_source 一意性 | ✅ hypothesis_v1 のみ |
| ローカル DB 容量影響 | 🟢 36,330 行追加で約 3MB 増 (無視できる) |

### 8.2 留意点

| 項目 | リスク | 対応 |
|-----|-------|------|
| 計画書 §1.2.1 と実 CSV のカラム不一致 | 計画書 DDL を採用すると import エラー | 本書 §1.2 の **実 CSV 整合 DDL** を採用 (既存 `upload_new_external_to_turso.py` の DDL と一致) |
| 計画書 §1.2.3 のカラム名 (`jsic_code`/`notes`) と CSV の `industry_code`/`note` の差異 | スクリプト実装時に混乱 | 本書 §2.2 の DDL を Single Source of Truth として後続実装に共有 |
| BOM 付き CSV (industry_structure) | sqlite3 .import 単独だとヘッダーに BOM 残留 | Python ワンライナー (`encoding='utf-8-sig'`) を推奨 |
| Phase 3 CSV の集計値 (AS, AR, AB, CR) | F2 計算で重複加算する危険 | F2 実装時に `industry_code NOT IN ('AS','AR','AB','CR')` で除外 |
| 重み側 T, X が構造側に未存在 | F2 の T (公務) / X (分類不能) は構造側に行がない | Worker B2 で T/X 用 fallback ロジックを実装 (例: 構造側 'AR' から派生) |
| Turso 投入は本書範囲外 | 重大事故記録 (2026-01-06, $195) | Phase 5 完成後、別タスクで Turso 投入 (1 回のみ実行を厳守) |

---

## 9. 次タスクへの引き継ぎ事項

| 引き継ぎ先 | 内容 |
|-----------|------|
| Worker B2 (`build_municipality_target_thickness.py`) | 本書 §3.3 の `load_occupation_industry_weight()` 設計を採用。§4 の整合性検証 SQL を pre-condition として実装内に組み込む |
| Worker A2 (テーブル名確定) | 最終ターゲット厚みテーブル名 `v2_municipality_target_thickness` を採用 (本書 §0 サマリー参照) |
| Worker C2 (UI 表示) | 本書 §3.2 の **「検証済み推定 β」/「検証済み推定」の自動切替**ルールを UI 実装に反映 |
| 計画書改訂 (`SURVEY_MARKET_INTELLIGENCE_PHASE3_F2_IMPLEMENTATION_PLAN.md`) | §1.2.1, §1.2.3 のスキーマ案を本書 §1.2, §2.2 の DDL に**置換推奨** (本タスク範囲外、別タスクで実施) |

---

**本書 1 ファイルのみ作成完了**
**ファイルパス**: `C:/Users/fuji1/AppData/Local/Temp/hellowork-deploy/docs/SURVEY_MARKET_INTELLIGENCE_PHASE3_INGEST_READINESS_CHECK.md`
