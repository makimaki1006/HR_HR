//! トークの振り返り (ADR-033、設計書 §22)。アポ取りの電話を録音の文字起こしから `checks.json` の項目で判定し、
//! 人ごとの「できていない割合」と印を、Google ログインした社内の人に見せる。
//!
//! - `checks`: 判定項目の定義 (`checks.json`、版 = 内容のハッシュ)
//! - `transcript`: 録音 URL の取り出しと話者分け
//! - `zoom`: Zoom Phone の録音・文字起こし (Server-to-Server OAuth)
//! - `judge`: MiniMax による項目ごとの判定 (差し替え可能な `Judge`)
//! - `lang`: 判定の文章が日本語だけかの検査 (中国語・韓国語・英単語が混ざったら再判定)
//! - `store`: 監査 Turso の表 `call_talk_reviews`
//! - `dials`: 架電数 (営業KPI と同じ Zoom 由来のシートの値)
//! - `dial_breakdown`: 発信の内訳 (つながった・失敗・切った・モニタリング・内線)。過去の日は日単位でキャッシュ
//! - `live_dials`: 今日の架電数を Zoom の通話ログから約 15 分おきに数える
//! - `contact` / `contact_store` / `contact_worker`: 接触・日程切りの判定 (表 `call_contact_marks`、別の周回)
//! - `scope`: アポ取り以外の電話を、かけた時点の取引のステージで外す (接触の判定の手前)
//! - `counts`: アポ数 (日ごとにキャッシュ)
//! - `apo_breakdown`: アポ数の内訳 (過去客・再商談・再アポ・純粋なアポ)
//! - `target`: 9 項目の判定の対象 = 接触した電話 (接触の印と Call の結び付け)
//! - `summary`: 集計と印 (純粋な計算)
//! - `worker`: 背景で対象の Call を見つけて判定する (`CALL_REVIEW_ENABLED=1` のときだけ)
//! - `routes`: `/api/crm/talk-review/*` (Google ログインした社内の人なら誰でも)
//!
//! **単一インスタンス前提** (他の worker と同じ)。

pub mod apo_breakdown;
pub mod checks;
pub mod config;
pub mod contact;
pub mod contact_store;
pub mod contact_worker;
pub mod counts;
pub mod dial_breakdown;
pub mod dials;
pub mod judge;
pub mod lang;
pub mod live_dials;
pub mod routes;
pub mod scope;
pub mod store;
pub mod summary;
pub mod target;
pub mod transcript;
pub mod types;
pub mod worker;
pub mod zoom;

pub use worker::spawn_worker;
