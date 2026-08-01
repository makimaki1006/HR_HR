//! 応募者ジャーニー診断の入力正規化・決定論的集計。
//!
//! LLM に任せるのは「ペルソナ仮説・検索行動・離脱仮説・対策の意味づけ」に限る。
//! CSV の列解釈、件数、給与分布、人気タグ、口コミ本文の有無はコードで確定し、
//! 入力にない数値を生成させない。

use std::collections::{BTreeMap, HashMap, HashSet};

use base64::{engine::general_purpose::STANDARD as BASE64_STANDARD, Engine as _};
use serde::Serialize;
use serde_json::{json, Value};

use crate::handlers::survey::salary_parser::{parse_salary, SalaryType};
use crate::handlers::survey::upload::{self, SurveyRecord};

const MAX_CSV_BYTES: usize = 15 * 1024 * 1024;
const COMPETITOR_BRIEF_LIMIT: usize = 40;
const COMPETITOR_TEXT_LIMIT: usize = 520;
const REVIEW_TEXT_LIMIT: usize = 900;

#[derive(Debug, Clone, Serialize)]
pub struct NamedCount {
    pub name: String,
    pub count: usize,
}

#[derive(Debug, Clone, Serialize)]
pub struct CoverageCount {
    pub label: String,
    pub count: usize,
    pub total: usize,
}

#[derive(Debug, Clone, Serialize)]
pub struct SalaryDistribution {
    pub group: String,
    pub count: usize,
    pub minimum_yen: i64,
    pub first_quartile_yen: i64,
    pub median_yen: i64,
    pub third_quartile_yen: i64,
    pub maximum_yen: i64,
    pub unit_note: String,
}

#[derive(Debug, Clone, Serialize)]
pub struct CompetitorBrief {
    pub source_ref: String,
    pub title: String,
    pub company: String,
    pub location: String,
    pub employment_type: String,
    pub salary_text: String,
    pub tags: Vec<String>,
    pub description_excerpt: String,
}

#[derive(Debug, Clone, Serialize)]
pub struct CompetitorSummary {
    pub filename: String,
    pub captured_at: Option<String>,
    pub encoding: String,
    pub raw_row_count: usize,
    pub record_count: usize,
    pub analysis_excluded_rows: usize,
    pub unique_company_count: usize,
    pub unique_url_count: usize,
    pub employment_types: Vec<NamedCount>,
    pub salary_distributions: Vec<SalaryDistribution>,
    pub top_locations: Vec<NamedCount>,
    pub top_tags: Vec<NamedCount>,
    pub popular_count: usize,
    pub super_popular_count: usize,
    pub coverage: Vec<CoverageCount>,
    pub briefs: Vec<CompetitorBrief>,
    /// 顧客給与の実測パーセンタイル計算にだけ使う。LLM・ブラウザへは渡さない。
    #[serde(skip_serializing)]
    salary_values_by_group: BTreeMap<String, Vec<i64>>,
}

#[derive(Debug, Clone, Serialize)]
pub struct ReviewEvidence {
    pub source_ref: String,
    pub posted_relative: String,
    pub text: String,
    pub reactions: String,
}

#[derive(Debug, Clone, Serialize)]
pub struct ReviewSummary {
    pub filename: String,
    pub captured_at: Option<String>,
    pub encoding: String,
    pub total_rows: usize,
    pub text_rows: usize,
    pub blank_text_rows: usize,
    pub duplicate_text_rows: usize,
    pub evidence: Vec<ReviewEvidence>,
    pub scope_note: String,
}

#[derive(Debug, Clone, Serialize)]
pub struct ClientSalaryPosition {
    pub client_salary_text: String,
    pub client_monthly_equivalent_yen: i64,
    pub comparison_group: String,
    pub sample_count: usize,
    pub median_yen: i64,
    pub first_quartile_yen: i64,
    pub third_quartile_yen: i64,
    pub percentile_position: f64,
    pub position_label: String,
    pub calculation_note: String,
}

/// ブラウザから受け取った base64 CSV を上限付きで復号する。
pub fn decode_csv_base64(encoded: &str, label: &str) -> Result<Vec<u8>, String> {
    if encoded.trim().is_empty() {
        return Err(format!("{label}が選択されていません。"));
    }
    // base64 は元バイトの約 4/3。復号前にも巨大入力を拒否する。
    if encoded.len() > (MAX_CSV_BYTES * 4 / 3) + 8 {
        return Err(format!("{label}が15MBを超えています。"));
    }
    let bytes = BASE64_STANDARD
        .decode(encoded.trim())
        .map_err(|_| format!("{label}を読み取れませんでした。"))?;
    if bytes.len() > MAX_CSV_BYTES {
        return Err(format!("{label}が15MBを超えています。"));
    }
    Ok(bytes)
}

