# Phase 3 UI 暫定回避案: 政令市の区における resident 表示欠損

> **重要: 本書は暫定仕様であり、撤廃前提である。**
> Worker A7 (区別人口データ調達) および Worker B7 (roll-down 設計実装) による
> 区別 resident 推定値の補完が完了した時点で、本書の全仕様 (バナー / fallback ロジック /
> DataSourceLabel enum など) は撤廃する。本番推奨ではない。

最終更新: 2026-05-04 / Worker C7
ステータス: 暫定 (INTERIM) / 撤廃予定
関連: Worker A7 (人口データ調達), Worker B7 (roll-down 設計)

---

## 1. 暫定スコープ

| 項目 | 内容 |
|------|------|
| 目的 | resident タブで政令市の区を開いた際の「データなし」見え方の暫定整備 |
| 対象 | `area_type='designated_ward'` かつ `v2_municipality_target_thickness` に行なし |
| 期間 | Worker A7/B7 の補完完了まで (撤廃条件は §7) |
| 適用範囲 | UI 表示およびフォールバック query のみ |
| 非対象 | DB スキーマ変更、resident 推定ロジック修正、新規データ投入 |

本暫定はあくまで「現状で UI を出した場合の最小限ガード」であり、本来の解決策
(区別 F2 推定の DB 投入) を妨げない構成とする。

---

## 2. 影響を受ける都市分類

`municipality_code_master` の `area_type` 5 値 × 2 タブ (workplace / resident):

| area_type | area_level | resident 表示 | workplace 表示 | 暫定対応 |
|-----------|------------|:---:|:---:|------|
| municipality | unit | 表示 | 表示 | 通常 (暫定なし) |
| special_ward | unit | 表示 | 表示 | 通常 (暫定なし) |
| designated_ward | unit | 欠損 | 表示 | **本書の主対象** |
| aggregate_city | aggregate | 欠損 | 欠損 | 親市集約 SUM 表示 + 区一覧誘導 |
| aggregate_special_wards | aggregate | 欠損 | 欠損 | 同上 (23 区集約) |

### 2.1 件数 (参考)

- designated_ward: 175 件 (横浜市 18 区, 大阪市 24 区 など)
- aggregate_city: 20 件 (政令指定都市本体)
- aggregate_special_wards: 1 件 (東京都区部)

---

## 3. 暫定 UI 仕様

### 3.1 画面遷移パターン

```
[市区町村選択] → 検索バー入力「横浜市」
  ├ 横浜市 (aggregate_city, 14100)
  │   └ ⚠ バナー: 「政令市本体は集約値です。区別の詳細を選択してください」
  │      + 18 区一覧へのリンクリスト
  │      + workplace は SUM 集約で参考表示可、resident は非表示
  │
  ├ 横浜市鶴見区 (designated_ward, 14101)
  │   ├ workplace タブ: 374 行 (15-1 実測) → 通常表示
  │   └ resident タブ: ⚠ 暫定バナー
  │                    workplace 値で代替の参考表示 (明示注記)
  │
  └ 横浜市西区 (designated_ward, 14103)
      └ 同上
```

### 3.2 resident タブの暫定表示 (designated_ward)

```
================================================
神奈川県 横浜市鶴見区 - resident タブ (常住地ベース)
================================================

⚠ 注意: 常住地ベースの推定指数は、政令市の区について
   現在補完中です。下記は workplace ベース (15-1 実測) の
   値を参考表示しています。

📊 workplace 実測 (代替表示) [バッジ: 暫定]:
  生産工程従事者: 12,345 人 (実測)
  事務従事者: 8,901 人 (実測)
  ...

🏭 工業集積地判定 (parent 横浜市から継承): No

[補完完了予定: Worker A7/B7 進行中]
================================================
```

### 3.3 配信優先度ランクの暫定

designated_ward では:

| 項目 | 暫定値 | 注記 |
|------|--------|------|
| distribution_priority | 親市 (parent_code) の値を継承 | 「区別優先度は補完前のため親市値を表示」 |
| thickness_index | NULL → workplace 比率で代用 | 「workplace ベース参考値」 |
| is_industrial_anchor | 親市の値を継承 | 「親市判定を継承」 |

例: 「横浜市鶴見区 (親市の配信優先度): C ランク ※区別優先度は補完前のため親市値」

### 3.4 aggregate_city / aggregate_special_wards の暫定

| タブ | 表示内容 |
|------|---------|
| workplace | 親市配下の unit 区の SUM (`SUM(population) WHERE parent_code = ?`) |
| resident | 非表示 + 「区別データを選択してください」バナー |

---

## 4. データ層の暫定挙動

### 4.1 SQL クエリ例 (designated_ward の resident タブ)

```sql
-- Step 1: resident 推定値を試行
SELECT
  v.thickness_index AS resident_index,
  v.distribution_priority,
  v.is_industrial_anchor,
  CASE
    WHEN v.thickness_index IS NULL THEN 'workplace_fallback'
    ELSE 'resident_actual'
  END AS data_source_label
FROM v2_municipality_target_thickness v
WHERE v.municipality_code = '14101';
```

