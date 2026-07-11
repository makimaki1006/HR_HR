//! 市場側シグナル生成 (計画書 §9)
//!
//! シグナル = 観測値を解釈可能な中間表現へ変換したもの。
//! いずれも確定判定ではなく、代替説明 (M&A・拠点移動・データ更新時差等 §9.1注意) を保持する。
//! 面談前に判定可能な市場側シグナルのみを扱う (応募数等の顧客データ依存はフェーズC)。
//!
//! 閾値は `config.rs` に一元化。各シグナルは fired / not_fired と根拠 evidence_ids を持つ。

use serde::{Deserialize, Serialize};

use super::config;
use super::evidence::{granularity, EvidenceKind, EvidenceStore};
use super::input::ConsultInput;

/// シグナル1件
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct Signal {
    /// S-01 形式のID
    pub id: String,
    /// シグナル名 (日本語)
    pub name: String,
    /// 発火したか
    pub fired: bool,
    /// 根拠の証拠ID (発火・非発火とも、判定に使った証拠を持つ)
    pub evidence_ids: Vec<String>,
    /// 解釈 (可能性表現)。発火時のみ意味を持つ
    pub interpretation: String,
    /// 代替説明 (§9.1注意: M&A・拠点移動・データ更新時差等)
    pub alternative_explanations: Vec<String>,
    /// データ欠損等で判定できなかった場合の注記 (空=判定できた)
    pub data_note: String,
}

impl Signal {
    fn not_evaluable(id: &str, name: &str, note: &str) -> Self {
        Signal {
            id: id.to_string(),
            name: name.to_string(),
            fired: false,
            evidence_ids: vec![],
            interpretation: String::new(),
            alternative_explanations: vec![],
            data_note: note.to_string(),
        }
    }
}

/// 掲載経過テキスト群から「30+日前」比率を算出する純関数。
///
/// 現行の集計パイプラインは掲載経過テキストを保持していないため本番では未使用だが、
/// 将来 `SurveyAggregation` に掲載経過分布が追加された際にそのまま接続できるよう
/// ロジックとテストを先行して用意する。
pub fn compute_posting_age_30plus_ratio(posting_age_texts: &[&str]) -> Option<f64> {
    let known: Vec<&&str> = posting_age_texts
        .iter()
        .filter(|t| !t.trim().is_empty())
        .collect();
    if known.is_empty() {
        return None;
    }
    let over30 = known.iter().filter(|t| t.contains("30+")).count();
    Some(over30 as f64 / known.len() as f64)
}

/// 全シグナルを評価する
pub fn evaluate_signals(input: &ConsultInput, store: &mut EvidenceStore) -> Vec<Signal> {
    vec![
        s01_long_running_postings(input, store),
        s02_client_salary_bottom_quartile(input, store),
        s03_client_salary_top_quartile(input, store),
        s04_min_wage_proximity(input, store),
        s05_market_salary_below_pref(input, store),
        s06_employee_decline_with_postings(input, store),
        s07_workforce_declining_region(input, store),
        s08_switcher_supply_thin(input, store),
        s09_switcher_supply_thick(input, store),
        s10_job_ratio_high(input, store),
        s11_job_ratio_low(input, store),
        s12_commute_inflow(input, store),
        s13_new_posting_ratio_high(input, store),
        s14_sample_insufficient(input, store),
        s15_posting_concentration(input, store),
        // ---- 拡充シグナル (2026-07-10) ----
        s16_net_migration_outflow(input, store),
        s17_daytime_outflow(input, store),
        s18_closure_over_opening(input, store),
        s19_opening_active(input, store),
        s20_unemployment_tight(input, store),
        s21_unemployment_slack(input, store),
        s22_rent_burden(input, store),
        s23_natural_decline(input, store),
        s24_holiday_mention_thin(input, store),
        s25_holiday_level_low(input, store),
        s26_tag_variety_thin(input, store),
        s27_popular_badge_concentration(input, store),
        s28_commute_delivery_check(input, store),
        s29_growing_companies(input, store),
        s30_nonregular_share_high(input, store),
    ]
}

/// S-01 継続掲載: 「30+日前」比率が閾値以上
fn s01_long_running_postings(input: &ConsultInput, store: &mut EvidenceStore) -> Signal {
    const ID: &str = "S-01";
    const NAME: &str = "長期掲載求人の比率が高い";
    match input.posting_age_30plus_ratio {
        Some(ratio) => {
            let eid = store.add(
                EvidenceKind::Aggregated,
                "掲載30日以上の求人比率",
                &format!("{:.0}", ratio * 100.0),
                "%",
                "今回の求人CSV集計",
                granularity::CSV,
                Some(input.total_postings),
                Some(input.as_of.clone()),
                "「30+日前」表示は下限値であり、正確な掲載開始日ではない",
            );
            Signal {
                id: ID.to_string(),
                name: NAME.to_string(),
                fired: ratio >= config::POSTING_AGE_30PLUS_RATIO_THRESHOLD,
                evidence_ids: vec![eid],
                interpretation: "長期間掲載が続く求人が多く、この市場では採用充足に時間がかかっている可能性があります。".to_string(),
                alternative_explanations: vec![
                    "常時採用方針で意図的に掲載を続けている可能性".to_string(),
                    "掲載更新のタイミングにより経過表示がリセットされていない可能性".to_string(),
                ],
                data_note: String::new(),
            }
        }
        None => Signal::not_evaluable(
            ID,
            NAME,
            "掲載経過データが現在の集計に含まれていないため判定できません (今後の拡張項目)",
        ),
    }
}

/// S-02 提示給与が市場下位25% (顧客給与入力がある場合のみ)
fn s02_client_salary_bottom_quartile(input: &ConsultInput, store: &mut EvidenceStore) -> Signal {
    const ID: &str = "S-02";
    const NAME: &str = "提示給与が市場の下位25%圏";
    let salary = input
        .client
        .target_salary_max
        .or(input.client.target_salary_min);
    match (salary, input.salary_percentile_of(salary.unwrap_or(0))) {
        (Some(s), Some(pct)) => {
            let eid = store.add(
                EvidenceKind::Aggregated,
                "提示給与の市場内パーセンタイル",
                &format!("{:.0}", pct),
                "%",
                "今回の求人CSV集計 + 顧客提示給与",
                granularity::CSV,
                Some(input.salary_n),
                Some(input.as_of.clone()),
                &format!("提示給与 {} 円で判定", s),
            );
            Signal {
                id: ID.to_string(),
                name: NAME.to_string(),
                fired: pct <= config::SALARY_PERCENTILE_LOW,
                evidence_ids: vec![eid],
                interpretation:
                    "提示給与が市場分布の下位圏にあり、給与面で比較負けしやすい可能性があります。"
                        .to_string(),
                alternative_explanations: vec![
                    "給与以外の条件 (休日・勤務時間・通勤) で補える可能性".to_string(),
                    "市場側の給与表示に固定残業代等が含まれ、見かけ上高く出ている可能性"
                        .to_string(),
                ],
                data_note: String::new(),
            }
        }
        _ => Signal::not_evaluable(
            ID,
            NAME,
            "顧客の提示給与が未入力のため判定できません (面談で確認)",
        ),
    }
}

/// S-03 提示給与が市場上位25% (顧客給与入力がある場合のみ)
fn s03_client_salary_top_quartile(input: &ConsultInput, store: &mut EvidenceStore) -> Signal {
    const ID: &str = "S-03";
    const NAME: &str = "提示給与が市場の上位25%圏";
    let salary = input
        .client
        .target_salary_max
        .or(input.client.target_salary_min);
    match (salary, input.salary_percentile_of(salary.unwrap_or(0))) {
        (Some(s), Some(pct)) => {
            let eid = store.add(
                EvidenceKind::Aggregated,
                "提示給与の市場内パーセンタイル",
                &format!("{:.0}", pct),
                "%",
                "今回の求人CSV集計 + 顧客提示給与",
                granularity::CSV,
                Some(input.salary_n),
                Some(input.as_of.clone()),
                &format!("提示給与 {} 円で判定", s),
            );
            Signal {
                id: ID.to_string(),
                name: NAME.to_string(),
                fired: pct >= config::SALARY_PERCENTILE_HIGH,
                evidence_ids: vec![eid],
                interpretation:
                    "提示給与が市場分布の上位圏にあり、給与面の訴求余地がある可能性があります。"
                        .to_string(),
                alternative_explanations: vec![
                    "給与が高い分、求められる要件が厳しく応募ハードルが高い可能性".to_string(),
                ],
                data_note: String::new(),
            }
        }
        _ => Signal::not_evaluable(
            ID,
            NAME,
            "顧客の提示給与が未入力のため判定できません (面談で確認)",
        ),
    }
}

