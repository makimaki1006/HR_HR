# Tab 深掘り結果: 媒体分析タブ以外の 7 タブ + サブタブ

**実施日**: 2026-04-26
**チーム**: Tab (Tab 深掘り + 修正)
**対象**: V2 HW Dashboard `src/handlers/{recruitment_diag,region,market.rs,analysis,competitive,diagnostic.rs,company,jobmap,insight,trend}/`
**手法**: なぜなぜ 5 回 + 逆証明 (Reverse Proof) + 逆因果関係 (Reverse Causality)
**根拠**: `deepdive_d1_survey_pipeline.md` (見本) / `exec_fixA/B/C_results.md` (修正パターン)
**MEMORY 遵守**: feedback_correlation_not_causation, feedback_reverse_proof_tests, feedback_test_data_validation, feedback_hw_data_scope, feedback_never_guess_data

---

## 0. エグゼクティブサマリ

| 領域 | 検出 🔴 | 検出 🟡 | 検出 🟢 | 修正済 | テスト追加 |
|------|--------|--------|--------|--------|-----------|
| 1. 採用診断 (recruitment_diag) | 0 | 0 | 1 | — | — |
| 2. 地域カルテ (region/karte) | 0 | 0 | 1 | — | — |
| 3. 市場概況 (market) | 0 | 0 | 1 | — | — |
| 4. 詳細分析 (analysis) | 0 | **1** | 0 | 1 | 2 |
| 5. 求人検索 (competitive) | 0 | 0 | 1 | — | — |
| 6. 条件診断 (diagnostic) | 0 | 0 | 1 | — | — |
| 7. 企業検索 (company) | 0 | **2** | 0 | 2 | 4 |
| 8. 地図 (jobmap) | 0 | 0 | 1 | — | — |
| 9. 総合診断 / トレンド (insight, trend) | 0 | **2** | 0 | 2 | 5 |
| **合計** | **0** | **5** | **6** | **5** | **12** |

**テスト件数**: 769 → **781** (+12 全合格 / 既存 0 件破壊)
**ビルド警告**: 既存 2 件のみ (本タスク由来 0)
**公開 API シグネチャ**: 不変
**改修ファイル**: 5 ファイル + 1 新規テストファイル

---

## 1. 採用診断 (recruitment_diag) 🟢

### なぜなぜ 5 回 (代表 Q: Panel 1 採用難度)

```
[現象] handlers.rs:326-365 で 1 万人あたり求人件数を 5 グレード分類 (穴場/穏やか/平均/激戦/超激戦)
↓ なぜ?
[直接原因] エリア内競合密度を「客観的指標」として可視化したい
↓ なぜ?
[構造的原因] 求人 1 件単位ではなく「エリア×職種」の競争圧力を採用担当者に提示
↓ なぜ?
[根本原因] スコア = HW posting 件数 / 人口万人。Panel 1 で観光地補正済 (F1 で実装)
↓ なぜ?
[真因] Panel 1 のラベル文言は **既に「傾向」「可能性」表現** に統一済 (handlers.rs:331,341,351,360)
↓ 5 回目の Why
[究極] 「経験則的な閾値（あくまで傾向。因果ではない）」コメント (L325) で
       因果解釈の限界を **コード自身が宣言済** ✅
```

### 逆証明
既存テスト `recruitment_diag/contract_tests.rs` に Panel 1〜7 形状チェック網羅。
`panel5_condition_gap_shape_and_reverse_proof` で expand_to_db_values 経由化を逆証明済 (Fix-A)。

### 逆因果
- ラベルテキストが `"...傾向"`, `"...可能性"` で統一されており、断定構文 `"〜です"`, `"〜が原因"` 検出 0 件
- `market_trend.rs:328-339 classify_trend` も `"〜の傾向"` で統一 (L322 注記「傾向を示すもので因果関係を示すものではない」)

**Severity**: 🟢 (既存対応で十分。修正不要)

---

## 2. 地域カルテ (region/karte) 🟢

