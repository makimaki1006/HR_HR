//! 架電クオリティ: タブ実装の置き場
//!
//! 2026-08-14。GAS 版 16ページを 1ページ = 1ファイルで移す。
//! **並列実装のため、1ファイル1担当を厳守する**（同じファイルを複数人が触らない）。
//!
//! 各タブが従う約束（ここを守らないと画面ごとに数字の意味が変わる）:
//!
//! 1. **列は名前で引く**。`SheetData::col()` を使い、位置で決め打ちしない。
//!    GAS 側で「G列を会社名と誤読して突合が全滅」した事故がある。
//!
//! 2. **分母0のときは 0 でなく `None`(null)を返す**。
//!    「架電0だからアポ率0%」と読ませない。実データに架電0でアポありの行が存在する。
//!
//! 3. **上限で切ったら `truncated` を必ず立てる**。黙って上位N件にしない。
//!
//! 4. **並びを安定させる**。HashMap の反復順をそのまま返さない。
//!
//! 5. **営業スコープの既定は role=sales**。メンバー未選択時に BPO/コンサルを
//!    混ぜない（GAS 側で 141名混在させてアポ率が 0.93%→0.63% に希釈された事故がある）。
//!
//! 6. 集計は**サーバ側**で完結させ、生データを丸ごと返さない。
//!    これが移行の目的そのもの（実測 33.1MB → 6.7KB）。

use serde::Serialize;

pub mod p0_overview;
pub mod p1_members;
pub mod p15_pipeline_mgmt;
pub mod p11_activity;
pub mod pbpo_dashboard;
pub mod p14_owner360;
pub mod p10_future_actions;
pub mod prisk_riskboard;
pub mod ptf_target;
pub mod p3_timeseries;
pub mod p13_timeline;
pub mod p12_churn;
pub mod p8_consulting_contact;
pub mod p2_habits;
pub mod pja_job_application;
pub mod p7_data_browser;

/// 各タブが返す共通の外枠。
/// 画面はこれを見て「いつのデータか」「絞り込みで何件が対象になったか」を出す。
#[derive(Debug, Serialize)]
pub struct TabPayload<T: Serialize> {
    pub data: T,
    /// このタブが読んだシートと、その行数（透明性のため必ず返す）
    pub sources: Vec<SourceInfo>,
    /// サーバ側の処理時間(ms)
    pub elapsed_ms: u128,
    /// **このエンドポイントが解釈できず捨てた引数名**。
    ///
    /// 空でも必ずキーを出す（空配列）。**キーごと消してはいけない**。
    /// 消すと「載っていない = 無かった」なのか「古いサーバ」なのかを
    /// 画面側が区別できなくなる。
    ///
    /// タブ側は `Vec::new()` を入れておけばよい。実際の中身はルータ側
    /// (`routes.rs`) が生のクエリ文字列を見て詰める。タブ関数は生の
    /// クエリ文字列を受け取らないため、ここで判定できるのはルータだけ。
    pub ignored_params: Vec<String>,
}

impl<T: Serialize> TabPayload<T> {
    /// ルータが「解釈できなかった引数」を後乗せする。
    ///
    /// タブ側の構築コードを全部書き換えずに済ませるための入口。
    /// 逆に言うと**ルータがこれを呼び忘れると常に空配列になる**ので、
    /// ハンドラは必ず `routes::finish`（`ignored` を必須引数に取る）経由で返すこと。
    pub fn with_ignored(mut self, ignored: Vec<String>) -> Self {
        self.ignored_params = ignored;
        self
    }
}

#[derive(Debug, Serialize)]
pub struct SourceInfo {
    pub sheet: String,
    pub total_rows: usize,
    pub matched_rows: usize,
    /// 常駐キャッシュから返したか。false なら Sheets を叩いている。
    pub from_cache: bool,
    /// 取得からの経過秒。画面の「最終更新」に使う。
    ///
    /// **GAS 版はここを「シートを読んだ時刻」として出しており、
    /// 実際のデータ生成時刻と混同させていた**。データ自体の生成時刻は
    /// 各シートの「生成時刻」列を見ること。
    pub age_secs: u64,
}

/// 「いま何月か」を **日本時間で** 返す（`YYYY-MM`）。
///
/// 2026-08-17 新設。各タブが `chrono::Local::now()` を使っていたが、
/// 本番(Render)のプロセスは UTC で動くため **毎月1日の 00:00〜09:00 JST に
/// 前月が「当月」になる**。GAS はブラウザ(JST)で判定しているので、
/// 月初の朝だけ画面の当月がずれるという再現しにくい食い違いになる。
pub fn jst_current_ym() -> String {
    let jst = chrono::FixedOffset::east_opt(9 * 3600).expect("JST offset");
    chrono::Utc::now().with_timezone(&jst).format("%Y-%m").to_string()
}

