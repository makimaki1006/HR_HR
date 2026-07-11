//! 4軸判定 (計画書 §8): 需要 / 供給 / 競争 / 自社競争力
//!
//! §8.2「総合点を作らない」: 4軸は別々に判定し、合成スコアは一切作らない。
//! 外部表示は 高・中・低 (+ データ不足時は「判定材料不足」) を基本とする。
//! 各判定は根拠 evidence_ids と判定理由文を持つ。

use serde::{Deserialize, Serialize};

use super::config;
use super::evidence::{granularity, EvidenceKind, EvidenceStore};
use super::input::ConsultInput;

/// 軸の判定水準 (§8.2)。総合点は存在しない。
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum AxisLevel {
    High,
    Medium,
    Low,
    /// 判定に必要なデータが不足 (欠損をゼロや低と混同しない。§6.5)
    Unknown,
}

impl AxisLevel {
    pub fn label_ja(&self) -> &'static str {
        match self {
            AxisLevel::High => "高",
            AxisLevel::Medium => "中",
            AxisLevel::Low => "低",
            AxisLevel::Unknown => "判定材料不足",
        }
    }
}

/// 1軸分の判定結果
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct AxisJudgment {
    /// 軸ID: demand / supply / competition / offer_competitiveness
    pub axis: String,
    /// 軸名 (日本語)
    pub axis_label: String,
    pub level: AxisLevel,
    /// 判定根拠の証拠ID
    pub evidence_ids: Vec<String>,
    /// 判定理由文 (可能性表現)
    pub reason: String,
}

/// 4軸判定の結果一式 (§8.2 の JSON 形式に対応)
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct AxesJudgment {
    pub demand: AxisJudgment,
    pub supply: AxisJudgment,
    pub competition: AxisJudgment,
    pub offer_competitiveness: AxisJudgment,
}

impl AxesJudgment {
    pub fn all(&self) -> [&AxisJudgment; 4] {
        [
            &self.demand,
            &self.supply,
            &self.competition,
            &self.offer_competitiveness,
        ]
    }
}

/// 4軸を判定する。証拠は `store` に登録し、判定は evidence_ids で参照する。
pub fn judge_axes(input: &ConsultInput, store: &mut EvidenceStore) -> AxesJudgment {
    let demand = judge_demand(input, store);
    let supply = judge_supply(input, store);
    let competition = judge_competition(input, store);
    let offer = judge_offer_competitiveness(input, store);
    AxesJudgment {
        demand,
        supply,
        competition,
        offer_competitiveness: offer,
    }
}

