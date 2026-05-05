# SURVEY MARKET INTELLIGENCE - Phase 3 Step 5: レポート表示仕様 (案B 並列保管対応版)

**Version**: 2.0 (案B / 続編)
**Status**: 確定（実装ガードレール）
**Date**: 2026-05-04
**Author**: Worker C4
**Base**: `SURVEY_MARKET_INTELLIGENCE_PHASE3_DISPLAY_SPEC.md` v1.0 (2026-05-04, Worker C2)
**Scope**: 旧 v1.0 の **続編** として、案B (workplace 実測 + resident 推定の **並列保管**) に対応する表示分岐ルールを追加する。旧 v1.0 は **置換ではなく並立**。

---

## 0. 本書の位置付け

### 0.1 旧書との関係

| 項目 | 旧書 (v1.0) | 本書 (v2.0 / 案B) |
|------|------------|------------------|
| 前提 | 推定 (F2) のみ | 実測 (15-1 国勢調査 R2) + 推定 (F2) **並列** |
| 表示 | 指数/ランク/濃淡のみ | (basis, data_label) で **4 通り分岐** |
| 人数表示 | 完全禁止 | 条件付き許可 (workplace + measured のみ) |
| 関係 | 全章 有効 | 旧書 §3 の Hard NG リストを **§7.1 で部分修正** |

**読み方**: 旧書で全体方針を理解した上で、本書で分岐ロジックを上書き適用する。

### 0.2 案B 全体像

| 軸 | データソース | 人数表示 |
|----|-------------|:-------:|
| `basis='workplace'` + `data_label='measured'` | e-Stat 15-1 国勢調査 R2 実測 | **OK** |
| `basis='workplace'` + `data_label='estimated_beta'` | F2 推定 (15-1 のない地域 fallback) | NG (指数のみ) |
| `basis='resident'` + `data_label='estimated_beta'` | F2 推定 | NG (指数のみ) |
| `basis='resident'` + `data_label='measured'` | (将来) e-Stat resident 実測 | (将来 OK) |

---

## 1. 表示判定ルール (UI 実装で必須)

```
表示判定 (UI 実装で必須):
1. basis 軸を取得: 'workplace' / 'resident'
2. data_label 軸を取得: 'measured' / 'estimated_beta'
3. (basis, data_label) によって表示形式を決定 (本書 §2 の表)
4. 凡例とラベルを必ず併記
```

### 1.1 擬似コード (Rust)

```rust
match (cell.basis.as_str(), cell.data_label.as_str()) {
    ("workplace", "measured")        => render_measured_workplace(cell),
    ("workplace", "estimated_beta")  => render_estimated_workplace(cell),
    ("resident",  "estimated_beta")  => render_estimated_resident(cell),
    ("resident",  "measured")        => render_measured_resident(cell),  // 将来
    _ => render_unknown_with_warning(cell),
}
```

### 1.2 必須ルール

- `basis` と `data_label` が **両方** DTO/レスポンスに含まれていなければ表示してはならない (UI 側で `unreachable!` にしない)
- 凡例 (📊 実測 / β 推定) と軸ラベル (🏢 従業地 / 🏠 常住地) は **必ず併記**
- 並列画面では `basis` の意味解説 (§3.3 注記) を画面上に必ず置く

---

## 2. (basis, data_label) ごとの表示仕様

| basis | data_label | 主表示 | ラベル | バッジ |
|-------|-----------|--------|--------|--------|
| workplace | measured | **人数 + 年齢/性別内訳** | 実測 (国勢調査 R2・従業地ベース) | 🏢 従業地 / 📊 実測 |
| workplace | estimated_beta | 指数 0-200 + 濃淡 | 検証済み推定 β (従業地) | 🏢 従業地 / β |
| resident | estimated_beta | 指数 0-200 + 濃淡 | 検証済み推定 β (常住地) | 🏠 常住地 / β |
| resident | measured | (将来) 人数 | 実測 (常住地・国勢調査 R2) | 🏠 常住地 / 📊 実測 |

### 2.1 各セルで使用可能な指標

| 指標 | workplace+measured | workplace+estimated_beta | resident+estimated_beta | resident+measured |
|------|:----:|:----:|:----:|:----:|
| `population` (実測就業者数) | OK | NG | NG | (将来 OK) |
| 年齢別内訳 | OK | NG | NG | (将来 OK) |
| 性別内訳 | OK | NG | NG | (将来 OK) |
| `thickness_index` (0-200) | (補助) | OK | OK | (補助) |
| `distribution_priority` (A-D) | (補助) | OK | OK | (補助) |
| `rank_in_occupation` (順位) | OK | OK | OK | OK |
| シナリオ濃淡 | NG | OK | OK | NG |
| `is_industrial_anchor` | OK | OK | OK | OK |

