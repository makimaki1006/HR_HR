//! 地域の人員推移を安全に集計するモジュール (2026-08-12)
//!
//! 企業データの `employee_delta_*` を地域単位 (市区町村 / 半径◯km) で集計する際に、
//! 1 社の事情が地域の傾向として表示されてしまう事故を型で防ぐ。
//!
//! # 単位 (実データで確定済み、推測ではない)
//!
//! `employee_delta_{1m,3m,6m,1y,2y}` は **対過去比のパーセント**である。
//!
//! ```text
//! delta = (現在の従業員数 - N期間前の従業員数) / N期間前の従業員数 × 100
//! 増減人数 = employee_count × delta / (100 + delta)
//! ```
//!
//! 2026-08-12 の実査 (`claudedocs/SALESNOW_MAP_PHASE0_FINDINGS_2026-08-12.md` §1) で、
//! 4 つの解釈のうち対過去比%のみが増減人数を整数にした (98.9%、10-50 人帯で 100.0%。
//! 他の解釈は 6.9〜9.4% でランダム基準線 4% と大差なし)。
//! 補強証拠として全 5 期間の最小値が厳密に -100.00 (それ未満 0 件) であり、
//! `delta = -100` の 1,919 社は `employee_count` が 100.0% ちょうど 0 だった。
//!
//! 本項目は 2026-04-30 / 2026-05-14 に 100 倍ずれ事故を 2 回起こしている。
//! 単位を変更する際は必ず実データの分布から再判定すること。
//!
//! # なぜ 2 つのゲートが要るのか
//!
//! **ゲート 1 (企業数) だけでは足りない**し、**人数加重にするだけでも足りない**。
//! この 2 つは別々の問題を解いている。
//!
//! - 単純平均 `AVG(delta)` は分母 (過去の人数) が小さい企業に支配される。
//!   1 人 → 2 人は +100%、1,000 人 → 1,001 人は +0.1%。同じ「1 人増」で 1,000 倍の差。
//!   実測: 東京都 × 人材・アウトソーシング 696 社の単純平均 +752.7% のうち
//!   99.4% (748.4 ポイント) が 1 社由来だった。人数加重なら +2.62%。
//! - ところが人数加重は「大きい 1 社が地域を占める」ケースではむしろ悪化する。
//!   実測: 山梨県南都留郡道志村 9 社は単純平均 +36.08% に対し人数加重 +159.17%。
//!   純増 573 人のうち 570 人 (98.4%) が 1 社の増員である。
//!   この 1 社の delta は +276.70% で、±300% の外れ値フィルタも通過してしまう。
//!
//! よって「人数加重にする」だけでは道志村は止まらない。集中度を独立に測る必要がある。
//!
//! # 閾値の根拠 (実測、勘で置いていない)
//!
//! 全国 1,855 市区町村の人数加重増減率の最大絶対値:
//!
//! | 適用ゲート | 残る市区町村 | 最大絶対値 |
//! |---|---|---|
//! | (なし) | 1,855 | 159.2% |
//! | 企業数 ≥ 30 | 1,101 | 62.2% |
//! | 企業数 ≥ 30 かつ 最大 1 社 < 50% | 1,043 | **9.2%** |
//! | 企業数 ≥ 30 かつ 最大 1 社 < 40% | 1,006 | 9.0% |
//! | 企業数 ≥ 50 かつ 最大 1 社 < 50% | 836 | 9.2% |
//!
//! 40% や 50 社まで厳しくしても改善しないため、30 社 / 50% を採用する。
//! 過去従業員数の下限ゲートは設けない。企業数 30 社を課すと過去従業員数の最小が
//! 491 人 (p1 = 810 人) になり、独立したゲートを足す根拠が実データに無いため。
//!
//! ゲート通過後も市区町村の 56.2% が残り、企業数の 92.3%・従業員数の 94.2% を
//! カバーする。落ちるのは元々ほとんど企業が存在しない地域である。
//!
//! # MEMORY 遵守
//! - `feedback_never_guess_data`: 単位も閾値も実データの分布から決めている
//! - `feedback_test_data_validation` / `feedback_reverse_proof_tests`:
//!   道志村・東京都人材の実値で逆証明する (本ファイル末尾のテスト)
//! - `feedback_correlation_not_causation`: 注記は「傾向として示していません」に留め、
//!   抑制理由を因果で語らない
//! - `feedback_hw_data_scope`: 本モジュールはハローワーク求人を入力にしない

