# Round 10 Phase 1B 完了報告 (cap 到達グループ内 de-tie)

**日付**: 2026-05-11
**性質**: build script 修正 + Rust 注記更新 + ローカル検証 PASS
**前提 docs**:
- `ROUND10_MODEL_F2_ROADMAP_AUDIT_2026_05_10.md` (4 並列 agent 設計監査)
- `ROUND10_PHASE1_COMPLETION_2026_05_10.md` (Phase 1A、却下: 全国 percentile で top 100 重複率 68%)

---

## 経緯と判断

| 段階 | 方式 | 結果 |
|---|---|---|
| Phase 1A (Round 10、却下) | 全国 percentile rank × 200 (`derive_thickness_index` 全件 percentile 化) | top 100 重複率 68% / dropped 32 muni (政令市行政区 26) / >=160 件 7→1 → **営業説明困難** |
| **Phase 1B (確定)** | cap 到達グループ内 de-tie のみ (`raw >= 200` だけ [199.001, 200.000] に展開) | **top 100 重複率 100% / dropped 0 / >=160 件 7→7 維持** |

---

## 採用方針

ユーザー判断: 「商品方針として営業ストーリー保護を優先、cap=200 同値だけほどく」

仕様 (案 R):
```python
if raw >= 200.0:
    thickness = 199.001 + within_capped_percentile * 0.999  # [199.001, 200.000]
else:
    thickness = raw  # [0, 199.99] 完全不変
```

- cap muni 全体が必ず非 cap muni より上 (順序保存 100%)
- range 上限 200 維持 (`is_priority_score_in_range` 不変)
- 「cap 未満は変更しない」厳守

---

## 検証結果 (ローカル PASS)

| 検証項目 | 結果 | 期待値 | 判定 |
|---|---|---|---|
| distinct=1 muni | 0 | 0 | ✅ |
| thickness=200 完全同値 muni | 0 | 0 | ✅ |
| top 100 重複率 | **100%** | ≥ 90% | ✅ |
| 順位差中央値 | 0.0 | ≤ 30 | ✅ |
| 順位差平均 | 0.7 | ≤ 30 | ✅ |
| 順位差最大 | 14 | ≤ 50 | ✅ |
| dropped/entered | 0/0 | 入れ替え少 | ✅ |
| >=160 件数 | 7 | 5-10 | ✅ (旧と同一) |
| score range | [14.64, 169.38] | [0, 200] | ✅ (旧と同一) |
| PDF 4 対象自治体 (新宿/千代田/伊達×2) | rank 差 0-3 | 大変動なし | ✅ |
| distinct=11 muni 数 (内部統計) | 1,636 (旧 1,522、+114) | 増加 | ✅ |

---

## 副作用観察 (商品方針通り、追加対応不要)

Phase 1B は **「内部一貫性向上、表は変わらない」** 性質を持つ:

| 表示要素 | Phase 1 前 | Phase 1B 後 |
|---|---|---|
| 配信ランキング表 top 20 | A | A (ほぼ完全同一) |
| KPI「配信検証候補(>=160)」件数 | 7 | 7 |
| priority 分布 (S/A/B/C/D) | 407/627/2090/7293/10428 | 同一 |
| **distinct=11 muni 数 (内部)** | 1,522 | **1,636 (+114)** |
| **distinct=1 muni 数 (内部)** | **104** | **0** |

→ レポートの見た目変化はほぼゼロ。営業説明コスト最小、技術的には 104 都市自治体の同値問題が完全解消。

---

## 実装変更

### `scripts/build_municipality_target_thickness.py:1189-1252`

`derive_thickness_index` を hybrid 方式 (cap 到達のみ de-tie) に書き換え。raw_idx >= 200 group を職業ごとに集めて `(i + 0.5) / n` plotting position で 199.001-200.000 に展開。

### `src/handlers/survey/report_html/market_intelligence.rs:2375-2390`

注記を簡潔化 (Phase 1A 時の「全国 percentile rank」表現を削除、「同点表示の取り扱い」に変更):

```
配信優先度スコアは 0〜200 のスケールで、重み合計 100 + ペナルティ調整後の値です。
160 以上を「重点配信」、130 以上を「拡張候補」、100 以上を「維持/検証」、それ未満を「優先度低」として分類します。
同点表示の取り扱い: 母集団厚み指数 (thickness) が上限に到達する都市部自治体では、
同点表示を避けるため、上限到達グループ内の元順位で小さな差を付けています (Round 10 Phase 1B / 2026-05-11)。
上限未到達自治体の値は変わりません。
```

---

## 次ステップ (ユーザー手動)

1. ✅ build → ingest → recruiting_scores 完了 (Local DB 反映済)
2. ローカル PDF 検証 (PowerShell で 8080 起動 → spec 実行)
3. **Turso 投入** (本番反映、無料枠リセット後)
4. 本番 PDF 検証

---

## 残課題

| ID | 内容 | 優先 |
|---|---|---|
| Phase 2 (competition 職業別) | 4 score 中もう 1 つの同値成分を職業差分化 | 中 (Phase 1B 効果測定後) |
| Phase 3 (commute weight 再配分) | 25%→0% 等 | 低 |
| Phase 1.5 (全国 percentile 移行) | 商品方針判断後 | 将来案 (`ROUND10_PHASE1_5_FUTURE_OPTION_2026_05_11.md`) |

---

## 監査メタデータ

- 修正ファイル: 2 件 (build_municipality_target_thickness.py + market_intelligence.rs)
- 検証スクリプト 2 件 (一時、commit 含む)
- Local DB 再投入: ユーザー手動完了
- DB 書込 by Claude: ゼロ
- cargo check: 24 warnings (既存と同レベル)

**Round 10 Phase 1B はローカル PDF 検証残のみ。Turso 投入 → 本番反映で完了。**