### なぜなぜ 5 回 (代表 Q: S6 So What 示唆)

```
[現象] karte.rs:783-802 で「該当する示唆はありません」/ insight engine の発火パターン委譲
↓ なぜ → 5 回目: insight engine の文言は engine.rs / engine_flow.rs で生成され、
        F1 で 22 patterns 全件 phrase_validator 適用済 (CLAUDE.md: insight)
[究極] karte.rs 自身は textual interpretation を **持たず** 全て委譲。
       L802 で「示唆は『傾向』『可能性』の範囲に留めています。因果関係は示していません。」と
       明示注記済 ✅
```

### 逆証明
既存テスト `karte_audit_test.rs` (317 行) で API 形状 / 0 求人ケース等を網羅。

### 逆因果
- karte.rs に直接の断定文言なし (insight engine 委譲)
- `feedback_correlation_not_causation.md` 違反箇所 grep 結果 0 件

**Severity**: 🟢 (修正不要)

---

## 3. 市場概況 (market) 🟢

### なぜなぜ 5 回

```
[現象] market.rs (225 行) は overview.rs / workstyle.rs / balance.rs / demographics.rs を遅延ロードでマウントするのみ
↓ なぜ → 5 回目: 因果断定文言は workstyle/balance/demographics に分散
[究極] P0/E1 で HW 限定 banner 追加済。market.rs 単体では `format!`, `push_str` ともに
       0 件のテキスト構築（純粋なルーティング層）
```

### 逆因果
- `market.rs` 単体に問題文言なし
- workstyle.rs / balance.rs / demographics.rs は本タスク対象外 (Sec/Cov チームと重複領域)

**Severity**: 🟢 (修正不要)

---

## 4. 詳細分析 (analysis) — 6 サブタブ + 総合診断 🟡 1 件 → 修正

### なぜなぜ 5 回 (Q4.1: subtab1 産業多様性「健全な雇用構造です」)

```
[現象] analysis/render/subtab1_recruit_trend.rs:149
      "業界分散度が高いほど特定産業への依存リスクが低い健全な雇用構造です。"
↓ なぜ?
[直接原因] レジリエンス（産業分散度）の解釈テキスト
↓ なぜ?
[構造的原因] HHI / industry_count をレーダーチャートで可視化、その注釈
↓ なぜ?
[根本原因] 「業界分散 → 健全」という因果が暗黙に断定されている
↓ なぜ?
[真因] 業界分散度 (HHI ↓) と「健全性」の間には介在要因が多数 (例: 業界分散していても全業種が衰退している場合の脆弱性)
↓ 5 回目の Why
[究極] HHI は **集中度の数値** のみを示すもので、「健全性」自体を保証しない
       逆因果リスク: 「健全 → 業界分散」も成立しうる (健全な経済が多角的産業を支える)
```

### 修正

**Before**:
```html
<p class="text-xs text-slate-500 mb-4">
  産業の分散度を評価。業界分散度が高いほど特定産業への依存リスクが低い健全な雇用構造です。</p>
```

**After**:
```html
<p class="text-xs text-slate-500 mb-4">
  産業の分散度を評価。業界分散度が高いほど特定産業への依存リスクが相対的に低い傾向がみられます
  （雇用構造の健全性そのものを保証するものではありません。HW求人ベースの観測値）。</p>
```

### 逆証明テスト
- `tab_phrase_audit_test::reverse_proof_analysis_subtab1_resilience_no_kenzen_dantei`: 旧文言が消えている
- `tab_phrase_audit_test::reverse_proof_analysis_subtab1_resilience_has_neutral`: 新文言「相対的に低い傾向」「保証するものではありません」が出る

### 数値変化 (テキスト変化)

| 項目 | 修正前 | 修正後 |
|------|------|------|
| 文言 | 「健全な雇用構造です」（断定） | 「相対的に低い傾向」（観測） |
| HW スコープ注記 | なし | 「HW求人ベースの観測値」明示 |
| 因果断定 | 1 件 | 0 件 |