/// 需要軸: 有効求人倍率を主指標とする (§8.1 需要軸)
fn judge_demand(input: &ConsultInput, store: &mut EvidenceStore) -> AxisJudgment {
    let mut evidence_ids = Vec::new();
    let (level, reason) = match input.job_openings_ratio {
        Some(ratio) => {
            let eid = store.add(
                EvidenceKind::Observed,
                "有効求人倍率",
                &format!("{:.2}", ratio),
                "倍",
                "一般職業紹介状況 (就業構造基本調査由来テーブル)",
                granularity::PREFECTURE,
                None,
                None,
                "県粒度の値。対象職種別ではなく全職種計の可能性がある点に留意",
            );
            evidence_ids.push(eid);
            if ratio >= config::JOB_RATIO_HIGH {
                (
                    AxisLevel::High,
                    format!(
                        "有効求人倍率が{:.2}倍と高く、求人側の需要が強い可能性があります。",
                        ratio
                    ),
                )
            } else if ratio >= config::JOB_RATIO_LOW {
                (
                    AxisLevel::Medium,
                    format!(
                        "有効求人倍率は{:.2}倍で、需要は中程度と考えられます。",
                        ratio
                    ),
                )
            } else {
                (
                    AxisLevel::Low,
                    format!(
                        "有効求人倍率が{:.2}倍と1倍を下回っており、需要は比較的落ち着いている可能性があります。",
                        ratio
                    ),
                )
            }
        }
        None => {
            // 拡充 (2026-07-10): 有効求人倍率がないときは開廃業で需要方向を補完する。
            // 🔴 P0-1: opening_rate/closure_rate は経済センサス調査間の累計。判定は年換算値で行う。
            match (
                input.business_opening_rate,
                input.business_closure_rate,
                input.annualized_opening_rate(),
                input.annualized_closure_rate(),
            ) {
                (Some(open), Some(close), Some(open_a), Some(close_a)) => {
                    let years_note = match input.business_dynamics_interval_years {
                        Some(y) => format!("調査間隔 約{:.0}年の累計", y),
                        None => "調査間隔 不明".to_string(),
                    };
                    let eid = store.add(
                        EvidenceKind::Observed,
                        "開業率 / 廃業率 (経済センサス調査間 累計・需要の代替)",
                        &format!("{:.1} / {:.1}", open, close),
                        "%",
                        "経済センサス 開廃業",
                        granularity::PREFECTURE,
                        None,
                        None,
                        &format!(
                            "有効求人倍率が取得できないため開廃業で代替。{}。判定は年換算値",
                            years_note
                        ),
                    );
                    evidence_ids.push(eid);
                    if open_a >= config::OPENING_RATE_ACTIVE_THRESHOLD && open_a > close_a {
                        (
                            AxisLevel::Medium,
                            format!("有効求人倍率は未取得のため代替判定: 年換算の開業率(約{:.1}%)が廃業率(約{:.1}%)を上回り、雇用の受け皿は拡大方向の可能性があります。", open_a, close_a),
                        )
                    } else if close_a > open_a {
                        (
                            AxisLevel::Low,
                            format!("有効求人倍率は未取得のため代替判定: 年換算の廃業率(約{:.1}%)が開業率(約{:.1}%)を上回り、雇用の受け皿は縮小方向の可能性があります。", close_a, open_a),
                        )
                    } else {
                        (
                            AxisLevel::Medium,
                            "有効求人倍率は未取得のため代替判定: 開廃業は拮抗しており需要は中程度の可能性があります。".to_string(),
                        )
                    }
                }
                _ => (
                    AxisLevel::Unknown,
                    "有効求人倍率データが取得できなかったため、需要水準は判定材料不足です。"
                        .to_string(),
                ),
            }
        }
    };
    AxisJudgment {
        axis: "demand".to_string(),
        axis_label: "需要 (求人側の活発さ)".to_string(),
        level,
        evidence_ids,
        reason,
    }
}

