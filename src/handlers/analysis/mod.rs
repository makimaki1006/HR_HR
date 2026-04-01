//! V2独自分析ハンドラー
//! Phase 1: C-4(欠員補充率), S-2(地域レジリエンス), C-1(透明性スコア)
//! Phase 1B: 給与構造分析, 給与競争力指数, 報酬パッケージ総合評価
//! Phase 2: L-1(テキスト温度計), L-3(異業種競合), A-1(異常値), S-1(カスケード)
//! Phase 2B: 求人原稿品質分析, キーワードプロファイル
//! Phase 3: 企業採用戦略4象限, 雇用者集中度(独占力), 空間的ミスマッチ
//! Phase 4: 外部データ統合（最低賃金マスタ, 最低賃金違反, 地域ベンチマーク）
//! Phase 5: 予測・推定（充足困難度, 地域間流動性, 給与分位表）
//! 全指標は雇用形態（正社員/パート/その他）でセグメント化

pub(crate) mod fetch;
mod handlers;
mod helpers;
mod render;

pub use handlers::{tab_analysis, analysis_subtab};