/// 地域の傾向として値を出すために必要な最小企業数。
///
/// 実測根拠: これを下回ると 1 社が地域の人員変動の過半を握る市区町村が多数派に近づく
/// (1〜4 社帯では 85%、5〜9 社帯では 46% が「最大 1 社 ≥ 50%」)。
pub const MIN_COMPANIES: usize = 30;

/// 最大 1 社が地域の人員変動に占めてよい上限 (%)。
///
/// 実測根拠: これを課すと地域増減率の最大絶対値が 62.2% → 9.2% に収まる。
/// 40% まで厳しくしても 9.0% にしかならず、切り捨てる地域だけが増える。
pub const MAX_TOP1_SHARE_PCT: f64 = 50.0;

/// 1 社分の人員推移 (集計の入力)。
#[derive(Clone, Copy, Debug, PartialEq)]
pub struct CompanyDelta {
    /// 現在の従業員数
    pub employee_count: i64,
    /// `employee_delta_*` の生値 (対過去比 %)
    pub delta_pct: f64,
}

impl CompanyDelta {
    pub const fn new(employee_count: i64, delta_pct: f64) -> Self {
        Self {
            employee_count,
            delta_pct,
        }
    }

    /// 増減人数を復元する。復元できない場合は `None`。
    ///
    /// `None` になるのは次の場合:
    /// - `employee_count <= 0` (分母が作れない)
    /// - `delta_pct <= -100` (対過去比の数学的下限。実データでは全社 `employee_count = 0`)
    /// - `delta_pct` が非有限
    ///
    /// # 四捨五入する理由
    ///
    /// 増減人数の真値は整数である (従業員数は整数なので)。`delta_pct` が小数第 2 位まで
    /// 丸められている分だけ計算結果が整数からずれる (実測の最大偏差 0.4983)。
    /// SQLite の `CAST(... AS INTEGER)` のように 0 方向へ切り捨てると、
    /// 実測で 66,794 社 (32.8%) が 1 人ずれ、全国純増減が 2,002 人 (-0.7%) 過小になる。
    /// 増加側も減少側も絶対値が縮む方向のバイアスなので必ず四捨五入する。
    pub fn change_headcount(&self) -> Option<i64> {
        if self.employee_count <= 0 || !self.delta_pct.is_finite() || self.delta_pct <= -100.0 {
            return None;
        }
        let change = self.employee_count as f64 * self.delta_pct / (100.0 + self.delta_pct);
        if !change.is_finite() {
            return None;
        }
        Some(change.round() as i64)
    }

    /// N 期間前の従業員数。
    pub fn past_headcount(&self) -> Option<i64> {
        self.change_headcount().map(|c| self.employee_count - c)
    }
}

/// 値を画面に出してよいかの判定結果。
#[derive(Clone, Copy, Debug, PartialEq)]
pub enum DisplayGate {
    /// 地域の傾向として提示してよい
    Show,
    /// 企業数が足りない
    TooFewCompanies { companies: usize, required: usize },
    /// 1 社が人員変動の大半を占める
    SingleCompanyDominant { top1_share_pct: f64, limit_pct: f64 },
    /// 集計できる企業が無い
    NoData,
}

impl DisplayGate {
    pub const fn is_shown(&self) -> bool {
        matches!(self, Self::Show)
    }

    /// 値を伏せた理由を画面にそのまま出せる日本語で返す。
    ///
    /// 表示する場合は `None`。伏せた理由を必ず言葉にするため、呼び出し側が
    /// 「空欄」だけを描画して利用者に理由が伝わらない状態を避ける。
    pub fn notice(&self) -> Option<String> {
        match self {
            Self::Show => None,
            Self::TooFewCompanies {
                companies,
                required,
            } => Some(format!(
                "この範囲の対象企業は {companies} 社です。\
                 {required} 社に満たないため、地域の傾向としては示していません。"
            )),
            Self::SingleCompanyDominant { top1_share_pct, .. } => Some(format!(
                "この範囲の人員変動の {top1_share_pct:.0}% を 1 社が占めています。\
                 地域の傾向としては示していません。"
            )),
            Self::NoData => Some("この範囲には人員推移を集計できる企業がありません。".to_string()),
        }
    }
}