/// Indeed / 求人ボックス CSV を既存の媒体分析パーサで解釈し、診断用に集約する。
pub fn summarize_competitor_csv(
    bytes: &[u8],
    filename: &str,
    captured_at: Option<String>,
) -> Result<CompetitorSummary, String> {
    let (decoded, encoding) = upload::decode_csv_bytes(bytes);
    let raw_row_count = csv::ReaderBuilder::new()
        .flexible(true)
        .from_reader(decoded.as_slice())
        .records()
        .count();
    let records = upload::parse_csv_bytes(bytes, None)?;
    Ok(summarize_competitor_records(
        &records,
        filename,
        captured_at,
        encoding,
        raw_row_count,
    ))
}

fn summarize_competitor_records(
    records: &[SurveyRecord],
    filename: &str,
    captured_at: Option<String>,
    encoding: &str,
    raw_row_count: usize,
) -> CompetitorSummary {
    let mut companies = HashSet::new();
    let mut urls = HashSet::new();
    let mut employment_counts: HashMap<String, usize> = HashMap::new();
    let mut location_counts: HashMap<String, usize> = HashMap::new();
    let mut tag_counts: HashMap<String, usize> = HashMap::new();
    let mut salary_groups: BTreeMap<String, Vec<i64>> = BTreeMap::new();
    let mut all_salaries = Vec::new();
    let mut popular_count = 0;
    let mut super_popular_count = 0;

    for record in records {
        if !record.company_name.trim().is_empty() {
            companies.insert(record.company_name.trim().to_string());
        }
        if let Some(url) = record
            .url
            .as_deref()
            .map(str::trim)
            .filter(|s| !s.is_empty())
        {
            urls.insert(url.to_string());
        }
        let employment = if record.employment_type.trim().is_empty() {
            "雇用形態不明".to_string()
        } else {
            record.employment_type.trim().to_string()
        };
        *employment_counts.entry(employment.clone()).or_default() += 1;

        let location = display_location(record);
        if !location.is_empty() {
            *location_counts.entry(location).or_default() += 1;
        }

        let tags = split_tags(&record.tags_raw);
        let has_super_popular = tags.iter().any(|tag| tag == "超人気");
        let has_popular = tags.iter().any(|tag| tag == "人気");
        if has_super_popular {
            super_popular_count += 1;
        } else if has_popular {
            popular_count += 1;
        }
        for tag in tags {
            if !tag.is_empty() && !looks_like_tag_overflow(&tag) {
                *tag_counts.entry(tag).or_default() += 1;
            }
        }

        if let Some(monthly) = record.salary_parsed.unified_monthly {
            if (50_000..=2_000_000).contains(&monthly) {
                salary_groups.entry(employment).or_default().push(monthly);
                all_salaries.push(monthly);
            }
        }
    }

    let mut salary_values_by_group = salary_groups.clone();
    salary_values_by_group.insert("全体".to_string(), all_salaries.clone());
    let mut salary_distributions = Vec::new();
    if !all_salaries.is_empty() {
        salary_distributions.push(distribution("全体", all_salaries));
    }
    for (group, values) in salary_groups {
        if !values.is_empty() {
            salary_distributions.push(distribution(&group, values));
        }
    }

    // CSV の並び順先頭だけに寄らないよう、全件から等間隔に最大40件を抽出する。
    // C番号は元CSVのデータ行番号を保ち、コンサルが原表へ戻れるようにする。
    let briefs = sample_indices(records.len(), COMPETITOR_BRIEF_LIMIT)
        .into_iter()
        .map(|index| {
            let record = &records[index];
            let combined = match (
                record.snippet.trim().is_empty(),
                record.description.trim().is_empty(),
            ) {
                (false, false) => {
                    format!("{} / {}", record.snippet.trim(), record.description.trim())
                }
                (false, true) => record.snippet.trim().to_string(),
                (true, false) => record.description.trim().to_string(),
                (true, true) => String::new(),
            };
            CompetitorBrief {
                source_ref: format!("C{}", index + 1),
                title: truncate_chars(record.job_title.trim(), 90),
                company: truncate_chars(record.company_name.trim(), 70),
                location: display_location(record),
                employment_type: if record.employment_type.trim().is_empty() {
                    "不明".to_string()
                } else {
                    record.employment_type.trim().to_string()
                },
                salary_text: truncate_chars(record.salary_raw.trim(), 100),
                tags: split_tags(&record.tags_raw),
                description_excerpt: truncate_chars(&combined, COMPETITOR_TEXT_LIMIT),
            }
        })
        .collect();

    CompetitorSummary {
        filename: filename.to_string(),
        captured_at,
        encoding: encoding.to_string(),
        raw_row_count,
        record_count: records.len(),
        analysis_excluded_rows: raw_row_count.saturating_sub(records.len()),
        unique_company_count: companies.len(),
        unique_url_count: urls.len(),
        employment_types: sorted_counts(employment_counts, 20),
        salary_distributions,
        top_locations: sorted_counts(location_counts, 12),
        top_tags: sorted_counts(tag_counts, 20),
        popular_count,
        super_popular_count,
        coverage: vec![
            CoverageCount {
                label: "給与表記あり".to_string(),
                count: records
                    .iter()
                    .filter(|r| r.salary_parsed.unified_monthly.is_some())
                    .count(),
                total: records.len(),
            },
            CoverageCount {
                label: "仕事内容・訴求あり".to_string(),
                count: records
                    .iter()
                    .filter(|r| !r.description.trim().is_empty() || !r.snippet.trim().is_empty())
                    .count(),
                total: records.len(),
            },
            CoverageCount {
                label: "年間休日を数値抽出".to_string(),
                count: records
                    .iter()
                    .filter(|r| r.annual_holidays.is_some())
                    .count(),
                total: records.len(),
            },
            CoverageCount {
                label: "特徴タグあり".to_string(),
                count: records
                    .iter()
                    .filter(|r| !r.tags_raw.trim().is_empty())
                    .count(),
                total: records.len(),
            },
        ],
        briefs,
        salary_values_by_group,
    }
}

