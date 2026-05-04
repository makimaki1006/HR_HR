# Phase 3 JIS 整備: `build_commute_flow_summary.py` JIS 移行設計 (Worker C)

作成日: 2026-05-04
対象: 既存 `scripts/build_commute_flow_summary.py` (擬似コード版) を JIS コード対応に移行

**ステータス: 設計提示のみ (実装未着手)**

---

## 1. 現状確認 (Read 結果)

### 1.1 擬似コード生成箇所

`scripts/build_commute_flow_summary.py` の中核 (line 95-100):

```python
def make_pseudo_code(prefecture: str, municipality: str) -> str:
    """JIS 市区町村コードのマスタ未整備のため、擬似コードを `prefecture:municipality_name` 形式で生成。"""
    return f"{prefecture}:{municipality}"
```

呼び出し箇所 (line 132-150 周辺の `build_summary_rows`):

```python
out.append({
    "destination_municipality_code": make_pseudo_code(dp, dm),  # ← 擬似コード
    "destination_prefecture": dp,
    "destination_municipality_name": dm,
    "origin_municipality_code": make_pseudo_code(op, om),       # ← 擬似コード
    "origin_prefecture": op,
    "origin_municipality_name": om,
    ...
})
```

→ destination/origin の **両方の `municipality_code`** が `prefecture:municipality_name` 形式の擬似値で生成されている。

### 1.2 入力テーブル

`v2_external_commute_od` (既存、名称ベース) のみ。JIS コードカラムは存在しない。

### 1.3 出力

| 出力先 | 状態 |
|--------|------|
| `data/generated/commute_flow_summary.csv` | 5.17 MB / 27,879 行 (gitignore 対象) |
| `data/hellowork.db::commute_flow_summary` | 27,879 行 (DROP+CREATE+INSERT) |

---

## 2. JIS 移行に必要な変更点

### 2.1 入力テーブル変更

| Before | After |
|--------|-------|
| `v2_external_commute_od` (名称のみ) | `v2_external_commute_od_with_codes` (Worker A 投入予定) |

### 2.2 擬似コード関数の置換

| Before | After |
|--------|-------|
| `make_pseudo_code(prefecture, municipality)` を呼ぶ | **JIS コードを入力テーブルから直接取得** (関数自体不要) |

### 2.3 必要な変更点 (5 箇所)

| # | 行 | 内容 | 変更タイプ |
|--:|----|------|-----------|
| (a) | L95-100 | `make_pseudo_code()` 関数定義 | **削除** (または互換維持で残してもよい) |
| (b) | L40 (定数) | `SOURCE_TABLE = "v2_external_commute_od"` | `"v2_external_commute_od_with_codes"` に変更 |
| (c) | L113-119 | `fetch_clean_commute_od()` の SELECT 文 | カラム名変更 (`origin_pref` → `origin_prefecture` 等)、`origin_code/dest_code` を SELECT に追加 |
| (d) | L130-150 | `build_summary_rows()` 内の dict 構築 | `make_pseudo_code(dp, dm)` → `dest_code` (DB から取得した値を直接使用) |
| (e) | L75-90 (DDL) | `commute_flow_summary` の DDL | 不変 (DDL は同じ、`municipality_code TEXT NOT NULL` は擬似でも JIS でも同じ) |

### 2.4 diff 案 (代表例)

#### (b)+(c): SELECT 文の変更

```diff
 def fetch_clean_commute_od(conn: sqlite3.Connection):
-    """v2_external_commute_od からヘッダー混入ガード適用 + self-loop 除外 で全レコード取得。"""
+    """v2_external_commute_od_with_codes (JIS 対応版) から取得。"""
     sql = """
-    SELECT origin_pref, origin_muni, dest_pref, dest_muni,
-           total_commuters, male_commuters, female_commuters, reference_year
-    FROM v2_external_commute_od
-    WHERE origin_pref IS NOT NULL AND origin_pref <> ''
-      AND origin_pref NOT IN ('都道府県', '出発地')
-      AND origin_muni IS NOT NULL AND origin_muni <> '' AND origin_muni <> '市区町村'
-      AND dest_pref IS NOT NULL AND dest_pref <> ''
-      AND dest_pref NOT IN ('都道府県', '到着地')
-      AND dest_muni IS NOT NULL AND dest_muni <> '' AND dest_muni <> '市区町村'
-      AND NOT (origin_pref = dest_pref AND origin_muni = dest_muni)
+    SELECT origin_prefecture, origin_municipality_name, origin_municipality_code,
+           dest_prefecture, dest_municipality_name, dest_municipality_code,
+           total_commuters, male_commuters, female_commuters, reference_year
+    FROM v2_external_commute_od_with_codes
+    WHERE origin_municipality_code IS NOT NULL
+      AND dest_municipality_code IS NOT NULL
+      AND origin_municipality_code != dest_municipality_code  -- self-loop 除外
       AND total_commuters IS NOT NULL AND total_commuters > 0
     """
     return conn.execute(sql).fetchall()
```