/// S-04 最低賃金近接: 市場給与の下位が最低賃金換算に近い
fn s04_min_wage_proximity(input: &ConsultInput, store: &mut EvidenceStore) -> Signal {
    const ID: &str = "S-04";
    const NAME: &str = "市場給与の下位帯が最低賃金に近接";
    if input.is_hourly {
        // 時給モード: 時給下限中央値 vs 最低賃金時給
        match (input.hourly_median_low, input.min_wage_hourly) {
            (Some(low), Some(mw)) if mw > 0.0 => {
                let eid = store.add(
                    EvidenceKind::Aggregated,
                    "時給下限の中央値 / 地域別最低賃金",
                    &format!("{} / {:.0}", low, mw),
                    "円/時",
                    "今回の求人CSV集計 / 地域別最低賃金",
                    granularity::PREFECTURE,
                    Some(input.salary_n),
                    Some(input.as_of.clone()),
                    "時給下限の中央値は参考値 (時給以外のレコードが混在する場合がある)",
                );
                Signal {
                    id: ID.to_string(),
                    name: NAME.to_string(),
                    fired: (low as f64) <= mw * config::MIN_WAGE_PROXIMITY_RATIO,
                    evidence_ids: vec![eid],
                    interpretation: "市場の時給下限帯が最低賃金に近く、わずかな上乗せでも相対的な優位を作れる可能性があります。".to_string(),
                    alternative_explanations: vec![
                        "最低賃金改定直後で市場が追随中の可能性".to_string(),
                    ],
                    data_note: String::new(),
                }
            }
            _ => Signal::not_evaluable(ID, NAME, "時給データまたは最低賃金データが不足しています"),
        }
    } else {
        match (input.salary_q1, input.min_wage_monthly_160h) {
            (Some(q1), Some(mw)) if mw > 0.0 => {
                let eid = store.add(
                    EvidenceKind::Aggregated,
                    "給与Q1 / 最低賃金×160時間換算",
                    &format!("{} / {:.0}", q1, mw),
                    "円/月",
                    "今回の求人CSV集計 / 地域別最低賃金",
                    granularity::PREFECTURE,
                    Some(input.salary_n),
                    Some(input.as_of.clone()),
                    "最低賃金×160時間は労働時間を仮定した換算値",
                );
                Signal {
                    id: ID.to_string(),
                    name: NAME.to_string(),
                    fired: (q1 as f64) <= mw * config::MIN_WAGE_PROXIMITY_RATIO,
                    evidence_ids: vec![eid],
                    interpretation: "市場の給与下位帯が最低賃金換算に近く、条件の底上げ余地が論点になる可能性があります。".to_string(),
                    alternative_explanations: vec![
                        "短時間勤務求人が混在し月額表示が低く出ている可能性".to_string(),
                    ],
                    data_note: String::new(),
                }
            }
            _ => Signal::not_evaluable(ID, NAME, "給与または最低賃金データが不足しています"),
        }
    }
}

/// S-05 市場給与が県所定内給与を下回る傾向
fn s05_market_salary_below_pref(input: &ConsultInput, store: &mut EvidenceStore) -> Signal {
    const ID: &str = "S-05";
    const NAME: &str = "市場給与中央値が県平均給与を下回る";
    if input.is_hourly {
        return Signal::not_evaluable(
            ID,
            NAME,
            "時給中心のCSVのため月給ベースの県平均との比較は行いません (単位が異なる)",
        );
    }
    match (input.salary_median, input.scheduled_earnings_latest) {
        (Some(median), Some(pref_wage)) if pref_wage > 0.0 => {
            let ratio = median as f64 / pref_wage;
            let eid = store.add(
                EvidenceKind::Aggregated,
                "給与中央値の県平均比",
                &format!("{:.2}", ratio),
                "倍",
                "今回の求人CSV集計 / 毎月勤労統計 地方調査",
                granularity::PREFECTURE,
                Some(input.salary_n),
                Some(input.as_of.clone()),
                "県平均は全産業の所定内給与。職種構成の差に留意",
            );
            Signal {
                id: ID.to_string(),
                name: NAME.to_string(),
                fired: ratio < config::SALARY_BELOW_PREF_RATIO,
                evidence_ids: vec![eid],
                interpretation: "この職種近傍の市場給与は県平均より低めで、給与以外の条件が比較軸になりやすい可能性があります。".to_string(),
                alternative_explanations: vec![
                    "県平均が全産業ベースのため、職種構成の違いで差が出ている可能性".to_string(),
                ],
                data_note: String::new(),
            }
        }
        _ => Signal::not_evaluable(ID, NAME, "給与中央値または県平均給与データが不足しています"),
    }
}

/// S-06 従業員減×募集継続 (企業データベースで名寄せできた企業のみ)
fn s06_employee_decline_with_postings(input: &ConsultInput, store: &mut EvidenceStore) -> Signal {
    const ID: &str = "S-06";
    const NAME: &str = "人員減少中でも募集を継続する企業の存在";
    let matched: Vec<&super::input::CompanyObservation> = input
        .companies
        .iter()
        .filter(|c| c.employee_delta_1y.is_some())
        .collect();
    if matched.is_empty() {
        return Signal::not_evaluable(
            ID,
            NAME,
            "企業データベースと名寄せできた掲載企業がないため判定できません",
        );
    }
    let hits: Vec<&&super::input::CompanyObservation> = matched
        .iter()
        .filter(|c| {
            c.employee_delta_1y.unwrap_or(0.0) < config::EMPLOYEE_DECLINE_THRESHOLD_PCT
                && c.posting_count >= config::CONTINUED_POSTING_MIN_COUNT
        })
        .collect();
    let mut evidence_ids = Vec::new();
    for c in &hits {
        let eid = store.add(
            EvidenceKind::Observed,
            &format!("{} の1年人員増減率と掲載件数", c.name),
            &format!(
                "{:+.1}% / {}件",
                c.employee_delta_1y.unwrap_or(0.0),
                c.posting_count
            ),
            "",
            "企業データベース + 今回の求人CSV集計",
            granularity::COMPANY,
            None,
            Some(input.as_of.clone()),
            "人員推移は企業データベースの参照時点に依存する参考値",
        );
        evidence_ids.push(eid);
    }
    if evidence_ids.is_empty() {
        // 名寄せはできたが該当なし: 判定に使った母数を証拠化
        let eid = store.add(
            EvidenceKind::Aggregated,
            "名寄せできた掲載企業数",
            &format!("{}", matched.len()),
            "社",
            "企業データベース + 今回の求人CSV集計",
            granularity::COMPANY,
            Some(matched.len()),
            Some(input.as_of.clone()),
            "",
        );
        evidence_ids.push(eid);
    }
    Signal {
        id: ID.to_string(),
        name: NAME.to_string(),
        fired: !hits.is_empty(),
        evidence_ids,
        interpretation: "人員が減少しながら募集が続く企業があり、この市場では欠員補充型の採用が発生している可能性があります。".to_string(),
        alternative_explanations: vec![
            "合併・分社等の組織改編で人員数が変動している可能性 (M&A等)".to_string(),
            "拠点間の人員移動やデータ更新時差による見かけ上の減少の可能性".to_string(),
        ],
        data_note: String::new(),
    }
}

/// S-07 働き手減少地域
fn s07_workforce_declining_region(input: &ConsultInput, store: &mut EvidenceStore) -> Signal {
    const ID: &str = "S-07";
    const NAME: &str = "働き手人口が減少する見込みの地域";
    match input.wa_decline_rate_muni {
        Some(rate) => {
            let eid = store.add(
                EvidenceKind::Observed,
                "働き手人口の将来増減率",
                &format!("{:+.1}", rate),
                "%",
                "国立社会保障・人口問題研究所 将来人口推計",
                granularity::MUNICIPALITY,
                None,
                None,
                "生産年齢人口の将来推計 (負値=減少)",
            );
            Signal {
                id: ID.to_string(),
                name: NAME.to_string(),
                fired: rate <= config::WORKFORCE_DECLINE_THRESHOLD_PCT,
                evidence_ids: vec![eid],
                interpretation: "対象地域の働き手人口は将来推計で大きく減少する見込みで、母集団形成が構造的に難しくなる可能性があります。".to_string(),
                alternative_explanations: vec![
                    "推計は現在の傾向の延長であり、大規模開発等で変わる可能性".to_string(),
                ],
                data_note: String::new(),
            }
        }
        None => Signal::not_evaluable(ID, NAME, "対象市区町村の将来人口推計データが不足しています"),
    }
}

/// S-08 転職希望層が薄い
fn s08_switcher_supply_thin(input: &ConsultInput, store: &mut EvidenceStore) -> Signal {
    const ID: &str = "S-08";
    const NAME: &str = "転職希望層が全国比で薄い";
    switcher_signal(input, store, ID, NAME, true)
}

/// S-09 転職希望層が厚い
fn s09_switcher_supply_thick(input: &ConsultInput, store: &mut EvidenceStore) -> Signal {
    const ID: &str = "S-09";
    const NAME: &str = "転職希望層が全国比で厚い";
    switcher_signal(input, store, ID, NAME, false)
}