/// 「今日」を **日本時間で** 返す。
///
/// 2026-08-17 新設。`chrono::Local::now().date_naive()` を使っていた箇所は、
/// 本番(Render)が UTC なので **毎日 00:00〜09:00 JST の9時間、日付が1日ずれます**。
/// 当月判定(`jst_current_ym`)のズレが月初の9時間だけだったのに対し、
/// こちらは**毎朝9時間**。しかも未来アクションを見るのはまさにその時間帯で、
/// 「期限切れ / 今日 / 今週 / 来週」の振り分けが丸ごと1日ずれます。
pub fn jst_today() -> chrono::NaiveDate {
    let jst = chrono::FixedOffset::east_opt(9 * 3600).expect("JST offset");
    chrono::Utc::now().with_timezone(&jst).date_naive()
}

/// 分母0を 0% にしないための共通ヘルパ。
/// 各タブで書くと必ずどこかで 0 を返す実装が混ざるので1箇所に置く。
pub fn rate(numerator: f64, denominator: f64) -> Option<f64> {
    if denominator > 0.0 {
        Some(numerator / denominator * 100.0)
    } else {
        None
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    /// タブが読むシートは、必ず2つの一覧に載っていること。
    ///
    /// 2026-08-17 追加。**先に書いた乖離検出テストに穴があった**。
    /// あれは `KNOWN_SHEETS` と `ALLOWED_SHEETS` を比べるだけなので、
    /// **どちらにも載っていないシートは捕まえられない**。
    /// 実際 `アラート除外リスト`（p8 の C-1 が読む運用シート）が
    /// 両方から漏れており、テストは通っていた。
    ///
    /// ここではソースそのものを読んで `SHEET_*: &str = "…"` を抜き、
    /// 実際に使われている名前を根拠にする。**一覧を人が手で保つのをやめる。**
    ///
    /// タブを増やしたら下の `include_str!` にも足すこと。忘れると
    /// 直後の枚数チェックで落ちる。
    #[test]
    fn タブが読むシートは両方の一覧に載っている() {
        let sources: &[&str] = &[
            include_str!("p0_overview.rs"),
        include_str!("p10_future_actions.rs"),
        include_str!("p11_activity.rs"),
        include_str!("p12_churn.rs"),
        include_str!("p13_timeline.rs"),
        include_str!("p14_owner360.rs"),
        include_str!("p15_pipeline_mgmt.rs"),
        include_str!("p1_members.rs"),
        include_str!("p2_habits.rs"),
        include_str!("p3_timeseries.rs"),
        include_str!("p7_data_browser.rs"),
        include_str!("p8_consulting_contact.rs"),
        include_str!("pbpo_dashboard.rs"),
        include_str!("pja_job_application.rs"),
        include_str!("prisk_riskboard.rs"),
        include_str!("ptf_target.rs")
        ];

        // タブを増やして include を足し忘れたら、ここで落ちる
        let declared = include_str!("mod.rs")
            .lines()
            .filter(|l| l.trim_start().starts_with("pub mod "))
            .count();
        assert_eq!(
            sources.len(),
            declared,
            "タブが {declared} 個あるのに、このテストは {} 個しか見ていない。
             include_str! に足すこと。",
            sources.len()
        );

        let known = crate::handlers::call_quality::sheets::KNOWN_SHEETS;
        let allowed = crate::handlers::call_quality::tabs::p7_data_browser::ALLOWED_SHEETS;

        let mut missing: Vec<String> = Vec::new();
        for src in sources {
            for line in src.lines() {
                let t = line.trim();
                if !t.starts_with("const SHEET") && !t.starts_with("pub const SHEET") {
                    continue;
                }
                let Some(rest) = t.split_once('=') else { continue };
                let Some(name) = rest.1.split('"').nth(1) else { continue };
                if !known.contains(&name) {
                    missing.push(format!("{name}（KNOWN_SHEETS に無い）"));
                }
                if !allowed.contains(&name) {
                    missing.push(format!("{name}（ALLOWED_SHEETS に無い）"));
                }
            }
        }
        assert!(
            missing.is_empty(),
            "タブが読むのに一覧に載っていないシートがある:
  {}",
            missing.join("
  ")
        );
    }

    #[test]
    fn 分母0は0パーセントでなくnone() {
        assert_eq!(rate(3.0, 0.0), None, "架電0でアポ3件を 0% と表示してはいけない");
        assert_eq!(rate(0.0, 0.0), None);
    }

    #[test]
    fn 通常の率は計算される() {
        assert_eq!(rate(1.0, 200.0), Some(0.5));
    }

    #[test]
    fn 分子0で分母ありは0パーセント() {
        // これは「本当に0%」なので None ではない
        assert_eq!(rate(0.0, 100.0), Some(0.0));
    }
}