/// 供給軸: 働き手増減率 + 転職希望率 (§8.1 供給軸)
fn judge_supply(input: &ConsultInput, store: &mut EvidenceStore) -> AxisJudgment {
    let mut evidence_ids = Vec::new();
    let mut minus_points = 0usize; // 供給が細い方向の材料数
    let mut plus_points = 0usize; // 供給が保たれている方向の材料数
    let mut fragments: Vec<String> = Vec::new();

    if let Some(rate) = input.wa_decline_rate_muni {
        let eid = store.add(
            EvidenceKind::Observed,
            "働き手人口の将来増減率",
            &format!("{:+.1}", rate),
            "%",
            "国立社会保障・人口問題研究所 将来人口推計",
            granularity::MUNICIPALITY,
            None,
            None,
            "生産年齢人口の将来推計に基づく増減率 (負値=減少)",
        );
        evidence_ids.push(eid);
        if rate <= config::WORKFORCE_DECLINE_THRESHOLD_PCT {
            minus_points += 1;
            fragments.push(format!(
                "働き手人口が将来推計で{:.1}%と大きく減少する見込み",
                rate
            ));
        } else if rate < 0.0 {
            fragments.push(format!("働き手人口は緩やかな減少見込み ({:.1}%)", rate));
        } else {
            plus_points += 1;
            fragments.push(format!("働き手人口は横ばい〜増加見込み ({:+.1}%)", rate));
        }
    }

    if let (Some(pref_rate), Some(nat_rate)) = (
        input.job_change_desire_rate_pref,
        input.job_change_desire_rate_national,
    ) {
        let eid = store.add(
            EvidenceKind::Observed,
            "転職希望率 (県/全国)",
            &format!("{:.1} / {:.1}", pref_rate, nat_rate),
            "%",
            "就業構造基本調査",
            granularity::PREFECTURE,
            None,
            None,
            "県値と全国値の比較。対象職種別ではない点に留意",
        );
        evidence_ids.push(eid);
        if nat_rate > 0.0 {
            let ratio = pref_rate / nat_rate;
            if ratio < config::SWITCHER_THIN_RATIO {
                minus_points += 1;
                fragments.push("転職希望層が全国比で薄め".to_string());
            } else if ratio > config::SWITCHER_THICK_RATIO {
                plus_points += 1;
                fragments.push("転職希望層が全国比で厚め".to_string());
            } else {
                fragments.push("転職希望層は全国並み".to_string());
            }
        }
    }

    // 拡充 (2026-07-10): 人口移動・失業率・自然増減を供給軸の補強材料にする
    if let Some(rate) = input.net_migration_rate {
        let eid = store.add(
            EvidenceKind::Observed,
            "純移動率",
            &format!("{:+.1}", rate),
            "‰",
            "住民基本台帳人口移動報告",
            granularity::MUNICIPALITY,
            None,
            None,
            "負値=転出超過",
        );
        evidence_ids.push(eid);
        if rate <= config::NET_MIGRATION_OUTFLOW_THRESHOLD_PERMILLE {
            minus_points += 1;
            fragments.push(format!("人口は転出超過 ({:+.1}‰)", rate));
        } else if rate > 0.0 {
            plus_points += 1;
            fragments.push(format!("人口は転入超過 ({:+.1}‰)", rate));
        }
    }

    if let (Some(pref_u), Some(nat_u)) = (
        input.unemployment_rate_pref,
        input.unemployment_rate_national,
    ) {
        if nat_u > 0.0 {
            let eid = store.add(
                EvidenceKind::Observed,
                "失業率 (県/全国)",
                &format!("{:.1} / {:.1}", pref_u, nat_u),
                "%",
                "国勢調査 労働力状態",
                granularity::PREFECTURE,
                None,
                None,
                "全産業計。低い=需給が締まる=供給に余裕が少ない",
            );
            evidence_ids.push(eid);
            let ratio = pref_u / nat_u;
            if ratio < config::UNEMPLOYMENT_TIGHT_RATIO {
                minus_points += 1;
                fragments.push("失業率は全国比で低く需給が締まり気味".to_string());
            } else if ratio > config::UNEMPLOYMENT_SLACK_RATIO {
                plus_points += 1;
                fragments.push("失業率は全国比でやや高く求職側に余裕".to_string());
            }
        }
    }

    let (level, reason) = if evidence_ids.is_empty() {
        (
            AxisLevel::Unknown,
            "人材供給に関するデータが取得できなかったため、供給水準は判定材料不足です。"
                .to_string(),
        )
    } else if minus_points >= 2
        || (minus_points >= 1 && plus_points == 0 && evidence_ids.len() >= 2)
    {
        (
            AxisLevel::Low,
            format!("{}。人材供給は細い可能性があります。", fragments.join("、")),
        )
    } else if plus_points >= 1 && minus_points == 0 {
        (
            AxisLevel::High,
            format!(
                "{}。人材供給は比較的保たれている可能性があります。",
                fragments.join("、")
            ),
        )
    } else {
        (
            AxisLevel::Medium,
            format!("{}。供給は中程度と考えられます。", fragments.join("、")),
        )
    };

    AxisJudgment {
        axis: "supply".to_string(),
        axis_label: "供給 (人材側の厚み)".to_string(),
        level,
        evidence_ids,
        reason,
    }
}

