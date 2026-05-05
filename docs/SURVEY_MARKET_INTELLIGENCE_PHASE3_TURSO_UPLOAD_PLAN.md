# Phase 3 Step 5 — Turso Upload 計画書 (実 upload 前レビュー用)

**作成日**: 2026-05-04
**作成者**: Worker X3
**状態**: 🟡 レビュー待ち (実 upload 未実施)
**前提**: Worker X1 (差分監査) / Worker X2 (スクリプト設計) と並列進行中。X1/X2 結果反映時に本書 §1, §3, §9 を更新する。

---

## 0. 目的とスコープ

Phase 3 で構築したローカル DB の解析テーブル群を Turso V2 (外部データ DB) に同期する手順を**実行前に**確定するための計画書。本書は **計画のみ** であり、本書承認まで実 upload は行わない。

対象 DB: ローカル `hellowork.db` (V2) → Turso V2 外部データ DB (14 テーブル既存 + 本件で追加/更新)

---

## 1. upload 対象テーブル (7 件)

| # | テーブル | ローカル行数 | 主要列 | 用途 | upload 必須度 |
|--:|---------|---:|------|------|:---:|
| 1 | `v2_external_population` | 1,917 | prefecture, municipality, age_x, total_population | F2 入力 + UI 表示 | 🔴 必須 |
| 2 | `v2_external_population_pyramid` | 17,235 | prefecture, municipality, age_group, male/female_count | F2 入力 + UI 表示 | 🔴 必須 |
| 3 | `municipality_occupation_population` | 729,949 | basis, data_label, source_name, ... | 商品の中核 | 🔴 必須 |
| 4 | `v2_municipality_target_thickness` | 20,845 | thickness_index, rank, priority, scenario_* | UI ダッシュボード | 🔴 必須 |
| 5 | `municipality_code_master` | 1,917 | code, area_type, parent_code | 結合キー | 🟡 既存と同期 |
| 6 | `commute_flow_summary` | 27,879 | 通勤 OD 集計 | F5 補正 + UI | 🟡 既存と同期 |
| 7 | `v2_external_commute_od_with_codes` | 86,762 | OD 生データ + JIS code | 派生集計の元 | 🟡 既存と同期 |

**合計 row writes 見積 (全置換時)**: 約 **886,621 writes**

> Worker X1 の差分監査結果が確定したら、各テーブルの remote 状況を本表横に追記する。

---

## 2. row writes 見積 (Turso 月間枠との比較)

| 項目 | 値 |
|------|---:|
| Turso V2 free tier writes/月 | 25,000,000 (typical) |
| 全置換見積 (886,621) | 月間枠の **約 3.5%** |
| 差分のみ最小ケース (約 750,000) | 月間枠の **約 3.0%** |
| 過去消費 (Phase 0-3 累計、概算) | 約 5-8% (前回までの実績) |
| **本件投入後の累計目安** | **約 10% 以下** |

→ **無料枠は十分安全圏**。ただし、再 upload や rollback で 2-3 倍に膨らむ可能性があるため `--max-writes` ガード必須。

---

## 3. strategy 選択 (テーブル別)

| # | テーブル | 推奨 strategy | 想定 writes | 理由 (X1 確認後に確定) |
|--:|---------|:------------:|---:|------|
| 1 | `v2_external_population` | **incremental** | 175 | designated_ward 175 件追加のみ、既存 1,742 維持 |
| 2 | `v2_external_population_pyramid` | **incremental** | 1,575 | designated_ward 分のみ追加 |
| 3 | `municipality_occupation_population` | **replace** | 729,949 | 新規テーブル、X1 で remote 不在確認待ち |
| 4 | `v2_municipality_target_thickness` | **replace** | 20,845 | 新規テーブル |
| 5 | `municipality_code_master` | skip or replace | 0 or 1,917 | X1 で 1,742→1,917 差分確認待ち |
| 6 | `commute_flow_summary` | **skip** | 0 | X1 で 27,879 行一致確認待ち |
| 7 | `v2_external_commute_od_with_codes` | **skip** | 0 | X1 で 86,762 行一致確認待ち |

**実 upload 想定 (確定後)**:
- 新規 replace 2 件: **750,794 writes**
- 既存 incremental 2 件: **1,750 writes**
- **合計: 約 752,544 writes** (無料枠の約 3.0%)

---

## 4. dry-run 手順

```bash
cd C:/Users/fuji1/AppData/Local/Temp/hellowork-deploy

# Step 1: ローカル状況 + 投入見積
python scripts/upload_phase3_step5.py --dry-run

# Step 2: Turso 既存確認 (READ-only)
python scripts/upload_phase3_step5.py --check-remote

# Step 3: ユーザー判断 (本書のレビュー)
# Step 4: テスト upload (1 テーブル先行)
python scripts/upload_phase3_step5.py --upload \
    --tables v2_municipality_target_thickness \
    --strategy replace
```

---

## 5. 本格 upload 手順 (ユーザー手動)

