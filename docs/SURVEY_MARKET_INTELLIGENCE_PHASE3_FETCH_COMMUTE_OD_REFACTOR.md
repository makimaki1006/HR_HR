# Phase 3 JIS 整備: `fetch_commute_od.py` 改修案 (Worker A)

作成日: 2026-05-04
対象: `scripts/fetch_commute_od.py` を最小改修して e-Stat 5 桁 cdArea を保持する

**ステータス: 設計提示のみ (実装未着手、e-Stat 再 fetch 未実行)**

---

## 1. 現状コード分析 (Read 結果)

### 1.1 5 桁コードを取得しているが破棄している箇所

`scripts/fetch_commute_od.py` の以下の流れで JIS 5 桁コードが消える:

| 行 | 内容 | コード保持状態 |
|---|------|:------------:|
| L94-106 | `code_to_pref_muni(code, name_map)` 関数 | 5 桁 `code` を引数で受け取り、`pref_name` と `muni` 名のみ返す → **code 破棄** |
| L156-157 | `origin_code = v.get("@area", "")` / `dest_code = v.get("@cat02", "")` | API レスポンスから 5 桁取得済 |
| L174-175 | `origin_pref, origin_muni = code_to_pref_muni(origin_code, area_map)` | 名前のみに変換 → コード変数はローカルスコープのみ |
| L180-187 | `results.append({...})` dict | `origin_pref/muni` のみ含み、**`origin_code`/`dest_code` は含まれない** |
| L202-247 | `insert_data*()` SQL | 5 つのカラム名 (origin_pref, origin_muni, dest_pref, dest_muni, total/male/female) のみで INSERT |
| L46-58 | `CREATE TABLE v2_external_commute_od` DDL | `*_code` カラム自体が存在しない |

→ **5 桁コードは API レスポンスから取得済だが、最終的な保存層で完全に破棄されている**。

### 1.2 既存 DDL (L46-58)

```sql
CREATE TABLE IF NOT EXISTS v2_external_commute_od (
    origin_pref TEXT NOT NULL,
    origin_muni TEXT NOT NULL,
    dest_pref TEXT NOT NULL,
    dest_muni TEXT NOT NULL,
    total_commuters INTEGER NOT NULL,
    male_commuters INTEGER DEFAULT 0,
    female_commuters INTEGER DEFAULT 0,
    reference_year INTEGER DEFAULT 2020,
    PRIMARY KEY (origin_pref, origin_muni, dest_pref, dest_muni)
);
```

PK は名称ベース (4 列)、コード列なし。

### 1.3 既存挙動の確認 (L161-178)

- L161: `if origin_code.endswith("000") or dest_code.endswith("000"): continue`
  → **都道府県レベル (XX000) を除外**、市区町村は保持
- L163: `if origin_code == "00000" or dest_code == "00000": continue`
  → **全国集計 (00000) も除外**
- L177: `if not origin_muni or not dest_muni: continue`
  → muni 名が空なら除外

つまり、政令指定都市の区 (例: `01101` = 札幌市中央区) は `endswith("000")` を満たさないため **保持される**。

---

## 2. 5 つのレビューポイント検証

| # | レビューポイント | 現状 | 改修可能性 |
|--:|-----------------|------|----------|
| 1 | origin/destination 双方の cdArea が保持されるか | ❌ 両方破棄中 | ✅ 改修可能 (L94-106 / L180 / L46 / L202 を 4 段で改修) |
| 2 | 政令市の区レベルコードが落ちないか | ✅ 既存ロジックで保持 (L161 で `endswith("000")` のみ除外) | ✅ 改修後もそのまま維持 |
| 3 | 既存の名称ベースカラムを壊さないか | ✅ origin_pref / origin_muni / dest_pref / dest_muni はそのまま | ✅ 名称カラム削除しない |
| 4 | 既存 v2_external_commute_od の互換維持 vs 別テーブル | 既存 PK は名称ベース | **推奨**: 別テーブル `v2_external_commute_od_with_codes` 方式 (詳細 §3.1) |
| 5 | 再 fetch 失敗時に現行データへ戻れるか | DELETE → INSERT で全置換 (L211, L226-) | **推奨**: 別テーブル方式なら現行 `v2_external_commute_od` は無傷 |

---