/// Google ビジネスプロフィール等からスクレイピングした口コミ CSV を解釈する。
///
/// 星評価は必須にしない。本文が空の行は件数には含めるが、LLM の内容分析には渡さない。
pub fn summarize_review_csv(
    bytes: &[u8],
    filename: &str,
    captured_at: Option<String>,
) -> Result<ReviewSummary, String> {
    let (decoded, encoding) = upload::decode_csv_bytes(bytes);
    let mut reader = csv::ReaderBuilder::new()
        .flexible(true)
        .from_reader(decoded.as_slice());
    let headers = reader
        .headers()
        .map_err(|e| format!("口コミCSVのヘッダーを読めません: {e}"))?
        .iter()
        .map(|h| h.trim_matches('\u{feff}').trim().to_string())
        .collect::<Vec<_>>();

    let text_index = find_header(
        &headers,
        &[
            "oa1nbd",
            "口コミ本文",
            "口コミ",
            "レビュー本文",
            "review_text",
            "review",
            "text",
            "本文",
        ],
    )
    .ok_or_else(|| "口コミ本文の列を特定できませんでした。".to_string())?;
    let date_index = find_header(
        &headers,
        &["y3ibjb", "投稿日", "投稿時期", "review_date", "date"],
    );
    let reaction_index = find_header(
        &headers,
        &[
            "uo5pt",
            "参考になった",
            "リアクション",
            "likes",
            "like_count",
        ],
    );

    let mut total_rows = 0;
    let mut blank_text_rows = 0;
    let mut duplicate_text_rows = 0;
    let mut seen_text = HashSet::new();
    let mut evidence = Vec::new();

    for row in reader.records() {
        let row = row.map_err(|e| format!("口コミCSVのデータ行を読めません: {e}"))?;
        total_rows += 1;
        let text = row.get(text_index).unwrap_or("").trim();
        if text.is_empty() {
            blank_text_rows += 1;
            continue;
        }
        let normalized = text.split_whitespace().collect::<String>();
        if !seen_text.insert(normalized) {
            duplicate_text_rows += 1;
            continue;
        }
        evidence.push(ReviewEvidence {
            source_ref: format!("R{}", total_rows),
            posted_relative: date_index
                .and_then(|index| row.get(index))
                .unwrap_or("")
                .trim()
                .to_string(),
            text: truncate_chars(text, REVIEW_TEXT_LIMIT),
            reactions: reaction_index
                .and_then(|index| row.get(index))
                .unwrap_or("")
                .trim()
                .to_string(),
        });
    }

    if total_rows == 0 {
        return Err("口コミCSVにデータ行がありません。".to_string());
    }

    Ok(ReviewSummary {
        filename: filename.to_string(),
        captured_at,
        encoding: encoding.to_string(),
        total_rows,
        text_rows: evidence.len(),
        blank_text_rows,
        duplicate_text_rows,
        evidence,
        scope_note: "口コミは会社の労働実態を確定する事実ではなく、求職者が検索時に目にし得る外部観測として扱う。単独のネガティブ情報も、認知上の影響仮説から除外しない。".to_string(),
    })
}

