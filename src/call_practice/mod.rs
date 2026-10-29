//! 架電の練習 (設計書 `docs/architecture/call-practice-design.md`)。第 1 段: 文字 (と音声認識で文字にした入力) だけの最小版。
//!
//! - `persona`: お客さん役の型 (`data/personas.json`、架空の会社・人物だけ)
//! - `roleplay`: お客さん役 (Claude の Messages API を生 HTTP でストリーミング)
//! - `session`: 練習の状態 (プロセス内メモリ。保存しない)
//! - `score`: 採点 (規則の部分 + MiniMax のコメント)
//! - `routes`: `/api/call-practice/*` (管理者だけ)
//! - `types`: API の型 (ts-rs で TypeScript へ)
//!
//! **単一インスタンス前提** (練習の状態がプロセス内メモリにあるため)。

pub mod persona;
pub mod roleplay;
pub mod routes;
pub mod score;
pub mod session;
pub mod types;
