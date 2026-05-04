# Phase 3: JIS 市区町村コード整備設計書

作成日: 2026-05-04
対象: Phase 3 Step 5 で使う `municipality_code` の JIS 5 桁化

---

## 1. 背景

### 1.1 現状の問題

Phase 3 Step A (`commute_flow_summary` 生成) で `municipality_code` を `prefecture:municipality_name` 形式の擬似コードで運用している (例: `"北海道:札幌市"`)。これは PK 一意性は確保するが、**JIS 5 桁市区町村コード (例: `"01101"` = 札幌市) ではない** ため、以下が不可能:

- `municipality_recruiting_scores` / `municipality_occupation_population` (Step 5 で投入予定の他 3 テーブル) との **コード JOIN**
- 既存 e-Stat / 国交省データとの **連結**
- 外部システム (SalesNow 等) の **コード突合**

### 1.2 探索結果 (2026-05-04 Worker B 実施)

ローカル `data/hellowork.db` 全 46 テーブル + `scripts/` + `data/*.csv` を調査した結果、JIS マスタは **完全に存在しない**:

| 場所 | 結果 |
|------|------|
| `municipality_geocode` | id は単なる連番 (1〜2,626)、JIS ではない |
| 全 `v2_external_*` テーブル | コードカラムなし、`prefecture` + `municipality` TEXT のみ |
| `salesnow_companies.csv` | jccode (SalesNow 独自)、JIS ではない |
| `scripts/fetch_commute_od.py` | e-Stat から **5 桁コード (cdArea) を一旦取得しているが**、DB 保存時に TEXT (pref/muni 名) に変換し **コード値を破棄している** |

→ **新規整備が必要**。

---

## 2. 整備候補

### 2.1 第一候補 (推奨): `fetch_commute_od.py` の cdArea 保持改修

#### 概要
既存 `scripts/fetch_commute_od.py` で **既に取得しているが破棄している 5 桁コード** を保持するように改修する。

#### 利点
- **新規外部 fetch 不要** (既存 e-Stat 通信を再利用)
- 取得済データの再走査だけ (CSV / API 再取得不要、低コスト)
- e-Stat の cdArea = 公式 JIS 5 桁コード = 信頼性高
- 都道府県 + 市区町村だけでなく **政令指定都市の区** (例: `"01101"` = 札幌市中央区) もカバー
- 改修サイズ小 (関数 1〜2 個 + DDL 拡張 + 再投入)

#### 設計

##### Step 1: 既存スクリプトの改修
**対象**: `scripts/fetch_commute_od.py`

```python
# Before (line 94-106 あたり、Worker B 報告から推定)
def code_to_pref_muni(code: str) -> tuple[str, str]:
    pref_code = code[:2]
    muni_code = code[2:5]
    return pref_name_lookup(pref_code), muni_name_lookup(pref_code, muni_code)

# 戻り値だけ拡張
def code_to_pref_muni_with_code(code: str) -> tuple[str, str, str]:
    """5 桁 JIS コードと pref/muni 名を返す"""
    pref_code = code[:2]
    muni_code = code[2:5]
    pref_name = pref_name_lookup(pref_code)
    muni_name = muni_name_lookup(pref_code, muni_code)
    return pref_name, muni_name, code  # ← code を返り値に含める
```

##### Step 2: DDL 拡張
**対象**: `v2_external_commute_od` テーブル

```sql
-- 追加カラム (NULL 許容で後方互換)
ALTER TABLE v2_external_commute_od ADD COLUMN origin_municipality_code TEXT;
ALTER TABLE v2_external_commute_od ADD COLUMN dest_municipality_code TEXT;

CREATE INDEX IF NOT EXISTS idx_commute_od_origin_code
ON v2_external_commute_od (origin_municipality_code);

CREATE INDEX IF NOT EXISTS idx_commute_od_dest_code
ON v2_external_commute_od (dest_municipality_code);
```

##### Step 3: 既存データへの code 充填
2 つの選択肢:

**A) e-Stat 再 fetch (完全)**
- 改修済み `fetch_commute_od.py --refetch` で 83,402 行を再取得
- e-Stat レート制限: 1 秒/req → 約 1〜2 時間
- データ更新の機会も兼ねられる

**B) 既存テーブルから pref+muni → code 逆引き (部分)**
- `fetch_commute_od.py` の cdArea マッピング辞書をエクスポートして CSV 化
- `UPDATE v2_external_commute_od SET origin_municipality_code = ? WHERE origin_pref = ? AND origin_muni = ?` で逆引き
- 高速だがコード辞書が必要

**推奨**: **A) 再 fetch**。e-Stat 公式と完全整合し、データ品質も担保。

##### Step 4: マスタテーブル `municipality_code_master` の派生
```sql
CREATE TABLE municipality_code_master (
    municipality_code TEXT PRIMARY KEY,
    prefecture TEXT NOT NULL,
    municipality_name TEXT NOT NULL,
    pref_code TEXT NOT NULL,  -- 上位 2 桁
    is_special_ward INTEGER DEFAULT 0,  -- 特別区フラグ
    source TEXT NOT NULL,  -- 'e-stat' | 'mlit-n03' | 'manual'
    source_year INTEGER NOT NULL,
    created_at TEXT NOT NULL DEFAULT CURRENT_TIMESTAMP
);

-- v2_external_commute_od 完了後に派生
INSERT OR IGNORE INTO municipality_code_master (municipality_code, prefecture, municipality_name, pref_code, source, source_year)
SELECT DISTINCT origin_municipality_code, origin_pref, origin_muni, SUBSTR(origin_municipality_code, 1, 2), 'e-stat', 2020
FROM v2_external_commute_od
WHERE origin_municipality_code IS NOT NULL
UNION
SELECT DISTINCT dest_municipality_code, dest_pref, dest_muni, SUBSTR(dest_municipality_code, 1, 2), 'e-stat', 2020
FROM v2_external_commute_od
WHERE dest_municipality_code IS NOT NULL;
```

##### Step 5: `commute_flow_summary` の擬似コード置換
```sql
-- 擬似コード "北海道:札幌市" → JIS "01100" 等を逆引き UPDATE
UPDATE commute_flow_summary
SET destination_municipality_code = (
    SELECT mcm.municipality_code FROM municipality_code_master mcm
    WHERE mcm.prefecture = commute_flow_summary.destination_prefecture
      AND mcm.municipality_name = commute_flow_summary.destination_municipality_name
);
-- origin 側も同様
```

#### 推定実装時間
- スクリプト改修: 2〜3 時間
- DDL ALTER + INDEX: 30 分
- e-Stat 再 fetch 実行: 1〜2 時間 (レート制限)
- マスタ派生 + 既存テーブル UPDATE: 30 分
- 検証: 1 時間
- **合計: 5〜7 時間**

#### リスク
- e-Stat レート制限超過 → 翌日待機必要
- e-Stat の cdArea が一部欠損 (合併済み旧自治体等) → manual 補完必要
- 既存テーブル名カラム (origin_pref / origin_muni) との二重保持で容量増 (約 20% 増加)

---

### 2.2 第二候補: 国土数値情報 N03 (行政区域) GeoJSON 抽出

#### 概要
国交省 国土数値情報 **N03 行政区域** GeoJSON を取得し、JIS コード対応表 CSV を抽出してマスタ化。

#### 利点
- 公式マスタ (国交省、年次更新)
- 一括取得 (複数 API 呼び出し不要)
- ポリゴン情報も含むため将来の地図系機能に流用可能
- e-Stat の cdArea 体系と JIS は同じ

#### 設計

##### Step 1: GeoJSON 取得
**対象**: 国交省 国土数値情報 N03-2025 (令和 7 年版)
- URL: `https://nlftp.mlit.go.jp/ksj/gml/data/N03/N03-2025/N03-2025_GML.zip`
- サイズ: 約 100 MB (ZIP)
- 抽出後 GeoJSON: 約 800 MB (全国合計)