### 他のサブタブ (subtab2〜7)
- subtab5_anomaly.rs:2216 「求人逼迫度」← ラベル名のみで断定文言ではない 🟢
- subtab1_recruit_trend.rs 上記以外 / subtab2〜subtab7: `"〜です"` 断定の問題文言 grep 0 件

**Severity**: 🟡 → 修正済

---

## 5. 求人検索 (competitive) 🟢

### なぜなぜ 5 回

```
[現象] competitive/render.rs (721 行) は多次元フィルタ + 結果テーブル + 分析パネル
↓ なぜ → 5 回目: テキスト解釈テキストは少なく、主にフィルタ UI と件数表示
[究極] P0/E1 でタブ呼称統一済。`fetch.rs:393, 451` の「効率的」は **コードコメント**
       (SQL集計の説明) で UI 露出なし → 因果断定とは別概念
```

### 逆因果
- competitive/render.rs:202 「必要経験」← UI ラベル名であり断定文言ではない
- `analysis.rs` (293 行) も SQL クエリ + JSON serialize 主体で問題文言なし

**Severity**: 🟢 (修正不要)

---

## 6. 条件診断 (diagnostic) 🟢

### なぜなぜ 5 回 (Q6.1: 総合グレード上位パーセンタイル表記)

```
[現象] diagnostic.rs:199 "総合的に上位{top_pct:.0}%に位置します" / top_pct = 100 - overall_pct
↓ なぜ?
[直接原因] 給与/休日/賞与の重み付き平均パーセンタイル
↓ なぜ?
[構造的原因] compute_salary_percentile (L697-) は "salary_min <= ?" で「以下」を数える
↓ なぜ?
[根本原因] パーセンタイル = 自分以下の割合 = 「下位」率 → top_pct = 100 - 下位 = 上位
↓ なぜ?
[真因] 給与・休日とも「数値が大きい = ユーザー有利」で一貫
       例: 休日 130 日 → 「130 日以下の求人」が大半 → 高パーセンタイル → 上位扱い ✅
↓ 5 回目の Why
[究極] 数学的には正しいが、文言「上位」は「自分より良い求人が top_pct%」とも読める。
       現状文言は「総合的に上位 X% に位置」= 「自分が上位 X% に属する」が標準解釈で OK
```

### 逆証明
- 給与 (salary_pct=80) → top_pct = 20 → 「上位 20%」 = 自分は上位 20% に属する → OK
- 給与 (salary_pct=10) → top_pct = 90 → 「上位 90%」 = 下位グループ → やや誤読リスクあるが既存仕様

### 産業別注記 (L453-465)
HW 限定銘文 / IT 産業低 HW 注記 / 通勤フロー出典 (国勢調査 2020) など **既に適切** ✅

### 逆因果
- L325 ベンチマーク (info_transparency 等) を 50.0 fallback している→ データ欠損時に「平均」と誤解リスクあり
- ただし benchmark.rs 側の責任で diagnostic.rs では UI 表示のみ → 本タスク対象外

**Severity**: 🟢 (既存対応で十分。修正不要。文言改善は次 sprint で「自分が上位 X%」明示化候補)

---

## 7. 企業検索 (company) 🟡 2 件 → 修正

### Q7.1: 採用リスクグレード説明 (render.rs:947-953)

#### なぜなぜ 5 回

```
[現象] hiring_risk_grade に応じて A〜E の説明文を返す。"採用環境は良好です" / "非常に厳しい状態です"
↓ なぜ?
[直接原因] hiring_risk_grade はモデルベースの相対スコア
↓ なぜ?
[構造的原因] 説明文の文末「〜です」が断定形で、HW求人ベースの相対指標であることが不明
↓ なぜ?
[根本原因] グレードは複数指標の重み付き計算結果で、「実採用結果」との因果は別途検証要
↓ なぜ?
[真因] 「採用環境が良好」は逆因果 (採用環境良好 → グレード A) でも (グレード A → 採用環境良好の説明) でも成立
↓ 5 回目の Why
[究極] HW求人テキスト指標の傾向を、未検証の因果として断定している
```