/// 競争軸: 今回CSVの同職種求人件数と掲載企業数 (§8.1 競争軸)
fn judge_competition(input: &ConsultInput, store: &mut EvidenceStore) -> AxisJudgment {
    let mut evidence_ids = Vec::new();
    let eid = store.add(
        EvidenceKind::Aggregated,
        // P1-8: 職種を特定していないため「同条件」は矛盾。今回データ内の件数として表現する
        "今回のデータ内の求人件数 (今回CSV)",
        &format!("{}", input.total_postings),
        "件",
        "今回の求人CSV集計",
        granularity::CSV,
        Some(input.total_postings),
        Some(input.as_of.clone()),
        "今回取得した媒体CSVの範囲内での件数。市場全体の求人数ではない",
    );
    evidence_ids.push(eid);
    let eid2 = store.add(
        EvidenceKind::Aggregated,
        "掲載企業数 (今回CSV)",
        &format!("{}", input.company_count),
        "社",
        "今回の求人CSV集計",
        granularity::CSV,
        Some(input.company_count),
        Some(input.as_of.clone()),
        "",
    );
    evidence_ids.push(eid2);

    let (level, reason) = if input.total_postings == 0 {
        (
            AxisLevel::Unknown,
            "今回CSVに求人が含まれていないため、競争環境は判定材料不足です。".to_string(),
        )
    } else if input.total_postings >= config::COMPETITION_POSTINGS_HIGH {
        (
            AxisLevel::High,
            format!(
                "今回のデータ内の求人が{}件・{}社と多く、求職者から比較されやすい環境の可能性があります。",
                input.total_postings, input.company_count
            ),
        )
    } else if input.total_postings >= config::COMPETITION_POSTINGS_MEDIUM {
        (
            AxisLevel::Medium,
            format!(
                "今回のデータ内の求人は{}件・{}社で、競争は中程度と考えられます。",
                input.total_postings, input.company_count
            ),
        )
    } else {
        (
            AxisLevel::Low,
            format!(
                "今回のデータ内の求人は{}件・{}社と比較的少なめです (今回CSVの範囲内)。",
                input.total_postings, input.company_count
            ),
        )
    };

    AxisJudgment {
        axis: "competition".to_string(),
        axis_label: "競争 (同職種求人の密度)".to_string(),
        level,
        evidence_ids,
        reason,
    }
}