/// 顧客求人の給与を競合 CSV の月給換算分布に置く。
pub fn client_salary_position(
    salary_text: &str,
    employment_type: &str,
    summary: &CompetitorSummary,
) -> Option<ClientSalaryPosition> {
    let parsed = parse_salary(salary_text, SalaryType::Monthly);
    let client = parsed.unified_monthly?;
    let exact = summary
        .salary_distributions
        .iter()
        .find(|d| d.group != "全体" && same_employment_group(&d.group, employment_type));
    let distribution = exact.or_else(|| {
        summary
            .salary_distributions
            .iter()
            .find(|d| d.group == "全体")
    })?;

    let salary_values = summary
        .salary_values_by_group
        .get(&distribution.group)
        .or_else(|| summary.salary_values_by_group.get("全体"))?;
    let at_or_below = salary_values
        .iter()
        .filter(|value| **value <= client)
        .count();
    let percentile = at_or_below as f64 / salary_values.len().max(1) as f64 * 100.0;

    let position_label = if client < distribution.first_quartile_yen {
        "第1四分位未満"
    } else if client < distribution.median_yen {
        "第1四分位以上・中央値未満"
    } else if client <= distribution.third_quartile_yen {
        "中央値以上・第3四分位以下"
    } else {
        "第3四分位超"
    };

    Some(ClientSalaryPosition {
        client_salary_text: salary_text.to_string(),
        client_monthly_equivalent_yen: client,
        comparison_group: distribution.group.clone(),
        sample_count: distribution.count,
        median_yen: distribution.median_yen,
        first_quartile_yen: distribution.first_quartile_yen,
        third_quartile_yen: distribution.third_quartile_yen,
        percentile_position: (percentile * 10.0).round() / 10.0,
        position_label: position_label.to_string(),
        calculation_note: "競合CSVと顧客求人の給与表記を既存パーサで月給換算。範囲表記は上下限の中点を代表値として配置し、固定残業代・賞与・手当の内訳差は別途確認が必要。".to_string(),
    })
}