fn switcher_signal(
    input: &ConsultInput,
    store: &mut EvidenceStore,
    id: &str,
    name: &str,
    thin: bool,
) -> Signal {
    match (
        input.job_change_desire_rate_pref,
        input.job_change_desire_rate_national,
    ) {
        (Some(pref_rate), Some(nat_rate)) if nat_rate > 0.0 => {
            let ratio = pref_rate / nat_rate;
            let eid = store.add(
                EvidenceKind::Observed,
                "転職希望率 (県/全国)",
                &format!("{:.1} / {:.1}", pref_rate, nat_rate),
                "%",
                "就業構造基本調査",
                granularity::PREFECTURE,
                None,
                None,
                "全職種計の値。対象職種の転職希望層とは差がある可能性",
            );
            let fired = if thin {
                ratio < config::SWITCHER_THIN_RATIO
            } else {
                ratio > config::SWITCHER_THICK_RATIO
            };
            Signal {
                id: id.to_string(),
                name: name.to_string(),
                fired,
                evidence_ids: vec![eid],
                interpretation: if thin {
                    "転職を考えている層が全国比で薄く、転職顕在層だけを狙う設計では母集団が不足する可能性があります。".to_string()
                } else {
                    "転職を考えている層が全国比で厚く、適切な露出があれば母集団を作りやすい可能性があります。".to_string()
                },
                alternative_explanations: vec![
                    "県単位の値であり、対象市区町村では傾向が異なる可能性".to_string(),
                ],
                data_note: String::new(),
            }
        }
        _ => Signal::not_evaluable(id, name, "転職希望率データが不足しています"),
    }
}

/// S-10 有効求人倍率が高い
fn s10_job_ratio_high(input: &ConsultInput, store: &mut EvidenceStore) -> Signal {
    const ID: &str = "S-10";
    const NAME: &str = "有効求人倍率が高い (売り手市場寄り)";
    job_ratio_signal(input, store, ID, NAME, true)
}

/// S-11 有効求人倍率が低い
fn s11_job_ratio_low(input: &ConsultInput, store: &mut EvidenceStore) -> Signal {
    const ID: &str = "S-11";
    const NAME: &str = "有効求人倍率が低い (市場は比較的緩やか)";
    job_ratio_signal(input, store, ID, NAME, false)
}

fn job_ratio_signal(
    input: &ConsultInput,
    store: &mut EvidenceStore,
    id: &str,
    name: &str,
    high: bool,
) -> Signal {
    match input.job_openings_ratio {
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
                "県粒度・全職種計の可能性がある点に留意",
            );
            let fired = if high {
                ratio >= config::JOB_RATIO_HIGH
            } else {
                ratio < config::JOB_RATIO_LOW
            };
            Signal {
                id: id.to_string(),
                name: name.to_string(),
                fired,
                evidence_ids: vec![eid],
                interpretation: if high {
                    "求職者1人に対する求人が多く、他社と条件・対応スピードで比較される場面が増える可能性があります。".to_string()
                } else {
                    "求人倍率は低めで、市場環境そのものよりも訴求・運用側の要因が採用結果を左右しやすい可能性があります。".to_string()
                },
                alternative_explanations: vec![
                    "職種別の倍率は全職種計と大きく異なる場合がある".to_string()
                ],
                data_note: String::new(),
            }
        }
        None => Signal::not_evaluable(id, name, "有効求人倍率データが不足しています"),
    }
}

/// S-12 通勤流入が多い
fn s12_commute_inflow(input: &ConsultInput, store: &mut EvidenceStore) -> Signal {
    const ID: &str = "S-12";
    const NAME: &str = "周辺地域からの通勤流入が多い";
    match input.commute_inflow_total {
        Some(inflow) if inflow > 0 => {
            let mut evidence_ids = Vec::new();
            let eid = store.add(
                EvidenceKind::Observed,
                "通勤流入合計",
                &format!("{}", inflow),
                "人",
                "国勢調査 通勤・通学OD",
                granularity::MUNICIPALITY,
                None,
                None,
                "対象市区町村へ他市区町村から通勤する人数",
            );
            evidence_ids.push(eid);
            let ratio_fired = match input.commute_outflow_total {
                Some(outflow) if outflow > 0 => {
                    inflow as f64 > outflow as f64 * config::COMMUTE_INFLOW_OUTFLOW_RATIO
                }
                _ => false,
            };
            let top3_text = input
                .commute_inflow_top3
                .iter()
                .map(|(p, m, n)| format!("{}{} {}人", p, m, n))
                .collect::<Vec<_>>()
                .join("、");
            if !top3_text.is_empty() {
                let eid2 = store.add(
                    EvidenceKind::Observed,
                    "通勤流入元上位",
                    &top3_text,
                    "",
                    "国勢調査 通勤・通学OD",
                    granularity::MUNICIPALITY,
                    None,
                    None,
                    "",
                );
                evidence_ids.push(eid2);
            }
            Signal {
                id: ID.to_string(),
                name: NAME.to_string(),
                fired: inflow >= config::COMMUTE_INFLOW_MIN || ratio_fired,
                evidence_ids,
                interpretation: "周辺地域から通勤で流入する働き手が多く、募集の配信地域を通勤圏まで広げる余地がある可能性があります。".to_string(),
                alternative_explanations: vec![
                    "流入は全職種の通勤であり、対象職種の通勤圏とは異なる可能性".to_string(),
                ],
                data_note: String::new(),
            }
        }
        _ => Signal::not_evaluable(
            ID,
            NAME,
            "通勤ODデータが不足しています (市区町村が特定できない場合も含む)",
        ),
    }
}

/// S-13 新着比率が高い
fn s13_new_posting_ratio_high(input: &ConsultInput, store: &mut EvidenceStore) -> Signal {
    const ID: &str = "S-13";
    const NAME: &str = "新着求人の比率が高い";
    if input.total_postings == 0 {
        return Signal::not_evaluable(ID, NAME, "今回CSVに求人が含まれていません");
    }
    let ratio = input.new_count as f64 / input.total_postings as f64;
    let eid = store.add(
        EvidenceKind::Aggregated,
        "新着求人比率",
        &format!("{:.0}", ratio * 100.0),
        "%",
        "今回の求人CSV集計",
        granularity::CSV,
        Some(input.total_postings),
        Some(input.as_of.clone()),
        "媒体上の新着表示に基づく",
    );
    Signal {
        id: ID.to_string(),
        name: NAME.to_string(),
        fired: ratio >= config::NEW_RATIO_HIGH,
        evidence_ids: vec![eid],
        interpretation: "新規の求人投入が活発で、市場の動きが速い可能性があります。掲載後の初動対応が比較されやすい環境です。".to_string(),
        alternative_explanations: vec![
            "媒体の新着表示ルール (再掲載も新着扱い等) の影響を受ける可能性".to_string(),
        ],
        data_note: String::new(),
    }
}

/// S-14 サンプル不足 (データ品質シグナル。§19: サンプル数が閾値未満なのに強い表現をしない)
fn s14_sample_insufficient(input: &ConsultInput, store: &mut EvidenceStore) -> Signal {
    const ID: &str = "S-14";
    const NAME: &str = "今回CSVのサンプル数が少ない";
    let eid = store.add(
        EvidenceKind::Aggregated,
        "今回CSVの求人件数",
        &format!("{}", input.total_postings),
        "件",
        "今回の求人CSV集計",
        granularity::CSV,
        Some(input.total_postings),
        Some(input.as_of.clone()),
        "",
    );
    Signal {
        id: ID.to_string(),
        name: NAME.to_string(),
        fired: input.total_postings < config::MIN_SAMPLE_POSTINGS,
        evidence_ids: vec![eid],
        interpretation: format!(
            "今回CSVの件数が{}件と少なく、市場全体を代表しない可能性があります。本ブリーフの給与・競争の判定は参考程度に扱ってください。",
            input.total_postings
        ),
        alternative_explanations: vec![
            "取得条件 (検索語・地域指定) が狭かった可能性".to_string(),
        ],
        data_note: String::new(),
    }
}