/// 自社競争力軸 (§8.1):
/// - 顧客提示給与があれば市場分布内のパーセンタイル
/// - なければ「今回CSV中央値の市場内位置 (県所定内給与比)」で代替し、その旨を明記
fn judge_offer_competitiveness(input: &ConsultInput, store: &mut EvidenceStore) -> AxisJudgment {
    let mut evidence_ids = Vec::new();

    // 顧客提示給与がある場合: パーセンタイル判定
    let client_salary = input
        .client
        .target_salary_max
        .or(input.client.target_salary_min);
    if let Some(salary) = client_salary {
        if let Some(pct) = input.salary_percentile_of(salary) {
            let eid = store.add(
                EvidenceKind::Aggregated,
                "提示給与の市場内パーセンタイル",
                &format!("{:.0}", pct),
                "%",
                "今回の求人CSV集計 + 顧客提示給与",
                granularity::CSV,
                Some(input.salary_n),
                Some(input.as_of.clone()),
                &format!(
                    "顧客提示給与 {} 円を今回CSVの給与分布 (n={}) と比較",
                    salary, input.salary_n
                ),
            );
            evidence_ids.push(eid);
            let (level, reason) = if pct >= config::SALARY_PERCENTILE_HIGH {
                (
                    AxisLevel::High,
                    format!(
                        "提示給与は市場分布の上位 (パーセンタイル{:.0}%) に位置し、給与面の競争力は高い可能性があります。",
                        pct
                    ),
                )
            } else if pct <= config::SALARY_PERCENTILE_LOW {
                (
                    AxisLevel::Low,
                    format!(
                        "提示給与は市場分布の下位 (パーセンタイル{:.0}%) に位置し、給与面では見劣りする可能性があります。",
                        pct
                    ),
                )
            } else {
                (
                    AxisLevel::Medium,
                    format!(
                        "提示給与は市場分布の中位圏 (パーセンタイル{:.0}%) です。",
                        pct
                    ),
                )
            };
            return AxisJudgment {
                axis: "offer_competitiveness".to_string(),
                axis_label: "自社競争力 (給与の市場内位置)".to_string(),
                level,
                evidence_ids,
                reason,
            };
        }
    }

    // 代替判定: 今回CSV中央値 vs 県所定内給与 (自社給与未入力の旨を明記)
    if let (Some(median), Some(pref_wage)) = (input.salary_median, input.scheduled_earnings_latest)
    {
        if !input.is_hourly && pref_wage > 0.0 {
            let ratio = median as f64 / pref_wage;
            let eid = store.add(
                EvidenceKind::Proxy,
                "市場給与中央値の県平均比 (代替指標)",
                &format!("{:.2}", ratio),
                "倍",
                "今回の求人CSV集計 / 毎月勤労統計 地方調査",
                granularity::PREFECTURE,
                Some(input.salary_n),
                Some(input.as_of.clone()),
                "自社給与が未入力のため、今回CSV中央値の市場内位置で代替。粒度差 (CSV=職種近傍 / 県=全産業) に留意",
            );
            evidence_ids.push(eid);
            let (level, reason) = if ratio > config::SALARY_ABOVE_PREF_RATIO {
                (
                    AxisLevel::Medium,
                    format!(
                        "自社給与が未入力のため代替判定: 今回CSVの給与中央値は県平均の{:.2}倍と高めの市場です。自社条件次第で位置づけが変わるため、面談で給与条件の確認が必要です。",
                        ratio
                    ),
                )
            } else if ratio < config::SALARY_BELOW_PREF_RATIO {
                (
                    AxisLevel::Medium,
                    format!(
                        "自社給与が未入力のため代替判定: 今回CSVの給与中央値は県平均の{:.2}倍と低めの市場です。自社条件次第で相対的な優位を作れる可能性があります (要確認)。",
                        ratio
                    ),
                )
            } else {
                (
                    AxisLevel::Medium,
                    format!(
                        "自社給与が未入力のため代替判定: 今回CSVの給与中央値は県平均並み ({:.2}倍) です。",
                        ratio
                    ),
                )
            };
            return AxisJudgment {
                axis: "offer_competitiveness".to_string(),
                axis_label: "自社競争力 (給与の市場内位置)".to_string(),
                level,
                evidence_ids,
                reason,
            };
        }
    }

    AxisJudgment {
        axis: "offer_competitiveness".to_string(),
        axis_label: "自社競争力 (給与の市場内位置)".to_string(),
        level: AxisLevel::Unknown,
        evidence_ids,
        reason: "自社給与の入力がなく、代替となる市場給与データも不足しているため判定材料不足です。面談で給与条件を確認してください。".to_string(),
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::handlers::consult::input::ClientInput;

    fn base_input() -> ConsultInput {
        ConsultInput {
            pref: "群馬県".to_string(),
            muni: "高崎市".to_string(),
            as_of: "2026-07-10".to_string(),
            total_postings: 200,
            company_count: 40,
            salary_values: (0..100).map(|i| 200_000 + i * 1_000).collect(),
            salary_median: Some(250_000),
            salary_q1: Some(225_000),
            salary_q3: Some(275_000),
            salary_n: 100,
            ..Default::default()
        }
    }

    #[test]
    fn demand_high_medium_low_unknown() {
        let mut input = base_input();
        let mut store = EvidenceStore::new();
        input.job_openings_ratio = Some(1.8);
        assert_eq!(judge_demand(&input, &mut store).level, AxisLevel::High);
        input.job_openings_ratio = Some(1.2);
        assert_eq!(judge_demand(&input, &mut store).level, AxisLevel::Medium);
        input.job_openings_ratio = Some(0.8);
        assert_eq!(judge_demand(&input, &mut store).level, AxisLevel::Low);
        input.job_openings_ratio = None;
        let j = judge_demand(&input, &mut store);
        assert_eq!(j.level, AxisLevel::Unknown);
        assert!(j.evidence_ids.is_empty());
    }

    #[test]
    fn supply_low_when_workforce_declines_and_switchers_thin() {
        let mut input = base_input();
        input.wa_decline_rate_muni = Some(-20.0);
        input.job_change_desire_rate_pref = Some(8.0);
        input.job_change_desire_rate_national = Some(10.0);
        let mut store = EvidenceStore::new();
        let j = judge_supply(&input, &mut store);
        assert_eq!(j.level, AxisLevel::Low);
        assert_eq!(j.evidence_ids.len(), 2);
    }

    #[test]
    fn supply_unknown_without_data() {
        let input = base_input();
        let mut store = EvidenceStore::new();
        assert_eq!(judge_supply(&input, &mut store).level, AxisLevel::Unknown);
    }

    #[test]
    fn competition_levels_follow_posting_count() {
        let mut input = base_input();
        let mut store = EvidenceStore::new();
        input.total_postings = 200;
        assert_eq!(judge_competition(&input, &mut store).level, AxisLevel::High);
        input.total_postings = 80;
        assert_eq!(
            judge_competition(&input, &mut store).level,
            AxisLevel::Medium
        );
        input.total_postings = 10;
        assert_eq!(judge_competition(&input, &mut store).level, AxisLevel::Low);
        input.total_postings = 0;
        assert_eq!(
            judge_competition(&input, &mut store).level,
            AxisLevel::Unknown
        );
    }

    #[test]
    fn offer_uses_percentile_when_client_salary_given() {
        let mut input = base_input();
        input.client = ClientInput {
            target_salary_max: Some(295_000), // 分布上位
            ..Default::default()
        };
        let mut store = EvidenceStore::new();
        let j = judge_offer_competitiveness(&input, &mut store);
        assert_eq!(j.level, AxisLevel::High);
        assert!(!j.evidence_ids.is_empty());

        input.client.target_salary_max = Some(200_000); // 分布下位
        let j = judge_offer_competitiveness(&input, &mut store);
        assert_eq!(j.level, AxisLevel::Low);
    }

    #[test]
    fn offer_falls_back_to_market_position_and_mentions_it() {
        let mut input = base_input();
        input.scheduled_earnings_latest = Some(260_000.0);
        let mut store = EvidenceStore::new();
        let j = judge_offer_competitiveness(&input, &mut store);
        assert_eq!(j.level, AxisLevel::Medium);
        assert!(
            j.reason.contains("自社給与が未入力"),
            "代替判定である旨を明記する: {}",
            j.reason
        );
    }

    #[test]
    fn offer_unknown_without_any_salary_data() {
        let mut input = base_input();
        input.salary_median = None;
        let mut store = EvidenceStore::new();
        let j = judge_offer_competitiveness(&input, &mut store);
        assert_eq!(j.level, AxisLevel::Unknown);
    }

    #[test]
    fn no_composite_score_exists() {
        // §8.2 総合点禁止の逆証明: AxesJudgment のシリアライズ結果に
        // 合成スコアらしきキーが存在しないこと
        let input = base_input();
        let mut store = EvidenceStore::new();
        let axes = judge_axes(&input, &mut store);
        let json = serde_json::to_string(&axes).unwrap();
        for banned in [
            "total_score",
            "overall_score",
            "composite",
            "総合点",
            "総合スコア",
        ] {
            assert!(
                !json.contains(banned),
                "総合点は作らない (§8.2): {} が含まれている",
                banned
            );
        }
    }

    #[test]
    fn all_axis_evidence_ids_exist_in_store() {
        let mut input = base_input();
        input.job_openings_ratio = Some(1.4);
        input.wa_decline_rate_muni = Some(-10.0);
        input.scheduled_earnings_latest = Some(255_000.0);
        let mut store = EvidenceStore::new();
        let axes = judge_axes(&input, &mut store);
        for axis in axes.all() {
            for id in &axis.evidence_ids {
                assert!(store.contains_id(id), "証拠ID {} が実在しない", id);
            }
        }
    }
}
