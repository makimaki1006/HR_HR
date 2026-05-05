# Phase 3 — 政令市の区別 F2 補完アルゴリズム設計

**作成日**: 2026-05-04
**Worker**: B7 (補完ロジック設計担当)
**前提**: Worker A7 が区別データ取得元を並列調査中。本書は取得元に依存せず**アルゴリズム設計**に集中する。
**スコープ**: `build_municipality_target_thickness.py` の F2 推定を 175 件の `area_type='designated_ward'` (政令指定都市の行政区) に拡張するための roll-down ロジック設計のみ。
**禁止事項**: DB 書き込み / 実装着手 / Turso / Rust / 既存ファイル変更。

---

## 1. 背景

| 項目 | 現状 |
|------|------|
| `municipality_code_master` | `area_type='designated_ward'`, `area_level='unit'`, `parent_code='14100'` (例: 横浜市鶴見区 14101 → 横浜市 14100) の親子構造あり |
| `build_municipality_target_thickness.py` の F2 推定 | 親市 (例: 横浜市 14100) 単位までしか出さない。175 区別の thickness_index が欠落 |
| F2 補正項 | F1 (人口比) / F2 (年齢性別) / F3 (産業構成) / F4 (昼夜間) / F5 (通勤 OD) / F6 (本社過剰) |
| Plan B 設計 | `municipality_occupation_population` および `v2_municipality_target_thickness` には親市集約コード (14100 等) を**入れない**。区別 unit のみ投入 |

---

## 2. 補完アルゴリズム全体像

```
入力:
  - 政令市本体の F2 推定値 (occupation 別 thickness_index) ─── 既に算出済み
  - 区別の人口比                                       ─── Worker A7 調達待ち
  - 区別の補正データ (industry/daytime/od/sn)             ─── 部分的に既存

処理:
  1. 親市 → 区への roll-down (F1 人口比按分が骨格)
  2. 区別データが存在する補正項は直接適用 (F5 OD は確実)
  3. 親市 SUM で整合性 scaling
  4. industrial_anchor 判定の伝搬

出力:
  - 区別 thickness_index per occupation (175 ward × N occupations)
  - 区別 rank / priority / scenario_*_index 派生指標
  - municipality_occupation_population 区別行
  - v2_municipality_target_thickness 区別行
```

---

## 3. roll-down 関数 (擬似コード)

```python
def rolldown_parent_to_wards(
    parent_code: str,
    wards: list[str],
    parent_thickness: dict[str, float],   # {occ_code: thickness_index}
    population_share: dict[str, float],   # {ward_code: ward_pop / sum(wards_pop)} 合計 1.0
    ward_overrides: dict[str, dict[str, float]] | None = None,
) -> dict[str, dict[str, float]]:
    """
    親市の F2 推定 (occupation 別) を、区の人口比で配分。
    ward_overrides が与えられた補正項は直接使用、それ以外は人口比按分。

    parent_code: 政令市本体コード (例: '14100')
    wards: 区コードのリスト (例: ['14101', '14102', ...])
    parent_thickness: 親市の occupation 別 thickness_index
    population_share: ward_code → 区の対親市人口比 (合計 1.0)
    ward_overrides: ward_code → {補正項キー: 値} (区別データが既存の場合)

    Returns: {ward_code: {occ_code: thickness_index}}
    """
    out = {}
    n_wards = len(wards)
    for w in wards:
        share = population_share.get(w, 1.0 / n_wards)  # フォールバック: 均等
        out[w] = {}
        for occ, parent_idx in parent_thickness.items():
            # 基本: 人口比按分
            ward_idx = parent_idx * share * n_wards  # 平均=親市値となるよう正規化
            # override があれば差し替え
            if ward_overrides and w in ward_overrides:
                ward_idx = apply_overrides(ward_idx, ward_overrides[w], occ)
            out[w][occ] = ward_idx
    # scaling: Σ_w out[w][occ] / n_wards == parent_thickness[occ] となるよう再正規化
    return normalize_to_parent_mean(out, parent_thickness, n_wards)
```

ポイント:

- `share * n_wards` で「平均=親市値」を保つ (人口比 = 1/N の区は親市値そのまま)
- `normalize_to_parent_mean` で丸め誤差を吸収し**親市平均との整合性を厳密に確保**
- `ward_overrides` は補正項単位 (industry/daytime/od/sn) のキーで指定し、未提供のキーは親市値を継承

---

## 4. 各補正項の区別配分方法