`thickness_index IS NULL` または行なしの場合、Rust 側で fallback query を発行:

```sql
-- Step 2: workplace 集計を fallback として取得
SELECT
  occupation_code,
  SUM(population) AS workplace_total
FROM municipality_occupation_population
WHERE basis = 'workplace'
  AND data_label = 'measured'
  AND municipality_code = '14101'
GROUP BY occupation_code;
```

### 4.2 親市集約 (aggregate_city) のクエリ

横浜市 (14100) を選択したケース:

```sql
SELECT
  mop.occupation_code,
  SUM(mop.population) AS aggregate_workplace
FROM municipality_occupation_population mop
JOIN municipality_code_master mcm
  ON mop.municipality_code = mcm.municipality_code
WHERE mcm.parent_code = '14100'
  AND mop.basis = 'workplace'
  AND mop.data_label = 'measured'
GROUP BY mop.occupation_code;
```

---

## 5. 文言ガイド (暫定)

### 5.1 必ず表示するバナー (designated_ward の resident タブ)

```
⚠ 暫定表示
このページの常住地ベース推定指数は、政令市の区について
現在補完中です。表示中の数値は workplace ベース (従業地、
2020 国勢調査) の参考値です。
[詳細を見る] -> 補完予定 docs リンク
```

### 5.2 NG 表現 (暫定でも禁止)

| NG | 理由 |
|----|------|
| 「鶴見区に生産工程職が ○○ 人住んでいる」 | workplace 値を resident と誤認させる |
| 「鶴見区の採用ターゲット候補総数」 | workplace と resident の混同を招く |
| 「鶴見区の常住者数」 | 値の出所を誤認させる |
| 「精密な resident 推定」 | 暫定値に精度を保証しない |

### 5.3 OK 表現 (暫定)

- 「鶴見区 (従業地ベース、参考表示): 生産工程 ○○ 人」
- 「鶴見区の resident 推定: 補完中」
- 「親市 (横浜市) の総合指数: ○○ (継承表示)」
- 「workplace ベース参考値 (15-1 実測)」

---

## 6. UI 暫定の発動条件

### 6.1 Rust handler 判定

```rust
/// designated_ward かつ resident 推定値が欠損しているかを判定。
/// 戻り値 true の場合、暫定バナー + workplace fallback を発動する。
fn is_designated_ward_resident_missing(muni_code: &str, db: &DB) -> bool {
    // 1. master で area_type='designated_ward' であること
    // 2. v2_municipality_target_thickness に当該 municipality_code の行がない
    //    または thickness_index IS NULL
    // 両方を満たす場合のみ true
}
```

### 6.2 DTO

```rust
enum DataSourceLabel {
    ResidentActual,      // 通常 (resident 推定値あり)
    WorkplaceFallback,   // 暫定 (workplace で代替)
    AggregateParent,     // 暫定 (aggregate_city の集約表示)
}
```

---

## 7. 撤廃条件 (本暫定の終了)

下記をすべて満たした時点で、本書の仕様 (バナー / fallback ロジック / enum 値
`WorkplaceFallback`/`AggregateParent`) を撤去する:

- [ ] Worker A7: 区別人口データを調達 (政令市 175 区分)
- [ ] Worker B7: roll-down 設計が実装され、区別 F2 推定が
      `v2_municipality_target_thickness` に投入される
- [ ] DB 検証: 175 designated_ward 全件で `thickness_index IS NOT NULL`
- [ ] UI 検証: 横浜市鶴見区 / 大阪市北区など主要区で resident タブが
      暫定バナーなしに表示される
- [ ] 暫定バナー HTML / fallback クエリ / `WorkplaceFallback` enum を撤去
- [ ] 本書を `archive/` に移動 (履歴として保管)

---

## 8. 実装範囲 (UI エンジニア向け)

| レイヤ | 実装内容 |
|--------|---------|
| HTML テンプレート | 暫定バナー部品 1 つ (再利用可能なパーシャル) |
| Rust handler | `area_type` チェック分岐 + workplace fallback クエリ呼び出し |
| DTO | `DataSourceLabel` enum (3 値) |
| フロント表示 | バッジ「暫定」を該当数値の隣に追加 |
| ルーティング | aggregate_city 選択時に区一覧へのリンクを生成 |

実装ファイル数の目安: テンプレート 1, handler 1 関数追加, DTO 1 enum。
既存 UI ファイルの大規模改修は不要。

---

## 9. 参考

- `SURVEY_MARKET_INTELLIGENCE_PHASE3_DISPLAY_SPEC.md` (本番表示仕様)
- `SURVEY_MARKET_INTELLIGENCE_PHASE3_DISPLAY_SPEC_PLAN_B.md` (Plan B 並行案)
- `municipality_code_master` テーブル仕様 (area_type, parent_code)
- `v2_municipality_target_thickness` テーブル仕様

---

(本書は暫定仕様であり、Worker A7/B7 完了後に撤廃する)
