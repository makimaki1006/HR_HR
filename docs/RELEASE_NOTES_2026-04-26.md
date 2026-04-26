# Release Notes — 2026-04-26

> Format: [Keep a Changelog 1.1](https://keepachangelog.com/en/1.1.0/) 準拠
>
> **Audience**: V2 HW Dashboard 利用者 (採用コンサル / 人事 / 採用市場リサーチャー) + 運用者
>
> **対象リリース**: 2026-04-26 (大規模並列実装による Audit P0/P1/P2 対応)

---

## Summary

監査 (2026-04-25) で抽出された P0/P1 課題のうち **UI 動線・ドメインロジック精度・コード健全性** に関わる項目を一括で修正したリリースです。

- ナビ動線改善: 「総合診断」「トレンド」を上位タブに昇格、タブ呼称 4 重ブレを「求人検索」に統一
- ドメインロジック修正: AP-1 給与改善の年間人件費が **約 1.55 倍** に変動 (賞与・法定福利を含めた現実的な値に修正)
- 用語統一: 「欠員率」を **「欠員補充率」** へ全面差し替え (労働経済統計の欠員率との誤読対策)
- コード健全性: 環境変数 4 個を `AppConfig` に統合、dead code 210 行 + `.bak` ファイル削除
- テスト基盤強化: 643 → **670 passed** (3 件の pre-existing failure 解消、phrase_validator を既存 22 patterns に適用)

> **重要**: 本リリースには **数値計算の変更** が含まれています (AP-1)。クライアント提出資料の数値が前回と一致しない場合は §Changed > 数値計算の変更 を参照してください。

---

## Changed (ユーザー影響あり)

### UI 表示変更

#### タブ呼称統一: 「求人検索」へ集約

旧版では同一機能が 4 通りのラベルで露出しており、混乱の原因となっていました。

| 影響箇所 | 旧 | 新 |
|---|---|---|
| 上位ナビボタン | 求人検索 (UI) / 競合調査 (コメント) / 企業調査 (H2) | **求人検索** に統一 |
| `competitive` タブ H2 | 競合調査レポート | **求人検索レポート** |
| `competitive` タブ HTML タイトル | 競合調査レポート - ... | **求人検索レポート - ...** |
| `company` タブ H2 | 🔎 企業分析 | **🔎 企業検索** |
| `survey` タブ呼称 | 競合調査 (コメント) | **媒体分析** |
| `insight` タブ内 sub label | competitive => "競合" / survey => "競合調査" | competitive => "求人検索" / survey => "媒体分析" |

ユーザーへの影響: ブラウザブックマークの URL (`/tab/competitive`) は **変更ありません**。

#### 「総合診断」「トレンド」を上位ナビへ昇格

これまで「詳細分析」内のグループ切替経由でしか到達できなかった以下のタブを、上位タブバー (`templates/dashboard_inline.html`) に追加しました。

- 🎯 **総合診断** (`/tab/insight`) — 38 insight pattern による示唆を一覧
- 📈 **トレンド** (`/tab/trend`) — HW 求人の時系列分析

これにより、採用コンサル / リサーチャーペルソナの主要分析機能の発見性が改善されました。

#### 「欠員率」→「欠員補充率」全面更新

労働経済統計の **欠員率** (= 未充足求人数 / 常用労働者数) と、本ダッシュボードの **欠員補充求人比率** (= recruitment_reason_code=1 の求人比率) の混同を防ぐため、UI 上の全表記を統一しました。

| 影響箇所 | ファイル |
|---|---|
| 求人動向サブタブ KPI ラベル | `src/handlers/analysis/render.rs:348, 363` |
| 業種別ランキング見出し | `src/handlers/analysis/render.rs:414` |
| ECharts legend / series 名 | `src/handlers/analysis/render.rs:370` |
| 業種×雇用形態テーブル | `src/handlers/analysis/render.rs:710-712` |
| jobmap セレクト項目 | `templates/tabs/jobmap.html:79` |

ユーザーへの影響: 数値計算ロジックは **変更ありません** (ラベルのみ修正)。

#### 雇用形態セレクトの選択肢統一 (jobmap)

地図タブ (`jobmap`) の雇用形態セレクトで **「業務委託」→「派遣・その他」** に変更。これは他タブ (competitive / recruitment_diag) と統一するための修正です。

| タブ | 選択肢 (修正前) | 選択肢 (修正後) |
|---|---|---|
| jobmap | 正社員 / 契約社員 / パート / 業務委託 | **正社員 / パート / 派遣・その他** |
| competitive | 正社員 / パート / 派遣・その他 (変更なし) | 同左 |
| recruitment_diag | 正社員 / パート / 派遣・その他 (変更なし) | 同左 |

#### HW 警告バナー集約 (採用診断 Panel 5)

採用診断タブの Panel 5 で表示されていた冗長な「HW 掲載求人のみ」警告を 1 行に集約しました。表示頻度・占有面積を削減しつつ、注意喚起は維持されます。

#### 市場概況 H2 直下に HW 限定バナー追加

市場概況タブの H2 直下に、本ダッシュボードが **ハローワーク掲載求人のみ** を対象とすることを明示するバナーを新設しました (`src/handlers/overview.rs:869-871`)。民間求人サイト (Indeed・求人ボックス・自社サイト等) は対象外です。

#### 詳細分析タブに「相関≠因果」フッター追加

詳細分析タブのコンテンツ末尾に、以下の注意書きを表示します (`src/handlers/analysis/handlers.rs:96+`)。

- 本分析はハローワーク掲載求人ベースです。民間求人サイトは含まれません。
- 相関関係と因果関係は別物のため、本ダッシュボードでは「傾向」「可能性」表現に留めています。

#### 媒体分析タブに「外れ値除外（IQR法）」UI 文言追加

給与統計カードと分布カードの見出しに、計算ロジックが既に IQR 法による外れ値除外を実施していることを明示する subtitle を追加しました (`src/handlers/survey/render.rs:341, 405`)。

#### 平均月給 KPI に tooltip 追加

市場概況タブの平均月給 (下限) KPI に「HW 求人は市場実勢より給与を低めに設定する慣習があります」の tooltip を追加しました (`src/handlers/overview.rs:887`)。

---

### 数値計算の変更 (重要 — クライアント資料の数値整合に注意)

#### AP-1 給与改善の年間人件費が約 1.55 倍に変動

採用コスト試算に **賞与 4 ヶ月** + **法定福利費 16%** を反映しました。

| 項目 | 修正前 | 修正後 |
|---|---|---|
| 年間人件費換算式 | `増額 × 12 ヶ月` | `増額 × (12 + 4) × (1 + 0.16)` ≒ `増額 × 18.56` |
| 例: 月額 +20,000 円 | 240,000 円/年 | **371,200 円/年** (約 1.55 倍) |
| 該当ファイル | `src/handlers/insight/engine.rs:943` | (定数: `helpers.rs::AP1_BONUS_MONTHS_DEFAULT=4.0`, `AP1_LEGAL_WELFARE_RATIO=0.16`) |

変更理由: 給与改善の経営判断において、月給 12 ヶ月のみで算出した値は実態 (社会保険料事業主負担 + 賞与) と乖離していたため。

ユーザーへの影響: 「給与改善示唆」の年間負担額表示が大きくなります。クライアント提出資料の数値は前回と一致しません。表示文言に「賞与 4 ヶ月 + 法定福利 16% 含む」を併記します。

#### RC-2 給与差判定の閾値を相対値化

職種間の給与水準差 (介護 vs IT 等) で固定閾値が誤判定を起こしていた問題を修正しました。

| 項目 | 修正前 | 修正後 |
|---|---|---|
| 閾値 | ±20,000 円 / +10,000 円 (固定) | **-10% / +5%** (相対) |
| 介護職 -4.2% (約 -10,000 円) | Warning (誤発火) | Info (適切) |
| IT 職 -12.5% | Info (過小発火) | **Warning** (適切) |
| 該当定数 | (なし) | `helpers.rs::RC2_SALARY_GAP_*_PCT` |

ユーザーへの影響: 介護・福祉系の給与差 Warning 発火率が低下し、IT・専門職の発火率が上昇します。

#### MF-1 医師密度コメントの単位訂正 (誤情報の是正)

監査時の指摘 (10 倍ズレ疑い) に対し、調査の結果 **計算は元から正しかった** ことが判明しました。コメント文の単位表記を「人/1万人 (10,000 人)」へ統一して誤読を防止しています。

ユーザーへの影響: **計算結果は変わりません**。MF-1 を含む過去のクライアント資料は引き続き有効です。

#### posting_change_3m / 1y_pct: 都道府県粒度であることを UI に明記

媒体分析タブで「○○市の 3 ヶ月人員推移 +20%」と表示されていた値は、実態として **都道府県全体** の値でした。実装は `hw_enrichment.rs:108-128` で都道府県単位 fetch → `HwAreaEnrichment` (key=`{prefecture}:{municipality}`) に流し込む仕様のため、UI 上に「※都道府県全体の値」注記を追加しました。

ユーザーへの影響: 数値そのものに変更はありませんが、市区町村別比較に使用する場合は注意が必要です。市区町村粒度のデータ提供は将来リリースで検討します。

---

### 内部変更 (運用者向け)

#### 環境変数 4 個を `AppConfig` に統合

`main.rs` で直接読み出していた以下の 4 環境変数を `src/config.rs::AppConfig` に統合しました。

- `TURSO_EXTERNAL_URL`
- `TURSO_EXTERNAL_TOKEN`
- `SALESNOW_TURSO_URL`
- `SALESNOW_TURSO_TOKEN`

運用者影響: 環境変数の指定方法に変更はありません。挙動は完全互換 (空文字列 = 未設定扱い)。

#### `AUDIT_IP_SALT` がデフォルト値の場合 起動時 warn 警告

`AUDIT_IP_SALT` が未設定 (= デフォルト値 `hellowork-default-salt` のまま) の場合、起動時に以下の warn ログを出力するようになりました。

```
WARN AUDIT_IP_SALT がデフォルト値です。本番では必ず固有の salt を環境変数に設定してください（IP ハッシュのレインボーテーブル攻撃対策）
```

運用者影響: Render の Environment 設定で `AUDIT_IP_SALT` を独自値に設定済の環境では警告は出ません。

#### `.gitignore` 強化

以下のパターンを追加し、E2E 成果物・mock CSV・バックアップファイルの誤コミットを防止します。

- `*.png` (例外: `docs/screenshots/*.png`, `static/guide/*.png`)
- `_*_mock.csv`, `_sec_tmp/`
- `*.bak`, `*.old`
- `*.profraw`, `target/llvm-cov/`

---

## Fixed (バグ修正)

| ID | 内容 | 影響 | 該当ファイル |
|---|---|---|---|
| jobmap Mismatch #1 | 地図タブツールチップで `name` キーが欠落し `undefined` 表示 | 全市区町村のホバー表示 | `src/handlers/jobmap/handlers.rs:399` |
| jobmap Mismatch #4 | 地図タブツールチップで `municipality` キーが欠落 | 同上 | `src/handlers/jobmap/company_markers.rs:128` |
| SW-F06 仕様乖離 | covid_recovery 判定が **人流のみ** で発火 (仕様は人流 AND 求人遅延) | 採用マインド慎重化の誤誘導 | `src/handlers/insight/engine_flow.rs:168-244` |
| SW-F02 vs SW-F05 矛盾 | 同一市区町村で「人材不足」と「観光ポテンシャル」が同時発火 | 示唆の自己矛盾 | `engine_flow.rs:73-95` (排他化: 1.3..1.5 で F02 / 1.5+ で F05) |
| LS-1 「未マッチ層」誤誘導 | 失業者全員が HW 未マッチであるかの誤読 | リサーチ用途で誤解 | `engine.rs:1426` (用語を「失業者数」+ 注記に修正) |
| IN-1 発火条件不明確 | 仕様コメントと実装の乖離 | 医療系求人薄での誤判定 | `engine.rs:1620-1665` (絶対値判定をコメント明記) |
| Panel 5 emp_type フィルタ | UI 値「パート」を SQL 検索 → ヒット 0 件 | 採用診断 Panel 5 の「データ不足」誤表示 | `recruitment_diag/condition_gap.rs:159-200` (`emp_classifier::expand_to_db_values` 経由に) |
| GE-1 phrase_validator 違反 | 「過疎傾向」hint 文にヘッジ語不在 | テスト失敗 (pre-existing) | `engine.rs:1690-1709` (「傾向がみられ」「うかがえます」付加) |
| SW-F06 「100%」表記 | recovery=1.0 で body に「100%」が入り `FORBIDDEN_PHRASES` 違反 | テスト失敗 (pre-existing) | `engine_flow.rs:168-244` (「1.00 倍」表記に変更) |

### `render_no_db_data` ラベル修正

DB 未接続時のフォールバック画面で `"雇用形態別分析"` と表示されていた箇所を `"詳細分析"` に修正しました (`src/handlers/analysis/handlers.rs:23, 36`)。

---

## Tested / Quality

### テスト件数推移

| 区分 | 修正前 | 修正後 |
|---|---|---|
| `cargo test --lib` 全体 | 643 passed / 3 failed (pre-existing) | **670 passed / 0 failed** |
| `insight::pattern_audit_test` | 116 passed / 3 failed | **129 passed / 0 failed** |
| `handlers::survey` | 193 passed | 193 passed (回帰 0) |
| `config::tests` | 2 passed | 5 passed (+3: TURSO/SALESNOW 環境変数) |

### 解消した pre-existing failure (3 件)

- `ge1_info_extreme_sparse_2026_04_23`
- `cross_rc3_positive_with_ge1_info_has_reference`
- `swf06_info_at_full_recovery`

### 適用範囲拡大: phrase_validator

既存 22 patterns (HS / FC / RC / AP / CZ / CF) に統一 helper `push_validated()` を適用し、断定表現 (「不足しています」「到達できます」等) の混在を解消しました。

### 新規モジュール: `emp_classifier.rs`

雇用形態の分類が survey と recruitment_diag で二重定義されていた問題に対し、統一モジュール `src/handlers/emp_classifier.rs` (Regular / PartTime / Other) を新設しました。10 件の単体テストで契約を担保。

> 後方互換のため `survey::classify_emp_group_label` と `recruitment_diag::expand_employment_type` は当面残置 (集計値の変動を伴う移行は次フェーズで release notes 付きで実施)。

### 新規逆証明テスト (10 件)

memory `feedback_reverse_proof_tests.md` 準拠で、修正前/修正後の具体値を assert 形式で記録しています。

| # | テスト名 | 検証 |
|---|---|---|
| 1 | `p2_all_patterns_pass_phrase_validator` | 全 patterns body が validate_insight_phrase 通過 |
| 2 | `p2_ls1_body_excludes_unmatched_layer_terminology` | LS-1 から「未マッチ層」削除 |
| 3 | `p2_swf02_swf05_mutually_exclusive_at_high_ratio` | F02/F05 排他化 |
| 4 | `p2_swf06_suppressed_when_posting_also_recovered` | F06 AND 条件 |
| 5 | `p2_ap1_annual_cost_includes_bonus_and_legal_welfare` | AP-1 ×16×1.16 |
| 6 | `p2_rc2_uses_relative_threshold` | RC-2 相対閾値 |
| 7 | `p2_emp_classifier_contract_and_gyomu_itaku_are_other` | 業務委託 → Other |
| 8 | `p2_emp_classifier_expand_other_includes_gyomu_itaku` | Other expand に業務委託含む |
| 9 | `p2_ge1_extreme_sparse_body_has_hedge_phrase` | GE-1 ヘッジ語確認 |
| 10 | `p2_swf06_full_recovery_body_no_100_percent` | F06 「100%」非含有 |

---

## Documentation

- ルート `CLAUDE.md` (2026-03-14 → 2026-04-26): 9 タブ構成 / Round 1-3 数値 / memory feedback 参照リンクで 350 行全面再構成
- `src/handlers/CLAUDE.md`: 空テンプレ → ハンドラ別責務一覧 (158 行)
- 横断リファレンス 5 種を新規追加:
  - `docs/insight_patterns.md` (38 patterns 全数)
  - `docs/tab_naming_reference.md` (タブ呼称統一規約)
  - `docs/env_variables_reference.md` (環境変数 19 個)
  - `docs/data_sources.md` (6 系統データソース × 9 タブ依存マトリクス)
  - `docs/memory_feedback_mapping.md` (14+ 違反防止ルール × 事故対応)
- `docs/bug_marker_workflow.md`: bug marker 運用ルール (#[ignore] silent failure 対策)
- `docs/dead_route_audit.md`: dead route 削除前の確認手順
- `docs/RELEASE_NOTES_2026-04-26.md` (本ファイル)
- `docs/MANUAL_E2E_2026-04-26.md`: 手動 E2E チェックリスト
- `tests/e2e/regression_2026_04_26.spec.ts`: 自動 E2E 回帰シナリオ

---

## Known Limitations

### CTAS fallback 14 箇所 (5/1 期日)

`src/handlers/survey/flow.rs:88,112,137,163,196,213,229,238,266,281` ほか `flow_context.rs:51,138,208` の計 14 箇所が、Turso 無料枠制約のため動的 GROUP BY による fallback 集計を継続中です。

- **トリガ**: 2026-05-01 Turso リセット
- **対応**: `docs/flow_ctas_restore.md` の手順で CTAS 投入 → コメントマーカーで grep 一発置換 → 逆証明 SQL で総和一致確認
- **影響**: 戻し作業を忘れると本番性能 10x 劣化が継続するリスク

### dead route `/api/insight/report*`

監査で dead route 候補として挙がった `/api/insight/report*` は、本ダッシュボード UI からは到達不可ですが、外部 API として現役利用されている可能性があります。削除は実行せず、確認手順 (`docs/dead_route_audit.md`) のみ整備しました。

### `analysis/render.rs` (4,594 行) / `survey/report_html.rs` (3,702 行)

ファイル肥大は把握済みですが、PDF 設計仕様書 (2026-04-24) の再構成完了後にサブタブ単位で分割する計画です。本リリースでは `report_html.rs` の dead helper 3 関数 (210 行) のみ削除しました。

---

## Migration / Upgrade Guide

### 開発者

```bash
git pull
cargo build --lib
cargo test --lib
# 期待: 670 passed; 0 failed; 1 ignored
```

### 運用者 (Render)

環境変数の追加・削除はありません。再デプロイのみで反映されます。

ただし `AUDIT_IP_SALT` をデフォルト値のまま運用している場合、起動ログに warn が出るようになりました。本番運用では必ず Render Environment で固有値を設定してください。

### クライアント資料を作成中の利用者

- AP-1 「給与改善示唆」の年間人件費は前回 (×12) と一致しません。最新値 (×18.56) で再生成してください。
- RC-2 給与差 Warning の発火傾向が職種間で変動します (介護↓ / IT↑)。
- 「欠員率」表記の資料は「欠員補充率」へ手動置換が必要です (計算値は同一)。

---

## Contributors

5 専門チーム並列監査 (α/β/γ/δ/ε) → 4 並列実装チーム (E1/E2/E3/E4) + F1-F4 統合チーム。

詳細は `docs/audit_2026_04_24/00_overall_assessment.md` 参照。

---

**Released**: 2026-04-26
**Next planned**: 2026-05-02 以降に P0 完遂後再監査 + CTAS 戻し作業 (5/1 Turso リセット後)