/// 地域 (市区町村 / 半径内 / 業種セル) の人員推移集計値。
///
/// 率ではなく人数を保持するため、`merge` で足し合わせても歪まない。
/// 半径◯km の集計は市区町村ごとの本構造体を `merge` して作る。
#[derive(Clone, Copy, Debug, Default, PartialEq)]
pub struct HeadcountAggregate {
    /// 集計対象の企業数
    pub companies: usize,
    /// 現在の従業員数の合計
    pub current_employees: i64,
    /// N 期間前の従業員数の合計
    pub past_employees: i64,
    /// 純増減 (人)
    pub net_change: i64,
    /// 増減人数の絶対値の合計。集中度の分母。
    ///
    /// 純増減ではなく絶対値を使うのは、増加と減少が打ち消し合って純増減が 0 近傍に
    /// なったときに集中度が発散するのを避けるため。
    pub total_abs_change: i64,
    /// 最大 1 社の増減人数の絶対値
    pub top1_abs_change: i64,
}

impl HeadcountAggregate {
    /// 企業ごとのレコードから集計する。復元できない企業は黙って除外せず、
    /// `companies` にも数えない (集計に寄与しないため)。
    pub fn from_companies<I: IntoIterator<Item = CompanyDelta>>(rows: I) -> Self {
        let mut agg = Self::default();
        for row in rows {
            let Some(change) = row.change_headcount() else {
                continue;
            };
            agg.companies += 1;
            agg.current_employees += row.employee_count;
            agg.past_employees += row.employee_count - change;
            agg.net_change += change;
            agg.total_abs_change += change.abs();
            agg.top1_abs_change = agg.top1_abs_change.max(change.abs());
        }
        agg
    }

    /// 集計済みの値から組み立てる (SQL 側で SUM した結果を受ける経路)。
    pub const fn from_parts(
        companies: usize,
        current_employees: i64,
        past_employees: i64,
        net_change: i64,
        total_abs_change: i64,
        top1_abs_change: i64,
    ) -> Self {
        Self {
            companies,
            current_employees,
            past_employees,
            net_change,
            total_abs_change,
            top1_abs_change,
        }
    }

    /// 複数地域を合算する (半径◯km の集計に使う)。
    ///
    /// 率を平均するのではなく人数を足すため、合算しても値が歪まない。
    /// 最大 1 社は各地域の最大の中の最大であり、全体の最大と厳密に一致する。
    pub fn merge<'a, I: IntoIterator<Item = &'a Self>>(parts: I) -> Self {
        let mut agg = Self::default();
        for p in parts {
            agg.companies += p.companies;
            agg.current_employees += p.current_employees;
            agg.past_employees += p.past_employees;
            agg.net_change += p.net_change;
            agg.total_abs_change += p.total_abs_change;
            agg.top1_abs_change = agg.top1_abs_change.max(p.top1_abs_change);
        }
        agg
    }

    /// 人数加重の増減率 (%)。過去従業員数が 0 以下なら `None`。
    ///
    /// 定義: 増減人数の合計 ÷ N 期間前の従業員数の合計 × 100。
    /// 各社の率を平均したものではない。
    pub fn weighted_rate_pct(&self) -> Option<f64> {
        if self.past_employees <= 0 {
            return None;
        }
        Some(self.net_change as f64 / self.past_employees as f64 * 100.0)
    }

    /// 最大 1 社が人員変動に占める割合 (%)。
    pub fn top1_share_pct(&self) -> Option<f64> {
        if self.total_abs_change <= 0 {
            return None;
        }
        Some(self.top1_abs_change as f64 / self.total_abs_change as f64 * 100.0)
    }

    /// 地域の傾向として値を出してよいかの判定。
    pub fn gate(&self) -> DisplayGate {
        if self.companies == 0 || self.past_employees <= 0 {
            return DisplayGate::NoData;
        }
        if self.companies < MIN_COMPANIES {
            return DisplayGate::TooFewCompanies {
                companies: self.companies,
                required: MIN_COMPANIES,
            };
        }
        if let Some(share) = self.top1_share_pct() {
            if share >= MAX_TOP1_SHARE_PCT {
                return DisplayGate::SingleCompanyDominant {
                    top1_share_pct: share,
                    limit_pct: MAX_TOP1_SHARE_PCT,
                };
            }
        }
        DisplayGate::Show
    }