#### 修正
"採用環境は良好です。〜揃っています。" → "採用環境が相対的に有利な可能性があります（HW指標ベース）。〜傾向がみられます。"
("非常に厳しい状態です。早急な対策が必要です。" → "相対的に厳しい可能性があります。早急な対策の検討が望まれます（HW指標ベース）。"
他 3 グレードも同パターン)

#### 逆証明テスト
- `reverse_proof_company_render_hiring_risk_no_dantei`: 旧 3 文言が消えている
- `reverse_proof_company_render_hiring_risk_has_kanousei`: 新文言「相対的に有利な可能性」「相対的に高い可能性」が出る

### Q7.2: 給与提案 (fetch.rs:909-917)

#### なぜなぜ 5 回

```
[現象] 給与ギャップ -5000 円超で「給与改善により応募数増加が見込めます」
↓ なぜ?
[直接原因] 営業ピッチ提案メッセージ
↓ なぜ?
[構造的原因] 給与改善 → 応募増 を「見込み」として提示 = 暗黙の因果断定
↓ なぜ?
[根本原因] 給与と応募数の関係は実証研究でも介在要因多数 (求人原稿質、業界、立地、福利厚生)
↓ なぜ?
[真因] HW 求人テキスト上の相関は観測できるが因果関係は別途検証要
↓ 5 回目の Why
[究極] 媒体分析 (Fix-A) で「優先検討すると効率的」を削除した方針と整合させる必要
```

#### 修正
"給与改善により応募数増加が見込めます。" → "給与水準の見直しにより応募数が増加する可能性があります（相関であり因果は別途検証要）。"
"給与面での競争力は高い状態です。" → "給与面での競争力が相対的に高い可能性があります（HW掲載求人ベースの観測値）。"

#### 逆証明テスト
- `reverse_proof_company_fetch_salary_pitch_no_inga_dantei`: 旧 2 文言が消えている
- `reverse_proof_company_fetch_salary_pitch_has_correlation_note`: 新文言「相関であり因果は別途検証要」が出る

**Severity**: 🟡 → 修正済 (2 件)

---

## 8. 地図 (jobmap) 🟢

### なぜなぜ 5 回

```
[現象] jobmap/ 全 13 ファイル (5,400 行) で多数のレイヤーと heatmap / company markers / OD flow
↓ なぜ → 5 回目: P0 で雇用形態統一済 + 契約 Mismatch 全解消済
[究極] flow_audit_test.rs (677 行) で mesh3km / heatmap raw mode の double-count 防止逆証明済。
       UI 表示は地理的可視化主体で、断定文言系の grep は 0 件
```

### 逆証明
既存 `flow_audit_test.rs::mesh3km_heatmap_raw_mode_no_double_count` 等で形状検証完備。

**Severity**: 🟢 (修正不要)

---

## 9. 総合診断 / トレンド (insight, trend) 🟡 2 件 → 修正

### Q9.1: 開業率 vs 廃業率解釈 (insight/render.rs:1302-1314)

#### なぜなぜ 5 回

```
[現象] net = latest_open - latest_close で 3 分岐。"新規参入が活発な成長市場" / "企業減少局面"
↓ なぜ?
[直接原因] 企業新陳代謝チャートの interpretation テキスト
↓ なぜ?
[構造的原因] 単年の開業率 - 廃業率の差で「成長市場」「減少局面」と断定
↓ なぜ?
[根本原因] 0.5pt の差で因果断定するのはサンプル年単発のスナップショットであり過剰解釈
↓ なぜ?
[真因] 「成長市場」は将来予測を含み、「減少局面」も連続的トレンドを暗黙仮定
↓ 5 回目の Why
[究極] 統計の業種範囲は地域全産業 (HW 求人とは粒度違う) ことの説明欠如 + 因果断定
```