/// S-15 特定企業への求人集中
fn s15_posting_concentration(input: &ConsultInput, store: &mut EvidenceStore) -> Signal {
    const ID: &str = "S-15";
    const NAME: &str = "特定企業への求人集中";
    if input.total_postings == 0 || input.companies.is_empty() {
        return Signal::not_evaluable(ID, NAME, "企業別の掲載件数データが不足しています");
    }
    let top = input
        .companies
        .iter()
        .max_by_key(|c| c.posting_count)
        .unwrap();
    let share = top.posting_count as f64 / input.total_postings as f64;
    let eid = store.add(
        EvidenceKind::Aggregated,
        "最多掲載企業のシェア",
        &format!("{:.0}", share * 100.0),
        "%",
        "今回の求人CSV集計",
        granularity::CSV,
        Some(input.total_postings),
        Some(input.as_of.clone()),
        &format!("最多掲載: {} ({}件)", top.name, top.posting_count),
    );
    Signal {
        id: ID.to_string(),
        name: NAME.to_string(),
        fired: share >= config::TOP_COMPANY_SHARE_THRESHOLD,
        evidence_ids: vec![eid],
        interpretation: "特定企業が募集の多くを占めており、その企業の採用動向が地域の需給に影響している可能性があります。".to_string(),
        alternative_explanations: vec![
            "同一企業の複数拠点・複数職種の掲載が集計上まとまっている可能性".to_string(),
        ],
        data_note: String::new(),
    }
}

/// S-16 転出超過: 純移動率が閾値以下 (負値=転出超過)
fn s16_net_migration_outflow(input: &ConsultInput, store: &mut EvidenceStore) -> Signal {
    const ID: &str = "S-16";
    const NAME: &str = "転入より転出が多い (転出超過)";
    match input.net_migration_rate {
        Some(rate) => {
            let eid = store.add(
                EvidenceKind::Observed,
                "純移動率",
                &format!("{:+.1}", rate),
                "‰",
                "住民基本台帳人口移動報告",
                granularity::MUNICIPALITY,
                None,
                None,
                "純移動率=(転入-転出)/人口。負値=転出超過",
            );
            Signal {
                id: ID.to_string(),
                name: NAME.to_string(),
                fired: rate <= config::NET_MIGRATION_OUTFLOW_THRESHOLD_PERMILLE,
                evidence_ids: vec![eid],
                interpretation: "地域全体で転出が転入を上回っており、若年・現役層の流出が母集団形成の逆風になっている可能性があります。".to_string(),
                alternative_explanations: vec![
                    "移動は全年齢の合計であり、対象職種の労働層とは動きが異なる可能性".to_string(),
                    "大学進学・施設移転等の一時要因で単年の移動が振れている可能性".to_string(),
                ],
                data_note: String::new(),
            }
        }
        None => Signal::not_evaluable(
            ID,
            NAME,
            "人口移動データが不足しています (市区町村が特定できない場合を含む)",
        ),
    }
}

/// S-17 昼間人口流出型: 昼夜間人口比率 < 閾値
fn s17_daytime_outflow(input: &ConsultInput, store: &mut EvidenceStore) -> Signal {
    const ID: &str = "S-17";
    const NAME: &str = "昼間人口が流出する地域 (ベッドタウン型)";
    match input.daytime_ratio {
        Some(ratio) => {
            let eid = store.add(
                EvidenceKind::Observed,
                "昼夜間人口比率",
                &format!("{:.1}", ratio),
                "%",
                "国勢調査 従業地・通学地集計",
                granularity::MUNICIPALITY,
                None,
                None,
                "100未満=昼間に人口が周辺へ流出 (通勤・通学)",
            );
            Signal {
                id: ID.to_string(),
                name: NAME.to_string(),
                fired: ratio < config::DAYTIME_RATIO_OUTFLOW_THRESHOLD,
                evidence_ids: vec![eid],
                interpretation: "昼間に働き手が周辺地域へ流出する居住地型の地域で、居住者を勤務地近くで採る前提が崩れやすく、通勤・勤務地条件の訴求が効きやすい可能性があります。".to_string(),
                alternative_explanations: vec![
                    "昼夜間比率は全産業の通勤であり、対象職種の勤務地選好とは異なる可能性".to_string(),
                ],
                data_note: String::new(),
            }
        }
        None => Signal::not_evaluable(ID, NAME, "昼夜間人口比率データが不足しています"),
    }
}

/// S-18 廃業率が開業率を上回る。
/// 🔴 元値は経済センサス調査間の累計。証拠には累計値 (調査間隔つき) を明示し、
///    判定は年換算値どうしの比較で行う (調査間隔が取れなければ判定不能)。
fn s18_closure_over_opening(input: &ConsultInput, store: &mut EvidenceStore) -> Signal {
    const ID: &str = "S-18";
    const NAME: &str = "廃業率が開業率を上回る地域";
    match (input.business_opening_rate, input.business_closure_rate) {
        (Some(open), Some(close)) => {
            let years_note = match input.business_dynamics_interval_years {
                Some(y) => format!("経済センサス調査間 累計 (調査間隔 約{:.0}年)", y),
                None => "経済センサス調査間 累計 (調査間隔 不明)".to_string(),
            };
            let eid = store.add(
                EvidenceKind::Observed,
                "開業率 / 廃業率 (経済センサス調査間 累計)",
                &format!("{:.1} / {:.1}", open, close),
                "%",
                "経済センサス 開廃業",
                granularity::PREFECTURE,
                None,
                None,
                &format!("事業所ベース。{}。年率比較には年換算が必要", years_note),
            );
            match (
                input.annualized_opening_rate(),
                input.annualized_closure_rate(),
            ) {
                (Some(open_a), Some(close_a)) => Signal {
                    id: ID.to_string(),
                    name: NAME.to_string(),
                    fired: close_a - open_a > config::CLOSURE_OVER_OPENING_MARGIN_PCT,
                    evidence_ids: vec![eid],
                    interpretation: format!(
                        "年換算の廃業率 (約{:.1}%) が開業率 (約{:.1}%) を上回っており、事業所の受け皿が縮小方向にある可能性があります。廃業に伴う人材の再就職ニーズが顕在化している可能性もあります。",
                        close_a, open_a
                    ),
                    alternative_explanations: vec![
                        "開廃業率は全産業ベースで、対象職種の業種動向とは異なる可能性".to_string(),
                        "調査間隔での累計を年換算した参考値であり、単年の増減とは異なる可能性".to_string(),
                    ],
                    data_note: String::new(),
                },
                _ => Signal {
                    id: ID.to_string(),
                    name: NAME.to_string(),
                    fired: false,
                    evidence_ids: vec![eid],
                    interpretation: String::new(),
                    alternative_explanations: vec![],
                    data_note: "開廃業率の調査間隔が特定できず、年換算での比較ができないため判定できません".to_string(),
                },
            }
        }
        _ => Signal::not_evaluable(ID, NAME, "開廃業データが不足しています"),
    }
}

/// S-19 開業が活発。
/// 🔴 判定は年換算した開業率 (累計 ÷ 調査間年数) に対して閾値を適用する。
fn s19_opening_active(input: &ConsultInput, store: &mut EvidenceStore) -> Signal {
    const ID: &str = "S-19";
    const NAME: &str = "開業が活発な地域";
    match input.business_opening_rate {
        Some(open) => {
            let years_note = match input.business_dynamics_interval_years {
                Some(y) => format!("経済センサス調査間 累計 (調査間隔 約{:.0}年)", y),
                None => "経済センサス調査間 累計 (調査間隔 不明)".to_string(),
            };
            let eid = store.add(
                EvidenceKind::Observed,
                "開業率 (経済センサス調査間 累計)",
                &format!("{:.1}", open),
                "%",
                "経済センサス 開廃業",
                granularity::PREFECTURE,
                None,
                None,
                &format!("事業所ベース。{}。判定は年換算値で行う", years_note),
            );
            match input.annualized_opening_rate() {
                Some(open_a) => Signal {
                    id: ID.to_string(),
                    name: NAME.to_string(),
                    fired: open_a >= config::OPENING_RATE_ACTIVE_THRESHOLD,
                    evidence_ids: vec![eid],
                    interpretation: format!(
                        "年換算の開業率が約{:.1}%と高めで、新規事業所の立ち上げが活発なため人材の採り合いが起きやすい環境の可能性があります。",
                        open_a
                    ),
                    alternative_explanations: vec![
                        "開業率は全産業ベースで、対象職種の採用競合とは異なる可能性".to_string(),
                        "調査間隔での累計を年換算した参考値である点に留意".to_string(),
                    ],
                    data_note: String::new(),
                },
                None => Signal {
                    id: ID.to_string(),
                    name: NAME.to_string(),
                    fired: false,
                    evidence_ids: vec![eid],
                    interpretation: String::new(),
                    alternative_explanations: vec![],
                    data_note: "開業率の調査間隔が特定できず年換算できないため判定できません".to_string(),
                },
            }
        }
        None => Signal::not_evaluable(ID, NAME, "開業率データが不足しています"),
    }
}

