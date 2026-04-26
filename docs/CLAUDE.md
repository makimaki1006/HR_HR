# docs/ ディレクトリ index

**最終更新**: 2026-04-26
**位置付け**: 設計仕様 / 運用手順 / 監査レポート / E2E 計画 のハブ。
**マスターリファレンス**: ルート [`CLAUDE.md`](../CLAUDE.md) を最初に読むこと。

---

## カテゴリ別

### 1. ユーザー向け
- [`USER_GUIDE.md`](USER_GUIDE.md) — タブ別の使い方
- [`USER_MANUAL.md`](USER_MANUAL.md) — 詳細マニュアル

### 2. 横断リファレンス (本監査で新設、2026-04-26)
- [`insight_patterns.md`](insight_patterns.md) — insight 38 patterns カタログ (HS/FC/RC/AP/CZ/CF/LS/HH/MF/IN/GE/SW-F + 閾値 + data source + phrase_validator 適用状況)
- [`tab_naming_reference.md`](tab_naming_reference.md) — タブ呼称統一意思決定 + 4 列リファレンステーブル
- [`env_variables_reference.md`](env_variables_reference.md) — 19 環境変数 × 種別/デフォルト/未設定時影響
- [`data_sources.md`](data_sources.md) — データソースマップ + タブ × データソース 依存マトリクス
- [`memory_feedback_mapping.md`](memory_feedback_mapping.md) — memory feedback 14 ルール → 実コード対応表

### 3. 設計仕様
- [`openapi.yaml`](openapi.yaml) — `/api/v1/*` (MCP/AI 連携)
- [`pdf_design_spec_2026_04_24.md`](pdf_design_spec_2026_04_24.md) — PDF レポート設計
- [`design_ssdse_a_backend.md`](design_ssdse_a_backend.md) / [`_frontend.md`](design_ssdse_a_frontend.md) / [`_expansion.md`](design_ssdse_a_expansion.md) — SSDSE-A
- [`design_agoop_backend.md`](design_agoop_backend.md) / [`_frontend.md`](design_agoop_frontend.md) / [`_jinryu.md`](design_agoop_jinryu.md) — Agoop 人流
- [`requirements_agoop_jinryu.md`](requirements_agoop_jinryu.md) / [`requirements_ssdse_a_expansion.md`](requirements_ssdse_a_expansion.md) — 要件

### 4. 運用・移行手順
- [`flow_ctas_restore.md`](flow_ctas_restore.md) — ★ 5/1 期日: CTAS 戻し手順
- [`turso_import_ssdse_phase_a.md`](turso_import_ssdse_phase_a.md) — SSDSE-A 投入
- [`turso_import_agoop.md`](turso_import_agoop.md) — Agoop 投入
- [`maintenance_posting_mesh1km.md`](maintenance_posting_mesh1km.md) — メッシュメンテ

### 5. 監査・QA
- [`audit_2026_04_24/`](audit_2026_04_24/) — ★ 2026-04-24 全面監査 (5 チーム + 統合 + P4 ドキュ再構成)
- [`contract_audit_2026_04_23.md`](contract_audit_2026_04_23.md) — 全タブ契約監査 (Mismatch #1-#5)
- [`qa_integration_round_1_3.md`](qa_integration_round_1_3.md) — Round 1-3 統合 QA
- [`industry-filter-review-report.md`](industry-filter-review-report.md) — 産業フィルタレビュー
- [`5EXPERT_REVIEW_REPORT.md`](5EXPERT_REVIEW_REPORT.md) — 5 専門家レビュー

### 6. E2E テスト
- [`E2E_TEST_PLAN.md`](E2E_TEST_PLAN.md) / [`E2E_TEST_PLAN_V2.md`](E2E_TEST_PLAN_V2.md) — E2E 計画 (機能/UX)
- [`E2E_COVERAGE_MATRIX.md`](E2E_COVERAGE_MATRIX.md) — カバレッジマトリクス
- [`E2E_REGRESSION_GUIDE.md`](E2E_REGRESSION_GUIDE.md) — リグレッション運用
- [`E2E_RESULTS_LATEST.md`](E2E_RESULTS_LATEST.md) — 最新結果

### 7. 計画・進捗
- [`IMPLEMENTATION_PLAN_V2.md`](IMPLEMENTATION_PLAN_V2.md) — V2 実装計画
- [`IMPROVEMENT_ROADMAP_V2.md`](IMPROVEMENT_ROADMAP_V2.md) — 改善ロードマップ
- [`SESSION_SUMMARY_2026-04-12.md`](SESSION_SUMMARY_2026-04-12.md) — セッションサマリ

---

## 命名規則
- `design_*.md`: 機能設計仕様 (前置き不要、最初から仕様)
- `requirements_*.md`: 要件定義
- `turso_import_*.md` / `maintenance_*.md`: 運用手順
- `*_audit_*.md`: 監査レポート
- `E2E_*.md`: テスト関連

## 新規追加時のルール
1. ファイル名は kebab-case または `FEATURE_TYPE.md`
2. 先頭に **作成日** + **対象範囲** を明記
3. ルート [`CLAUDE.md`](../CLAUDE.md) `§13 ドキュメント索引` に追加リンクすること

---

**改訂履歴**:
- 2026-04-26: 全面投入 (P4 / audit_2026_04_24 #10 対応)。横断リファレンス 5 種を新設追加
- 旧版: 空テンプレ (`*No recent activity*` のみ)