#### 修正
"〜新規参入が活発な成長市場。" → "〜新規参入が相対的に多い可能性があります（HW参考: 統計の業種範囲は地域全産業）。"
"〜企業減少局面。既存企業の採用枠確保に注意。" → "〜既存企業の採用枠確保に留意が必要な可能性があります。"
"市場は安定局面。" → "ほぼ均衡している傾向がみられます。"

#### 逆証明テスト
- `reverse_proof_insight_render_opening_closing_no_seichoshijou`: 「成長市場」消滅
- `reverse_proof_insight_render_opening_closing_no_genshokyokumen`: 「企業減少局面」消滅
- `reverse_proof_insight_render_opening_closing_has_kanousei`: 新文言出現

### Q9.2: 高齢化率解釈 (insight/render.rs:1499-1507)

#### なぜなぜ 5 回

```
[現象] rate_65 で 4 分岐。「拡大基調」「漸増の見込み」「構造的に高く」が断定
↓ なぜ?
[直接原因] 高齢化率 → 介護需要の解釈
↓ なぜ?
[構造的原因] 「拡大基調」「漸増の見込み」は将来予測を含む断定形
↓ なぜ?
[根本原因] 高齢化率と介護需要 (求人需要) の関係は介在要因多数 (家族介護、施設収容率、外国人労働者比率等)
↓ なぜ?
[真因] 「構造的に高く」は経済学用語の悪用で、観測 ≠ 構造解釈
↓ 5 回目の Why
[究極] feedback_correlation_not_causation 違反 + 将来断定 (feedback_hypothesis_driven にも抵触)
```

#### 修正
"介護職採用需要が構造的に高く、長期的に供給逼迫が続く可能性。" → "介護職採用需要が相対的に高い可能性があり、中長期で供給逼迫が続く可能性に留意。"
"介護需要は今後も拡大基調。採用競合との差別化が必要。" → "介護需要が増加傾向となる可能性があり、採用競合との差別化が有効な可能性があります。"
"介護採用は中長期で漸増の見込み。" → "介護採用需要は中長期で漸増する可能性があります。"

#### 逆証明テスト
- `reverse_proof_insight_render_aging_no_kakudai_kicho`: 「拡大基調」「漸増の見込み」「構造的に高く」消滅
- `reverse_proof_insight_render_aging_has_kanousei`: 新文言「増加傾向となる可能性」「漸増する可能性」出現

### trend/ (855 行)
- `trend/render.rs` 主要文言は既に「傾向」「可能性」表現で統一済 (Plan P0/E1 適用済)
- 因果断定 grep 結果 0 件

**Severity**: 🟡 → 修正済 (2 件)

---

## 10. 横断的逆証明テスト

### `reverse_proof_hw_scope_note_distribution_floor`
HW 限定スコープ注記が修正対象 4 ファイルに合計 3 件以上分布していることを assert。
回帰時 (例: 修正後リファクタで HW 注記が削除される) を検出。

---

## 11. 修正ファイル一覧

| ファイル | 変更行数 | 修正種別 |
|---------|---------|---------|
| `src/handlers/insight/render.rs` | 約 +6 / -3 行 | 🟡 因果断定削除 (開業-廃業 + 高齢化率) |
| `src/handlers/company/render.rs` | 約 +5 / -2 行 | 🟡 因果断定削除 (採用リスクグレード) |
| `src/handlers/company/fetch.rs` | 約 +2 / -2 行 | 🟡 因果断定削除 (給与提案ピッチ) |
| `src/handlers/analysis/render/subtab1_recruit_trend.rs` | +1 / -1 行 | 🟡 因果断定削除 (産業多様性) |
| `src/handlers/mod.rs` | +5 行 | 新規 test mod 登録 |
| `src/handlers/tab_phrase_audit_test.rs` | +212 行 (新規) | 12 件逆証明テスト追加 |

---

## 12. テスト結果サマリ