JIS コードベースなのでヘッダー混入ガード (`prefecture <> '都道府県'`) は不要 (コード = 5 桁数字でヘッダー値を取らない)。`self-loop 除外` も `origin_code != dest_code` に簡潔化。

#### (d): `build_summary_rows()` の変更

```diff
 def build_summary_rows(raw_rows):
     """destination ごとに TOP N を抽出し flow_share を計算 → list[dict]。"""
     by_dest = defaultdict(list)
-    for op, om, dp, dm, total, male, female, year in raw_rows:
-        by_dest[(dp, dm, year or DEFAULT_SOURCE_YEAR)].append((op, om, total))
+    for (op, om, oc, dp, dm, dc, total, male, female, year) in raw_rows:
+        # キーは JIS code ベース (擬似コード時代との互換: 名称も保持)
+        by_dest[(dc, dp, dm, year or DEFAULT_SOURCE_YEAR)].append((oc, op, om, total))

     estimated_at = datetime.now(timezone.utc).isoformat(timespec="seconds")
     out = []
-    for (dp, dm, year), origins in by_dest.items():
+    for (dest_code, dp, dm, year), origins in by_dest.items():
         dest_total = sum(c for _, _, c in origins)
         if dest_total <= 0:
             continue
         origins.sort(key=lambda x: x[2], reverse=True)
-        for rank, (op, om, count) in enumerate(origins[:TOP_N], start=1):
+        for rank, (origin_code, op, om, count) in enumerate(origins[:TOP_N], start=1):
             out.append(
                 {
-                    "destination_municipality_code": make_pseudo_code(dp, dm),
+                    "destination_municipality_code": dest_code,  # JIS 5桁
                     "destination_prefecture": dp,
                     "destination_municipality_name": dm,
-                    "origin_municipality_code": make_pseudo_code(op, om),
+                    "origin_municipality_code": origin_code,    # JIS 5桁
                     "origin_prefecture": op,
                     "origin_municipality_name": om,
                     ...
                 }
             )
     return out
```

#### `make_pseudo_code()` の扱い

選択肢:
- **(A) 削除** (推奨): 用途消滅。コードベースから完全除去
- **(B) 互換維持**: 万が一の fallback 用に残す。`if not code: code = make_pseudo_code(pref, muni)` のような防御コード

判断: **(A) 削除** を推奨。`v2_external_commute_od_with_codes` で code が NULL の場合は WHERE で除外しているため fallback 不要。

---

## 3. 既存擬似コード版テーブルとの互換性判断

### 3.1 互換性: なし (PK 形式が異なる)

| | 擬似コード版 | JIS 版 |
|---|------------|--------|
| `municipality_code` 形式 | `"北海道:札幌市"` | `"01101"` |
| PK 一意性 | 名称組み合わせで一意 | 5 桁コードで一意 |
| 行数 (推定) | 27,879 | 異なる可能性大 (政令市の区が独立行で増える、合併補正で減る等) |

→ **同一テーブル内に擬似と JIS を混在させない**。

### 3.2 移行戦略: 上書き再生成 (推奨)

| ステップ | 内容 |
|--------|------|
| 1 | `build_commute_flow_summary.py` を JIS 版に改修 |
| 2 | ローカル `data/hellowork.db::commute_flow_summary` を **DROP+CREATE+再生成** (擬似版を完全置換) |
| 3 | Turso 反映時も同様に DROP+CREATE+INSERT で上書き (`upload_to_turso.py` 既存ロジック) |
| 4 | 既存 `data/generated/commute_flow_summary.csv` (擬似版) は CSV 自体を削除 → 新版が同パスに上書き出力 |