---

## 3. UI モック (4 種類)

### 3.1 workplace 実測 (15-1)

```
================================================
神奈川県 川崎市 - 08_生産工程 (従業地ベース 🏢📊)
================================================
実測 就業者数: 86,313 人 (国勢調査 R2)
内訳:
  男性 70,221 人 / 女性 16,092 人
  25-34 歳: 22,408 人 (主力層)
  35-44 歳: 18,755 人
  45-54 歳: 16,201 人
  ...

※ 従業地 (work-based) です。「川崎市にある事業所で働いている人」
   を意味します。「川崎市に住んでいる人」とは異なります。
================================================
```

### 3.2 resident 推定 (F2)

```
================================================
神奈川県 川崎市 - 08_生産工程 (常住地ベース 🏠β)
================================================
ターゲット厚み指数: 138 (推定 / 全国平均比 +38%)
配信優先度: A ランク (上位 5%)
全国順位: 7 位 / 1,742 市区町村
産業集積タイプ: 🏭 工業集積地

採用シナリオ濃淡:
  保守 ▆▆ / 標準 ▆▆▆▆ (推奨) / 強気 ▆▆▆▆▆▆ (上振れ余地)

※ 常住地 (resident-based) の推定値です。「川崎市に住んでいる
   生産工程従事者の濃淡」を相対的に示しています。実測の人数
   ではありません。weight_source = hypothesis_v1 / β版。
================================================
```

### 3.3 混在画面 (workplace 実測 + resident 推定 並列)

```
================================================
神奈川県 川崎市 - 08_生産工程
================================================

🏢 従業地 (実測 / 国勢調査 R2):
  実測就業者数: 86,313 人
  男性 70,221 / 女性 16,092

🏠 常住地 (推定 β / 検証済み推定):
  ターゲット厚み指数: 138 (推定)
  配信優先度: A ランク (上位 5%)

[凡例]
  📊 実測: 国勢調査 R2 (基準年 2020)
  β  推定: F2 モデル (weight_source=hypothesis_v1)

※ 従業地と常住地は異なる概念です:
   従業地 = その地域で働いている人
   常住地 = その地域に住んでいる人
================================================
```

### 3.4 都道府県内ランキング (混在モード)

```
=== 神奈川県内 08_生産工程 ターゲット (常住地推定) ===
1. 横浜市      指数 156 (推定 β) | 配信優先度 A
2. 川崎市      指数 138 (推定 β) | 配信優先度 A
3. 相模原市    指数 122 (推定 β) | 配信優先度 B
...

=== 神奈川県内 08_生産工程 就業者 (従業地実測) ===
1. 横浜市     217,629 人 (実測)
2. 川崎市      86,313 人 (実測)
3. 相模原市    38,740 人 (実測)
...
```

---

## 4. 「実測」「推定」の表記ルール

### 4.1 必ず併記する項目

- 数値の前後に「実測」または「(推定)」を必ず付与
- 大きな数字には「(国勢調査 R2)」や「β」バッジ
- 単位 (人 / 指数 / ランク) を明確化

### 4.2 Hard NG (実測モードでも NG な表現)

| NG | 理由 |
|----|------|
| 「川崎市に生産工程職が 86,313 人住んでいる」 | 実測は **従業地**、住んでいるとは違う |
| 「川崎市の市場規模 ○○億円」 | 人数 × 単価 = 派生指標、実測でない |
| 「川崎市の採用ターゲット総数 ○○○人」 | 実測 + 推定の混同 |

### 4.3 グレーゾーン (要文脈)

| 表現 | OK 条件 |
|------|---------|
| 「川崎市の生産工程従事者数」 | (従業地) を併記 |
| 「川崎市で働いている生産工程従事者」 | OK |
| 「川崎市に住んでいる」 | (常住地推定) を併記 |
| 「採用候補プール」 | (推定指数) を併記 |

---

## 5. 注意書き文言 (実測対応)

### 5.1 Standard (実測 + 推定 混在画面)

```
本ダッシュボードは 2 種類のデータを並列表示しています:

📊 従業地ベース (実測):
  国勢調査 R2 (令和 2 年・2020 年) の市区町村×職業別就業者数を
  実測値で表示しています。「その地域で働いている人」を意味します。

β 常住地ベース (推定 β版):
  独自推定モデル F2 (Model F2、estimate_grade A-) で算出した
  「その地域に住んでいる人材の濃淡」を相対指数で表示しています。
  実測値ではありません。
  weight_source = hypothesis_v1 (e-Stat 実測値置換予定)。

採用配信地域選定: 従業地 (実測人数) を主軸に推奨。
採用候補プール濃淡: 常住地 (推定指数) を補助指標として参照。
```

