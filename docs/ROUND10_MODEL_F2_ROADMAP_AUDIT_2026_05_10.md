# Round 10 Model F2 中期改修 ロードマップ監査

**日付**: 2026-05-10
**性質**: read-only 設計監査 (実装なし)
**前提**: Round 8 P1-6 + Round 9 P2-G で発見された Model F2 の 2 件の問題
**監査体制**: 4 並列 agent (target_thickness / commute / competition / 全体ロードマップ)

---

## 0. エグゼクティブ・サマリ

| Phase | 採用 | タスク | 効果 |
|---|---|---|---|
| **Phase 1** | ✅ 推奨 | target_thickness を percentile-based に置換 | 都市部 cap saturation 解消、104 muni × 11 occ 同値が全部別 rank に |
| **Phase 2** | ✅ 推奨 | competition_score を職業別化 (postings.occupation_major_code 活用) | 1 自治体内で 5 桁の職業差、commute と独立な指標 |
| **Phase 3** | ⚠️ 検討 | commute weight 25% → 0% に再配分 (commute 職業別化は不可のため) | 同値 100% 成分の重みを下げる |
| **Phase 4** | ❌ 不要 | cap 撤廃 / salary 職業別 / 全体再設計 | 顧客説明責任 / 統計的整合性 / cost-benefit |

---

## 1. 背景: 2 件の構造的問題

### 1-1. cap saturation (104 自治体で同値)

`build_municipality_target_thickness.py:1189-1203`:
```python
out[(pref, muni)][occ] = max(0.0, min(idx, 200.0))   # cap=200
```

**実測 (Agent A 調査)**: 都市部の raw_idx は cap=200 を大幅超過
| 順位 | 自治体 | workplace pop | proxy raw_idx | cap 倍率 |
|---|---|---|---|---|
| 1 | 千代田区 | 805,042 | **2,725** | 13.6× |
| 2 | 港区 | 775,056 | 2,623 | 13.1× |
| 3 | 中央区 | 541,845 | 1,834 | 9.2× |
| 4 | 新宿区 | 510,011 | 1,726 | 8.6× |

raw_idx > 200 の自治体: **250 / 1,896 (13.2%)**、職業別では港区 occ A = 3,902 (全国平均の 39 倍)。

### 1-2. 4 score 中 3 つが職業差ゼロ

| 成分 | 同値率 | 重み | 原因 |
|---|---|---|---|
| target_thickness | 5.49% (都市部 104 muni のみ同値) | 50% | cap=200 |
| commute_access | **100%** (全 1,895 muni) | 25% | `'all'` で計算 |
| competition | **100%** (全 1,895 muni) | 15% | postings 全職業合算 |
| salary_living | **100%** | 10% | 生活コスト proxy = muni 定数 (正しい) |

---

## 2. Phase 1: target_thickness → percentile-based 置換 (推奨)

### 採用根拠 (Agent A)

| 観点 | 評価 |
|---|---|
| DB スキーマ・range check (`is_priority_score_in_range`, `validate_outputs`) | 完全互換 ([0, 200] 維持) |
| 他 3 score 成分との対称性 | `normalize_to_200` (`build_municipality_recruiting_scores.py:75-86`) を再利用 |
| 同値解消 | 千代田区 occ A (raw_idx=3,842) と港区 occ A (raw_idx=3,902) が別 percentile に → 職業別 priority_score 差発生 |
| 営業先への説明 | 「100 = 全国平均」→「100 = 全国中央値」に意味変化、説明可能 |

### 実装スコープ

- `scripts/build_municipality_target_thickness.py:1189-1203`: `derive_thickness_index` を percentile (per-occ 全国 rank → `(rank/n) * 200`) に置換 (~30 行)
- `scripts/build_municipality_recruiting_scores.py:175-177`: コメント `thickness_to_index_200` → `percentile_to_200` に更新
- E2E テスト: `distinct_scores >= 8` を 104 muni で assert (逆証明テスト)

### ロードマップ

1. ローカル実装 (Python build script)
2. ローカル DB で再生成 (ユーザー手動、Claude DB 書込禁止)
3. ローカル PDF で順位変動確認
4. Turso 投入 (無料枠リセット 5/1 以降または容量確認後)
5. 本番 PDF で確認

### 不採択案

| 案 | 不採択理由 |
|---|---|
| cap 撤廃 (200 → 500/1000/無制限) | 配色閾値・バー表示が崩壊、UI 設計やり直し、説明コスト大 |
| log10 スケール変換 | 「100 = 全国平均」直感消失、Round 9 P2-G ですでに却下記録あり |
| ハイブリッド (cap + 都市部別指標) | 単一指標で意思決定したい運用要件に逆行 |

