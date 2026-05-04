# Worker C 調査: `v2_commute_flow_summary` vs `commute_flow_summary` テーブル関係

作成日: 2026-05-04
ステータス: 調査結果 (Read-only、変更なし)

---

## 1. 結論 (要約)

**2 つのテーブルは完全に別物**。名前が紛らわしいが、由来・用途・スキーマ・参照箇所すべて異なる。

| 項目 | `v2_commute_flow_summary` (既存) | `commute_flow_summary` (Phase 3 Step A 新規) |
|------|--------------------------------|--------------------------------------------|
| 由来 | `scripts/fetch_commute_od.py:60-71, 251-` (`compute_summaries`) で生成 | `scripts/build_commute_flow_summary.py` (Worker A、Phase 3 Step A) で生成 |
| 行数 | **3,778 行** (1,894 inflow + 1,884 outflow) | **27,879 行** (各 destination の TOP 20 origin が独立行) |
| カラム数 | 8 | 19 |
| PK | (prefecture, municipality, direction) | (destination_municipality_code, origin_municipality_code, occupation_group_code, source_year) |
| TOP10/20 表現 | `top10_json` JSON 集約 (1 行に圧縮) | 各 origin が独立行 (TOP 20 を 20 行で展開) |
| `municipality_code` カラム | **なし** | あり (擬似コード `prefecture:municipality_name`) |
| `occupation_group_code` | なし | あり (現状 `'all'` 固定) |
| 推定値カラム (`estimated_target_flow_*`) | なし | あり (現状 NULL) |
| Turso 反映 | 未投入 | 未投入 (Phase 3 Step A 手順書あり) |
| **Rust 参照** | **なし (死蔵テーブル)** | あり (Phase 3 Step 1 + Step 3 から参照) |

---

## 2. 詳細スキーマ比較

### 2.1 `v2_commute_flow_summary` (既存、行数 3,778)

```sql
CREATE TABLE v2_commute_flow_summary (
    prefecture TEXT NOT NULL,
    municipality TEXT NOT NULL,
    direction TEXT NOT NULL,                -- 'inflow' / 'outflow'
    total_commuters INTEGER NOT NULL,
    self_commute_count INTEGER DEFAULT 0,
    self_commute_rate REAL DEFAULT 0,
    partner_count INTEGER DEFAULT 0,
    top10_json TEXT NOT NULL DEFAULT '[]',  -- TOP10 を JSON 配列で集約
    PRIMARY KEY (prefecture, municipality, direction)
);
```

**direction 分布**:
- `inflow`: 1,894 行 (= DISTINCT destination 数)
- `outflow`: 1,884 行 (= DISTINCT origin 数)

**サンプル** (三重県いなべ市の inflow):
```
('三重県', 'いなべ市', 'inflow', 13875, 16699, 0.5462, 56,
 '[{"pref": "三重県", "muni": "桑名市", "count": 3882}, ...]')
```

→ いなべ市への流入元 56 自治体合計 13,875 人、自市内通勤 16,699 人 (self_rate 54.6%)、TOP10 流入元を JSON で持つ。

### 2.2 `commute_flow_summary` (Phase 3 Step A 新規、行数 27,879)

```sql
CREATE TABLE commute_flow_summary (
    destination_municipality_code TEXT NOT NULL,    -- "北海道:札幌市" 擬似コード
    destination_prefecture TEXT NOT NULL,
    destination_municipality_name TEXT NOT NULL,
    origin_municipality_code TEXT NOT NULL,         -- "北海道:札幌市北区" 擬似コード
    origin_prefecture TEXT NOT NULL,
    origin_municipality_name TEXT NOT NULL,
    occupation_group_code TEXT NOT NULL DEFAULT 'all',
    occupation_group_name TEXT NOT NULL DEFAULT '全職業',
    flow_count INTEGER NOT NULL DEFAULT 0,
    flow_share REAL,
    target_origin_population INTEGER,
    estimated_target_flow_conservative INTEGER,
    estimated_target_flow_standard INTEGER,
    estimated_target_flow_aggressive INTEGER,
    estimation_method TEXT,
    estimated_at TEXT,
    rank_to_destination INTEGER NOT NULL,
    source_year INTEGER NOT NULL,
    created_at TEXT NOT NULL DEFAULT CURRENT_TIMESTAMP,
    PRIMARY KEY (destination_municipality_code, origin_municipality_code, occupation_group_code, source_year)
);
```