pub fn diagnosis_schema() -> Value {
    let string_array = || json!({"type":"array","items":{"type":"string"}});
    json!({
        "type":"object",
        "properties":{
            "case_profile":{
                "type":"object",
                "properties":{
                    "company_name":{"type":"string"},
                    "job_title":{"type":"string"},
                    "occupation":{"type":"string"},
                    "prefecture":{"type":"string"},
                    "municipality":{"type":"string"},
                    "employment_type":{"type":"string"},
                    "analysis_summary":{"type":"string"}
                },
                "required":["company_name","job_title","occupation","prefecture","municipality","employment_type","analysis_summary"]
            },
            "condition_findings":{
                "type":"array",
                "items":{
                    "type":"object",
                    "properties":{
                        "dimension":{"type":"string"},
                        "client_observation":{"type":"string"},
                        "market_observation":{"type":"string"},
                        "relative_evaluation":{"type":"string"},
                        "candidate_effect":{"type":"string"},
                        "evidence_refs":string_array()
                    },
                    "required":["dimension","client_observation","market_observation","relative_evaluation","candidate_effect","evidence_refs"]
                }
            },
            "review_findings":{
                "type":"array",
                "items":{
                    "type":"object",
                    "properties":{
                        "source_ref":{"type":"string"},
                        "external_observation":{"type":"string"},
                        "candidate_perception_hypothesis":{"type":"string"},
                        "relevant_search":{"type":"string"},
                        "client_confirmation":{"type":"string"}
                    },
                    "required":["source_ref","external_observation","candidate_perception_hypothesis","relevant_search","client_confirmation"]
                }
            },
            "personas":{
                "type":"array",
                "items":{
                    "type":"object",
                    "properties":{
                        "id":{"type":"string"},
                        "label":{"type":"string"},
                        "profile":{"type":"string"},
                        "transfer_reason":{"type":"string"},
                        "market_basis":string_array(),
                        "eligibility":{"type":"string","enum":["応募可能性が高い","条件確認が必要","現条件では難しい"]},
                        "likely_behavior":{"type":"string","enum":["応募へ進む","検索・比較する","求人閲覧段階で離脱する"]},
                        "behavior_reason":{"type":"string"},
                        "employer_fit_hypothesis":{"type":"string"},
                        "search_queries":{
                            "type":"array",
                            "items":{
                                "type":"object",
                                "properties":{
                                    "query":{"type":"string"},
                                    "stage":{"type":"string"},
                                    "intent":{"type":"string"},
                                    "reason":{"type":"string"},
                                    "basis_type":{"type":"string","enum":["求人由来","職種あるある","口コミ由来","競合比較","応募段階"]},
                                    "importance":{"type":"string","enum":["高","中","低"]}
                                },
                                "required":["query","stage","intent","reason","basis_type","importance"]
                            }
                        },
                        "journey":{
                            "type":"array",
                            "items":{
                                "type":"object",
                                "properties":{
                                    "stage":{"type":"string"},
                                    "candidate_action":{"type":"string"},
                                    "question_or_expectation":{"type":"string"},
                                    "dropoff_trigger":{"type":"string"},
                                    "countermeasure":{"type":"string"},
                                    "channel":{"type":"string"},
                                    "evidence_refs":string_array()
                                },
                                "required":["stage","candidate_action","question_or_expectation","dropoff_trigger","countermeasure","channel","evidence_refs"]
                            }
                        },
                        "post_application_actions":string_array(),
                        "if_employer_wants_actions":string_array(),
                        "if_not_target_action":{"type":"string"}
                    },
                    "required":["id","label","profile","transfer_reason","market_basis","eligibility","likely_behavior","behavior_reason","employer_fit_hypothesis","search_queries","journey","post_application_actions","if_employer_wants_actions","if_not_target_action"]
                }
            },
            "priority_actions":{
                "type":"array",
                "items":{
                    "type":"object",
                    "properties":{
                        "persona_id":{"type":"string"},
                        "stage":{"type":"string"},
                        "risk":{"type":"string"},
                        "cause_type":{"type":"string"},
                        "countermeasure":{"type":"string"},
                        "channel":{"type":"string"},
                        "client_confirmation":{"type":"string"},
                        "priority":{"type":"string","enum":["高","中","低"]},
                        "evidence_refs":string_array()
                    },
                    "required":["persona_id","stage","risk","cause_type","countermeasure","channel","client_confirmation","priority","evidence_refs"]
                }
            },
            "client_questions":string_array(),
            "limitations":string_array()
        },
        "required":["case_profile","condition_findings","review_findings","personas","priority_actions","client_questions","limitations"]
    })
}

