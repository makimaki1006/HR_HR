//! 比較母集団 (build_comparison_cohort) を実データで監査する一時的な検査バイナリ。
//!
//! 公開 API (build_comparison_cohort) の判定結果と、同一ロジックを再実装した
//! 選別器の結果を突き合わせ、採用/除外された全レコードを列挙する。
//! CompetitorSummary.briefs は最大40件に間引かれるため、全件列挙には再実装側を使う。
//!
//! 実行:
//!   cargo run --release --bin probe_cohort -- <競合CSV> [<競合CSV2> ...]

use rust_dashboard::handlers::survey::upload::{self, SurveyRecord};
use rust_dashboard::job_gen::journey;
use std::collections::HashSet;

// ---- journey.rs の private ヘルパの再実装 (journey.rs 553-559 / 510-551 / 2276-2302 と同内容) ----

fn normalize_match_text(value: &str) -> String {
    value
        .chars()
        .filter(|character| !character.is_whitespace())
        .flat_map(char::to_lowercase)
        .collect()
}

fn normalize_occupation_keywords(values: &[String]) -> Vec<String> {
    const STOP_WORDS: [&str; 28] = [
        "スタッフ",
        "社員",
        "正社員",
        "職員",
        "店員",
        "パート",
        "アルバイト",
        "仕事",
        "求人",
        "業務",
        "職種",
        "募集",
        "販売",
        "介護",
        "営業",
        "事務",
        "接客",
        "製造",
        "作業",
        "管理",
        "配送",
        "運転",
        "看護",
        "保育",
        "店舗",
        "サービス",
        "サポート",
        "オペレーター",
    ];
    let mut seen = HashSet::new();
    values
        .iter()
        .map(|value| value.trim())
        .filter(|value| value.chars().count() >= 2)
        .filter(|value| !STOP_WORDS.contains(value))
        .filter(|value| seen.insert(normalize_match_text(value)))
        .take(8)
        .map(str::to_string)
        .collect()
}

fn employment_group(value: &str) -> Option<&'static str> {
    if value.contains("パート") || value.contains("アルバイト") {
        Some("part")
    } else if value.contains("正社員") || value.contains("正職員") {
        Some("regular")
    } else if value.contains("契約") || value.contains("嘱託") {
        Some("contract")
    } else if value.contains("派遣") {
        Some("temporary")
    } else if value.contains("業務委託") || value.contains("請負") {
        Some("contractor")
    } else {
        None
    }
}

fn same_employment_group(left: &str, right: &str) -> bool {
    match (employment_group(left), employment_group(right)) {
        (Some(l), Some(r)) => l == r,
        (None, None) => {
            let l = normalize_match_text(left);
            let r = normalize_match_text(right);
            !l.is_empty() && l == r
        }
        _ => false,
    }
}

fn matched_keywords(title: &str, keywords: &[String]) -> Vec<String> {
    let t = normalize_match_text(title);
    keywords
        .iter()
        .filter(|k| t.contains(&normalize_match_text(k)))
        .cloned()
        .collect()
}

fn muni(record: &SurveyRecord) -> String {
    record
        .location_parsed
        .municipality
        .clone()
        .unwrap_or_else(|| "-".into())
}

fn pref(record: &SurveyRecord) -> String {
    record
        .location_parsed
        .prefecture
        .clone()
        .unwrap_or_else(|| "-".into())
}

/// 「雇用形態列が無い CSV で、行のどの列から 正社員 等が拾われたか」を再現する。
/// upload.rs 413-433 のフォールバック走査に相当する説明用の推定。
fn employment_evidence(record: &SurveyRecord) -> String {
    let cols: [(&str, &str); 5] = [
        ("job_title", &record.job_title),
        ("snippet", &record.snippet),
        ("company", &record.company_name),
        ("tags", &record.tags_raw),
        ("description", &record.description),
    ];
    for (name, value) in cols {
        for token in [
            "正社員",
            "契約社員",
            "派遣社員",
            "パート",
            "アルバイト",
            "業務委託",
            "紹介予定派遣",
            "嘱託",
            "請負",
        ] {
            if value.contains(token) {
                return format!("{name}:{token}");
            }
        }
    }
    format!("給与単位推定({:?})", record.salary_parsed.salary_type)
}

struct Case {
    label: &'static str,
    job_title: &'static str,
    occupation: &'static str,
    keywords: &'static [&'static str],
    prefecture: &'static str,
    municipality: &'static str,
    employment_type: &'static str,
}

