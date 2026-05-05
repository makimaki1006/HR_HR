# Phase 3 Step 5: 政令市の区別 resident 推定欠損 — データ取得元調査

**Worker**: A7
**日付**: 2026-05-04
**対象**: `build_municipality_target_thickness.py` の入力 `v2_external_population` における designated_ward (政令市の区) 175 件の常住人口データ欠損
**作業範囲**: 調査のみ。READ-only。

---

## 0. 結論

**推奨ルート**: **A 案 — e-Stat 国勢調査 R2「人口等基本集計」(sid=0003445080 系) から designated_ward 175 件の常住地ベース×年齢×性別人口を新規 fetch**

- B (既存テーブル流用) は実質不可能 (区別データが全テーブルで欠損)
- C (15-1 workplace から逆算) は workplace ≠ resident の構造的ずれが大きく、F2 推定の品質が逆に悪化するリスク
- A は既存 `v2_external_*` 系の取り込みパイプラインを踏襲でき、データ整合性も最も高い

**実装日数見積**: **2.0 ～ 2.5 人日** (e-Stat API 取得スクリプト 0.5 日 + ETL 統合 0.5 日 + `build_municipality_target_thickness.py` 修正 0.5 日 + 検証 0.5 日 + 余裕 0.5 日)

---

## 1. ローカル DB の現状調査結果

DB: `data/hellowork.db` (READ-only)
designated_ward 全件: **175 件** (`municipality_code_master` の `area_type='designated_ward'`)

### 1.1 `v2_external_population`

| 項目 | 値 |
|------|-----|
| 全体行数 | 1,742 |
| `municipality LIKE '%区'` 行数 | 23 (= 東京 23 特別区のみ) |
| designated_ward 突合数 | **0 / 175** |
| 参照日 | 2020-10-01 (R2 国勢調査) |

横浜市 → 「神奈川県・横浜市」の 1 行のみ。鶴見区など個別の区は **完全欠損**。
→ Phase 3 Step 5 の症状を完全に再現。

### 1.2 `v2_external_population_pyramid`

| 項目 | 値 |
|------|-----|
| ward 行数 (`%区`) | 207 (= 東京 23 区 × 約 9 年齢階級) |
| designated_ward 突合数 | **0 / 175** |

年齢×性別ピラミッドも区別データ欠損。F2 推定で必要な年齢分布補完にも使えない。

### 1.3 `v2_external_daytime_population`

| 項目 | 値 |
|------|-----|
| ward 行数 | 23 (東京特別区のみ) |
| designated_ward 突合数 | **0 / 175** |

昼夜間人口比も同様に欠損。

### 1.4 `commute_flow_summary`

| 項目 | 値 |
|------|-----|
| 全体行数 | 27,879 |
| designated_ward as destination | **175 / 175** |
| designated_ward as origin | **175 / 175** |

JIS 5 桁ベースのため designated_ward を完備。ただし、これは「通勤フロー件数」であり、年齢×性別の常住人口を直接持つわけではない。`target_origin_population` カラムは存在するが、これは origin ward 全体の通勤者母数のため、F2 用の常住人口（年齢×性別×区）を直接代用できない。**部分的に 1 ステップ目で活用可能だが、補完元としては不十分。**

### 1.5 `municipality_occupation_population` (15-1 workplace measured)

| 項目 | 値 |
|------|-----|
| basis × data_label | `workplace × measured`, `resident × estimated_beta` |
| designated_ward (workplace × measured) | **175 / 175** ✅ |

**重要**: 15-1 workplace × measured では designated_ward 175 件すべてに **年齢×性別×職業×区** の実測値が入っている。区別の従業地ベース人口総和は計算可能。
ただし以下の構造的問題:

- 15-1 は「**従業地・通学地ベース**」。大阪市中央区の workplace=376,752 は「中央区で働く人」であり、「中央区に住む人」ではない。
- 政令市中心区では昼夜間人口比 5～10 倍も珍しくなく、resident の代理として使うと **過大評価** (中央区/北区/博多区/中区など) と **過小評価** (郊外区) が同時に発生。
- F2 推定が現状値より悪化するリスクが高い。

---

## 2. 15-1 workplace を resident 補完に使う案 (C 案) の評価

| 観点 | 評価 |
|------|------|
| カバレッジ | ◎ designated_ward 175/175 完備 |
| 年齢×性別粒度 | ◎ 既存スキーマで取得済 |
| 常住地としての妥当性 | ✗ 政令市中心区で構造的に過大、郊外区で過小 |
| 補正係数の入手性 | △ 政令市本体の dayttime/nighttime 比は v2_external_daytime_population にあるが、区別の昼夜比は欠損。比例配分の根拠が弱い |
| F2 推定の質 | ✗ 現状 (欠損) よりは数値が出るが、ロジックの説明が困難。営業ツール用途で「鶴見区の resident 人口=workplace の値」と説明するのは実務上耐えない |

**結論**: 緊急避難策にしかならず、A 案実施までの繋ぎ以上の価値は無い。

---