```bash
# scripts/fetch_municipality_master_n03.py (新規)
python scripts/fetch_municipality_master_n03.py
# → data/jis_municipality_master.csv (約 1,800 行)
```

##### Step 2: CSV 抽出
GeoJSON の properties 抽出:

```python
import json
import csv

with open('N03-2025.geojson', encoding='utf-8') as f:
    data = json.load(f)

with open('data/jis_municipality_master.csv', 'w', encoding='utf-8', newline='') as f:
    writer = csv.writer(f)
    writer.writerow(['municipality_code', 'prefecture', 'municipality_name', 'pref_code'])
    seen = set()
    for feat in data['features']:
        p = feat['properties']
        code = p.get('N03_007')  # 全国地方公共団体コード (5桁)
        pref = p.get('N03_001')   # 都道府県名
        muni = p.get('N03_004')   # 市区町村名
        if code and code not in seen:
            seen.add(code)
            writer.writerow([code, pref, muni, code[:2]])
```

##### Step 3: DB 投入
```sql
CREATE TABLE municipality_code_master (
    municipality_code TEXT PRIMARY KEY,
    prefecture TEXT NOT NULL,
    municipality_name TEXT NOT NULL,
    pref_code TEXT NOT NULL,
    source TEXT NOT NULL DEFAULT 'mlit-n03',
    source_year INTEGER NOT NULL DEFAULT 2025,
    created_at TEXT NOT NULL DEFAULT CURRENT_TIMESTAMP
);

-- import_external_csv.py (既存) で投入可
```

##### Step 4: 既存テーブル UPDATE
第一候補 Step 5 と同じ。

#### 推定実装時間
- ZIP ダウンロード: 30 分 (回線次第)
- スクリプト実装 + 抽出: 2〜3 時間
- DB 投入 + 検証: 1 時間
- 既存テーブル UPDATE: 30 分
- **合計: 4〜5 時間**

#### リスク
- ZIP 100 MB / 解凍 800 MB → 一時ストレージ消費大
- N03 の `N03_007` (5 桁コード) が一部 NULL の自治体あり (ポリゴン分割の都合) → 集約必要
- ポリゴン座標を捨てるため、地図系で再利用するなら別途保存

---

### 2.3 候補比較表

| 項目 | 第一候補 (cdArea 保持) | 第二候補 (N03 GeoJSON) |
|------|----------------------|---------------------|
| 公式性 | ✅ e-Stat 公式 | ✅ 国交省公式 |
| 既存資産流用 | ✅ 既存スクリプト改修のみ | ❌ 新規 fetch + parse |
| 一時ストレージ | 小 (API レスポンスのみ) | 大 (ZIP 100MB + GeoJSON 800MB) |
| 実装時間 | 5〜7 時間 | 4〜5 時間 |
| データ年次性 | 既存の 2020 国勢調査ベース | 2025 行政区域 (最新) |
| 政令市の区対応 | ✅ cdArea で取得 | △ N03 では市単位 (区は別扱い) |
| ポリゴン併用可能 | ❌ 名称のみ | ✅ 将来の地図系流用可 |
| 既存パイプラインとの整合 | ✅ 既存 fetch 仕様内 | △ 新規スクリプト要追加 |

---

## 3. 推奨選定

### 3.1 第一候補を推奨

**理由**:
1. e-Stat 通信は既に運用中 (`fetch_commute_od.py`) で実績あり
2. **政令指定都市の区 (札幌市中央区等) 対応** が必要 (Step 5 の occupation_population で要件)
3. 既存パイプラインへの破壊変更がない (NULL 許容カラム追加のみ)
4. データ年次は既存 commute_od (2020) と一致 → JOIN 整合性確保

### 3.2 第二候補は補助で運用 (将来)

- 第一候補で取れない自治体があった場合の **fallback マスタ**
- 将来の地図系機能 (タイルマップ / Sankey) でポリゴンを使う際の **ジオメトリ源**

---

## 4. 実装フェーズ計画 (第一候補ベース)