## 3. 推奨改修方針: **別テーブル方式 (v2_external_commute_od_with_codes)**

### 3.1 採用理由

ユーザー指示「既存テーブル破壊より新テーブル v2_external_commute_od_with_codes または派生CSV の方が安全」と整合。

| 項目 | 既存テーブル拡張 (ALTER) | **別テーブル新設 (推奨)** |
|------|-------------------------|------------------------|
| 既存 Rust ハンドラ (table_exists 経由) への影響 | NULL 列追加で互換は保つが SELECT * 系で挙動微妙化 | **影響ゼロ** |
| 再 fetch 失敗時のロールバック | 旧 ALTER 状態を残し DELETE で空に | **既存テーブル無傷、新テーブルだけ DROP** |
| 既存 PK 制約 (名称ベース) との衝突 | PK 維持 (code は補助列) | **新 PK で明確 (code ベース)** |
| Phase 5 後続テーブル (recruiting_scores 等) との JOIN | 既存テーブルに pseudo-code が混じる懸念 | **新テーブルだけ JIS 完全版として運用** |
| 検証時の比較 | 旧 vs 新 テーブル同居 | 同上 |

### 3.2 新 DDL 案

```sql
-- 新テーブル: code 主キー
CREATE TABLE IF NOT EXISTS v2_external_commute_od_with_codes (
    origin_municipality_code TEXT NOT NULL,  -- JIS 5 桁
    dest_municipality_code TEXT NOT NULL,    -- JIS 5 桁
    -- 名称は表示用 (JOIN は code で実施)
    origin_prefecture TEXT NOT NULL,
    origin_municipality_name TEXT NOT NULL,
    dest_prefecture TEXT NOT NULL,
    dest_municipality_name TEXT NOT NULL,
    -- 数値
    total_commuters INTEGER NOT NULL,
    male_commuters INTEGER DEFAULT 0,
    female_commuters INTEGER DEFAULT 0,
    reference_year INTEGER DEFAULT 2020,
    -- メタ
    source TEXT NOT NULL DEFAULT 'estat_0003454527',
    created_at TEXT NOT NULL DEFAULT CURRENT_TIMESTAMP,
    PRIMARY KEY (origin_municipality_code, dest_municipality_code, reference_year)
);

CREATE INDEX IF NOT EXISTS idx_cod_codes_origin
ON v2_external_commute_od_with_codes (origin_municipality_code);

CREATE INDEX IF NOT EXISTS idx_cod_codes_dest
ON v2_external_commute_od_with_codes (dest_municipality_code);

CREATE INDEX IF NOT EXISTS idx_cod_codes_pref
ON v2_external_commute_od_with_codes (origin_prefecture, dest_prefecture);
```

PK が `(origin_code, dest_code, reference_year)` で **名称完全独立**。

---

## 4. 最小改修 diff 案

### 4.1 改修箇所一覧

| # | 行 | 内容 | 変更タイプ |
|--:|----|------|-----------|
| (a) | L46-58 | DDL 追加 (新テーブル) | 追加 |
| (b) | L94-106 | `code_to_pref_muni` 互換維持 + 新関数 `code_to_pref_muni_with_code` | 追加 |
| (c) | L155-187 | results dict に `origin_code`/`dest_code` 含める | 修正 |
| (d) | L202-248 | 新 `insert_data_with_codes()` 関数追加 (既存 `insert_data` は未変更) | 追加 |
| (e) | `main()` | 既存パス + 新パス 両方を呼び出すフラグ追加 | 修正 |

### 4.2 diff 案 (代表例、実装は別 PR)

#### (a) L43-71 の `create_tables` 関数を拡張

