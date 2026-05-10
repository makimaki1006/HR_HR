# Round 10 Phase 1 完了報告 (build script 側)

**日付**: 2026-05-10
**性質**: build script 修正 + Rust 注記更新 (Local DB 再投入はユーザー手動)
**前提 docs**: `ROUND10_MODEL_F2_ROADMAP_AUDIT_2026_05_10.md` (4 並列 agent 設計監査)

---

## 完了内容

| Step | タスク | 状態 |
|---|---|---|
| 1a | `derive_thickness_index` を percentile-based に置換 | ✅ build script 修正済 |
| 1b | Round 9 cap saturation 注記の文言更新 | ✅ Rust 修正済 |
| 1c | Pre snapshot 取得 (Phase 1 前ベースライン) | ✅ `data/_tmp_phase1_pre_snapshot.csv` (1,895 行) |
| 1d | 検証スクリプト (前後比較) | ✅ `scripts/_tmp_phase1_verify.py` |
| 1e | ユーザー手動投入手順書 | ✅ `docs/ROUND10_PHASE1_USER_INGEST_GUIDE_2026_05_10.md` |
| 2 | ローカル CSV 再生成 | ⏳ ユーザー手動 (DB 書込のため) |
| 3 | ローカル DB 再投入 | ⏳ ユーザー手動 |
| 4 | 検証スクリプト実行 (同値率測定) | ⏳ ユーザー手動 |
| 5 | ローカル PDF 検証 | ⏳ ユーザー手動 |
| 6 | Turso 投入 (本番反映) | ⏳ ユーザー手動 |
| 7 | 本番 PDF 検証 | ⏳ ユーザー手動 |

---

## 実装変更

### `scripts/build_municipality_target_thickness.py:1189-1217`

旧:
```python
def derive_thickness_index(model_result: dict) -> dict:
    """0-200 正規化 (100 = 全国平均、cap 200)."""
    occ_vals = ...; nat_avg = ...
    out[(pref, muni)][occ] = max(0.0, min(idx, 200.0))  # cap=200
```

新:
```python
def derive_thickness_index(model_result: dict) -> dict:
    """0-200 正規化 (100 = 全国中央値、percentile rank ベース、cap saturation なし)."""
    by_occ = defaultdict(dict)
    for (pref, muni), occ_dict in model_result.items():
        for occ, v in occ_dict.items():
            by_occ[occ][(pref, muni)] = v
    out = defaultdict(dict)
    for occ, vals in by_occ.items():
        sorted_keys = sorted(vals.keys(), key=lambda k: vals[k])
        n = len(sorted_keys)
        for i, key in enumerate(sorted_keys):
            pct = (i + 0.5) / n
            idx = round(pct * 200.0, 4)
            out[key][occ] = max(0.0, min(idx, 200.0))
    return dict(out)
```

`build_municipality_recruiting_scores.normalize_to_200` と同じ plotting position 方式 (`pct = (i + 0.5) / n`)。出力構造・range 制約・スキーマ・validate_outputs はすべて維持。

### `src/handlers/survey/report_html/market_intelligence.rs:2375-2388`

Round 9 P2-G の「大都市圏 cap saturation により職業差ゼロ」注記を削除し、Phase 1 後の正規化方式説明に置換:

```
配信優先度スコアは 0〜200 のスケールで...
thickness 指数の正規化方式: 全国 percentile rank で 0〜200 に正規化しています (Round 10 Phase 1)。
同一自治体でも職業ごとに分布が異なるため、職業別に異なる priority_score を取ります。
thickness=200 付近は「全国 percentile 99 以上」を意味し、以前の cap saturation ではありません。
配信判断は本ランキングの代表職種に加え、産業 × 性別セクションと推奨アクション (4 象限図) を併用してください。
```

---

## Pre snapshot 結果 (Phase 1 前ベースライン)

```
--- 同値 muni 数 (distinct=1) ---
  distinct= 1:   104 muni  ← Phase 1 で 0 になる期待値
  distinct=11: 1,522 muni
--- thickness=200 cap saturation muni 数 ---
  全職業 cap muni: 104  ← Phase 1 で 0 になる期待値
--- top_score バケット ---
  重点配信(>=160): 7 / 拡張(130-160): 102 / 維持(100-130): 322 / 低(<100): 1,464
```

---

## ユーザー手動オペレーション (`ROUND10_PHASE1_USER_INGEST_GUIDE_2026_05_10.md`)

```powershell
# Step 1: ローカル CSV 再生成
python scripts\build_municipality_target_thickness.py --csv-only

# Step 2: Local DB 再投入 (thickness + recruiting_scores 連鎖)
python scripts\ingest_v2_thickness_to_local.py --apply
python scripts\build_municipality_recruiting_scores.py --apply

# Step 3: 検証
python scripts\_tmp_phase1_verify.py
# → ✅ 全不変条件 PASS なら Step 4 へ

# Step 4-7: ローカル PDF → Turso 投入 → 本番 PDF
# (詳細は ROUND10_PHASE1_USER_INGEST_GUIDE 参照)
```

---

## 期待される検証結果

| 指標 | Phase 1 前 (実測) | Phase 1 後 (期待) |
|---|---|---|
| distinct=1 muni 数 | 104 (5.49%) | **< 5** |
| thickness=200 cap muni | 104 | **0** |
| range 違反 | 0 | **0** |
| 上位 100 重複率 | — | **≥ 80%** |
| 重点配信(>=160) muni 数 | 7 | 概ね同等 (±5 件) |

---

## 残課題

- **Phase 2 (competition_score 職業別化)**: Phase 1 効果を見てから判断
- **Phase 3 (commute weight 再配分)**: Phase 1+2 後に判断
- **逆証明 unit test 追加**: Phase 1 後の DB 計算結果から fixture 作成 → `tests/...market_intelligence.rs::invariant_phase1_*_diversity` 追加 (Phase 1 完了後の Round 10 続編)

---

## 監査メタデータ

- 修正ファイル: 2 件 (build_municipality_target_thickness.py + market_intelligence.rs)
- 一時ファイル: 3 件 (build script は git 含む、_tmp_*.py + .csv は除外)
- DB 書込: ゼロ (ユーザー手動)
- cargo check: 24 warnings (既存と同レベル)
- Round 9 注記との互換: 削除 + 置換で一貫性維持

**Round 10 Phase 1 build script 側完了。ユーザー手動投入後に検証 → 本番反映 → 完了。**