### 3.3 代替戦略: 並行運用 (非推奨)

| | 擬似コード版 | JIS 版 |
|---|------------|--------|
| テーブル名 | `commute_flow_summary` (既存) | `commute_flow_summary_jis` (新設) |
| Step 3 HTML 層の参照先 | 擬似版 | 新版 |
| 切替方法 | フィーチャーフラグ |

→ **デメリット**: テーブルが 2 つに分裂、Step 3 HTML を一時的に擬似版で動かしながら JIS 版にも対応するのは複雑化。**推奨せず**。

---

## 4. JIS 版実行の前提条件 (チェックリスト)

| # | 前提 | 確認方法 | 期待 |
|--:|------|---------|------|
| 1 | Worker A の `fetch_commute_od.py` 改修完了 | `grep "v2_external_commute_od_with_codes" scripts/fetch_commute_od.py` | 1 件以上ヒット |
| 2 | e-Stat 再 fetch 完了 (`v2_external_commute_od_with_codes` 投入済) | `sqlite3 data/hellowork.db "SELECT COUNT(*) FROM v2_external_commute_od_with_codes"` | 80,000 行以上 |
| 3 | JIS code 充填率 100% | `... WHERE origin_municipality_code IS NULL OR dest_municipality_code IS NULL` | 0 件 |
| 4 | Worker B の `municipality_code_master` 投入完了 | `sqlite3 data/hellowork.db "SELECT COUNT(*) FROM municipality_code_master"` | 約 1,900 行 |
| 5 | 既存 commute_flow_summary (擬似版) のバックアップ取得 (任意) | `sqlite3 data/hellowork.db ".dump commute_flow_summary" > backup_commute_flow_summary_pseudo.sql` | バックアップファイル生成 |

→ 全 5 項目 OK で `build_commute_flow_summary.py --jis` モード実行可。

---

## 5. JIS 版実行手順

### 5.1 改修 + 実行 (ユーザー手動)

```bash
# 1. 改修 (Worker A 改修と同期して実装)
# scripts/build_commute_flow_summary.py を §2.4 の diff に従って修正

# 2. ローカル DB に再生成 (擬似版を完全上書き)
cd C:\Users\fuji1\AppData\Local\Temp\hellowork-deploy
python scripts/build_commute_flow_summary.py
# → 期待出力:
#   v2_external_commute_od_with_codes: 80,000+ 行
#   集計後 (TOP 20 × destination): 約 28,000 行 (擬似版とほぼ同数)
#   検証 SQL: 全 10 項目 pass

# 3. CSV 出力先確認 (data/generated/commute_flow_summary.csv が JIS 版に置換)
head -2 data/generated/commute_flow_summary.csv
# 期待: destination_municipality_code が "01101" 等の 5 桁コード
```

### 5.2 検証 SQL (JIS 版固有)

```sql
-- 1. 擬似コード残存チェック
SELECT COUNT(*) FROM commute_flow_summary
WHERE destination_municipality_code LIKE '%:%'
   OR origin_municipality_code LIKE '%:%';
-- 期待: 0 (擬似コード完全消滅)

-- 2. JIS code 5 桁チェック
SELECT COUNT(*) FROM commute_flow_summary
WHERE LENGTH(destination_municipality_code) != 5
   OR LENGTH(origin_municipality_code) != 5;
-- 期待: 0

-- 3. 数値以外混入チェック (5 桁数字のみであること)
SELECT COUNT(*) FROM commute_flow_summary
WHERE destination_municipality_code GLOB '[^0-9]*'
   OR origin_municipality_code GLOB '[^0-9]*';
-- 期待: 0

-- 4. master との JOIN 整合
SELECT COUNT(*) FROM commute_flow_summary AS cfs
LEFT JOIN municipality_code_master AS mcm
  ON mcm.municipality_code = cfs.destination_municipality_code
WHERE mcm.municipality_code IS NULL;
-- 期待: 0 (全 destination が master に存在)

-- 5. origin 側も同様
SELECT COUNT(*) FROM commute_flow_summary AS cfs
LEFT JOIN municipality_code_master AS mcm
  ON mcm.municipality_code = cfs.origin_municipality_code
WHERE mcm.municipality_code IS NULL;
-- 期待: 0
```