pub fn build_diagnosis_prompt(
    client_source: &str,
    verified_facts: &Value,
    competitor: &CompetitorSummary,
    reviews: &ReviewSummary,
    client_salary: Option<&ClientSalaryPosition>,
    public_stats: &Value,
    employer_note: &str,
) -> String {
    const COMMON_SEARCH_AXES: &str = r#"
- 報酬: 給料、手取り、固定残業代、賞与、昇給、手当、歩合
- 時間と休日: 残業、拘束時間、始終業、夜勤、シフト、年間休日、希望休
- 仕事内容の実態: 1日の流れ、担当範囲、繁忙期、ノルマ、クレーム
- 身体負担と安全: 重量物、立ち仕事、暑さ寒さ、事故、保護具、休憩
- 経験と教育: 未経験、研修、独り立ち、資格、失敗時の支援
- 人間関係: 上司、教育担当、相談先、職場の距離感
- 生活: 通勤、転勤、帰宅時刻、家族時間、住宅費
- キャリアと安定: 正社員、登用、勤続、評価、将来性
- 企業認知: 会社名＋口コミ／評判／残業／給料／事故等
- 応募と選考: 応募資格、面接質問、選考期間、職場見学、オファー条件
"#;
    let stages = "求人認知 → 求人閲覧 → 自然検索 → 他求人比較 → 応募判断 → 応募後連絡 → 面接 → オファー・入社判断";
    let source_excerpt = truncate_chars(client_source, 10_000);
    let competitor_json =
        serde_json::to_string_pretty(competitor).unwrap_or_else(|_| "{}".to_string());
    let reviews_json = serde_json::to_string_pretty(reviews).unwrap_or_else(|_| "{}".to_string());
    let salary_json =
        serde_json::to_string_pretty(&client_salary).unwrap_or_else(|_| "null".to_string());

    format!(
        r#"あなたは採用コンサルタントです。以下の実測・観測データから、ペルソナ別の採用カスタマージャーニーと離脱対策を作成してください。

# この診断の目的
- ペルソナを作るだけでなく、求人認知から応募・面接・オファー・入社判断までを再現する。
- 「応募へ進む人」「検索・比較する人」「求人を見て離脱する人」を最低1案ずつ含む、重複しすぎない4案を作る。
- 各ペルソナを企業が採用したいかは最終的にコンサルが選ぶ。employer_fit_hypothesis は仮説に留める。
- ペルソナは年齢・性別・MBTIではなく、転職理由、経験、生活上の制約、優先条件、検索行動で表現する。

# 事実・推定の規律
- 顧客企業について事実と呼べるのは、顧客求人の検証済み事実だけ。
- 競合CSVは、スクレイピング時点の掲載求人の観測。提示された件数・分布を再計算しない。
- 口コミは外部で観測された文面。会社の労働実態とは断定しない。ただし求職者が1件のネガティブ口コミを重く受け取る可能性は残す。
- 公的統計は地域の母集団を説明する補助情報であり、個人の応募意向や人数を保証しない。
- 検索ボリュームはこの後コードで取得する。検索数を作らない。
- 根拠が無い場合は「未確認」「顧客確認が必要」とする。
- 求人にない制度・条件を対策として事実化しない。情報追加で解決する問題と、条件・実態の変更が必要な問題を分ける。
- 「絶対」「必ず」「全員」などの断定を使わない。

# ペルソナの判定
1. eligibility は求人の必須条件から判定する。
2. likely_behavior は求人の条件、競合比較、情報不足、職種一般の不安、口コミを重ねて判定する。
3. 「応募可能性が高い」と「応募へ進む」は別概念。資格上応募可能でも検索・比較で離脱し得る。
4. 求人閲覧段階で離脱するペルソナが企業の採用対象なら、認知・検索・比較・応募転換と応募後の両方を対策する。
5. 企業の対象外なら、無理に引き留めず、ミスマッチ応募だけを減らす。

# 自然検索の網羅
求人・口コミに直接書かれている語だけでなく、職種・業界で一般に確認されやすい「あるある」を basis_type=職種あるある と明示して含める。
各ペルソナに5〜8件、検索段階と検索理由が異なるクエリを作る。会社名が特定できる場合は社名検索と一般検索を混ぜる。
検索軸:
{common_search_axes}

# ジャーニー
各ペルソナについて次の全段階を順番どおり返す:
{stages}

# 顧客求人の検証済み事実
{facts}

# 顧客求人原文（会社名・求人名・職種の判別、および引用確認用）
{client_source}

# 競合求人CSVの決定論的集計と比較用サンプル
{competitor}

# 顧客求人給与の相対位置（コード計算、取得できない場合はnull）
{client_salary}

# 口コミCSVの観測本文
{reviews}

# 公的統計・人流の補助情報
{public_stats}

# 顧客が採りたい人物についての担当者メモ（空なら未確認）
{employer_note}

# 出力上の注意
- condition_findings の数値は入力JSONに存在する値だけを使う。
- review_findings は該当する R番号を source_ref に入れる。
- evidence_refs は verified fact のキー、C番号、R番号、「公的統計」「職種一般仮説」のいずれかで示す。
- priority_actions は、求人票、採用サイト・FAQ、口コミ返信・情報発信、応募フォーム、応募後連絡、面接、オファー、実態・条件変更のいずれで行うか channel に明示する。
- 最終的な採用結論は出さず、コンサルが判断するための仮説と確認事項を返す。"#,
        common_search_axes = COMMON_SEARCH_AXES,
        stages = stages,
        facts = serde_json::to_string_pretty(verified_facts).unwrap_or_else(|_| "{}".to_string()),
        client_source = source_excerpt,
        competitor = competitor_json,
        client_salary = salary_json,
        reviews = reviews_json,
        public_stats =
            serde_json::to_string_pretty(public_stats).unwrap_or_else(|_| "{}".to_string()),
        employer_note = if employer_note.trim().is_empty() {
            "未入力"
        } else {
            employer_note.trim()
        },
    )
}

fn display_location(record: &SurveyRecord) -> String {
    match (
        record.location_parsed.prefecture.as_deref(),
        record.location_parsed.municipality.as_deref(),
    ) {
        (Some(pref), Some(muni)) => format!("{pref} {muni}"),
        _ => truncate_chars(record.location_raw.trim(), 80),
    }
}

fn split_tags(raw: &str) -> Vec<String> {
    raw.split([',', '、', '・'])
        .map(str::trim)
        .filter(|s| !s.is_empty())
        .map(str::to_string)
        .collect()
}

fn looks_like_tag_overflow(tag: &str) -> bool {
    let t = tag.trim();
    t.ends_with('+') && t.trim_end_matches('+').chars().all(|c| c.is_ascii_digit())
}