fn unemployment_signal(
    input: &ConsultInput,
    store: &mut EvidenceStore,
    id: &str,
    name: &str,
    tight: bool,
) -> Signal {
    match (
        input.unemployment_rate_pref,
        input.unemployment_rate_national,
    ) {
        (Some(pref), Some(nat)) if nat > 0.0 => {
            let ratio = pref / nat;
            let eid = store.add(
                EvidenceKind::Observed,
                "失業率 (県/全国)",
                &format!("{:.1} / {:.1}", pref, nat),
                "%",
                "国勢調査 労働力状態",
                granularity::PREFECTURE,
                None,
                None,
                "全産業計の失業率。対象職種の需給とは差がある可能性",
            );
            let fired = if tight {
                ratio < config::UNEMPLOYMENT_TIGHT_RATIO
            } else {
                ratio > config::UNEMPLOYMENT_SLACK_RATIO
            };
            Signal {
                id: id.to_string(),
                name: name.to_string(),
                fired,
                evidence_ids: vec![eid],
                interpretation: if tight {
                    "失業率が全国比で低く、働き手に余裕が少ない労働需給の締まった地域の可能性があります。".to_string()
                } else {
                    "失業率が全国比でやや高く、求職側に余裕がある可能性があります。露出と条件を整えれば母集団を作りやすい可能性があります。".to_string()
                },
                alternative_explanations: vec![
                    "失業率は全産業計であり、対象職種の需給とは異なる可能性".to_string(),
                ],
                data_note: String::new(),
            }
        }
        _ => Signal::not_evaluable(id, name, "失業率データ (県/全国) が不足しています"),
    }
}

/// S-20 失業率が全国比で低い (需給が締まっている)
fn s20_unemployment_tight(input: &ConsultInput, store: &mut EvidenceStore) -> Signal {
    unemployment_signal(
        input,
        store,
        "S-20",
        "失業率が全国比で低い (需給が締まる)",
        true,
    )
}

/// S-21 失業率が全国比で高い (余剰寄り)
fn s21_unemployment_slack(input: &ConsultInput, store: &mut EvidenceStore) -> Signal {
    unemployment_signal(
        input,
        store,
        "S-21",
        "失業率が全国比で高い (余剰寄り)",
        false,
    )
}

/// S-22 1畳あたり家賃が全国比で高い (居住コストの相対位置)。
/// 🔴 P0-2: median_rent_jpy の実体は「1畳あたり家賃」であり月額家賃ではない。
///    月額家賃としての給与バランス判定は廃止し (勝手な畳数仮定で月額を捏造しない)、
///    全国中央値に対する相対位置のみを扱う。全国基準が取れなければ判定材料不足に降格する。
fn s22_rent_burden(input: &ConsultInput, store: &mut EvidenceStore) -> Signal {
    const ID: &str = "S-22";
    const NAME: &str = "1畳あたり家賃が全国比で高い (居住コスト)";
    match (input.rent_per_tatami, input.rent_relative_to_national()) {
        (Some(rent), Some(ratio)) => {
            let eid = store.add(
                EvidenceKind::Observed,
                "1畳あたり家賃 (全国中央値比)",
                &format!("{}円 (全国比 {:.2}倍)", rent, ratio),
                "",
                "住宅・土地統計",
                granularity::PREFECTURE,
                None,
                Some(input.as_of.clone()),
                "1畳あたり家賃 (総数)。月額家賃ではない。全国中央値との相対位置として扱う",
            );
            // 全国比 1.1倍超を「相対的に高い」とみなす (居住コストの論点提示)
            Signal {
                id: ID.to_string(),
                name: NAME.to_string(),
                fired: ratio > 1.1,
                evidence_ids: vec![eid],
                interpretation: "居住コスト (1畳あたり家賃) が全国と比べて相対的に高く、遠方からの転居を伴う採用ではハードルになり得る可能性があります (面談で採用対象が転居前提か通勤前提かを確認)。".to_string(),
                alternative_explanations: vec![
                    "家賃は県代表値で、勤務地周辺の実勢とは差がある可能性".to_string(),
                    "持ち家率が高い地域では家賃負担の影響が小さい可能性".to_string(),
                ],
                data_note: String::new(),
            }
        }
        (Some(rent), None) => {
            // 全国基準が無いときは相対位置を判定できない。値だけは証拠化 (月額換算はしない)。
            let eid = store.add(
                EvidenceKind::Observed,
                "1畳あたり家賃",
                &format!("{}", rent),
                "円",
                "住宅・土地統計",
                granularity::PREFECTURE,
                None,
                Some(input.as_of.clone()),
                "1畳あたり家賃 (総数)。月額家賃ではない。全国中央値が取得できず相対位置は判定不能",
            );
            Signal {
                id: ID.to_string(),
                name: NAME.to_string(),
                fired: false,
                evidence_ids: vec![eid],
                interpretation: String::new(),
                alternative_explanations: vec![],
                data_note: "全国中央値が取得できず、家賃の相対位置を判定できません (材料不足)"
                    .to_string(),
            }
        }
        _ => Signal::not_evaluable(ID, NAME, "家賃データが不足しています"),
    }
}

/// S-23 自然減 (出生 < 死亡)
fn s23_natural_decline(input: &ConsultInput, store: &mut EvidenceStore) -> Signal {
    const ID: &str = "S-23";
    const NAME: &str = "人口が自然減の地域";
    match input.natural_change {
        Some(change) => {
            let eid = store.add(
                EvidenceKind::Observed,
                "自然増減 (出生-死亡)",
                &format!("{:+}", change),
                "人",
                "人口動態統計",
                granularity::MUNICIPALITY,
                None,
                None,
                "負値=死亡が出生を上回る自然減",
            );
            Signal {
                id: ID.to_string(),
                name: NAME.to_string(),
                fired: change < 0,
                evidence_ids: vec![eid],
                interpretation: "出生より死亡が多い自然減の地域で、地元の若年供給が細っていく構造的な背景がある可能性があります。".to_string(),
                alternative_explanations: vec![
                    "自然減は全年齢の動態であり、転入で補われている可能性 (S-16と併読)".to_string(),
                ],
                data_note: String::new(),
            }
        }
        None => Signal::not_evaluable(ID, NAME, "人口動態データが不足しています"),
    }
}

/// S-24 年間休日の記載/訴求が薄い (§5.0: 記載検出。欠落は否定情報ではない)
fn s24_holiday_mention_thin(input: &ConsultInput, store: &mut EvidenceStore) -> Signal {
    const ID: &str = "S-24";
    const NAME: &str = "年間休日の記載・訴求が薄い";
    match input.holiday_mention_ratio() {
        Some(ratio) => {
            let eid = store.add(
                EvidenceKind::Aggregated,
                "年間休日を記載/抽出できた求人比率",
                &format!("{:.0}", ratio * 100.0),
                "%",
                "今回の求人CSV集計",
                granularity::CSV,
                Some(input.total_postings),
                Some(input.as_of.clone()),
                "記載/抽出できた比率。記載がない=休日がない ではない (§5.0)",
            );
            Signal {
                id: ID.to_string(),
                name: NAME.to_string(),
                fired: ratio < config::HOLIDAY_MENTION_THIN_RATIO,
                evidence_ids: vec![eid],
                interpretation: "求人上で年間休日を明示している求人が少なく、休日を条件比較の判断材料にできていない市場の可能性があります。休日条件の明示が差別化になり得ます。".to_string(),
                alternative_explanations: vec![
                    "求人カード/抜粋に休日が載らないだけで、求人票本体には記載がある可能性 (§5.0)".to_string(),
                ],
                data_note: String::new(),
            }
        }
        None => Signal::not_evaluable(ID, NAME, "今回CSVに求人が含まれていません"),
    }
}

/// S-25 年間休日120日以上の求人が少ない
fn s25_holiday_level_low(input: &ConsultInput, store: &mut EvidenceStore) -> Signal {
    const ID: &str = "S-25";
    const NAME: &str = "年間休日120日以上の求人が少ない";
    match input.holiday_pct_ge_120 {
        Some(ratio) if input.annual_holidays_n > 0 => {
            let eid = store.add(
                EvidenceKind::Aggregated,
                "年間休日120日以上の求人比率",
                &format!("{:.0}", ratio * 100.0),
                "%",
                "今回の求人CSV集計",
                granularity::CSV,
                Some(input.annual_holidays_n),
                Some(input.as_of.clone()),
                "年間休日を記載/抽出できた求人が母数",
            );
            Signal {
                id: ID.to_string(),
                name: NAME.to_string(),
                fired: ratio < config::HOLIDAY_GE120_LOW_RATIO,
                evidence_ids: vec![eid],
                interpretation: "年間休日120日以上を掲げる求人が少ない市場で、休日日数が求職者の比較軸として弱い可能性があります。自社が休日面で上回れば訴求余地がある可能性があります。".to_string(),
                alternative_explanations: vec![
                    "記載/抽出できた求人のみを母数とするため、実勢とはずれる可能性".to_string(),
                ],
                data_note: String::new(),
            }
        }
        _ => Signal::not_evaluable(ID, NAME, "年間休日データが不足しています"),
    }
}

