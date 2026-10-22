//! 確認コードのメール。計画 8 章: 会社の Google Workspace の Gmail から送る (別のサービスは使わない)。
//! 送信用のアカウントはまだ決まっていないので、ここでは差し替えられる [`Mailer`] だけを持つ。
use std::future::Future;
use std::pin::Pin;

pub type MailFuture<'a> = Pin<Box<dyn Future<Output = Result<(), MailError>> + Send + 'a>>;

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct MailError;

pub trait Mailer: Send + Sync {
    /// 件名にもコードを入れ、本文は短い文字だけ (リンク・画像は入れない。計画 5.2)
    fn send_code<'a>(&'a self, to: &'a [String], code: &'a str) -> MailFuture<'a>;
}

/// 件名と本文。送信の実装はこれを使う
pub fn code_message(code: &str) -> (String, String) {
    (
        format!("確認コード {code}（応募者の管理）"),
        format!(
            "応募者の管理にログインするための確認コードです。\n\n{code}\n\n\
このコードは 10 分間有効です。\n\
お心当たりがない場合は、このメールを破棄し、弊社の担当者にご連絡ください。\n"
        ),
    )
}

/// 手元の確認用 (debug ビルドだけ)。送らずに、サーバーのログにコードを出す。
#[cfg(debug_assertions)]
pub struct DevLogMailer;

#[cfg(debug_assertions)]
impl Mailer for DevLogMailer {
    fn send_code<'a>(&'a self, to: &'a [String], code: &'a str) -> MailFuture<'a> {
        Box::pin(async move {
            tracing::warn!(
                "[portal dev] 確認コード {code} → {} (手元の確認用。送信していない)",
                to.join(", ")
            );
            Ok(())
        })
    }
}
