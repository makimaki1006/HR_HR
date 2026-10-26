//! 案件（稼働中の取引）の「段階」。
//!
//! 2026-10-26 ユーザー決定。上から最初に当てはまるものを付ける。
//! 1. 満了日を過ぎても稼働中 … 段階は付けない（ステージの直し漏れ）。別の画面で一覧にする
//! 2. 段階0 契約前 … 開始日が今日より後
//! 3. 段階3 更新の判断 … 今日が `min(満了日 − 60日, 開始日 + 契約日数 × 0.7)` 以降
//! 4. 段階4 2回目以降 … `renewal_no` が 1 以上
//! 5. 段階2 最初の判断材料 … 面接数が 1 以上（HubSpot の取引の項目 mensetu。いずれ顧客ごとの別のデータから読む）
//! 6. 段階1 立ち上げ … 上のどれでもない
//!
//! 契約は 3・6・12 か月。段階3に入る日は次のとおり（開始日を 0 日目とする）。
//!
//! | 契約 | 日数 | 段階3に入る日 | 目安 |
//! |------|------|--------------|------|
//! | 3か月 | 90 日 | 30 日目 | 開始から約1か月（満了の60日前） |
//! | 6か月 | 181 日 | 121 日目 | 開始から約4か月（満了の60日前） |
//! | 12か月 | 365 日 | 256 日目 | 開始から約8.5か月（契約の7割） |
//!
//! 開始日・満了日が空で判定できない案件は「不明」。
//! 段階5（終わり方）は解約・充足のステージで、稼働中の表には出ない。名前だけ用意している。

use chrono::NaiveDate;

/// 更新の判断に入る境目（満了日の何日前か）
pub const RENEWAL_DAYS_BEFORE_EXPIRY: i64 = 60;
/// 更新の判断に入る境目（契約日数のどこまで進んだか）
pub const RENEWAL_CONTRACT_RATIO: f64 = 0.7;

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Phase {
    /// 満了日を過ぎても稼働中（段階は付けない）
    Overdue,
    /// 開始日か満了日が空で判定できない
    Unknown,
    Pre,
    Launch,
    FirstEvidence,
    RenewalDecision,
    Repeat,
    /// 解約・充足のステージ。稼働中の表では使わない
    #[allow(dead_code)]
    Ending,
}

impl Phase {
    /// 段階の数（0〜5）。付けないもの・不明は None
    pub fn number(self) -> Option<u8> {
        match self {
            Phase::Pre => Some(0),
            Phase::Launch => Some(1),
            Phase::FirstEvidence => Some(2),
            Phase::RenewalDecision => Some(3),
            Phase::Repeat => Some(4),
            Phase::Ending => Some(5),
            Phase::Overdue | Phase::Unknown => None,
        }
    }
    /// 画面に出す短い名前
    pub fn label(self) -> &'static str {
        match self {
            Phase::Pre => "0 契約前",
            Phase::Launch => "1 立ち上げ",
            Phase::FirstEvidence => "2 最初の判断材料",
            Phase::RenewalDecision => "3 更新の判断",
            Phase::Repeat => "4 2回目以降",
            Phase::Ending => "5 終わり方",
            Phase::Overdue => "満了後",
            Phase::Unknown => "不明",
        }
    }
    /// 絞り込みの鍵
    pub fn key(self) -> &'static str {
        match self {
            Phase::Pre => "0",
            Phase::Launch => "1",
            Phase::FirstEvidence => "2",
            Phase::RenewalDecision => "3",
            Phase::Repeat => "4",
            Phase::Ending => "5",
            Phase::Overdue => "overdue",
            Phase::Unknown => "unknown",
        }
    }
}

/// 絞り込みの鍵の並び（段階の順。満了後・不明は最後）
pub const KEYS: [&str; 7] = ["0", "1", "2", "3", "4", "overdue", "unknown"];

/// 鍵から画面に出す名前
pub fn label_of_key(key: &str) -> &'static str {
    [
        Phase::Pre,
        Phase::Launch,
        Phase::FirstEvidence,
        Phase::RenewalDecision,
        Phase::Repeat,
        Phase::Ending,
        Phase::Overdue,
        Phase::Unknown,
    ]
    .into_iter()
    .find(|p| p.key() == key)
    .map_or("不明", Phase::label)
}