/// S-26 訴求タグの種類が少ない
fn s26_tag_variety_thin(input: &ConsultInput, store: &mut EvidenceStore) -> Signal {
    const ID: &str = "S-26";
    const NAME: &str = "求人カードの訴求タグの種類が少ない";
    if input.total_postings == 0 {
        return Signal::not_evaluable(ID, NAME, "今回CSVに求人が含まれていません");
    }
    let top = input
        .top_tags
        .iter()
        .take(5)
        .map(|(t, n)| format!("{}({})", t, n))
        .collect::<Vec<_>>()
        .join("、");
    let eid = store.add(
        EvidenceKind::Aggregated,
        "観測できた求人カードタグの種類数",
        &format!("{}", input.distinct_tag_count),
        "種類",
        "今回の求人CSV集計",
        granularity::CSV,
        Some(input.total_postings),
        Some(input.as_of.clone()),
        &if top.is_empty() {
            "求人カード上に表示されたタグのみ (福利厚生の全体ではない §5.0)".to_string()
        } else {
            format!("上位タグ: {} / 求人カード表示分のみ (§5.0)", top)
        },
    );
    Signal {
        id: ID.to_string(),
        name: NAME.to_string(),
        fired: input.distinct_tag_count < config::TAG_VARIETY_THIN_THRESHOLD,
        evidence_ids: vec![eid],
        interpretation: "求人カード上で観測できる訴求タグの種類が少なく、条件面の見せ方が横並びになりやすい市場の可能性があります。タグの付け方で差をつける余地がある可能性があります。".to_string(),
        alternative_explanations: vec![
            "タグは媒体がカード上に表示した一部で、実際の条件はより多い可能性 (§5.0)".to_string(),
        ],
        data_note: String::new(),
    }
}

/// S-27 人気バッジ求人の集中
fn s27_popular_badge_concentration(input: &ConsultInput, store: &mut EvidenceStore) -> Signal {
    const ID: &str = "S-27";
    const NAME: &str = "人気表示のある求人の比率が高い";
    match input.popular_ratio {
        Some(ratio) => {
            let eid = store.add(
                EvidenceKind::Aggregated,
                "人気/超人気バッジのある求人比率",
                &format!("{:.0}", ratio * 100.0),
                "%",
                "今回の求人CSV集計",
                granularity::CSV,
                Some(input.total_postings),
                Some(input.as_of.clone()),
                &format!(
                    "うち超人気バッジ {}件。媒体のバッジ表示に基づく",
                    input.super_popular_count
                ),
            );
            Signal {
                id: ID.to_string(),
                name: NAME.to_string(),
                fired: ratio >= config::POPULAR_BADGE_HIGH_RATIO,
                evidence_ids: vec![eid],
                interpretation: "人気表示のある求人が多く、応募が一部の目立つ求人に集まりやすい市場の可能性があります。露出とカード上の見え方が応募獲得を左右しやすい可能性があります。".to_string(),
                alternative_explanations: vec![
                    "バッジ付与ルールは媒体側の基準で、実際の応募数とは一致しない可能性".to_string(),
                ],
                data_note: String::new(),
            }
        }
        None => Signal::not_evaluable(ID, NAME, "人気バッジデータが不足しています"),
    }
}

/// S-28 通勤流入上位を配信圏に含むか要確認 (§10.1 配信地域の論点。要確認形式)
fn s28_commute_delivery_check(input: &ConsultInput, store: &mut EvidenceStore) -> Signal {
    const ID: &str = "S-28";
    const NAME: &str = "通勤流入上位地域を配信圏に含むか要確認";
    if input.commute_inflow_top3.is_empty() {
        return Signal::not_evaluable(ID, NAME, "通勤流入元の内訳データが不足しています");
    }
    let top3_text = input
        .commute_inflow_top3
        .iter()
        .map(|(p, m, n)| format!("{}{} {}人", p, m, n))
        .collect::<Vec<_>>()
        .join("、");
    let eid = store.add(
        EvidenceKind::Observed,
        "通勤流入元 上位3市区町村",
        &top3_text,
        "",
        "国勢調査 通勤・通学OD",
        granularity::MUNICIPALITY,
        None,
        None,
        "これらの市区町村を求人の配信対象に含めているかは面談で要確認",
    );
    Signal {
        id: ID.to_string(),
        name: NAME.to_string(),
        // 「発火=論点として提示」。上位流入元があれば必ず配信圏の確認論点として立てる
        fired: true,
        evidence_ids: vec![eid],
        interpretation: "通勤で流入する働き手の多い市区町村があり、これらを求人の配信対象に含められているかで母集団の広さが変わる可能性があります (配信設定は要確認)。".to_string(),
        alternative_explanations: vec![
            "流入は全職種の通勤であり、対象職種の通勤実態とは異なる可能性".to_string(),
        ],
        data_note: String::new(),
    }
}

/// S-29 成長企業の存在 (企業データベースで名寄せできた企業のみ)
fn s29_growing_companies(input: &ConsultInput, store: &mut EvidenceStore) -> Signal {
    const ID: &str = "S-29";
    const NAME: &str = "人員を増やしながら募集する企業の存在";
    let matched: Vec<&super::input::CompanyObservation> = input
        .companies
        .iter()
        .filter(|c| c.employee_delta_1y.is_some())
        .collect();
    if matched.is_empty() {
        return Signal::not_evaluable(
            ID,
            NAME,
            "企業データベースと名寄せできた掲載企業がないため判定できません",
        );
    }
    let hits: Vec<&&super::input::CompanyObservation> = matched
        .iter()
        .filter(|c| {
            c.employee_delta_1y.unwrap_or(0.0) >= config::EMPLOYEE_GROWTH_THRESHOLD_PCT
                && c.posting_count >= config::CONTINUED_POSTING_MIN_COUNT
        })
        .collect();
    let mut evidence_ids = Vec::new();
    for c in &hits {
        let eid = store.add(
            EvidenceKind::Observed,
            &format!("{} の1年人員増減率と掲載件数", c.name),
            &format!(
                "{:+.1}% / {}件",
                c.employee_delta_1y.unwrap_or(0.0),
                c.posting_count
            ),
            "",
            "企業データベース + 今回の求人CSV集計",
            granularity::COMPANY,
            None,
            Some(input.as_of.clone()),
            "人員推移は企業データベースの参照時点に依存する参考値",
        );
        evidence_ids.push(eid);
    }
    if evidence_ids.is_empty() {
        let eid = store.add(
            EvidenceKind::Aggregated,
            "名寄せできた掲載企業数",
            &format!("{}", matched.len()),
            "社",
            "企業データベース + 今回の求人CSV集計",
            granularity::COMPANY,
            Some(matched.len()),
            Some(input.as_of.clone()),
            "",
        );
        evidence_ids.push(eid);
    }
    Signal {
        id: ID.to_string(),
        name: NAME.to_string(),
        fired: !hits.is_empty(),
        evidence_ids,
        interpretation: "人員を増やしながら募集を続ける拡大採用型の企業があり、地域の人材を積極的に採る競合として比較対象になっている可能性があります。".to_string(),
        alternative_explanations: vec![
            "人員増は合併・拠点統合等の組織要因の可能性 (M&A等)".to_string(),
            "人員推移データの参照時点と現在で状況が変わっている可能性".to_string(),
        ],
        data_note: String::new(),
    }
}

