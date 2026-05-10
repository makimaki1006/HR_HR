# Round 10 Phase 1.5: 全国 percentile 移行 (将来案、現時点不採用)

**日付**: 2026-05-11
**性質**: 将来案として記録のみ、現時点で実装しない
**判断**: Round 10 Phase 1B (cap 到達グループ内 de-tie) を採用、Phase 1.5 は商品方針が変わった場合の選択肢として保留

---

## 概要

Phase 1B (採用) は「cap=200 同値の解消」を最小スコープで実施。Phase 1.5 (本案) は「thickness を全国 percentile rank で全面再構築」する **より抜本的な変更**。

| 比較軸 | Phase 1B (採用) | Phase 1.5 (将来案) |
|---|---|---|
| 同値解消 | ✅ | ✅ |
| 都市部内の真の差反映 | ❌ 圧縮 (199.001-200) | ✅ raw_idx 13.6× と 8.6× が別 percentile |
| 営業説明責任 | 軽微 (内部改善) | **重** (ranking 思想変更を顧客説明) |
| top 100 重複率 | 100% | 68% (Phase 1A 実測) |
| 既存 >=160 閾値 | そのまま使える | 再設計必要 (>=140 等) |
| 商品ストーリー | 「都市部優位」維持 | **「全国均等視点に進化」** |

---

## Phase 1.5 を採用すべき条件

以下が満たされた場合に Phase 1.5 への移行を検討:

1. **「都市部優位」ranking が営業上の価値を生まないと判明**
   - 顧客が農山村の採用機会も評価したいと言う
   - 配信戦略が「全国均等」「特定産業特化」に舵を切る
   - 「都市部 cap saturation」の説明にも限界が出てくる

2. **業界別最賃比中央値などの補完データが整備済**
   - Phase 1.5 で農山村が浮上する場合、「なぜ農林漁業町が重点配信か」を業界別データで裏付ける

3. **顧客への移行説明資料が準備可能**
   - 旧 ranking との比較表
   - 「全国均等視点」の商品メッセージ
   - 既存契約レポートとの不連続を許容するリリースノート

---

## Phase 1.5 実装案 (採用時)

`scripts/build_municipality_target_thickness.py:derive_thickness_index` を以下に置換:

```python
def derive_thickness_index(model_result: dict) -> dict:
    """0-200 正規化 (全国 percentile rank ベース、Phase 1.5)."""
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

加えて:
- 閾値再設計: 重点 ≥140 / 拡張 ≥120 / 維持 ≥100 等 (rank 100 が約 132 になるため)
- 注記更新: 「全国 percentile rank で正規化、職業ごとに異なる priority_score」
- 既存テスト 3 件の閾値値修正

---

## Phase 1A 実測結果 (Phase 1.5 と同等)

Round 10 Phase 1A (全国 percentile) を試行した実測値:

| 指標 | Phase 1A 実測 |
|---|---|
| top 100 重複率 | 68% (90% 基準を未達) |
| dropped (pre top100 → post out) | 32 muni (うち政令市行政区 26、通常市町村 6) |
| entered (pre out → post top100) | 32 muni (うち通常市町村 32、農林漁業 26) |
| >=160 件数 | 7 → 1 (閾値整合崩壊) |
| 順位差中央値 | 236 / 平均 266.1 / 最大 840 |
| PDF 対象自治体 (千代田区) score | 67.36 → 79.58 (+12) |
| PDF 対象自治体 (北海道伊達市) score | 60.69 → 87.44 (+27) |

→ ranking 大変動、営業説明責任重 → Phase 1.5 採用には商品方針判断必須。

---

## 監査メタデータ

- 状態: 将来案、不採用
- 採用条件: 上記 3 条件すべて満たす
- 採用判断者: ユーザー (商品方針)

**Phase 1.5 は本ラウンドで実装しない。Phase 1B (Round 10 採用) で運用継続。**
