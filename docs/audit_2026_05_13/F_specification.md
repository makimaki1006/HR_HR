# F 領域監査: 仕様整合 / 表現

- 日付: 2026-05-13
- 対象: `src/handlers/survey/report_html/`, `src/handlers/insight/`, `docs/`
- モード: read-only

## サマリー (約200 words)

BtoB レポート (survey/report_html) は中立表現・相関≠因果・HW 範囲制約の各原則が大半で遵守されており、`industry_salary.rs` / `industry_mismatch.rs` / `lifestyle.rs` / `demographics.rs` / `executive_summary.rs` には明示 caveat と invariant test (HW 連想語 ban、因果ワード ban) が組み込まれている。`feedback_neutral_expression_for_targets.md`/`feedback_correlation_not_causation.md`/`feedback_hw_data_scope.md` 準拠は survey 側ではほぼ達成。雇用形態は V2 系コードで一貫して「正社員」を主とし、表記揺れ対応として「正職員」を含める分岐が明示されており V1/V2 混同の痕跡なし。一方で **P0 違反**: `src/handlers/insight/report.rs:351` および `render.rs:935` で禁止語「劣位」が出力テキストに直接埋め込まれている (survey は OK だが insight ハンドラは未対応)。**P1**: 章番号体系の不整合 — `salesnow.rs` のコメントは「第 12 章」とするが h2 出力が「第5章 地域注目企業」「第5章 地域企業 ベンチマーク」と 2 セクションで「第5章」を重複使用。Public/Full バリアントの章番号設計が未整理。**P2**: 図番号で 図 2-x/7-x/9-x が欠番、章番号と図番号の章ナンバリング基準が不一致 (notes が 第6章 で章 7-11 飛ばし)。

## P0 (Critical)

### P0-1: insight ハンドラで禁止評価語「劣位」を出力

- `src/handlers/insight/report.rs:351`
  ```rust
  "{}件の地域比較指標のうち、{}件が他地域に対して劣位。",
  ```
- `src/handlers/insight/render.rs:935`
  ```rust
  "他の地域と比べて優位か劣位か?",
  ```
- 違反: `feedback_neutral_expression_for_targets.md` (BtoB レポートで「劣位」禁止)
- 修正案: 「下位」「相対的に小さい」または「ベンチマーク差分」等の中立語へ。survey 側 `salesnow.rs:1046,1061` は invariant test で「劣位」を出力させないと検証済みだが、insight 側は未カバー。

### P0-2: insight 設問テキストで「劣位」露出 (PDF/UI 双方の可能性)

- `render.rs:935` の chapter_questions 配列は章タイトル直下の設問として表示されるため、レポート閲覧者 (提案先企業) の目に触れる。survey と同等の中立化が必要。

## P1 (Major)

### P1-1: 章番号体系の不整合 (salesnow が第5章 / 第12章 二重)

- 既存配置: 第3章 HW 連携 (`hw_enrichment.rs:90`) / 第4章 求職者心理 (`seeker.rs:23`) / 第6章 注記 (`notes.rs:29`)
- 矛盾:
  - `salesnow.rs:30` h2 = `第5章 地域注目企業 (規模の大きい順)`
  - `salesnow.rs:222,248,427` h2 = `第5章 地域企業 ベンチマーク ...` (同番号で 2 セクション)
  - `mod.rs:3865,3933,3938,3940,3985,3988` コメント = `第 12 章 / 第 12B 章`
- 影響: 目次・章参照・PDF 章番号で読者が混乱。test (`mod.rs:4001-4005`) は「第5章 地域注目企業」と「第5章 地域企業 ベンチマーク」両方を assert しており、設計意図と齟齬。
- 修正案: salesnow_companies = 第5章、company_segments = 第5B 章 (または別番号) に分離。コメントの「第12章」を実装に合わせて更新。

### P1-2: notes 章番号が章間に空隙 (第7-11章相当が欠番)

- notes は `第6章` だが、その前に存在する章は 1/3/4/5 (および salary_stats/wage 等は章番号 h2 を持たず figure prefix「図 8-x」「図 10-x」のみで章を示唆)。
- 章番号 vs 図番号で基準が二重化しており、図番号の章プレフィックス (1,3,4,5,6,8,8B,10,11) と h2 章番号 (3,4,5,6) が一致しない。
- 修正案: 章番号と図番号の章プレフィックスを 1 系統に統一。

### P1-3: Public/Full バリアント章可視性のテスト矛盾

- `mod.rs:3985-3991`: MI variant で「第5章 地域注目企業」「第5章 地域企業 ベンチマーク」は非表示
- `mod.rs:4001-4005`: Full variant で両方表示
- コメントでは「第 12 章」と記載 (mod.rs:3988) → 出力 h2 は「第5章」。test 文言と実出力が乖離し、保守時の誤解原因。

## P2 (Minor)

### P2-1: 図番号の章プレフィックス欠番

- 観測される章プレフィックス: 1, 3, 4, 5, 6, 8, 8B, 10, 11
- 欠番: 2, 7, 9 (該当章に図表が存在しない可能性が高いが、章番号と図番号の整合性ポリシーが文書化されていない)
- 推奨: `helpers.rs:520` の `render_figure_caption` doc に章番号方針を明記。