/// 更新の判断（段階3）に入る日。`phase_of_deal` の境目と同じ式で、開始日から何日目かを切り上げて日付にする。
pub fn renewal_decision_date(start: NaiveDate, expiry: NaiveDate) -> NaiveDate {
    let total = (expiry - start).num_days() as f64;
    let line = (total - RENEWAL_DAYS_BEFORE_EXPIRY as f64).min(total * RENEWAL_CONTRACT_RATIO);
    start + chrono::Duration::days(line.ceil() as i64)
}

/// 段階を決める。`mensetu` は HubSpot の取引の項目 mensetu の面接数（いずれ顧客ごとの別のデータから読む）
pub fn phase_of_deal(
    start: Option<NaiveDate>,
    expiry: Option<NaiveDate>,
    renewal_no: Option<i64>,
    mensetu: Option<f64>,
    today: NaiveDate,
) -> Phase {
    if expiry.is_some_and(|e| e < today) {
        return Phase::Overdue;
    }
    if start.is_some_and(|s| s > today) {
        return Phase::Pre;
    }
    let (Some(s), Some(e)) = (start, expiry) else {
        return Phase::Unknown;
    };
    let total = (e - s).num_days() as f64;
    let line = (total - RENEWAL_DAYS_BEFORE_EXPIRY as f64).min(total * RENEWAL_CONTRACT_RATIO);
    if ((today - s).num_days() as f64) >= line {
        return Phase::RenewalDecision;
    }
    if renewal_no.is_some_and(|n| n >= 1) {
        return Phase::Repeat;
    }
    if mensetu.is_some_and(|m| m >= 1.0) {
        return Phase::FirstEvidence;
    }
    Phase::Launch
}

#[cfg(test)]
mod tests {
    use super::*;

    fn d(s: &str) -> NaiveDate {
        NaiveDate::parse_from_str(s, "%Y-%m-%d").unwrap()
    }
    fn at(
        start: &str,
        expiry: &str,
        renewal: Option<i64>,
        mensetu: Option<f64>,
        today: &str,
    ) -> Phase {
        phase_of_deal(Some(d(start)), Some(d(expiry)), renewal, mensetu, d(today))
    }

    #[test]
    fn three_month_contract_day_29_and_30() {
        // 2026-01-01〜04-01 は 90 日。60 日前 = 開始 30 日目、7 割 = 63 日目
        assert_eq!(
            at("2026-01-01", "2026-04-01", Some(0), None, "2026-01-30"),
            Phase::Launch
        ); // 29 日目
        assert_eq!(
            at("2026-01-01", "2026-04-01", Some(0), None, "2026-01-31"),
            Phase::RenewalDecision
        ); // 30 日目
    }

    #[test]
    fn boundary_days_for_3_6_12_month_contracts() {
        // (開始, 満了, 境目の前日, 境目の日)
        let cases = [
            ("2026-01-01", "2026-04-01", "2026-01-30", "2026-01-31"), // 3か月: 29 日目 / 30 日目
            ("2026-01-01", "2026-07-01", "2026-05-01", "2026-05-02"), // 6か月: 120 日目 / 121 日目
            ("2026-01-01", "2027-01-01", "2026-09-13", "2026-09-14"), // 12か月: 255 日目 / 256 日目
        ];
        for (s, e, before, on) in cases {
            assert_eq!(
                at(s, e, Some(0), None, before),
                Phase::Launch,
                "{s}〜{e} の前日"
            );
            assert_eq!(
                at(s, e, Some(0), None, on),
                Phase::RenewalDecision,
                "{s}〜{e} の境目"
            );
        }
    }

    #[test]
    fn three_month_contract_with_interview_jumps_2_to_3() {
        assert_eq!(
            at("2026-01-01", "2026-04-01", Some(0), Some(2.0), "2026-01-15"),
            Phase::FirstEvidence
        );
        assert_eq!(
            at("2026-01-01", "2026-04-01", Some(0), Some(2.0), "2026-01-31"),
            Phase::RenewalDecision
        );
    }