| Phase | 作業 | 期間 | 担当 |
|:-----:|------|:----:|------|
| **P1** | `fetch_commute_od.py` 改修 (cdArea 保持) | 2〜3h | 実装担当 |
| **P2** | `v2_external_commute_od` DDL ALTER + INDEX | 30m | ユーザー手動 (or 実装担当) |
| **P3** | e-Stat 再 fetch 実行 (`--refetch` モード) | 1〜2h | ユーザー手動 |
| **P4** | `municipality_code_master` テーブル新設 + 派生 | 30m | 実装担当 |
| **P5** | `commute_flow_summary` の `municipality_code` 置換 (UPDATE) | 30m | ユーザー手動 |
| **P6** | 検証 (整合性: pref+muni → code 一致) | 1h | 実装担当 |
| **P7** | Turso 反映 (`upload_to_turso.py` で commute_od + master + summary) | 30m | ユーザー手動 |
| **合計** | | **5〜7 時間** | |

---

## 5. 検証 SQL (P6 用)

```sql
-- 1. master 行数 (期待: 1,900 弱、特別区含めると 2,000 強)
SELECT COUNT(*) FROM municipality_code_master;

-- 2. master 一意性
SELECT COUNT(*) - COUNT(DISTINCT municipality_code) FROM municipality_code_master;
-- 期待: 0

-- 3. 47 都道府県 = 47
SELECT COUNT(DISTINCT pref_code) FROM municipality_code_master;
-- 期待: 47

-- 4. commute_od の code 充填率
SELECT
    SUM(CASE WHEN origin_municipality_code IS NOT NULL THEN 1 ELSE 0 END) * 100.0 / COUNT(*) AS origin_filled_pct,
    SUM(CASE WHEN dest_municipality_code IS NOT NULL THEN 1 ELSE 0 END) * 100.0 / COUNT(*) AS dest_filled_pct
FROM v2_external_commute_od;
-- 期待: 95% 以上

-- 5. 名前 → code の整合 (重複なし)
SELECT prefecture, municipality_name, COUNT(DISTINCT municipality_code) AS code_count
FROM municipality_code_master
GROUP BY prefecture, municipality_name
HAVING code_count > 1;
-- 期待: 0 件 (1 自治体 = 1 コード)

-- 6. commute_flow_summary 置換後の code 充填率
SELECT
    SUM(CASE WHEN destination_municipality_code LIKE '%:%' THEN 1 ELSE 0 END) AS pseudo_remaining,
    SUM(CASE WHEN destination_municipality_code NOT LIKE '%:%' THEN 1 ELSE 0 END) AS jis_filled
FROM commute_flow_summary;
-- 期待: pseudo_remaining = 0
```

---

## 6. 着手判断基準

### 6.1 着手すべきタイミング

- Step 5 の他 3 テーブル (occupation_population / living_cost_proxy / recruiting_scores) のいずれかを着手する **直前**
- なぜなら他 3 テーブルは JIS コード前提で設計されており、JOIN 不整合を防ぐため事前整備が必須

### 6.2 着手しなくてもよい場合

- `commute_flow_summary` 単体のまま運用 (通勤流入元セクションだけ表示する Phase 3 暫定モード)
- 配信地域ランキング / 母集団レンジ機能は不要

---

## 7. 禁止事項 (本書範囲)

| 項目 | 状態 |
|------|:---:|
| AI による fetch_commute_od.py 改修コミット | ❌ 設計書のみ |
| AI による Turso ALTER TABLE 実行 | ❌ ユーザー手動 |
| AI による N03 GeoJSON ダウンロード | ❌ 必要時にユーザー手動 |
| push | ❌ |

---

## 8. 完了条件 (本設計書の)

- [x] 第一候補 (cdArea 保持) の設計記載
- [x] 第二候補 (N03 GeoJSON) の設計記載
- [x] 候補比較表
- [x] 推奨選定とその根拠
- [x] 実装フェーズ計画 (P1〜P7)
- [x] 検証 SQL
- [x] 着手判断基準

本書をもって、JIS コード整備のオプションが固定された。実着手はユーザー判断。