fn sorted_counts(counts: HashMap<String, usize>, limit: usize) -> Vec<NamedCount> {
    let mut rows = counts
        .into_iter()
        .map(|(name, count)| NamedCount { name, count })
        .collect::<Vec<_>>();
    rows.sort_by(|a, b| b.count.cmp(&a.count).then_with(|| a.name.cmp(&b.name)));
    rows.truncate(limit);
    rows
}

fn distribution(group: &str, mut values: Vec<i64>) -> SalaryDistribution {
    values.sort_unstable();
    let n = values.len();
    SalaryDistribution {
        group: group.to_string(),
        count: n,
        minimum_yen: values[0],
        first_quartile_yen: quantile(&values, 1, 4),
        median_yen: quantile(&values, 1, 2),
        third_quartile_yen: quantile(&values, 3, 4),
        maximum_yen: values[n - 1],
        unit_note: "各求人の給与表記を月給換算。範囲表記は上下限の中点、時給×167時間、日給×21日、週給×4.33。".to_string(),
    }
}

fn quantile(sorted: &[i64], numerator: usize, denominator: usize) -> i64 {
    if sorted.is_empty() {
        return 0;
    }
    let index = (sorted.len() - 1) * numerator / denominator;
    sorted[index]
}

fn same_employment_group(left: &str, right: &str) -> bool {
    fn group(value: &str) -> &'static str {
        if value.contains("パート") || value.contains("アルバイト") {
            "part"
        } else if value.contains("正社員") {
            "regular"
        } else if value.contains("契約") {
            "contract"
        } else if value.contains("派遣") {
            "temporary"
        } else if value.contains("業務委託") {
            "contractor"
        } else {
            "other"
        }
    }
    group(left) == group(right)
}

fn find_header(headers: &[String], candidates: &[&str]) -> Option<usize> {
    headers.iter().position(|header| {
        let normalized = header.trim().to_lowercase();
        candidates
            .iter()
            .any(|candidate| normalized == candidate.to_lowercase())
    })
}

fn truncate_chars(value: &str, limit: usize) -> String {
    let mut chars = value.chars();
    let truncated = chars.by_ref().take(limit).collect::<String>();
    if chars.next().is_some() {
        format!("{truncated}…")
    } else {
        truncated
    }
}