**サンプル** (札幌市の流入元 TOP3):
```
('北海道:札幌市', '北海道', '札幌市', '北海道:札幌市北区', '北海道', '札幌市北区', 'all', '全職業', 127892, 0.1366, ...)
('北海道:札幌市', '北海道', '札幌市', '北海道:札幌市東区', '北海道', '札幌市東区', 'all', '全職業', 117909, 0.1260, ...)
('北海道:札幌市', '北海道', '札幌市', '北海道:札幌市中央区', '北海道', '札幌市中央区', 'all', '全職業', 107913, 0.1153, ...)
```

→ destination ごとに TOP 20 origin が独立行で展開。職業別 (現状 'all' のみ) + 推定流入数の枠あり。

---

## 3. Rust 参照箇所

### 3.1 `commute_flow_summary` (新規、Rust 参照あり)

```
src/handlers/analysis/fetch/market_intelligence.rs    ← Phase 3 Step 1 で追加
src/handlers/survey/report_html/market_intelligence.rs ← Phase 3 Step 3 で追加
```

具体的には `fetch_commute_flow_summary` 関数 (Step 1) で `table_exists` フォールバック付きで読み取り、Step 3 HTML レンダラから DTO 経由で参照される設計。

### 3.2 `v2_commute_flow_summary` (既存、Rust 参照**なし**)

`grep -rn "v2_commute_flow_summary" src/` 結果: **0 件ヒット**。

→ **死蔵テーブル**。`fetch_commute_od.py` で生成されているが、Rust ハンドラからは一切読まれていない。

参考: ローカル + docs での参照箇所 (Rust 以外):
- `scripts/fetch_commute_od.py` (生成元)
- `docs/SURVEY_MARKET_INTELLIGENCE_PHASE3_STEP_A_UPLOAD_CHECKLIST.md` (Worker C 旧 docs、参照)
- `docs/turso_v2_sync_report_2026-05-03.md` / `2026-05-04.md` (sync 検証結果)

---

## 4. 由来の確認

### 4.1 `v2_commute_flow_summary` の生成タイミング

`scripts/fetch_commute_od.py` 末尾の `main()` で:
1. e-Stat 国勢調査 OD データ取得
2. `v2_external_commute_od` (実 OD 行列) 投入
3. **`compute_summaries(conn)` 呼出** → `v2_commute_flow_summary` を生成

つまり OD 取得スクリプトの**副産物**として生成される。

### 4.2 `commute_flow_summary` の生成タイミング

`scripts/build_commute_flow_summary.py` (Phase 3 Step A で私が新規作成) を **明示的に実行** することで生成。fetch_commute_od.py とは独立した別パイプライン。

---

## 5. 判定: テーブル名の混乱について

| 観点 | 評価 |
|------|:----:|
| 名前の類似性 | 🔴 高 (区別が `v2_` プレフィックスのみ) |
| 用途の重複 | 🟡 部分的に重複 (どちらも「通勤フロー要約」) |
| Rust 参照分離 | ✅ 明確 (新は Phase 3、旧は未参照) |
| 命名の妥当性 | 🟡 微妙 (Phase 3 新テーブルは `municipality_recruiting_flow_summary` 等のほうが衝突回避できた) |

### 5.1 リネーム候補 (任意、推奨はしない)

| 旧 | 新 | メリット | デメリット |
|----|----|---------|-----------|
| `v2_commute_flow_summary` (既存) | `v2_commute_flow_summary_legacy` | 死蔵明示 | 既存スクリプト + sync report 全部要修正 |
| `commute_flow_summary` (Phase 3) | `municipality_recruiting_flow_summary` | 用途明示 | Phase 3 docs + Rust + Turso CSV 全部要修正 |