### 5.2 コンパクト注記 (UI 制約時)

```
※ 従業地=実測 (国勢調査 R2) / 常住地=推定 β。両者は別概念。 [詳細]
```

### 5.3 PDF レポート フッター注記

```
本レポートは Hellowork-Deploy が e-Stat 公開統計と SalesNow 企業データベースを統合した
推定指標と、国勢調査 R2 (15-1) 従業地実測値を **並列表示** しています。
従業地 (workplace) は実測、常住地 (resident) は推定 β版です。
推定モデル: Model F2 (estimate_grade A-) | weight_source: hypothesis_v1
```

---

## 6. ラベル / バッジ仕様

| ラベル | 表示位置 | 色 | 用途 |
|-------|---------|-----|------|
| 🏢 従業地 / 📊 実測 | 数値の左 | 緑 | 15-1 実測 |
| 🏠 常住地 / β | 数値の左 | 黄 | F2 推定 |
| (国勢調査 R2) | 数値の右、小 | 灰 | 出典 |
| (推定) | 数値の右、小 | 黄 | β |
| 配信優先度 A | 強調バッジ | 緑 | 上位 5% |
| 🏭 工業集積地 | バッジ | 灰 | is_industrial_anchor |

### 6.1 バッジ実装ルール

- 🏢 / 🏠 は **必ず数値の左** (basis 軸の即時識別性)
- 📊 / β は **必ず basis バッジの直後** (data_label の即時識別性)
- 出典 / β / 配信優先度 / 集積タイプ は **数値の右側** に配置
- 混在画面では 🏢 ブロックと 🏠 ブロックを **視覚的に区切る** (枠線 or 余白)

---

## 7. ガードレール (エンジニア向け)

### 7.1 旧 DISPLAY_SPEC.md の禁止リストとの差分

旧書 §3 (= v1.0 §2) の Hard NG リストは以下のように **修正**:

| 旧 NG | 新仕様 |
|-------|--------|
| 「○市の生産工程従事者は約 86,000 人」 | (従業地・実測) を併記すれば OK |
| 「○市には 86,000 人の生産工程職」 | (従業地) を併記すれば OK、ただし「住んでいる」とは書かない |
| 「○市の生産工程ターゲット総数 ○○○人」 | OK 化不可 (実測 + 推定の混同なので) |
| 「採用市場規模 ○○億円」 | NG 維持 (派生指標) |

**差分件数**: NG → 条件付き OK 化 = **2 件** / NG 維持 = **2 件** / 完全 NG (混同系) = **1 件**

### 7.2 v1.0 §9.4 ログ出力ルールの修正

旧書では「○○市の生産工程従事者: ○○人」という人数ログを禁止していたが、
v2.0 では `basis=workplace, data_label=measured` のセルに限り **実測ログ OK**:

```
OK: "kawasaki 08_seisan basis=workplace measured population=86313"
OK: "kawasaki 08_seisan basis=resident estimated_beta thickness_index=138"
NG: "kawasaki 08_seisan population=86313"  // basis/data_label なし
```

### 7.3 DTO 修正 (v1.0 §9.2 の修正版)

```rust
// Rust DTO (新)
pub struct OccupationCellDto {
    pub municipality_code: String,
    pub municipality_name: String,
    pub basis: String,           // 'workplace' / 'resident'
    pub occupation_code: String,
    pub occupation_name: String,
    pub age_class: String,
    pub gender: String,

    // 実測モード
    pub population: Option<i64>,         // measured のみ
    pub data_label: String,              // 'measured' / 'estimated_beta'
    pub source_name: String,             // 'census_15_1' / 'model_f2_v1'

    // 推定モード
    pub estimate_index: Option<f64>,     // estimated_beta のみ (0-200)
    pub rank_in_occupation: Option<i64>,
    pub distribution_priority: Option<String>,  // 'A','B','C','D'
    pub weight_source: Option<String>,   // 'hypothesis_v1' / 'estat_R2_xxx'
    pub is_industrial_anchor: bool,
}
```

### 7.4 v1.0 §9.2 の禁止 DTO ルールの修正

旧書では `population` を **完全禁止** としていたが、v2.0 では:

| 旧 (v1.0) | 新 (v2.0 案B) |
|-----------|--------------|
| `population: i32` 完全禁止 | `population: Option<i64>` 許可 (`data_label='measured'` 時のみ Some) |
| `target_count`, `estimated_workers` 禁止 | 維持 (NG) |
| `market_size_yen` 禁止 | 維持 (NG) |