    #[test]
    fn six_month_contract_sixty_days_before_comes_first() {
        // 2026-01-01〜07-01 は 181 日。60 日前 = 121 日目、7 割 = 126.7 日目
        assert_eq!(
            at("2026-01-01", "2026-07-01", Some(0), None, "2026-04-30"),
            Phase::Launch
        ); // 119 日目
        assert_eq!(
            at("2026-01-01", "2026-07-01", Some(0), None, "2026-05-02"),
            Phase::RenewalDecision
        ); // 121 日目
    }

    #[test]
    fn twelve_month_contract_seventy_percent_comes_first() {
        // 2026-01-01〜2027-01-01 は 365 日。7 割 = 255.5 日目、60 日前 = 305 日目
        assert_eq!(
            at("2026-01-01", "2027-01-01", Some(0), None, "2026-09-12"),
            Phase::Launch
        ); // 254 日目
        assert_eq!(
            at("2026-01-01", "2027-01-01", Some(0), None, "2026-09-14"),
            Phase::RenewalDecision
        ); // 256 日目
    }

    #[test]
    fn renewal_one_near_expiry_is_3() {
        assert_eq!(
            at("2026-01-01", "2026-07-01", Some(1), Some(3.0), "2026-06-01"),
            Phase::RenewalDecision
        );
        assert_eq!(
            at("2026-01-01", "2026-07-01", Some(1), None, "2026-02-01"),
            Phase::Repeat
        );
    }

    #[test]
    fn interviews_zero_and_one() {
        assert_eq!(
            at("2026-01-01", "2026-12-31", Some(0), Some(0.0), "2026-02-01"),
            Phase::Launch
        );
        assert_eq!(
            at("2026-01-01", "2026-12-31", Some(0), Some(1.0), "2026-02-01"),
            Phase::FirstEvidence
        );
        assert_eq!(
            at("2026-01-01", "2026-12-31", Some(0), None, "2026-02-01"),
            Phase::Launch
        );
    }

    #[test]
    fn future_start_overdue_and_empty_dates() {
        assert_eq!(
            at("2026-03-01", "2026-12-31", Some(0), Some(5.0), "2026-02-01"),
            Phase::Pre
        );
        assert_eq!(
            at("2026-01-01", "2026-01-31", Some(0), None, "2026-02-01"),
            Phase::Overdue
        );
        // 満了日ちょうどは過ぎていない
        assert_eq!(
            at("2026-01-01", "2026-02-01", Some(0), None, "2026-02-01"),
            Phase::RenewalDecision
        );
        assert_eq!(
            phase_of_deal(None, Some(d("2026-12-31")), Some(0), None, d("2026-02-01")),
            Phase::Unknown
        );
        assert_eq!(
            phase_of_deal(Some(d("2026-01-01")), None, Some(0), None, d("2026-02-01")),
            Phase::Unknown
        );
        assert_eq!(
            phase_of_deal(None, None, None, None, d("2026-02-01")),
            Phase::Unknown
        );
        // 開始が未来なら満了日が空でも契約前
        assert_eq!(
            phase_of_deal(Some(d("2026-03-01")), None, None, None, d("2026-02-01")),
            Phase::Pre
        );
    }

    #[test]
    fn renewal_decision_date_matches_phase_boundary() {
        // 3・6・12か月（90・181・365日）の境目の日と、その前日の段階
        let cases = [
            ("2026-01-01", "2026-04-01", "2026-01-31"),
            ("2026-01-01", "2026-07-01", "2026-05-02"),
            ("2026-01-01", "2027-01-01", "2026-09-14"),
        ];
        for (s, e, day) in cases {
            assert_eq!(renewal_decision_date(d(s), d(e)), d(day), "{s}〜{e}");
            assert_eq!(at(s, e, Some(0), None, day), Phase::RenewalDecision);
            let before = d(day) - chrono::Duration::days(1);
            assert_eq!(
                phase_of_deal(Some(d(s)), Some(d(e)), Some(0), None, before),
                Phase::Launch
            );
        }
    }

    #[test]
    fn labels_and_numbers() {
        assert_eq!(Phase::RenewalDecision.label(), "3 更新の判断");
        assert_eq!(Phase::Overdue.number(), None);
        assert_eq!(Phase::Ending.number(), Some(5));
    }
}
