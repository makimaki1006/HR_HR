//! 統合 PDF レポート (P1-03)
//!
//! ペルソナ A (採用コンサル) 決定打: 「介護×東京の戦略提案 PDF を作るのに
//! 8 枚スクショ → PowerPoint 貼り込みが必要」を解消する。
//!
//! # ルート
//! - `GET /report/integrated` → A4 印刷最適化された 1 つの統合 HTML
//!   - ブラウザで `window.print()` → 「PDF として保存」で 1 PDF が生成される
//!
//! # クエリパラメータ
//! - `prefecture` : 都道府県（既定 = セッション値）
//! - `municipality` : 市区町村（既定 = セッション値）
//! - `logo_url` : クライアントロゴ画像 URL（任意。差し替え用、Phase 3 拡張）
//!
//! # 構成（A4 縦印刷時の 1 章 = 1 〜 数ページ）
//! 1. **表紙**: タイトル / 地域 / 産業 / 作成日 / 出典
//! 2. **TL;DR (Executive Summary)**: 1 ページに KPI 6 枚 + So What 主要 3 件
//! 3. **第 1 章 採用診断**: 求人件数・正社員率・給与下限平均・事業所数
//! 4. **第 2 章 地域カルテ**: 構造 KPI 9 枚（人口/世帯/高齢化率/失業率等）
//! 5. **第 3 章 So What 示唆**: insight engine 全カテゴリ
//! 6. **第 4 章 推奨アクション**: ActionProposal カテゴリのみ抽出
//! 7. **巻末**: 出典・データスコープ・免責事項
//!
//! # 設計原則
//! - 既存ハンドラのロジックを **再利用** (重複実装禁止)
//!   - `insight::engine::generate_insights` (示唆エンジン)
//!   - `insight::fetch::build_insight_context` (データ取得)
//!   - `region::karte` の `fetch_karte_bundle` 相当を内部呼び出し可能であれば再利用
//! - HTML は単一の `<!DOCTYPE html>` で完結し、ネストされた `<html>` を作らない
//! - `@media print` で `.page-break-before: always` を保証
//! - `feedback_hw_data_scope.md` 遵守: HW 求人のみであることを表紙と巻末に明記
//! - `feedback_correlation_not_causation.md` 遵守: 「傾向」「可能性」表現

pub mod render;

#[cfg(test)]
mod contract_tests;

pub use render::integrated_report;
