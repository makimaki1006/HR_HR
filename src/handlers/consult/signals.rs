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
            signals.len() >= 10 && signals.len() <= 15,
            "シグナルは10〜15個 (実際: {})",
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
