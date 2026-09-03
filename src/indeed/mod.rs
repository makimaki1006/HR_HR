//! Indeed 採用市場データ。
//!
//! # 2 つの出口、1 つの集計
//! * `/tab/indeed`   … 社内。分解・仮説・言ってはいけないことまで出す
//! * `/report/indeed` … 顧客。定点・図・散文だけ。商談の入口として使う
//!
//! 出す範囲は違うが、**集計は必ずこのモジュールを通す**。
//! 画面ごとに集計を持つと、社内で話した数字と顧客に渡した数字が食い違う。
//!
//! # データの出どころ
//! `data/indeed_insights.db`（分析層・約 18MB）。生の DB（125MB）は載せない。
//! 分析層は `scripts/indeed_build_insights.js` が作る。
//!
//! # 移植の裏取り
//! 変化の判定と言い回しは `scripts/indeed_trend_fit.js` からの移植。
//! `tests/fixtures/indeed_golden.json` と `tests/indeed_golden_test.rs` で
//! 数字と文字列の一致を固定している。JS 側を直したら金型を作り直すこと。

pub mod aggregate;
pub mod data;
pub mod trend;
pub mod wording;

/// 顧客レポートを外部に出してよいかの切り替え。
///
/// Indeed の採用市場レポートを社外へ配ってよいかは、利用条件の確認が済んでいない。
/// 確認が取れるまで、顧客向けの経路は既定で閉じておく。
/// 環境変数 `INDEED_PUBLIC=on` で開く。
pub fn public_report_enabled() -> bool {
    std::env::var("INDEED_PUBLIC")
        .map(|v| {
            let v = v.trim().to_ascii_lowercase();
            v == "on" || v == "1" || v == "true"
        })
        .unwrap_or(false)
}

#[cfg(test)]
mod tests {
    use super::*;

    /// 顧客レポートの開閉。
    ///
    /// # なぜ 1 つのテストにまとめてあるか
    /// 環境変数はプロセス全体で 1 つしかない。以前は「既定で閉じている」と
    /// 「on のときだけ開く」を別々のテストにしていたが、テストは既定で並列に
    /// 走るため、片方が消している最中にもう片方が読んで落ちることがあった。
    /// 実際に落ちた。共有しているものを触るテストは分けない。
    #[test]
    fn 顧客レポートは既定で閉じておりonのときだけ開く() {
        // 既定で閉じていること。開け忘れより、閉め忘れのほうが取り返しがつかない
        std::env::remove_var("INDEED_PUBLIC");
        assert!(!public_report_enabled(), "指定が無ければ閉じているべき");

        for v in ["on", "1", "true", "ON", "True"] {
            std::env::set_var("INDEED_PUBLIC", v);
            assert!(public_report_enabled(), "{v} で開くべき");
        }
        for v in ["off", "0", "false", "", "yes", "maybe"] {
            std::env::set_var("INDEED_PUBLIC", v);
            assert!(!public_report_enabled(), "{v} で開いてはいけない");
        }
        std::env::remove_var("INDEED_PUBLIC");
    }
}