/// S-30 非正規求人の比率が高い
fn s30_nonregular_share_high(input: &ConsultInput, store: &mut EvidenceStore) -> Signal {
    const ID: &str = "S-30";
    const NAME: &str = "正社員以外 (パート等) の求人比率が高い";
    match input.nonregular_share() {
        Some(share) => {
            let dist = input
                .employment_type_dist
                .iter()
                .take(4)
                .map(|(t, n)| format!("{}({})", t, n))
                .collect::<Vec<_>>()
                .join("、");
            let eid = store.add(
                EvidenceKind::Aggregated,
                "正社員/正職員以外の求人比率",
                &format!("{:.0}", share * 100.0),
                "%",
                "今回の求人CSV集計",
                granularity::CSV,
                Some(input.total_postings),
                Some(input.as_of.clone()),
                &format!("雇用形態内訳: {} / 雇用形態列のあるCSVのみ (§5.0)", dist),
            );
            Signal {
                id: ID.to_string(),
                name: NAME.to_string(),
                fired: share >= config::NONREGULAR_SHARE_HIGH_RATIO,
                evidence_ids: vec![eid],
                interpretation: "市場の求人は正社員以外 (パート・契約等) の比率が高く、正社員採用であれば雇用の安定を訴求できる可能性があります。逆に正社員採用の競合は限られる可能性があります。".to_string(),
                alternative_explanations: vec![
                    "雇用形態の表記ゆれや、一方のCSVにのみ雇用形態列がある点に留意 (§5.0)".to_string(),
                ],
                data_note: String::new(),
            }
        }
        None => Signal::not_evaluable(
            ID,
            NAME,
            "雇用形態の内訳データが不足しています (雇用形態列のないCSVを含む)",
        ),
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::handlers::consult::input::{ClientInput, CompanyObservation};

    fn base_input() -> ConsultInput {
        ConsultInput {
            pref: "群馬県".to_string(),
            muni: "高崎市".to_string(),
            as_of: "2026-07-10".to_string(),
            total_postings: 100,
            new_count: 10,
            company_count: 30,
            salary_values: (0..100).map(|i| 200_000 + i * 1_000).collect(),
            salary_median: Some(250_000),
            salary_q1: Some(225_000),
            salary_q3: Some(275_000),
            salary_n: 100,
            ..Default::default()
        }
    }

    #[test]
    fn compute_posting_age_ratio_pure_fn() {
        assert_eq!(compute_posting_age_30plus_ratio(&[]), None);
        assert_eq!(compute_posting_age_30plus_ratio(&["", "  "]), None);
        let r = compute_posting_age_30plus_ratio(&["30+日前", "13日前", "30+日前", "5時間前"]);
        assert_eq!(r, Some(0.5));
    }

    #[test]
    fn s01_fires_on_high_ratio_and_reports_missing_data() {
        let mut input = base_input();
        let mut store = EvidenceStore::new();
        input.posting_age_30plus_ratio = Some(0.5);
        let s = s01_long_running_postings(&input, &mut store);
        assert!(s.fired);
        assert!(!s.evidence_ids.is_empty());

        input.posting_age_30plus_ratio = Some(0.2);
        assert!(!s01_long_running_postings(&input, &mut store).fired);

        input.posting_age_30plus_ratio = None;
        let s = s01_long_running_postings(&input, &mut store);
        assert!(!s.fired);
        assert!(!s.data_note.is_empty(), "欠損時は data_note で明示する");
    }

    #[test]
    fn s02_s03_client_salary_quartiles() {
        let mut input = base_input();
        let mut store = EvidenceStore::new();
        // 下位: 210,000 は分布の下位25%以下
        input.client = ClientInput {
            target_salary_max: Some(210_000),
            ..Default::default()
        };
        assert!(s02_client_salary_bottom_quartile(&input, &mut store).fired);
        assert!(!s03_client_salary_top_quartile(&input, &mut store).fired);
        // 上位: 290,000 は分布の上位25%以上
        input.client.target_salary_max = Some(290_000);
        assert!(!s02_client_salary_bottom_quartile(&input, &mut store).fired);
        assert!(s03_client_salary_top_quartile(&input, &mut store).fired);
        // 未入力: not evaluable
        input.client.target_salary_max = None;
        let s = s02_client_salary_bottom_quartile(&input, &mut store);
        assert!(!s.fired && !s.data_note.is_empty());
    }

    #[test]
    fn s04_min_wage_proximity_monthly_and_hourly() {
        let mut input = base_input();
        let mut store = EvidenceStore::new();
        input.salary_q1 = Some(160_000);
        input.min_wage_monthly_160h = Some(158_000.0);
        assert!(s04_min_wage_proximity(&input, &mut store).fired);
        input.salary_q1 = Some(225_000);
        assert!(!s04_min_wage_proximity(&input, &mut store).fired);

        input.is_hourly = true;
        input.hourly_median_low = Some(1_000);
        input.min_wage_hourly = Some(985.0);
        assert!(s04_min_wage_proximity(&input, &mut store).fired);
        input.hourly_median_low = Some(1_300);
        assert!(!s04_min_wage_proximity(&input, &mut store).fired);
    }

    #[test]
    fn s05_below_pref_and_hourly_skip() {
        let mut input = base_input();
        let mut store = EvidenceStore::new();
        input.scheduled_earnings_latest = Some(300_000.0);
        // 250,000 / 300,000 = 0.83 < 0.9 → fired
        assert!(s05_market_salary_below_pref(&input, &mut store).fired);
        input.scheduled_earnings_latest = Some(255_000.0);
        assert!(!s05_market_salary_below_pref(&input, &mut store).fired);
        // 時給モードでは単位不一致のため判定しない
        input.is_hourly = true;
        let s = s05_market_salary_below_pref(&input, &mut store);
        assert!(!s.fired && !s.data_note.is_empty());
    }

    #[test]
    fn s06_employee_decline_requires_matched_companies() {
        let mut input = base_input();
        let mut store = EvidenceStore::new();
        // 名寄せなし → not evaluable
        input.companies = vec![CompanyObservation {
            name: "A社".to_string(),
            posting_count: 5,
            employee_count: None,
            employee_delta_1y: None,
        }];
        let s = s06_employee_decline_with_postings(&input, &mut store);
        assert!(!s.fired && !s.data_note.is_empty());

        // 減少×募集継続 → fired
        input.companies = vec![CompanyObservation {
            name: "B社".to_string(),
            posting_count: 3,
            employee_count: Some(200),
            employee_delta_1y: Some(-8.0),
        }];
        let s = s06_employee_decline_with_postings(&input, &mut store);
        assert!(s.fired);
        assert!(
            !s.alternative_explanations.is_empty(),
            "M&A等の代替説明を保持する (§9.1)"
        );

        // 増加中 → not fired (だが判定はできている)
        input.companies[0].employee_delta_1y = Some(5.0);
        let s = s06_employee_decline_with_postings(&input, &mut store);
        assert!(!s.fired && s.data_note.is_empty());
    }

    #[test]
    fn s07_workforce_decline_threshold() {
        let mut input = base_input();
        let mut store = EvidenceStore::new();
        input.wa_decline_rate_muni = Some(-20.0);
        assert!(s07_workforce_declining_region(&input, &mut store).fired);
        input.wa_decline_rate_muni = Some(-5.0);
        assert!(!s07_workforce_declining_region(&input, &mut store).fired);
        input.wa_decline_rate_muni = None;
        let s = s07_workforce_declining_region(&input, &mut store);
        assert!(!s.fired && !s.data_note.is_empty());
    }

    #[test]
    fn s08_s09_switcher_thin_thick() {
        let mut input = base_input();
        let mut store = EvidenceStore::new();
        input.job_change_desire_rate_pref = Some(8.0);
        input.job_change_desire_rate_national = Some(10.0);
        assert!(s08_switcher_supply_thin(&input, &mut store).fired);
        assert!(!s09_switcher_supply_thick(&input, &mut store).fired);
        input.job_change_desire_rate_pref = Some(12.0);
        assert!(!s08_switcher_supply_thin(&input, &mut store).fired);
        assert!(s09_switcher_supply_thick(&input, &mut store).fired);
    }

    #[test]
    fn s10_s11_job_ratio() {
        let mut input = base_input();
        let mut store = EvidenceStore::new();
        input.job_openings_ratio = Some(1.6);
        assert!(s10_job_ratio_high(&input, &mut store).fired);
        assert!(!s11_job_ratio_low(&input, &mut store).fired);
        input.job_openings_ratio = Some(0.9);
        assert!(!s10_job_ratio_high(&input, &mut store).fired);
        assert!(s11_job_ratio_low(&input, &mut store).fired);
    }

    #[test]
    fn s12_commute_inflow_absolute_and_relative() {
        let mut input = base_input();
        let mut store = EvidenceStore::new();
        input.commute_inflow_total = Some(15_000);
        assert!(s12_commute_inflow(&input, &mut store).fired);
        // 絶対数は少ないが流出比で発火
        input.commute_inflow_total = Some(5_000);
        input.commute_outflow_total = Some(3_000);
        assert!(s12_commute_inflow(&input, &mut store).fired);
        // どちらも満たさない
        input.commute_inflow_total = Some(5_000);
        input.commute_outflow_total = Some(6_000);
        assert!(!s12_commute_inflow(&input, &mut store).fired);
        input.commute_inflow_total = None;
        let s = s12_commute_inflow(&input, &mut store);
        assert!(!s.fired && !s.data_note.is_empty());
    }

    #[test]
    fn s13_new_ratio() {
        let mut input = base_input();
        let mut store = EvidenceStore::new();
        input.new_count = 40; // 40%
        assert!(s13_new_posting_ratio_high(&input, &mut store).fired);
        input.new_count = 10; // 10%
        assert!(!s13_new_posting_ratio_high(&input, &mut store).fired);
    }

    #[test]
    fn s14_sample_insufficiency() {
        let mut input = base_input();
        let mut store = EvidenceStore::new();
        input.total_postings = 10;
        assert!(s14_sample_insufficient(&input, &mut store).fired);
        input.total_postings = 100;
        assert!(!s14_sample_insufficient(&input, &mut store).fired);
    }

    #[test]
    fn s15_concentration() {
        let mut input = base_input();
        let mut store = EvidenceStore::new();
        input.companies = vec![
            CompanyObservation {
                name: "大手A".to_string(),
                posting_count: 40,
                ..Default::default()
            },
            CompanyObservation {
                name: "B社".to_string(),
                posting_count: 5,
                ..Default::default()
            },
        ];
        assert!(s15_posting_concentration(&input, &mut store).fired);
        input.companies[0].posting_count = 10;
        assert!(!s15_posting_concentration(&input, &mut store).fired);
    }

    #[test]
    fn s16_net_migration_outflow_fires_and_reports_missing() {
        let mut input = base_input();
        let mut store = EvidenceStore::new();
        input.net_migration_rate = Some(-4.0);
        assert!(s16_net_migration_outflow(&input, &mut store).fired);
        input.net_migration_rate = Some(1.0);
        assert!(!s16_net_migration_outflow(&input, &mut store).fired);
        input.net_migration_rate = None;
        let s = s16_net_migration_outflow(&input, &mut store);
        assert!(!s.fired && !s.data_note.is_empty());
    }

    #[test]
    fn s18_s19_business_dynamics_use_annualized_values() {
        let mut input = base_input();
        let mut store = EvidenceStore::new();
        // 調査間隔 5年。累計 開15%/廃25% → 年換算 開3%/廃5% → 廃業超過発火・開業活発は非発火
        input.business_dynamics_interval_years = Some(5.0);
        input.business_opening_rate = Some(15.0);
        input.business_closure_rate = Some(25.0);
        assert!(s18_closure_over_opening(&input, &mut store).fired);
        assert!(!s19_opening_active(&input, &mut store).fired);
        // 累計 開35%/廃20% → 年換算 開7%/廃4% → 廃業超過は非発火・開業活発は発火 (7% >= 6.5%)
        input.business_opening_rate = Some(35.0);
        input.business_closure_rate = Some(20.0);
        assert!(!s18_closure_over_opening(&input, &mut store).fired);
        assert!(s19_opening_active(&input, &mut store).fired);
    }

    #[test]
    fn s19_does_not_fire_on_cumulative_when_annualized_below_threshold() {
        // P0-1 実データ退行防止: 大分県2021 累計29.79% / 5年 ≈ 5.96% → 発火しない
        let mut input = base_input();
        let mut store = EvidenceStore::new();
        input.business_dynamics_interval_years = Some(5.0);
        input.business_opening_rate = Some(29.79);
        let s = s19_opening_active(&input, &mut store);
        assert!(!s.fired, "年換算 約6% では開業活発シグナルは発火しない");
    }

    #[test]
    fn s18_s19_not_evaluable_without_interval() {
        // 調査間隔が取れないときは年換算できず判定不能 (発火せず data_note を出す)
        let mut input = base_input();
        let mut store = EvidenceStore::new();
        input.business_dynamics_interval_years = None;
        input.business_opening_rate = Some(29.79);
        input.business_closure_rate = Some(29.58);
        let s19 = s19_opening_active(&input, &mut store);
        assert!(!s19.fired && !s19.data_note.is_empty());
        let s18 = s18_closure_over_opening(&input, &mut store);
        assert!(!s18.fired && !s18.data_note.is_empty());
    }

    #[test]
    fn s20_s21_unemployment_ratio() {
        let mut input = base_input();
        let mut store = EvidenceStore::new();
        input.unemployment_rate_pref = Some(2.0);
        input.unemployment_rate_national = Some(2.8);
        assert!(s20_unemployment_tight(&input, &mut store).fired);
        assert!(!s21_unemployment_slack(&input, &mut store).fired);
        input.unemployment_rate_pref = Some(3.5);
        assert!(!s20_unemployment_tight(&input, &mut store).fired);
        assert!(s21_unemployment_slack(&input, &mut store).fired);
    }

    #[test]
    fn s22_rent_uses_relative_position_not_monthly_balance() {
        // P0-2: 1畳あたり家賃の全国比のみで判定。月額家賃バランスは廃止。
        let mut input = base_input();
        let mut store = EvidenceStore::new();
        input.rent_per_tatami = Some(2274); // 東京都相当
        input.rent_per_tatami_national = Some(1200);
        // 2274/1200 ≈ 1.9倍 > 1.1 → 相対的に高いで発火
        let s = s22_rent_burden(&input, &mut store);
        assert!(s.fired);
        // 証拠は円単位の相対位置であり「円/月」を出さない (月額捏造しない)
        let ev = s
            .evidence_ids
            .first()
            .and_then(|id| store.items().iter().find(|e| &e.id == id).cloned());
        assert!(ev.is_some());
        assert_ne!(ev.unwrap().unit, "円/月");
    }

    #[test]
    fn s22_not_evaluable_without_national_baseline() {
        // 全国基準が無ければ相対位置を判定できない → 発火せず data_note (月額換算はしない)
        let mut input = base_input();
        let mut store = EvidenceStore::new();
        input.rent_per_tatami = Some(917); // 大分県相当
        input.rent_per_tatami_national = None;
        let s = s22_rent_burden(&input, &mut store);
        assert!(!s.fired && !s.data_note.is_empty());
        // 大分917が全国比0.76なら発火しない (相対的に低い)
        input.rent_per_tatami_national = Some(1200);
        let s2 = s22_rent_burden(&input, &mut store);
        assert!(!s2.fired, "全国比0.76倍は高くないので発火しない");
    }

    #[test]
    fn s24_holiday_mention_thin_test() {
        let mut input = base_input();
        let mut store = EvidenceStore::new();
        input.total_postings = 100;
        input.annual_holidays_n = 30; // 30% < 50%
        assert!(s24_holiday_mention_thin(&input, &mut store).fired);
        input.annual_holidays_n = 80;
        assert!(!s24_holiday_mention_thin(&input, &mut store).fired);
    }

    #[test]
    fn s26_tag_variety_thin_test() {
        let mut input = base_input();
        let mut store = EvidenceStore::new();
        input.distinct_tag_count = 3;
        assert!(s26_tag_variety_thin(&input, &mut store).fired);
        input.distinct_tag_count = 12;
        assert!(!s26_tag_variety_thin(&input, &mut store).fired);
    }

    #[test]
    fn s28_commute_delivery_always_fires_when_data_present() {
        let mut input = base_input();
        let mut store = EvidenceStore::new();
        let s = s28_commute_delivery_check(&input, &mut store);
        assert!(!s.fired && !s.data_note.is_empty(), "内訳なしなら判定不能");
        input.commute_inflow_top3 = vec![("群馬県".to_string(), "前橋市".to_string(), 5000)];
        let s = s28_commute_delivery_check(&input, &mut store);
        assert!(s.fired, "流入内訳があれば要確認論点として発火");
        assert!(s.interpretation.contains("要確認"));
    }

    #[test]
    fn s29_growing_companies_test() {
        let mut input = base_input();
        let mut store = EvidenceStore::new();
        input.companies = vec![CompanyObservation {
            name: "成長社".to_string(),
            posting_count: 4,
            employee_count: Some(500),
            employee_delta_1y: Some(8.0),
        }];
        assert!(s29_growing_companies(&input, &mut store).fired);
        input.companies[0].employee_delta_1y = Some(1.0);
        assert!(!s29_growing_companies(&input, &mut store).fired);
    }

    #[test]
    fn s30_nonregular_share() {
        let mut input = base_input();
        let mut store = EvidenceStore::new();
        input.employment_type_dist = vec![("パート".to_string(), 60), ("正社員".to_string(), 40)];
        assert!(s30_nonregular_share_high(&input, &mut store).fired);
        input.employment_type_dist = vec![("正社員".to_string(), 80), ("パート".to_string(), 20)];
        assert!(!s30_nonregular_share_high(&input, &mut store).fired);
        input.employment_type_dist = vec![];
        let s = s30_nonregular_share_high(&input, &mut store);
        assert!(!s.fired && !s.data_note.is_empty());
    }

    #[test]
    fn all_signals_have_unique_ids_and_valid_evidence_refs() {
        let mut input = base_input();
        input.posting_age_30plus_ratio = Some(0.5);
        input.job_openings_ratio = Some(1.6);
        input.wa_decline_rate_muni = Some(-16.0);
        input.scheduled_earnings_latest = Some(300_000.0);
        input.min_wage_monthly_160h = Some(160_000.0);
        input.job_change_desire_rate_pref = Some(8.0);
        input.job_change_desire_rate_national = Some(10.0);
        input.commute_inflow_total = Some(20_000);
        let mut store = EvidenceStore::new();
        let signals = evaluate_signals(&input, &mut store);
        assert!(
            signals.len() >= 28 && signals.len() <= 32,
            "シグナルは拡充後28〜32個 (実際: {})",
            signals.len()
        );
        let mut ids: Vec<&str> = signals.iter().map(|s| s.id.as_str()).collect();
        ids.sort();
        ids.dedup();
        assert_eq!(ids.len(), signals.len(), "シグナルIDが重複");
        for s in &signals {
            for eid in &s.evidence_ids {
                assert!(
                    store.contains_id(eid),
                    "{}: 証拠ID {} が実在しない",
                    s.id,
                    eid
                );
            }
        }
    }
}