| 補正項 | 親市値の出所 | 区別への配分方法 | 区別データ可用性想定 |
|-------|------------|----------------|------------------|
| **F1 人口比** | 親市総人口 | 区別人口比で按分 (Worker A7 調達) | 高 (e-Stat 国勢調査区別あり) |
| **F2 年齢性別** | 親市の年齢性別ピラミッド | 区別ピラミッドがあれば直接、なければ親市平均で按分 | 中 (e-Stat の市区町村小区分が区別含むか要確認) |
| **F3 産業構成** | `v2_external_industry_structure` 親市行 | 区別経済センサスがあれば直接、なければ親市 F3 を全区均等適用 | 低〜中 (経済センサスの区別データ量要確認) |
| **F4 昼夜間** | 親市の昼夜間人口比 | 区別の昼夜間が e-Stat にあれば直接、なければ親市値を全区適用 | 低 (区別データの存在性が低い) |
| **F5 通勤 OD** | 親市の流入率 | `commute_flow_summary` は **JIS 5 桁準拠で既に区別 OD あり** ✅ 直接使用 | 高 (既存) |
| **F6 本社過剰** | SalesNow 親市集計 | SalesNow 住所カラムから区を抽出再計算可能 | 中 (住所パース実装次第) |

---

## 5. フォールバック優先度

| 優先度 | 補正 | 区別データ実装方針 | フォールバック |
|:----:|-----|----------------|--------------|
| 高 | F5 (OD) | `commute_flow_summary` 直接使用 | (不要) |
| 高 | F1 (人口比) | Worker A7 調達 e-Stat 区別人口 | 親市人口を区数で均等按分 (精度低) |
| 中 | F2 (年齢性別) | e-Stat 市区町村小区分から区別ピラミッド抽出 | 親市ピラミッドを全区継承 |
| 中 | F6 (SalesNow) | SalesNow 住所→区マッピング再集計 | 親市 F6 を全区均等適用 |
| 低 | F3 (産業構成) | 経済センサス区別データ取得 | 親市 F3 を全区均等適用 |
| 低 | F4 (昼夜間) | (取得困難) | 親市 F4 を全区均等適用 |

---

## 6. industrial_anchor 判定の取扱い

`compute_industrial_anchor` の 4 条件 AND は親市単位で判定。区別での扱い:

- **(a) 案 (デフォルト採用)**: 親市の anchor 判定を全区に伝搬
  - 利点: 実装が単純、Worker A7 のデータ量に依存しない
  - 欠点: 横浜市が anchor=False の場合、工業臨海区も False になる
- **(b) 案 (将来拡張)**: 区別データが揃えば再計算
  - F3 産業構成 + F4 昼夜間が区別で揃った後に有効化
  - Phase 4 以降の拡張案として留保

豊田市は政令市ではない `area_type='municipality'` のため anchor 判定は元のまま影響なし。問題は政令市 (横浜/大阪/名古屋/川崎/福岡 等) の区。

---

## 7. 親市集約値の扱い (Plan B 整合)

| テーブル | 親市コード (例 14100) | 区別コード (例 14101-14118) |
|---------|--------------------|--------------------------|
| `municipality_occupation_population` | **入れない** | 投入 |
| `v2_municipality_target_thickness` | **入れない** | 投入 |
| UI で「横浜市全体」表示 | 派生 view で 18 区を SUM/AVG (§11) | 個別表示 |

これにより重複集計を防ぎ、Phase 3 の Plan B 設計と整合する。

---

## 8. 推定式 (基本ケース)

```
thickness_index_ward[w, occ] = thickness_index_parent[parent(w), occ]
                              × (population_ward[w] / population_parent[parent(w)])
                              × ratio_normalization

ratio_normalization:
  Σ_w {thickness_index_ward[w, occ]} / N_wards == thickness_index_parent[parent, occ]
  となる scaling 係数 (基本 1.0、丸め誤差吸収用)
```

override 適用時 (例: F5 OD が区別で異なる):
```
thickness_index_ward[w, occ] = base_rolldown[w, occ] × (od_ward[w] / od_parent[parent(w)])
```

---

## 9. 検証戦略

| 検証項目 | 判定基準 |
|--------|--------|
| 親市平均整合性 | `Σ_w thickness_index_ward[w, occ] / N_wards ≈ thickness_index_parent[parent, occ]` (誤差 < 1%) |
| 区別 rank 安定性 | 1 区が 1 occ で全国 TOP 1 にならない (人口希釈効果の確認) |
| 横浜 18 区範囲 | 各区の指数が市平均から ±50% の範囲に収まる |
| 親市 occupation rank 不変 | 区別投入後、親市の occupation 別順位が大きく変動しない |
| anchor 伝搬整合性 | 親市 anchor=True の区は全て True (デフォルト採用時) |
| OD override 効果 | F5 区別データ適用区で、override なし版との差分が確認できる |
| サンプル目視 | 横浜/大阪/名古屋/川崎/福岡の代表区 (各 3 区) で異常値なし |