```diff
 def create_tables(conn):
-    """通勤ODテーブルと集約テーブルを作成"""
+    """通勤ODテーブルと集約テーブルを作成 (JIS code 版含む)"""
     conn.executescript("""
         CREATE TABLE IF NOT EXISTS v2_external_commute_od (
             origin_pref TEXT NOT NULL,
             origin_muni TEXT NOT NULL,
             ...
             PRIMARY KEY (origin_pref, origin_muni, dest_pref, dest_muni)
         );
+        -- Phase 3 JIS 整備: code 保持版 (新テーブル、既存と並行運用)
+        CREATE TABLE IF NOT EXISTS v2_external_commute_od_with_codes (
+            origin_municipality_code TEXT NOT NULL,
+            dest_municipality_code TEXT NOT NULL,
+            origin_prefecture TEXT NOT NULL,
+            origin_municipality_name TEXT NOT NULL,
+            dest_prefecture TEXT NOT NULL,
+            dest_municipality_name TEXT NOT NULL,
+            total_commuters INTEGER NOT NULL,
+            male_commuters INTEGER DEFAULT 0,
+            female_commuters INTEGER DEFAULT 0,
+            reference_year INTEGER DEFAULT 2020,
+            source TEXT NOT NULL DEFAULT 'estat_0003454527',
+            created_at TEXT NOT NULL DEFAULT CURRENT_TIMESTAMP,
+            PRIMARY KEY (origin_municipality_code, dest_municipality_code, reference_year)
+        );
+        CREATE INDEX IF NOT EXISTS idx_cod_codes_origin
+        ON v2_external_commute_od_with_codes (origin_municipality_code);
+        CREATE INDEX IF NOT EXISTS idx_cod_codes_dest
+        ON v2_external_commute_od_with_codes (dest_municipality_code);
         ...
     """)
```

#### (c) L155-187: results dict 拡張

```diff
                 origin_pref, origin_muni = code_to_pref_muni(origin_code, area_map)
                 dest_pref, dest_muni = code_to_pref_muni(dest_code, cat02_map)

                 if not origin_muni or not dest_muni:
                     continue

                 results.append({
                     "origin_pref": origin_pref,
                     "origin_muni": origin_muni,
+                    "origin_code": origin_code,  # JIS 5 桁
                     "dest_pref": dest_pref,
                     "dest_muni": dest_muni,
+                    "dest_code": dest_code,      # JIS 5 桁
                     "sex": sex_label,
                     "count": count,
                 })
```

`code_to_pref_muni()` は **既存呼び出し側を壊さないため変更しない** (戻り値 2 タプルのまま)。`origin_code`/`dest_code` は呼び出し元で API レスポンスから別途取得済 (L156-157) なので、そのまま結果 dict に追加するだけ。

#### (d) 新 `insert_data_with_codes()` 関数追加

```python
def insert_data_with_codes(conn, all_data):
    """JIS code 保持版 (新テーブル v2_external_commute_od_with_codes へ投入)"""
    merged = {}
    for row in all_data:
        # PK は code ベース
        key = (row["origin_code"], row["dest_code"])
        if key not in merged:
            merged[key] = {
                "origin_code": row["origin_code"],
                "origin_pref": row["origin_pref"],
                "origin_muni": row["origin_muni"],
                "dest_code": row["dest_code"],
                "dest_pref": row["dest_pref"],
                "dest_muni": row["dest_muni"],
                "total": 0, "male": 0, "female": 0,
            }
        merged[key][row["sex"]] = row["count"]

    conn.execute("DELETE FROM v2_external_commute_od_with_codes")
    inserted = 0
    for d in merged.values():
        total = d["total"] if d["total"] > 0 else d["male"] + d["female"]
        if total < MIN_COMMUTERS:
            continue
        conn.execute(
            "INSERT OR REPLACE INTO v2_external_commute_od_with_codes "
            "(origin_municipality_code, dest_municipality_code, "
            " origin_prefecture, origin_municipality_name, "
            " dest_prefecture, dest_municipality_name, "
            " total_commuters, male_commuters, female_commuters, reference_year) "
            "VALUES (?, ?, ?, ?, ?, ?, ?, ?, ?, 2020)",
            (d["origin_code"], d["dest_code"],
             d["origin_pref"], d["origin_muni"],
             d["dest_pref"], d["dest_muni"],
             total, d["male"], d["female"])
        )
        inserted += 1
    conn.commit()
    return inserted
```

#### (e) main 拡張 (CLI フラグで切替)

```python
parser.add_argument("--with-codes", action="store_true",
    help="JIS 5 桁 code 保持版 (v2_external_commute_od_with_codes) も同時投入")

# fetch ループ後
if args.with_codes:
    inserted = insert_data_with_codes(conn, all_data)
    print(f"  v2_external_commute_od_with_codes: {inserted} 行投入完了")
```

---

## 5. 実行戦略 (実装後)

### 5.1 段階 1: 並行投入 (推奨)

