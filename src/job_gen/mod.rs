//! 求人票自動生成パイプライン(工程分割+検証ゲート)。
//!
//! 正本: `docs/job_creation_media_engine_generation_pipeline_v1_2026-07-24.md`
//! v0.5 §4.1 の A→B→C→D→E 骨格の D を「戦略生成+原稿生成」に分割し、
//! E に NGワード検証を追加した実装。
//!
//! 工程(API=Gemini呼び出し回数):
//! - ① [`fact_extract`]: 事実抽出(原文→不変項目、根拠引用つき) + 引用実在チェック
//! - ② [`strategy`]: 市場分析(該当職種の知識のみ注入)
//! - ③ [`strategy`]: ペルソナ設計(3〜5案)
//! - ④ [`strategy`]: キャッチコピー(ペルソナ別) + NGワード検証
//! - ⑤ [`strategy`]: 画像ディレクション
//! - ⑥ [`strategy`]: スマホ原稿(ペルソナ別) + 文字数・NGワード検証
//! - ⑦ [`hrhacker`]: HRハッカー84列原稿 + 数値照合[E] + 文字数 + NGワード
//! - ⑧ [`strategy`]: A/Bテスト助言
//!
//! 検証ゲートは全てコード([`ng_words`], [`validate`])。LLM に検証させない。
//! 検証を通らない項目は空欄+人間レビュー行き(それっぽい値で埋めない)。
//!
//! 設計は既存モジュールと同じ: プロンプト構築・レスポンス解析・検証は純粋関数
//! (ユニットテスト可能)、ライブ HTTP は [`crate::media_engine::gemini`] 経由のみ
//! (HR_HR 統合でプロセス共通レートリミッタを共有)。

pub mod fact_extract;
pub mod handlers;
pub mod hrhacker;
pub mod inputs;
pub mod knowledge;
pub mod ng_words;
pub mod strategy;
pub mod types;
pub mod validate;