fn sample_indices(total: usize, limit: usize) -> Vec<usize> {
    if total == 0 || limit == 0 {
        return Vec::new();
    }
    if limit == 1 {
        return vec![0];
    }
    if total <= limit {
        return (0..total).collect();
    }
    (0..limit)
        .map(|position| position * (total - 1) / (limit - 1))
        .collect()
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn google_review_csv_without_rating_is_accepted() {
        let csv = "\u{feff}yC3ZMb href,Vpc5Fe,GSM50,y3Ibjb,OA1nbd,uo5PT\n\
https://example.com/a,投稿者A,3 件,4 か月前,安全教育が気になります,❤️1\n\
https://example.com/b,投稿者B,2 件,1 年前,,\n";
        let summary = summarize_review_csv(
            csv.as_bytes(),
            "google-2026-07-31.csv",
            Some("2026-07-31".into()),
        )
        .expect("review csv");
        assert_eq!(summary.total_rows, 2);
        assert_eq!(summary.text_rows, 1);
        assert_eq!(summary.blank_text_rows, 1);
        assert_eq!(summary.evidence[0].source_ref, "R1");
        assert_eq!(summary.captured_at.as_deref(), Some("2026-07-31"));
    }

    #[test]
    fn duplicate_review_text_is_removed_from_llm_evidence() {
        let csv = "OA1nbd,y3Ibjb\n同じ口コミ,1年前\n同じ 口コミ,2年前\n";
        let summary =
            summarize_review_csv(csv.as_bytes(), "reviews.csv", None).expect("review csv");
        assert_eq!(summary.total_rows, 2);
        assert_eq!(summary.text_rows, 1);
        assert_eq!(summary.duplicate_text_rows, 1);
    }

    #[test]
    fn indeed_sp_competitor_csv_uses_existing_parser_and_salary_distribution() {
        let csv = "css-1hwmqh1,css-bxyec3 href,css-bxyec3,css-lx9x6g,css-14qk2ra,css-18rxko3,css-18rxko3 (2),jobsearch-JobCard-tag,css-1vlebyu,css-u74ql7\n\
正社員,https://example.com/1,配送ドライバー,地場配送,会社A,東京都 大田区,月給 300000円,研修あり,仕事内容A,人気\n\
正社員,https://example.com/2,配送ドライバー,日勤,会社B,東京都 大田区,月給 340000円,資格取得支援あり,仕事内容B,超人気\n";
        let summary = summarize_competitor_csv(csv.as_bytes(), "indeed-2026-07-10.csv", None)
            .expect("competitor csv");
        assert_eq!(summary.record_count, 2);
        assert_eq!(summary.raw_row_count, 2);
        assert_eq!(summary.analysis_excluded_rows, 0);
        assert_eq!(summary.unique_company_count, 2);
        assert_eq!(summary.popular_count, 1);
        assert_eq!(summary.super_popular_count, 1);
        let all = summary
            .salary_distributions
            .iter()
            .find(|row| row.group == "全体")
            .expect("overall salary");
        assert_eq!(all.count, 2);
        assert_eq!(all.minimum_yen, 300_000);
        assert_eq!(all.maximum_yen, 340_000);
    }

    #[test]
    fn client_salary_position_prefers_same_employment_group() {
        let summary = CompetitorSummary {
            filename: "x.csv".into(),
            captured_at: None,
            encoding: "UTF-8".into(),
            raw_row_count: 5,
            record_count: 5,
            analysis_excluded_rows: 0,
            unique_company_count: 5,
            unique_url_count: 5,
            employment_types: vec![],
            salary_distributions: vec![
                SalaryDistribution {
                    group: "全体".into(),
                    count: 5,
                    minimum_yen: 150_000,
                    first_quartile_yen: 180_000,
                    median_yen: 220_000,
                    third_quartile_yen: 280_000,
                    maximum_yen: 350_000,
                    unit_note: String::new(),
                },
                SalaryDistribution {
                    group: "正社員".into(),
                    count: 3,
                    minimum_yen: 250_000,
                    first_quartile_yen: 260_000,
                    median_yen: 300_000,
                    third_quartile_yen: 320_000,
                    maximum_yen: 350_000,
                    unit_note: String::new(),
                },
            ],
            top_locations: vec![],
            top_tags: vec![],
            popular_count: 0,
            super_popular_count: 0,
            coverage: vec![],
            briefs: vec![],
            salary_values_by_group: BTreeMap::from([
                (
                    "全体".to_string(),
                    vec![150_000, 180_000, 220_000, 280_000, 350_000],
                ),
                ("正社員".to_string(), vec![250_000, 300_000, 350_000]),
            ]),
        };
        let position =
            client_salary_position("月給31万円", "正社員", &summary).expect("salary position");
        assert_eq!(position.comparison_group, "正社員");
        assert_eq!(position.client_monthly_equivalent_yen, 310_000);
        assert_eq!(position.position_label, "中央値以上・第3四分位以下");
    }

    #[test]
    fn diagnosis_prompt_separates_truth_observation_and_hypothesis() {
        let competitor = CompetitorSummary {
            filename: "c.csv".into(),
            captured_at: None,
            encoding: "UTF-8".into(),
            raw_row_count: 0,
            record_count: 0,
            analysis_excluded_rows: 0,
            unique_company_count: 0,
            unique_url_count: 0,
            employment_types: vec![],
            salary_distributions: vec![],
            top_locations: vec![],
            top_tags: vec![],
            popular_count: 0,
            super_popular_count: 0,
            coverage: vec![],
            briefs: vec![],
            salary_values_by_group: BTreeMap::new(),
        };
        let reviews = ReviewSummary {
            filename: "r.csv".into(),
            captured_at: None,
            encoding: "UTF-8".into(),
            total_rows: 0,
            text_rows: 0,
            blank_text_rows: 0,
            duplicate_text_rows: 0,
            evidence: vec![],
            scope_note: String::new(),
        };
        let prompt = build_diagnosis_prompt(
            "求人原文",
            &json!({}),
            &competitor,
            &reviews,
            None,
            &json!({}),
            "",
        );
        assert!(prompt.contains("顧客求人の検証済み事実だけ"));
        assert!(prompt.contains("職種・業界で一般に確認されやすい"));
        assert!(prompt.contains("求人認知 → 求人閲覧 → 自然検索"));
    }

    #[test]
    fn competitor_brief_sampling_spans_the_full_csv() {
        assert_eq!(sample_indices(3, 40), vec![0, 1, 2]);
        let indices = sample_indices(196, 40);
        assert_eq!(indices.len(), 40);
        assert_eq!(indices.first(), Some(&0));
        assert_eq!(indices.last(), Some(&195));
        assert!(indices.windows(2).all(|pair| pair[0] < pair[1]));
    }
}