---

## 6. ロールバック方針

### 6.1 シナリオ A: JIS 版で問題発覚 → 擬似版に戻す

```bash
# バックアップから復元
sqlite3 data/hellowork.db < backup_commute_flow_summary_pseudo.sql
# → commute_flow_summary が擬似版に復元

# 既存スクリプトを git から戻す
git checkout HEAD~1 -- scripts/build_commute_flow_summary.py
# (改修コミット前なら git stash でも可)
```

### 6.2 シナリオ B: ローカルだけ JIS 版に置換し、Turso は擬似版維持

ローカル開発時の挙動:
- `fetch_commute_flow_summary` (Step 1 fetch 関数) は `query_turso_or_local` で **Turso 優先**
- Turso が擬似版なら本番表示は擬似版継続
- ローカル開発時のみローカル JIS 版が見える

これは過渡期 (Worker A 改修中など) で **意図的に活用可能な戦略**。

### 6.3 シナリオ C: Turso まで JIS 版置換後の戻し (緊急)

```sql
-- Turso CLI で実行 (Claude/AI からは禁止)
DROP TABLE commute_flow_summary;
-- → ローカル擬似版 → upload_to_turso.py 再実行で擬似版を再投入
```

非推奨。基本は §6.1 のローカルロールバック。

---

## 7. 推定実装時間

| 作業 | 時間 |
|------|:----:|
| `build_commute_flow_summary.py` 改修 (5 箇所 diff) | 1 時間 |
| ローカル実行 + 検証 SQL (§5.2) | 30 分 |
| 名称揺れ等の補正 (必要なら) | 30 分 |
| **合計** | **約 2 時間** |

Worker A + B 完了が前提。

---

## 8. 制約と禁止事項遵守

| 項目 | 状態 |
|------|:---:|
| 実装着手 | ❌ 設計のみ |
| ローカル DB への再生成 | ❌ ユーザー手動 |
| Turso upload | ❌ |
| `.env` / token 読み | ❌ 不要 |
| Rust 実装 | ❌ |
| push | ❌ |

---

## 9. Worker A/B/C の連携順序

```
Worker A: fetch_commute_od.py 改修 (実装 2h + e-Stat 再 fetch 1〜2h)
   ↓ v2_external_commute_od_with_codes 投入完了
Worker B: build_municipality_code_master.py 実装 + 実行 (1.5h)
   ↓ municipality_code_master 投入完了
Worker C (本書): build_commute_flow_summary.py JIS 化 (2h)
   ↓ commute_flow_summary が JIS 版に置換
Step A 再 upload (commute_flow_summary を Turso 再投入、JIS 版)
   ↓
Step 5 着手の前提 1/4 が「JIS 版」で完全解除
```

合計: **約 5.5〜6.5 時間** (Worker A + B + C + 連携検証)。

---

## 10. 完了条件 (本書の)

- [x] 擬似コード生成箇所の特定 (line 95-100, 132-150)
- [x] JIS 移行に必要な変更点 5 箇所
- [x] diff 案 (削除/修正の代表例)
- [x] 既存擬似版テーブルとの互換性判断 (上書き再生成推奨)
- [x] 前提条件チェックリスト (5 項目)
- [x] 実行手順 + 検証 SQL (5 項目)
- [x] ロールバック方針 (3 シナリオ)
- [x] Worker A/B/C 連携順序

---

## 11. 関連 docs

- 改修案 (前提): `SURVEY_MARKET_INTELLIGENCE_PHASE3_FETCH_COMMUTE_OD_REFACTOR.md` (Worker A)
- マスタ DDL: `SURVEY_MARKET_INTELLIGENCE_PHASE3_MUNICIPALITY_CODE_MASTER.md` (Worker B)
- 全体計画: `SURVEY_MARKET_INTELLIGENCE_PHASE3_JIS_CODE_PLAN.md`
- 擬似版手順 (廃止予定): `SURVEY_MARKET_INTELLIGENCE_PHASE3_STEP_A_COMMUTE_FLOW_UPLOAD.md` (JIS 版完了後は注意書き追加 or 廃止)