fn main() -> anyhow::Result<()> {
    let paths: Vec<String> = std::env::args().skip(1).collect();
    if paths.is_empty() {
        anyhow::bail!("使い方: probe_cohort <競合CSV> [<競合CSV2> ...]");
    }

    let cases = [
        Case {
            label: "実測再現(川崎市・中型ドライバー・正社員)",
            job_title: "ルート配送中型ドライバー",
            occupation: "ドライバー",
            keywords: &[
                "大型ドライバー",
                "トラックドライバー",
                "ドライバー",
                "配送",
                "運転手",
            ],
            prefecture: "神奈川県",
            municipality: "川崎市",
            employment_type: "正社員",
        },
        Case {
            label: "キーワード最小(ドライバーのみ)",
            job_title: "ルート配送中型ドライバー",
            occupation: "ドライバー",
            keywords: &["ドライバー"],
            prefecture: "神奈川県",
            municipality: "川崎市",
            employment_type: "正社員",
        },
        Case {
            label: "雇用形態=契約社員",
            job_title: "ルート配送中型ドライバー",
            occupation: "ドライバー",
            keywords: &[
                "大型ドライバー",
                "トラックドライバー",
                "ドライバー",
                "配送",
                "運転手",
            ],
            prefecture: "神奈川県",
            municipality: "川崎市",
            employment_type: "契約社員",
        },
        Case {
            label: "雇用形態=パート・アルバイト",
            job_title: "ルート配送中型ドライバー",
            occupation: "ドライバー",
            keywords: &[
                "大型ドライバー",
                "トラックドライバー",
                "ドライバー",
                "配送",
                "運転手",
            ],
            prefecture: "神奈川県",
            municipality: "川崎市",
            employment_type: "パート・アルバイト",
        },
    ];

    for path in &paths {
        let bytes = std::fs::read(path)?;
        let name = std::path::Path::new(path)
            .file_name()
            .and_then(|n| n.to_str())
            .unwrap_or("input.csv")
            .to_string();

        let records = upload::parse_csv_bytes(&bytes, None).map_err(|e| anyhow::anyhow!(e))?;
        let (_, encoding) = upload::decode_csv_bytes(&bytes);

        println!("################ FILE: {name} ################");
        println!("encoding={encoding} parsed_records={}", records.len());

        // 雇用形態の分布 (パーサが付けた値)
        let mut emp_counts: std::collections::BTreeMap<String, usize> = Default::default();
        for r in &records {
            *emp_counts
                .entry(if r.employment_type.trim().is_empty() {
                    "(空)".to_string()
                } else {
                    r.employment_type.clone()
                })
                .or_default() += 1;
        }
        println!("---- employment_type 分布 ----");
        for (k, v) in &emp_counts {
            println!("  {k}\t{v}");
        }

        // 市区町村分布
        let mut muni_counts: std::collections::BTreeMap<String, usize> = Default::default();
        for r in &records {
            *muni_counts
                .entry(format!("{} / {}", pref(r), muni(r)))
                .or_default() += 1;
        }
        println!("---- prefecture/municipality 分布 ----");
        for (k, v) in &muni_counts {
            println!("  {k}\t{v}");
        }

        for case in &cases {
            println!("\n================ CASE: {} ================", case.label);
            let kw_owned: Vec<String> = case.keywords.iter().map(|s| s.to_string()).collect();

            // --- 公開 API ---
            let (cohort, summary) = journey::build_comparison_cohort(
                &bytes,
                &name,
                Some("2026-07-10".into()),
                case.job_title,
                case.occupation,
                &kw_owned,
                case.prefecture,
                case.municipality,
                case.employment_type,
                "",
            )
            .map_err(|e| anyhow::anyhow!(e))?;
            println!("[API] {}", serde_json::to_string(&cohort)?);
            println!(
                "[API] summary.record_count={}",
                summary.as_ref().map(|s| s.record_count).unwrap_or(0)
            );

            // --- 再実装による全件列挙 ---
            let client_title = format!("{} {}", case.job_title, case.occupation);
            let category = rust_dashboard::job_gen::knowledge::classify_job_title(&client_title);
            println!("[REPLICA] classify_job_title(\"{client_title}\") = {category}");
            let mut raw = kw_owned.clone();
            if category != "その他" {
                raw.push(category.clone());
            }
            let keywords = normalize_occupation_keywords(&raw);
            println!("[REPLICA] keywords = {keywords:?}");

            let title_hits: Vec<&SurveyRecord> = records
                .iter()
                .filter(|r| !matched_keywords(&r.job_title, &keywords).is_empty())
                .collect();
            let occ: Vec<&SurveyRecord> = title_hits
                .iter()
                .copied()
                .filter(|r| same_employment_group(&r.employment_type, case.employment_type))
                .collect();
            let muni_hits: Vec<&SurveyRecord> = occ
                .iter()
                .copied()
                .filter(|r| {
                    r.location_parsed
                        .municipality
                        .as_deref()
                        .map(normalize_match_text)
                        == Some(normalize_match_text(case.municipality))
                })
                .collect();
            let pref_hits: Vec<&SurveyRecord> = occ
                .iter()
                .copied()
                .filter(|r| {
                    r.location_parsed
                        .prefecture
                        .as_deref()
                        .map(normalize_match_text)
                        == Some(normalize_match_text(case.prefecture))
                })
                .collect();
            println!(
                "[REPLICA] title_hit={} +employment={} muni={} pref={}",
                title_hits.len(),
                occ.len(),
                muni_hits.len(),
                pref_hits.len()
            );
            let selected: Vec<&SurveyRecord> = if muni_hits.len() >= 5 {
                muni_hits.clone()
            } else {
                pref_hits.clone()
            };
            println!(
                "[CHECK] API matched={} REPLICA selected={} -> {}",
                cohort.matched_record_count,
                selected.len(),
                if cohort.matched_record_count == selected.len() {
                    "一致"
                } else {
                    "不一致"
                }
            );

            println!("---- 採用された全レコード ----");
            println!("row\tkeyword\ttitle\tcompany\tlocation_raw\tpref\tmuni\temp_type\temp_evidence\tsalary");
            for r in &selected {
                println!(
                    "{}\t{}\t{}\t{}\t{}\t{}\t{}\t{}\t{}\t{}",
                    r.row_index + 1,
                    matched_keywords(&r.job_title, &keywords).join("|"),
                    r.job_title.replace('\t', " "),
                    r.company_name.replace('\t', " "),
                    r.location_raw.replace('\t', " "),
                    pref(r),
                    muni(r),
                    r.employment_type,
                    employment_evidence(r),
                    r.salary_raw.replace('\t', " ")
                );
            }

            // 除外理由の内訳: 職種ヒットしたが落ちた行
            println!("---- 職種キーワードは当たったが除外された行 ----");
            println!("row\treason\ttitle\tlocation_raw\tpref\tmuni\temp_type\temp_evidence");
            let selected_rows: HashSet<usize> = selected.iter().map(|r| r.row_index).collect();
            for r in &title_hits {
                if selected_rows.contains(&r.row_index) {
                    continue;
                }
                let reason = if !same_employment_group(&r.employment_type, case.employment_type) {
                    "雇用形態不一致"
                } else if muni_hits.len() >= 5 {
                    "市区町村不一致"
                } else {
                    "都道府県不一致"
                };
                println!(
                    "{}\t{}\t{}\t{}\t{}\t{}\t{}\t{}",
                    r.row_index + 1,
                    reason,
                    r.job_title.replace('\t', " "),
                    r.location_raw.replace('\t', " "),
                    pref(r),
                    muni(r),
                    r.employment_type,
                    employment_evidence(r)
                );
            }
        }

        // 偽陰性調査: 職種キーワードに当たらなかったが、タイトルに運送系の語を含む行
        println!("\n================ 偽陰性候補 (広義ドライバー語をタイトルに含むがキーワード非該当) ================");
        let broad = [
            "ドライバー",
            "運転手",
            "運転士",
            "乗務員",
            "配送",
            "配達",
            "トラック",
            "トレーラー",
            "ダンプ",
            "セールスドライバー",
            "4t",
            "4t",
            "10t",
            "中型",
            "大型",
            "けん引",
            "牽引",
            "デリバリー",
            "宅配",
            "運送",
            "輸送",
        ];
        let narrow: Vec<String> = [
            "大型ドライバー",
            "トラックドライバー",
            "ドライバー",
            "運転手",
        ]
        .iter()
        .map(|s| s.to_string())
        .collect();
        println!("row\thit_broad\ttitle\tpref\tmuni\temp_type");
        for r in &records {
            let b = matched_keywords(
                &r.job_title,
                &broad.iter().map(|s| s.to_string()).collect::<Vec<_>>(),
            );
            let n = matched_keywords(&r.job_title, &narrow);
            if !b.is_empty() && n.is_empty() {
                println!(
                    "{}\t{}\t{}\t{}\t{}\t{}",
                    r.row_index + 1,
                    b.join("|"),
                    r.job_title.replace('\t', " "),
                    pref(r),
                    muni(r),
                    r.employment_type
                );
            }
        }
    }

    Ok(())
}