→ **どちらもコスト > メリット**。現状のまま運用、本書 + 既存 docs で関係を明示する方が現実的。

---

## 6. 影響と推奨対応

### 6.1 死蔵テーブル `v2_commute_flow_summary` の扱い

| 選択 | 内容 | 推奨 |
|------|------|:---:|
| (a) 現状維持 (生成だけして放置) | 既存パイプライン無変更、Rust 影響なし | ⚪ 中立 |
| (b) `fetch_commute_od.py` から `compute_summaries()` 呼出を削除 | 死蔵テーブル生成停止、ファイル容量削減 | 🟡 微妙 (将来 Rust 側で使う可能性も) |
| (c) Rust ハンドラで `v2_commute_flow_summary` を読み始める | 既存テーブルを活用 | 🟡 Phase 3 設計と用途重複 |

→ **(a) 現状維持** を推奨。Phase 3 は新 `commute_flow_summary` で進める。

### 6.2 ドキュメント整合

既存 docs での参照確認:
- `SURVEY_MARKET_INTELLIGENCE_PHASE3_STEP_A_UPLOAD_CHECKLIST.md`: ✅ `commute_flow_summary` (新) を参照 (混同なし)
- `turso_v2_sync_report_*.md`: 両テーブルが別行で表示されている (混同なし)

→ **既存 docs の修正不要**。本書 (Worker C) を新規追加し、関係を明示するだけで十分。

### 6.3 sync report との整合

`docs/turso_v2_sync_report_2026-05-04.md` での状態:

| テーブル | local | Turso | Status |
|---------|:-----:|:-----:|--------|
| `v2_commute_flow_summary` | 3,778 行 | 不在? | (要再確認) |
| `commute_flow_summary` | 27,879 行 | 不在 | REMOTE_MISSING |

`v2_commute_flow_summary` の Turso 反映状態は最新 sync report で要確認。死蔵テーブルなので Turso 投入は不要。

---

## 7. 結論と次のアクション

### 7.1 結論

- 2 テーブルは **別物 + 別パイプライン + 別 PK 構造**
- Phase 3 で参照されているのは新 `commute_flow_summary` (Phase 3 Step A 由来) のみ
- 旧 `v2_commute_flow_summary` は **Rust 未参照の死蔵テーブル** (現状維持で問題なし)

### 7.2 推奨アクション (本書では実行しない、ユーザー判断待ち)

| # | アクション | 優先度 |
|--:|-----------|:------:|
| 1 | 本書を docs として追加 commit (関係を明示) | 中 |
| 2 | `v2_commute_flow_summary` の Turso 反映状態を最新 sync report で確認 | 低 |
| 3 | `compute_summaries()` の現状維持判断確定 (Phase 3 で活用しない宣言) | 中 |
| 4 | リネーム検討 (推奨せず、本書 §5.1) | - |

---

## 8. 制約と禁止事項遵守

| 項目 | 状態 |
|------|:---:|
| Turso upload | ❌ 実行せず |
| DB 書き込み | ❌ READ-only (テーブル一覧 / PRAGMA / COUNT / SELECT のみ) |
| Rust 変更 | ❌ |
| `.env` / token 読み | ❌ 不要 |
| push | ❌ |

---

## 9. 関連 docs

- Worker A 改修: `SURVEY_MARKET_INTELLIGENCE_PHASE3_FETCH_COMMUTE_OD_REFACTOR.md`
- Worker B1 master DDL: `SURVEY_MARKET_INTELLIGENCE_PHASE3_MUNICIPALITY_CODE_MASTER.md` (改訂版で area_type 追加)
- Worker C 移行設計: `SURVEY_MARKET_INTELLIGENCE_PHASE3_BUILD_COMMUTE_FLOW_JIS_MIGRATION.md` (`commute_flow_summary` JIS 化)
- Step A upload 手順: `SURVEY_MARKET_INTELLIGENCE_PHASE3_STEP_A_COMMUTE_FLOW_UPLOAD.md`
- Step A upload chechlist: `SURVEY_MARKET_INTELLIGENCE_PHASE3_STEP_A_UPLOAD_CHECKLIST.md`