    /// 画面に出してよい増減率。ゲートを通らなければ `None`。
    ///
    /// 呼び出し側が `weighted_rate_pct()` を直接使うと抑制を素通りできてしまうため、
    /// 表示経路ではこちらを使う。地図の塗り分けも `None` なら色を塗らない。
    pub fn displayed_rate_pct(&self) -> Option<f64> {
        if self.gate().is_shown() {
            self.weighted_rate_pct()
        } else {
            None
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    /// 実データ: 山梨県南都留郡道志村の全 9 社 (2026-08-12 CSV 実測値)
    ///
    /// (employee_count, employee_delta_1y)
    const DOSHI_MURA: [(i64, f64); 9] = [
        (776, 276.70), // 株式会社加藤電器製作所
        (28, 12.00),
        (27, -6.90),
        (19, 11.76),
        (46, -2.13),
        (4, 33.33),
        (5, 0.00),
        (2, 0.00),
        (26, 0.00),
    ];

    fn doshi() -> HeadcountAggregate {
        HeadcountAggregate::from_companies(
            DOSHI_MURA.iter().map(|&(ec, d)| CompanyDelta::new(ec, d)),
        )
    }

    // ============================================================
    // 単位の逆証明 (100 倍ずれ事故 2 回の再発防止)
    // ============================================================

    #[test]
    fn delta_is_percent_against_past_headcount() {
        // 実データ最大の外れ値: 1 年前 1 人 → 現在 5,210 人
        let c = CompanyDelta::new(5210, 520_900.0);
        assert_eq!(c.change_headcount(), Some(5209));
        assert_eq!(c.past_headcount(), Some(1));

        // 対現在比だと 5210 * 520900/100 = 2,713 万人になり桁が違う。
        // 比率だと 5210 * 520900/520901 ≒ 5,210 人で past が 0 になる。
        // どちらも上の値にはならない。
    }

    #[test]
    fn doshi_mura_each_company_change_is_integer() {
        // 増減人数の真値は整数。復元値が整数になることが単位判定の根拠だった。
        let expected = [570, 3, -2, 2, -1, 1, 0, 0, 0];
        for (i, &(ec, d)) in DOSHI_MURA.iter().enumerate() {
            assert_eq!(
                CompanyDelta::new(ec, d).change_headcount(),
                Some(expected[i]),
                "道志村 {i} 社目 (employee_count={ec}, delta={d})"
            );
        }
    }

    #[test]
    fn rounds_instead_of_truncating() {
        // 加藤電器製作所: 776 × 276.70 / 376.70 = 569.9006...
        // 切り捨てると 569 人 (真値 570 人に対し 1 人過小)
        assert_eq!(CompanyDelta::new(776, 276.70).change_headcount(), Some(570));
        // 道志化学工業所: 27 × -6.90 / 93.10 = -2.0011...
        // 0 方向切り捨てだと -2 だが、-1.99 のような値では -1 になり減少が過小になる
        assert_eq!(CompanyDelta::new(19, 11.76).change_headcount(), Some(2)); // 1.99928 → 2
    }

    #[test]
    fn rejects_unreconstructable_input() {
        // delta = -100 は対過去比の下限。実データでは全社 employee_count = 0 だった
        assert_eq!(CompanyDelta::new(0, -100.0).change_headcount(), None);
        assert_eq!(CompanyDelta::new(100, -100.0).change_headcount(), None);
        assert_eq!(CompanyDelta::new(100, -150.0).change_headcount(), None);
        assert_eq!(CompanyDelta::new(0, 5.0).change_headcount(), None);
        assert_eq!(CompanyDelta::new(-5, 5.0).change_headcount(), None);
        assert_eq!(CompanyDelta::new(100, f64::NAN).change_headcount(), None);
        assert_eq!(
            CompanyDelta::new(100, f64::INFINITY).change_headcount(),
            None
        );
    }

    // ============================================================
    // 道志村: 1 社の増員が地域の傾向として表示されないこと
    // ============================================================

    #[test]
    fn doshi_mura_aggregate_matches_measured_values() {
        let a = doshi();
        assert_eq!(a.companies, 9);
        assert_eq!(a.current_employees, 933);
        assert_eq!(a.past_employees, 360);
        assert_eq!(a.net_change, 573);
        assert_eq!(a.total_abs_change, 579);
        assert_eq!(a.top1_abs_change, 570);

        // 人数加重増減率 +159.17%
        let rate = a.weighted_rate_pct().expect("過去従業員数がある");
        assert!(
            (rate - 159.17).abs() < 0.01,
            "人数加重増減率が実測と違う: {rate}"
        );
        // 最大 1 社の占有率 98.4%
        let share = a.top1_share_pct().expect("変動がある");
        assert!(
            (share - 98.4).abs() < 0.1,
            "最大1社の占有率が実測と違う: {share}"
        );
    }

    #[test]
    fn doshi_mura_is_suppressed_on_map() {
        let a = doshi();
        assert!(!a.gate().is_shown());
        assert_eq!(
            a.displayed_rate_pct(),
            None,
            "9 社で純増の 98.4% が 1 社由来の地域に +159.17% を表示してはならない"
        );
        // 抑制理由が利用者に伝わること
        let notice = a.gate().notice().expect("理由を返す");
        assert!(notice.contains("9 社"), "実際の企業数を示す: {notice}");
        assert!(notice.contains("30 社"), "必要な企業数を示す: {notice}");
    }

    #[test]
    fn headcount_weighting_alone_does_not_stop_doshi_mura() {
        // 条件 1 (単純平均 → 人数加重) だけでは不十分であることの逆証明。
        // 道志村は単純平均 +36.08% に対し人数加重 +159.17% で、加重の方が大きい。
        let a = doshi();
        let simple_avg: f64 =
            DOSHI_MURA.iter().map(|&(_, d)| d).sum::<f64>() / DOSHI_MURA.len() as f64;
        let weighted = a.weighted_rate_pct().unwrap();
        assert!(
            (simple_avg - 36.08).abs() < 0.01,
            "単純平均が実測と違う: {simple_avg}"
        );
        assert!(
            weighted > simple_avg,
            "この地域では人数加重の方が大きい ({weighted} > {simple_avg})。\
             だから集中度ゲートが独立に必要"
        );
        // ±300% の外れ値フィルタでも止まらないことを示す
        assert!(
            DOSHI_MURA.iter().all(|&(_, d)| d.abs() <= 300.0),
            "道志村の全社が ±300% 以内。外れ値フィルタは素通りする"
        );
    }

    #[test]
    fn suppresses_single_company_dominance_even_with_enough_companies() {
        // 30 社あるが 1 社が変動の大半を占めるケース
        let mut rows = vec![CompanyDelta::new(1000, 100.0)]; // +500 人
        rows.extend((0..29).map(|_| CompanyDelta::new(100, 1.0))); // 各 +1 人
        let a = HeadcountAggregate::from_companies(rows);
        assert_eq!(a.companies, 30);
        assert!(a.companies >= MIN_COMPANIES, "企業数ゲートは通る");
        match a.gate() {
            DisplayGate::SingleCompanyDominant { top1_share_pct, .. } => {
                assert!(top1_share_pct > 90.0, "占有率: {top1_share_pct}");
            }
            other => panic!("一社集中で止まるべき: {other:?}"),
        }
        assert_eq!(a.displayed_rate_pct(), None);
    }

    // ============================================================
    // 東京都 × 人材・アウトソーシング: 現行実装との差
    // ============================================================

    #[test]
    fn tokyo_staffing_simple_average_diverges_from_weighted_rate() {
        // 実データ 696 社の集計値 (2026-08-12 CSV 実測)
        let a = HeadcountAggregate::from_parts(696, 1_099_581, 1_071_508, 28_073, 59_031, 5_209);

        let rate = a.displayed_rate_pct().expect("ゲートを通る");
        assert!((rate - 2.62).abs() < 0.01, "人数加重増減率: {rate}");

        // 現行実装 AVG(employee_delta_1y) が返す値は +752.70%。
        // 1 社 (1 人 → 5,210 人) が 748.4 ポイント、つまり 99.4% を占めていた。
        const CURRENT_AVG_IMPL: f64 = 752.70;
        assert!(
            CURRENT_AVG_IMPL / rate > 250.0,
            "単純平均は人数加重の 250 倍以上ずれる: {CURRENT_AVG_IMPL} vs {rate}"
        );

        // 同じ 1 社を人数で見れば 5,209 / 59,031 = 8.8% にすぎない
        let share = a.top1_share_pct().unwrap();
        assert!((share - 8.82).abs() < 0.01, "最大1社の占有率: {share}");
        assert!(share < MAX_TOP1_SHARE_PCT, "集中度ゲートは通る");
    }

    // ============================================================
    // 半径集計 (merge) が汚染されないこと
    // ============================================================

    #[test]
    fn radius_merge_sums_headcount_not_rates() {
        // 小さい地域 (過去 100 人 / +50 人 = +50%) と
        // 大きい地域 (過去 10,000 人 / +100 人 = +1%) を合算する
        let small = HeadcountAggregate::from_parts(30, 150, 100, 50, 50, 10);
        let large = HeadcountAggregate::from_parts(500, 10_100, 10_000, 100, 300, 20);
        let merged = HeadcountAggregate::merge([&small, &large]);

        assert_eq!(merged.companies, 530);
        assert_eq!(merged.past_employees, 10_100);
        assert_eq!(merged.net_change, 150);

        let rate = merged.weighted_rate_pct().unwrap();
        // 人数加重: 150 / 10,100 = +1.49%
        assert!((rate - 1.485).abs() < 0.01, "合算後の増減率: {rate}");
        // 率の平均 (50% + 1%) / 2 = 25.5% にはならない
        assert!(rate < 5.0, "率を平均すると 25.5% になってしまう: {rate}");
    }

    #[test]
    fn radius_merge_keeps_overall_top1_company() {
        // merge 後の top1 が各地域の top1 の最大と一致すること
        let a = HeadcountAggregate::from_parts(40, 1000, 900, 100, 120, 30);
        let b = doshi(); // top1 = 570
        let merged = HeadcountAggregate::merge([&a, &b]);
        assert_eq!(merged.top1_abs_change, 570, "全体の最大 1 社を保持する");
        assert_eq!(merged.total_abs_change, 120 + 579);
        assert_eq!(merged.companies, 49);
    }

    #[test]
    fn radius_merge_can_show_when_concentration_resolves() {
        // 道志村単独では出せないが、半径内に十分な企業が入れば地域として成立する。
        // ただし 1 社集中が解消していることが条件。
        let big = HeadcountAggregate::from_parts(200, 20_000, 19_800, 200, 3_000, 80);
        let merged = HeadcountAggregate::merge([&big, &doshi()]);
        assert_eq!(merged.companies, 209);
        // 570 / (3000 + 579) = 15.9% で集中度ゲートを通る
        let share = merged.top1_share_pct().unwrap();
        assert!(share < MAX_TOP1_SHARE_PCT, "占有率: {share}");
        assert!(merged.gate().is_shown());
        let rate = merged.displayed_rate_pct().unwrap();
        // (200 + 573) / (19,800 + 360) = +3.83%
        assert!((rate - 3.834).abs() < 0.01, "合算後の増減率: {rate}");
    }

    // ============================================================
    // 境界とデータ欠落
    // ============================================================

    #[test]
    fn no_data_yields_no_value() {
        let empty = HeadcountAggregate::from_companies(Vec::<CompanyDelta>::new());
        assert_eq!(empty.gate(), DisplayGate::NoData);
        assert_eq!(empty.displayed_rate_pct(), None);
        assert!(empty.gate().notice().is_some(), "理由を必ず返す");
    }

    #[test]
    fn zero_change_everywhere_is_not_treated_as_concentrated() {
        // total_abs_change = 0 のとき 0 割りにならず、増減率 0% を表示できること
        let rows: Vec<_> = (0..30).map(|_| CompanyDelta::new(100, 0.0)).collect();
        let a = HeadcountAggregate::from_companies(rows);
        assert_eq!(a.net_change, 0);
        assert_eq!(a.top1_share_pct(), None);
        assert_eq!(a.gate(), DisplayGate::Show);
        assert_eq!(a.displayed_rate_pct(), Some(0.0));
    }

    #[test]
    fn concentration_stays_finite_when_gains_and_losses_cancel() {
        // 純増減が 0 でも、絶対値を分母にしているので集中度は測れる
        let a = HeadcountAggregate::from_companies(vec![
            CompanyDelta::new(200, 100.0), // +100 人
            CompanyDelta::new(100, -50.0), // -100 人
        ]);
        assert_eq!(a.net_change, 0);
        assert_eq!(a.total_abs_change, 200);
        assert_eq!(a.top1_share_pct(), Some(50.0));
    }

    #[test]
    fn displayed_rate_always_passes_through_gate() {
        // displayed_rate_pct はゲートを必ず通す。weighted_rate_pct は生値を返す。
        let a = doshi();
        assert!(a.weighted_rate_pct().is_some(), "生値は計算できる");
        assert!(a.displayed_rate_pct().is_none(), "表示経路では None");
    }

    #[test]
    fn notice_is_empty_only_when_shown() {
        let ok = HeadcountAggregate::from_parts(100, 10_000, 9_900, 100, 500, 50);
        assert!(ok.gate().is_shown());
        assert_eq!(ok.gate().notice(), None);
    }
}