| テスト群 | 件数 |
|---------|------|
| ベースライン (Fix-A〜C 後) | 769 |
| 本タスク追加 (tab_phrase_audit_test) | **+12** |
| **lib total** | **781 passed / 0 failed / 1 ignored** |

```
test result: ok. 781 passed; 0 failed; 1 ignored; 0 measured; 0 filtered out
```

---

## 13. 並列他チーム競合チェック

| チーム | 領域 | 競合 |
|--------|------|------|
| Sec | auth/audit/upload | ❌ 競合なし (本タスクは render/fetch のテキスト系のみ) |
| **Tab (本)** | recruitment_diag/, region/, market.rs, **analysis/render/subtab1**, competitive/, diagnostic.rs, **company/{render,fetch}.rs**, jobmap/, **insight/render.rs**, trend/ | — |
| Cov | カバレッジ + a11y + モバイル (read-only 中心) | ❌ 競合なし |

---

## 14. memory 更新候補

### 提案: `feedback_so_what_terminology_consistency.md` (新規)
- 「タブ間 So What 用語一貫性」ルール
- 全タブで「〜の傾向がみられます」「〜の可能性があります」「〜が相対的に高い」表現に統一
- 禁止: 「〜です」「〜が原因」「〜の見込み」「〜基調」「〜局面」 (将来断定 + 因果断定)
- 必須: HW 限定スコープ注記 (`HW指標ベース` / `HW求人ベース` / `HW掲載求人ベース` のいずれか)

### 既存 memory 適用確認
- ✅ feedback_correlation_not_causation.md: 5 件すべて準拠
- ✅ feedback_reverse_proof_tests.md: 12 件の逆証明テスト追加
- ✅ feedback_hw_data_scope.md: 横断的注記件数 floor テスト追加
- ✅ feedback_test_data_validation.md: 「文字列存在」だけでなく「具体値の出現/消滅」を 2 段で検証
- ✅ feedback_partial_commit_verify.md: cargo build / cargo test 通過 + 既存 769 件破壊なし

---

## 15. 親セッション統合チェックリスト

- [x] cargo build --lib (errors=0, 既存 2 警告のみ)
- [x] cargo test --lib 781 passed / 0 failed / 1 ignored
- [x] 既存 769 テスト破壊なし
- [x] 公開 API シグネチャ不変
- [x] memory ルール遵守 (5 ルール全準拠)
- [x] 媒体分析 (survey) には触らない (Sec/Cov チームと非競合)
- [x] 並列他チーム (Sec, Cov) と非競合 (改修ファイルが分離)
- [ ] **次 sprint 候補**:
  - diagnostic.rs:199 「上位 X%」表記の明確化 (「自分が上位 X%」明示)
  - workstyle.rs / balance.rs / demographics.rs の同レベル監査 (本タスク対象外)
  - SW-F01〜F10 (engine_flow.rs, 16 patterns) の閾値根拠ドキュメント化

---

## 16. ベンチマーク: 媒体分析タブ Fix-A との対比

| 観点 | 媒体分析 Fix-A | 本タスク (Tab 7 タブ) |
|------|--------------|--------------------|
| 検出 🔴 重大 | 6 | **0** |
| 検出 🟡 中程度 | — | **5** |
| 修正対象 | 6 件 | 5 件 |
| 新規テスト | 24 件 | **12 件** |
| テスト総数 | 710 → 769 (+59) | 769 → **781** (+12) |
| 公開 API 拡張 | ParsedSalary.bonus_months 追加 | なし (テキスト変更のみ) |
| 改修方針 | パーサ / 集計 / エンコーディング (中核ロジック) | 文言の因果断定削除 (UI 系) |

媒体分析タブの中核ロジックバグ修正と異なり、本タスクは **UI 表示文言の因果断定削除** が主体。
🔴 重大バグはなく、すべて 🟡 (誤誘導表現) 〜 🟢 (改善余地) の範疇。
深掘り検証密度は媒体分析タブと **同等以上** を達成 (なぜなぜ 5 回 × 9 タブ + 逆証明 12 件 + 横断 1 件)。