### P2-2: 「集中」語の利用パターン

- `wage.rs:441,456,458,462` / `market_tightness.rs:3694-3713`: HHI 指数の専門用語「低集中/中集中/高集中/集中型市場」として使用。学術定義に準拠 (公取委 HHI 閾値) しており評価語ではないが、`feedback_neutral_expression_for_targets.md` で「集中」が禁止語に挙がっているため、注記で「市場集中度 (経済学用語、評価ではない)」と明示するのが望ましい。
- `industry_mismatch.rs:803` (コメントのみ、出力テキストではない): 「サービス業…一極集中する問題」— コメントなので影響なし。
- `hw_enrichment.rs:197` (出力テキスト): 「どのエリアに媒体側の露出が<strong>集中</strong>しているかの参考値」— 中立記述として OK。

### P2-3: 「縮小」語の利用

- `salesnow.rs:761,805,1058,1060,1063` / `market_tightness.rs:1359,1984`: 「縮小傾向」「縮小基調」を SalesNow 人員推移の合成示唆として出力。invariant test (`salesnow.rs:969-977`) で全規模マイナス時に必須化、全規模プラス時に禁止と逆証明済み。
- ただし `feedback_neutral_expression_for_targets.md` で「縮小」も禁止語候補。注記での丁寧化（例「人員推移マイナス基調 (採用市場の流動性高まる可能性)」)を検討。

### P2-4: 雇用形態の表記揺れ (V1 用語混入なし、ただし dashboard.css は V1 残骸の可能性)

- `static/css/dashboard.css:17`: `--color-emp-regular: #009E73; /* 緑: 正職員 */` — コメントが「正職員」(V1 用語)。V2 では「正社員」。css コメントのみで挙動影響なし。
- `src/handlers/emp_classifier.rs:58,89,151,154`, `src/handlers/workstyle.rs:398`, `src/handlers/survey/aggregator.rs:582`, `src/handlers/survey/upload.rs:849`, `report_html/{employment,executive_summary,hw_enrichment,summary}.rs`: 全て「正社員」「正職員」両対応 (V2 で V1 系媒体取込みケアの正当な分岐) → OK。
- `summary.rs:97`: 出力テキストに「雇用形態『正社員・正職員』の行が占める比率」— V2 レポートで V1 用語が読者に見える。V1/V2 用語混同警戒の観点では「正社員 (正職員含む)」表記が望ましい (現状でも問題は軽微)。

### P2-5: HW 範囲制約注記のカバレッジ

- 「HW 掲載求人のみ」「全求人市場ではない」の趣旨注記は survey 全体で 19 箇所 (executive_summary, hw_enrichment, industry_mismatch, market_tightness, mod.rs) に存在。`notes.rs:51` で出典「e-Stat 政府統計（最低賃金・欠員補充率・人口統計）」と統括。
- 欠落チェック: `wage.rs` (最低賃金比較) / `region.rs` (可住地密度 KPI) / `lifestyle.rs` (社会生活基本調査) — これらは外部統計章で HW 範囲制約は当該章では不要だが、各章フッターで出典明記済み。
- 良好。

## 章間の数値整合 (確認のみ)

- 静的解析の範囲では検出不可。executive_summary の KPI と詳細章 (employment/wage/market_tightness) で同一指標を二重計算している箇所は確認すべき (build を伴わないため、ここでは指摘のみ)。

## 推奨アクション順序

1. **P0-1/P0-2**: insight ハンドラに salesnow 同等の「禁止語 invariant test」を導入し、「劣位」「優位」を「下位/上位」「比較差分」へ書換 (即時、本番影響あり)。
2. **P1-1/P1-3**: salesnow 第5章 (2 セクション) と「第 12 章」コメントの整合化、test 文言の更新。
3. **P1-2/P2-1**: 章番号 vs 図番号体系の正式仕様化 (docs に章番号マスタ表を追加)。
4. **P2-2/P2-3**: 「集中」「縮小」が学術用語として残る箇所に注釈追加。

## 用語表記揺れの組合せ一覧

| 用語 | 表記揺れの組合せ | 出現箇所 (代表) |
|------|----------------|---------------|
| 雇用形態 | 正社員 / 正職員 / フルタイム | `emp_classifier.rs:44,58`, `summary.rs:97`, `dashboard.css:17` |
| HHI 評価 | 低集中 / 中集中 / 高集中 / 中程度集中 / 集中型市場 / 分散型市場 | `wage.rs:441-462`, `market_tightness.rs:3694-3713` |
| 採用市場語 | 縮小傾向 / 縮小基調 / 縮小局面 / 中小縮小 | `salesnow.rs:761,1058,1063`, `market_tightness.rs:1359,1984` |
| 章プレフィックス | 第3章/第4章/第5章/第6章 (h2) ↔ 第12章/第12B章 (コメント) | `salesnow.rs:30 vs mod.rs:3865` |
| 出典文言 | 「e-Stat 政府統計」/「政府統計（e-Stat）」/「CSIS」 | `notes.rs` (要文言統一監査、対象外) |

## 終了

- 違反/不整合 計 9 件 (P0=2, P1=3, P2=4)
- read-only 監査のため修正は実施せず