```powershell
$env:TURSO_EXTERNAL_URL = "..."
$env:TURSO_EXTERNAL_TOKEN = "..."

# 1. 新規 2 テーブル先行 (replace)
python scripts/upload_phase3_step5.py --upload `
    --tables municipality_occupation_population v2_municipality_target_thickness `
    --strategy replace --max-writes 800000

# 2. 既存 2 テーブルの差分 (incremental)
python scripts/upload_phase3_step5.py --upload `
    --tables v2_external_population v2_external_population_pyramid `
    --strategy incremental --max-writes 50000

# 3. verify
python scripts/upload_phase3_step5.py --verify
```

---

## 6. rollback 方針

### 6.1 失敗時の即時 rollback (新規テーブル)

```sql
DROP TABLE IF EXISTS municipality_occupation_population;
DROP TABLE IF EXISTS v2_municipality_target_thickness;
```

### 6.2 designated_ward 追加分のみの rollback

```sql
-- v2_external_population
DELETE FROM v2_external_population
WHERE municipality IN (
  SELECT municipality_name FROM municipality_code_master
  WHERE area_type='designated_ward'
);

-- v2_external_population_pyramid
DELETE FROM v2_external_population_pyramid
WHERE municipality IN (
  SELECT municipality_name FROM municipality_code_master
  WHERE area_type='designated_ward'
);
```

### 6.3 部分失敗時の整合性回復

`source_name` / `source_year` で WHERE 句を効かせて影響範囲を限定する。例:

```sql
DELETE FROM municipality_occupation_population
WHERE source_name='ssdse_a_2024' AND source_year=2024;
```

---

## 7. 安全装置 (実 upload 時必須)

- `--max-writes 800000` 設定で上限ガード (超過時は自動中断)
- token / URL の **マスク表示** (log 出力時は末尾 4 文字のみ)
- 進捗 100k 行ごとの commit (途中失敗時の被害最小化)
- 失敗時の **自動リトライなし** (人間判断で再実行)
- `--yes` 確認 (interactive prompt をデフォルト有効、`--yes` で skip)
- 実行前に **ローカル DB ファイル lock** (read-only) で改変防止
- token 期限切れチェック (`--check-remote` を upload 直前に必ず実行)

---

## 8. リスク

| リスク | 対策 |
|--------|------|
| token 期限切れ | upload 直前に `--check-remote` で確認 |
| 月間 writes 枠超過 | `--max-writes` ガード + 事前見積 (本書 §2) |
| upload 中の中断 (network 等) | バッチ単位 commit + resume 不可前提で再実行 |
| ローカル DB の追加変更 (途中で write) | upload 前に DB ファイル lock |
| 既存 Turso 行の意図せぬ削除 | strategy=`replace` は **新規テーブルのみ** に限定 |
| Rust 側 query の途中失敗 | upload は Rust 統合前に完了 (本書方針) |
| 同名テーブル重複 (mop など V1 と被る可能性) | X1 で remote 名前空間確認、必要なら接頭辞付与 |
| designated_ward 行の prefecture 表記揺れ | upload 前に `assert_no_dup` チェック (incremental 内) |

---

## 9. 実 upload 前のレビューチェックリスト (全 9 件)

ユーザー承認の前に以下を確認:

- [ ] Worker X1 差分調査結果が出揃っている
- [ ] Worker X2 改修案で **案 B (専用スクリプト) 採用** が確定
- [ ] 7 テーブルのうち各 strategy 確定 (本書 §3)
- [ ] row writes 見積が無料枠の 5% 以下 (本書 §2)
- [ ] rollback SQL が全テーブル分用意されている (本書 §6)
- [ ] `--dry-run` 出力でローカル行数確認
- [ ] `--check-remote` で Turso 既存行数確認
- [ ] 1 テーブル先行 upload で動作確認 (`v2_municipality_target_thickness` 推奨)
- [ ] ユーザー手動で `$env:TURSO_EXTERNAL_TOKEN` 設定済

---

## 10. 関連ドキュメント

- `SURVEY_MARKET_INTELLIGENCE_PHASE3_TURSO_UPLOAD_DIFF_AUDIT.md` (Worker X1, **結果待ち**)
- `SURVEY_MARKET_INTELLIGENCE_PHASE3_TURSO_UPLOAD_SCRIPT_DESIGN.md` (Worker X2, **結果待ち**)
- `SURVEY_MARKET_INTELLIGENCE_PHASE3_TURSO_UPLOAD_PROCEDURE.md` (将来、本書承認後の手順書)
- `verify_turso_v2_sync.py` (既存、参考)
- `SURVEY_MARKET_INTELLIGENCE_PHASE3_DESIGNATED_WARD_DATA_AUDIT.md` (designated_ward 175 件の根拠)

---

## 11. 次のステップ

1. Worker X1 の差分監査結果を本書 §1, §3 に反映 (各テーブルの remote 行数列追加)
2. Worker X2 のスクリプト設計案 (案 A/B/C) から本書 §4-5 で前提とする案 B を確定
3. ユーザーに本書 (X1/X2 統合済み版) をレビュー依頼
4. 承認後、`SURVEY_MARKET_INTELLIGENCE_PHASE3_TURSO_UPLOAD_PROCEDURE.md` を別途作成し実 upload に進む

---

**注**: 本書は計画のみ。実 upload・スクリプト実装・Rust 変更・push は本書範囲外。