既存 fetch でコード取得済データを **メモリ上で展開した時点で 2 テーブルに同時投入**。e-Stat 再 fetch は 1 回で完了。

```bash
# 改修後
python scripts/fetch_commute_od.py --with-codes
# → v2_external_commute_od (既存 PK) + v2_external_commute_od_with_codes (code PK) 両方更新
```

### 5.2 段階 2: 検証

```sql
-- 1. 行数比較 (新テーブルが既存以上)
SELECT
    (SELECT COUNT(*) FROM v2_external_commute_od) AS old_count,
    (SELECT COUNT(*) FROM v2_external_commute_od_with_codes) AS new_count;
-- 期待: new_count >= old_count (新テーブルは year 別に複数行可、既存は (origin, dest) 単位)

-- 2. JIS code 完全性
SELECT COUNT(*) FROM v2_external_commute_od_with_codes
WHERE origin_municipality_code IS NULL OR dest_municipality_code IS NULL;
-- 期待: 0

-- 3. 新テーブルから既存テーブルを再現可能 (名称が完全一致)
SELECT COUNT(*) FROM v2_external_commute_od AS old
LEFT JOIN v2_external_commute_od_with_codes AS new
  ON new.origin_prefecture = old.origin_pref
  AND new.origin_municipality_name = old.origin_muni
  AND new.dest_prefecture = old.dest_pref
  AND new.dest_municipality_name = old.dest_muni
WHERE new.origin_municipality_code IS NULL;
-- 期待: 0 件 (既存テーブル全行が新テーブルに対応する)
```

### 5.3 段階 3: 失敗時ロールバック

```sql
-- 新テーブルだけ DROP、既存テーブルは無傷
DROP TABLE IF EXISTS v2_external_commute_od_with_codes;
```

→ 既存 `v2_external_commute_od` (Turso 反映済 83,402 行) には**一切影響なし**。

---

## 6. 推定実装時間

| 段階 | 作業 | 時間 |
|:----:|------|:----:|
| (a) DDL 追加 | `create_tables()` 拡張 | 30 分 |
| (b)-(c) results dict 拡張 | code 取得済を活用 | 30 分 |
| (d) `insert_data_with_codes()` 新規 | 上記コードベース | 30 分 |
| (e) CLI フラグ + main 拡張 | argparse 追加 | 15 分 |
| 単体テスト (mock データで動作確認) | | 30 分 |
| **小計 (実装)** | | **約 2 時間** |
| e-Stat 再 fetch 実行 (ユーザー手動) | レート制限あり | **1〜2 時間** |
| 検証 SQL 実行 | §5.2 | 30 分 |
| **合計** | | **約 3.5〜4.5 時間** |

---

## 7. 制約と禁止事項遵守

| 項目 | 状態 |
|------|:---:|
| e-Stat 再 fetch 実行 | ❌ 設計のみ、本書では実行しない |
| Turso upload | ❌ |
| `.env` / token 読み | ❌ 本書では参照不要 |
| DB 本体への書き込み | ❌ DDL/INSERT は実装後にユーザー手動 |
| Rust 実装 | ❌ Python スクリプトのみ |
| push | ❌ |

---

## 8. 完了条件 (本書の)

- [x] 現状コード分析 (5 桁コード破棄箇所 6 行特定)
- [x] 5 つのレビューポイント検証
- [x] 推奨方針 = 別テーブル `v2_external_commute_od_with_codes`
- [x] 最小改修 diff 案 (5 箇所)
- [x] 実行戦略 (並行投入 + 検証 + ロールバック)
- [x] 推定実装時間 (実装 2h + fetch 1〜2h + 検証 30m = 約 4h)

---

## 9. 関連 docs

- マスタ DDL: `SURVEY_MARKET_INTELLIGENCE_PHASE3_MUNICIPALITY_CODE_MASTER.md` (Worker B)
- 移行設計: `SURVEY_MARKET_INTELLIGENCE_PHASE3_BUILD_COMMUTE_FLOW_JIS_MIGRATION.md` (Worker C)
- 全体計画: `SURVEY_MARKET_INTELLIGENCE_PHASE3_JIS_CODE_PLAN.md` (第一候補の詳細実装案)
- 上流 DDL 参照元: `survey_market_intelligence_phase0_2_schema.sql`