---

## 10. 実装ロードマップ

| Phase | 作業 | 所要 | 依存 |
|------:|------|:----:|------|
| 1 | Worker A7 結果待ち (区別人口データの取得経路確定) | 0.5 日 | A7 |
| 2 | `build_municipality_target_thickness.py` に rolldown 関数追加 | 1.0 日 | Phase 1 |
| 3 | Worker A7 が調達した区別データのローカル投入 (`v2_external_population` 拡張) | 0.5 日 | Phase 1 |
| 4 | F2 再実行で 175 designated_ward を含む CSV 生成 | 0.5 日 | Phase 2-3 |
| 5 | `municipality_occupation_population` と `v2_municipality_target_thickness` の 2 テーブルに再投入 | 0.5 日 | Phase 4 |
| 6 | サンプル検証 (横浜/大阪/名古屋/川崎/福岡の代表区) | 0.5 日 | Phase 5 |

**合計 3.5 日 (Worker A7 のデータ調達後)**

---

## 11. UI 側での「政令市全体」集計

Plan B により親市コード (14100 等) はテーブルに**存在しない**ため、UI で「横浜市」全体を見るには派生 view または SQL で集計する。

```sql
-- 区別を SUM / AVG / 重み付き平均で親市集約
SELECT
    mcm.parent_code AS parent_municipality_code,
    mcm.prefecture,
    occ_code,
    AVG(v.thickness_index)             AS avg_thickness_index,
    SUM(v.scenario_standard_index)     AS sum_scenario_standard,
    SUM(v.scenario_aggressive_index)   AS sum_scenario_aggressive,
    COUNT(*)                           AS ward_count
FROM v2_municipality_target_thickness v
JOIN municipality_code_master mcm
  ON v.municipality_code = mcm.municipality_code
WHERE mcm.area_type = 'designated_ward'
GROUP BY mcm.parent_code, mcm.prefecture, occ_code;
```

人口加重平均が必要な場合は `v2_external_population` を JOIN して `SUM(idx × pop) / SUM(pop)` 形式で算出する。

---

## 12. 既知のリスク

| # | リスク | 影響 | 緩和策 |
|---|------|-----|------|
| 1 | Worker A7 で区別常住人口データが取得できない | roll-down が単純均等按分になり精度低下 | 親市人口/区数で均等按分を許容、ロードマップ Phase 1 でフォールバック判定 |
| 2 | F3/F4/F6 が親市値継承の場合、区別差別化が出ない | Model F2 で工業都市が浮上した効果が消失、区間で thickness が均一化 | 中長期で Phase 4 (b) 案 (区別再計算) を検討 |
| 3 | 親市が anchor=False の場合、区別もすべて False | 横浜/大阪等で anchor boost が効かない | (a) 案デフォルト採用は受容。将来 (b) 案で個別判定 |
| 4 | 産業構造 (F3) が区別で大きく異なる (横浜港湾区 vs 山手地区) | 同一 thickness で扱われる差別化困難 | 経済センサスの区別データ取得を Phase 4+ で評価 |

---

## 13. 出力先テーブル仕様 (再掲)

| テーブル | キー | 投入対象 | 補足 |
|---------|-----|--------|------|
| `municipality_occupation_population` | `(municipality_code, occupation_code)` | 175 区 × N occ | 親市コードは入れない |
| `v2_municipality_target_thickness` | `(municipality_code, occupation_code)` | 同上 | thickness_index, scenario_*_index, rank, priority 含む |

---

## 14. まとめ

- 政令市の区別 F2 推定は **親市 → 区の roll-down (人口比按分 + override)** を骨格とする
- F5 (OD) は既存 `commute_flow_summary` 5 桁準拠で**確実に区別差別化**できる
- F1 区別人口は Worker A7 調達結果が骨格、フォールバックは均等按分
- F2/F3/F4/F6 は段階的に区別データを取り込む (デフォルトは親市値継承)
- Plan B との整合性を保ち、親市コードは投入しない (UI 側で集計 view)
- 実装は **3.5 日 + Worker A7 のデータ調達待ち**

設計のみ。実装着手は Worker A7 結果と本書の合意後。