**MUST**: `population` フィールドを使う際は **必ず** `data_label` と `basis` を同 DTO 内に持たせる。
**MUST NOT**: `population` を `data_label` チェック無しで `unwrap()` しない (推定セルで panic)。

### 7.5 SQL クエリ (v1.0 §9.3 の修正)

```sql
-- OK (新)
SELECT basis, data_label, population, estimate_index, source_name
FROM occupation_cells
WHERE municipality_code = ? AND occupation_code = ?;

-- NG 維持
SELECT SUM(employees) AS total_population FROM ...;  -- 命名による混同
```

---

## 8. 顧客プレゼンの新セールストーク (案B 用)

| 旧 NG | 新 OK |
|-------|-------|
| 「○○市は採用しやすい」 | 「○○市の従業地ベースでは生産工程従事者が ○ 万人 (実測)。常住地推定でも上位 5% (推定 β)」 |
| 「○○市にはターゲットが厚い」 | 「○○市は従業地・常住地の両面で工業集積地です」 |
| 「○○市に ○ 万人の人材プールがある」 | 「○○市は従業地実測 ○ 万人、常住地推定でも上位 ○%」 |

### 8.1 営業文脈での 3 段階表現

1. **実測の事実** (workplace + measured): 「川崎市の事業所で働く生産工程従事者は **86,313 人** (国勢調査 R2 実測)」
2. **推定の濃淡** (resident + estimated_beta): 「常住地ベースでも厚み指数 **138 / A ランク**」
3. **複合判断**: 「働く場所と住む場所の両面でターゲットが濃い地域」

---

## 9. テスト観点の追加 (v1.0 §10 の補強)

### 9.1 ドメイン不変条件 (案B 追加分)

```
- (basis, data_label) は 4 組合せのいずれか
- data_label='measured' なら population が Some であること
- data_label='estimated_beta' なら estimate_index が Some であること
- data_label='measured' なら estimate_index は None でも OK
- basis='workplace' AND data_label='measured' のみ population 表示が許可される
- source_name と (basis, data_label) の整合性:
    workplace+measured       → 'census_15_1'
    workplace+estimated_beta → 'model_f2_v1'
    resident+estimated_beta  → 'model_f2_v1'
```

### 9.2 視覚レビューチェックリスト (追加)

```
□ 🏢 / 🏠 バッジが basis 軸で正しく分かれて表示される
□ 📊 / β バッジが data_label 軸で正しく分かれて表示される
□ 混在画面で 🏢 と 🏠 が視覚的に区切られている (枠線/余白)
□ 「住んでいる」「働いている」の文言が basis に応じて正しく出る
□ workplace+measured セルで population 表示、estimated_beta セルで指数のみ
□ 凡例 (📊 実測 / β 推定) が画面上に常に見える
```

---

## 10. サマリ (案B 7 つの新原則)

1. **(basis, data_label) 4 組合せで表示分岐** (§2)
2. **workplace+measured のみ人数表示 OK** (§4.1)
3. **「住んでいる」「働いている」を basis に応じて使い分け** (§4.3)
4. **🏢 / 🏠 バッジで basis 軸を即時識別** (§6)
5. **📊 / β バッジで data_label 軸を即時識別** (§6)
6. **DTO に basis/data_label/source_name を必須化** (§7.3)
7. **混在画面では従業地/常住地の概念差を必ず注記** (§5.1, §3.3)

### 10.1 旧書 v1.0 との関係マトリクス

| v1.0 章 | v2.0 での扱い |
|--------|--------------|
| §1 主指標 | 維持 (推定セルでのみ適用) |
| §2 人数表示禁止 | **§7.1 / §7.4 で部分修正** (workplace+measured のみ許可) |
| §3 指数/ランク/濃淡 | 維持 (推定セルでのみ適用) |
| §4 注意書き | **§5 で混在対応版を追加** |
| §5 推定ラベル | 維持 |
| §6 セールストーク | **§8 で案B 用を追加** |
| §7 表示優先順位 | 維持 (推定セルでのみ適用) |
| §8 ダッシュボード UI 例 | **§3 で 4 種類モックを追加** |
| §9 ガードレール | **§7.3 / §7.4 / §7.5 で DTO/SQL/ログ を修正** |
| §10 テスト観点 | **§9 で 案B 不変条件を追加** |
| §11-13 変更管理/参考/サマリ | 維持 |

---

**本書をもって Phase 3 Step 5 表示仕様の案B (並列保管) 対応を確定する。旧 v1.0 と並立して有効。**
**(basis, data_label) のいずれかが欠落した DTO は UI に渡してはならない。**