## 3. e-Stat 常住地ベース表の調査結果 (A 案)

WebFetch で確認した候補:

| 統計表 ID | 表名 | ベース | 区粒度 | 用途適合 |
|----------|------|--------|--------|----------|
| **0003445080** | 「世帯の家族類型，世帯人員の人数別一般世帯数－全国，都道府県，**市区町村**」(R2 人口等基本集計) | 常住地 | 政令市の区を含む | 世帯系のため、年齢×性別人口は別表参照 |
| 0003445138 | 「男女，年齢（5歳階級），国籍総数か日本人別人口」(R2) | 常住地 | 国・都道府県 | ✗ 区粒度なし |
| 0003454503 | 「男女，年齢，産業別就業者数（15歳以上）」(15-1 相当) | **従業地** | designated_ward 含む | 既に取得済 (15-1) |

**最有力**: 「人口等基本集計」(toukei=00200521, tstat=000001136464) 系のうち、**男女×年齢（5歳階級）×市区町村** の表。R2 国勢調査 人口等基本集計には 210 表あり、市区町村粒度 (政令市の区を含む) で年齢×性別の常住人口を提供する表が存在することは公式インデックスで確認 (sid 一覧の細かい確認は appId 取得後の API 検索で最終確定する想定)。

代表的に想定される sid:
- 「男女，年齢，配偶関係別人口（市区町村）」(R2)
- 「男女，年齢（5歳階級）別人口（市区町村）」(R2)

これらは **既存 `v2_external_population` (1,742 行 = 都道府県 47 + 市区町村 1,719 - 政令市の区 0)** に **政令市の区 175 件** を追加するだけで完結する。

---

## 4. 推奨ルート 比較

| 案 | 内容 | 工数 | 品質 | リスク |
|----|------|------|------|--------|
| **A** | e-Stat R2 人口等基本集計 から designated_ward 175 件を fetch、`v2_external_population` & `_pyramid` に追加 | **2.0～2.5 日** | ◎ | 低 (e-Stat 公式データ) |
| B | 既存テーブル流用 | — | — | **実行不能** (全テーブル欠損) |
| C | 15-1 workplace から区別比を逆算 | 0.5 日 | ✗ | 中～高 (構造的ずれ) |

→ **A 案を採用。** B は技術的に成立しない。C は緊急避難でも採用すべきでない。

---

## 5. 実装ロードマップ (A 案)

### Phase A-1: データ取得 (0.5 日)

- e-Stat API (statsDataId 系) で R2 人口等基本集計の **男女×年齢（5歳階級）×市区町村** 表を特定
  - 最終 sid 確定は appId を使った `getStatsList` 呼び出しで行う (本調査範囲外)
- 取得スクリプト: `scripts/fetch_estat_resident_population_wards.py`
- 取得対象: designated_ward 175 件 (JIS 5 桁コード フィルタ)
- 出力: 中間 CSV (prefecture, municipality, total, male, female, age_0_14, age_15_64, age_65_over, age_group×gender ピラミッド)

### Phase A-2: ETL 統合 (0.5 日)

- 既存 `v2_external_population` スキーマに合わせ INSERT
  - `municipality` カラムに「鶴見区」など区名のみで投入 (現状の特別区 23 件と同じ命名)
  - prefecture は「神奈川県」「大阪府」等の親都道府県
  - reference_date='2020-10-01'
- 同様に `v2_external_population_pyramid` にも 175×9～10 階級 ≒ 1,575 行追加
- (オプション) `v2_external_daytime_population` の区別データも同 fetch で取得し追加

### Phase A-3: パイプライン修正 (0.5 日)

- `build_municipality_target_thickness.py` の JOIN 条件確認
  - 現状: `prefecture + municipality` で JOIN している場合、追加データで自動補完される可能性
  - 政令市本体 (横浜市) と区 (横浜市鶴見区) で重複参照しないようロジック確認
- 必要に応じて `municipality_code_master.area_type='designated_ward'` を優先する分岐を追加

### Phase A-4: 検証 (0.5 日)

- designated_ward 175 件の F2 推定が全件出力されること
- 政令市本体 (横浜市など 20 件) と区合計の整合性チェック (区合計 ≒ 本体の ±1% 以内)
- 15-1 workplace との比 (= 昼夜間人口比) が政令市中心区で 2～10 倍、郊外区で 0.7～1.0 倍の妥当範囲に収まること

### 余裕枠 (0.5 日)

- 既存テストの修正、ドキュメント更新

---

## 付録: 参照クエリ

```sql
-- designated_ward 全件
SELECT municipality_code, prefecture, municipality_name
FROM municipality_code_master
WHERE area_type='designated_ward'
ORDER BY municipality_code;

-- v2_external_population に追加すべき件数
SELECT COUNT(*) FROM municipality_code_master mcm
WHERE mcm.area_type='designated_ward'
  AND NOT EXISTS (
    SELECT 1 FROM v2_external_population p
    WHERE p.prefecture=mcm.prefecture AND p.municipality=mcm.municipality_name
  );
-- 期待値: 175
```