---

## 3. Phase 2: competition_score 職業別化 (推奨)

### 採用根拠 (Agent C)

| 観点 | 評価 |
|---|---|
| postings の職業情報 | `occupation_major_code` **100% 充足** (469,027/469,027) |
| マッピング | postings 15 区分 → thickness 11 区分は **1:1 直接対応** (業界→職業マッピング不要) |
| 職業差実証 | 新宿区 13104: 管理 0.0026 / 保安 0.1378 / 農林漁業 3.26 (**5 桁差**) |
| スコープ制約 | postings は HW 由来だが、competition_score は内部計算のみ → MI 混入リスクなし |

### 実装スコープ

- `scripts/build_municipality_recruiting_scores.py:133-172` の `fetch_competition_score` を職業別化 (+50-70 行)
- postings 名寄せ: `prefecture+municipality` で 93.3% 成功、政令市の区表記揺れ要対処 (`municipality_code_master.alt_names` 活用)
- `compete_v` キーを `muni → (muni, occ)` に変更
- DB 再投入: `municipality_recruiting_scores` 全 20,845 行 DROP+INSERT

### リスク

- 名寄せ失敗 6.7% → density=0 として処理 (現状と同じ挙動)
- HW シェア制約 (HW 求人のみ) を顧客に明示する注記必須

---

## 4. Phase 3: commute weight 再配分 (検討)

### 経緯

Agent B 監査で commute_access の職業別化は **構造的に不可能** と判明:
- e-Stat 公開 API に「市区町村×職業×通勤OD」なし
- `estimate_index` 擬似按分は target_thickness と多重共線性 (r > 0.8)

### 代替案

`build_municipality_recruiting_scores.py:228` の重み配分を変更:
```python
# 旧
weights = {"target": 50.0, "commute": 25.0, "competition": 15.0, "salary": 10.0}

# 新案 (commute を 0 に、再配分)
weights = {"target": 60.0, "commute": 0.0, "competition": 30.0, "salary": 10.0}
# または
weights = {"target": 65.0, "commute": 10.0, "competition": 15.0, "salary": 10.0}  # commute 軽減
```

### 採否判断

Phase 1+2 完了後、cap saturation 解消の効果を測定してから判断。Phase 1+2 で十分な職業差が出るなら commute はそのまま (重み 25% 維持) でよい。

---

## 5. 不採用の根拠

### cap 撤廃 (Agent A 案 A)

- 配色閾値 (`<80/80-120/120-160/>160`) が極端値 (max=2,725) で崩壊
- 都市部以外の差が視覚的に潰れる
- UI 配色設計やり直し

### salary 職業別補正 (Agent D Phase 3g)

- salary_living は **生活コスト proxy** (家賃・物価指数)、職業差はゼロが正解
- 職業別補正は名目賃金との二重計上になる

### Model F2 全体再設計 (Agent D Phase 3h)

- 30-40 人日 = 1.5-2 ヶ月、現在の運用と非両立
- Phase 1+2 で十分な改善が得られれば不要

---

## 6. リスクマトリクス

| Phase | DB 再投入 | 順位変動 | テスト破壊 | 説明責任 |
|---|---|---|---|---|
| Phase 1 (thickness percentile) | 1 テーブル 20,845 行 + 連鎖 | **重** (全自治体ランキング再構築) | 中 (新規逆証明テスト追加) | 中 (「全国順位ベース」と説明) |
| Phase 2 (competition 職業別) | 1 テーブル 20,845 行 | 中 (大都市圏 104 muni のみ変動) | 中 | 軽 (機能向上として説明) |
| Phase 3 (重み再配分) | 1 テーブル 20,845 行 | 中 | 軽 | 中 (重み変更の根拠説明) |

---

## 7. 監査メタデータ

- 並列 agent: 4 件 (A: thickness / B: commute / C: competition / D: 全体)
- 実装変更: ゼロ
- DB 書込: ゼロ
- docs のみ作成 (Round 10 設計監査)
- 影響範囲: なし (read-only)

---

## 8. 着手判断必要事項

実装着手前にユーザー判断が必要:

1. **Phase 1 (thickness percentile) を本ラウンドで実装するか / 別ラウンドで再検討するか**
2. **Phase 1+2 をまとめて実装するか / Phase 1 のみ先行実装するか**
3. **Turso 同期タイミング** (無料枠リセット待ち / 容量確認後)
4. **Round 9 cap saturation 注記との整合**: Phase 1 実装で同値解消 → 注記の文言を「都市部でも職業差を表示」に更新する必要あり

---

**Round 10 設計監査完了。実装着手はユーザー判断後。**
