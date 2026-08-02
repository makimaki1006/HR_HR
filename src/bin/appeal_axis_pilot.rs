//! 訴求軸分類パイロット (2026-07-25)。
//!
//! 目的: 「採用提案パッケージ」モックの STEP2 (競合の言葉分析) に載せる数値を、
//! **手作りのダミーではなく実ロジックの実行結果**で出す (ユーザー指示: イメージだけだと
//! 実装着手時にずれるため)。ここで流用した関数がそのまま本実装の部品になる。
//!
//! 流用している実ロジック:
//! - CSV 解析: `handlers::survey::upload::parse_csv_bytes` (エンコーディング検出込み、本番と同一)
//! - タグ集計・タグ×給与: `handlers::survey::aggregator::aggregate_records`
//!   (サニタイズ + IQR 母集団統一済みの監査済みロジック)
//! - 訴求文分類: `media_engine::keyword_cluster` の response_schema / merge
//!   (LLM は振り分けのみ。捏造 hallucinated / 漏れ unassigned を構造でカウント)
//! - NG 検査: `job_gen::ng_words::NgRules` (埋め込み 50 ルール、掲載点検と同一)
//!
//! 実行: `cargo run --release --bin appeal_axis_pilot -- <CSVパス> [出力JSONパス]`
//! 必要 env: GEMINI_API_KEY (未設定なら分類ステップをスキップして他だけ出す)

use rust_dashboard::handlers::survey::aggregator::aggregate_records;
use rust_dashboard::handlers::survey::upload::parse_csv_bytes;
use rust_dashboard::job_gen::ng_words::NgRules;
use rust_dashboard::media_engine::{config, gemini, keyword_cluster};
use serde_json::{json, Value};
use std::collections::HashMap;

/// 訴求軸カテゴリ (本実装でもこの一覧を正とする想定のたたき台)。
fn appeal_categories() -> Vec<String> {
    [
        "給与・手当",
        "休日・休暇",
        "勤務時間・残業の少なさ",
        "職場の雰囲気・人間関係",
        "教育・資格支援",
        "仕事内容",
        "安定性・企業規模",
        "通勤・立地",
        "福利厚生",
        "その他",
    ]
    .iter()
    .map(|s| s.to_string())
    .collect()
}

/// 頻出訴求語の観測リスト (コードのみで部分一致カウント)。
const COMMON_PHRASES: [&str; 8] = [
    "アットホーム",
    "未経験歓迎",
    "未経験OK",
    "高収入",
    "月収35万",
    "駅チカ",
    "土日祝休み",
    "残業なし",
];

/// 空白地帯の観測パターン (明記している競合が少ないと想定される訴求)。
const WHITESPACE_PATTERNS: [(&str, &[&str]); 3] = [
    (
        "残業時間の具体値",
        &["残業月", "残業時間", "残業は月", "残業10", "残業20"],
    ),
    (
        "帰宅・帰庫時刻の目安",
        &["帰庫", "帰社", "に帰れ", "時退社", "時帰"],
    ),
    (
        "直行直帰・日帰り",
        &["直行直帰", "日帰り", "地場のみ", "泊まりなし", "車中泊なし"],
    ),
];

/// 分類プロンプト (keyword_cluster::build_prompt と同じ構造・同じ出力契約で、
/// 対象をキーワードから訴求文に置き換えたもの。ID を返させて merge で厳密照合する)。
fn build_appeal_prompt(items: &[(String, String)], categories: &[String]) -> String {
    let cat_list = categories.join("\n- ");
    let item_list = items
        .iter()
        .map(|(id, text)| format!("{id}: {text}"))
        .collect::<Vec<_>>()
        .join("\n");
    format!(
        "あなたは求人広告の分析者です。以下の求人の訴求文を、指定カテゴリに振り分けてください。\n\
\n\
# ルール\n\
- 各訴求文を、その文が最も強く訴求している軸のカテゴリ1つに割り当てる。\n\
- assignments の keyword フィールドには**行頭の ID (例: R012) だけ**をそのまま入れる。訴求文は書かない。\n\
- 与えられた ID 以外を出力しない。全ての ID を必ずどれかのカテゴリに割り当てる。\n\
- 判断に迷う場合は「その他」に入れる。新しいカテゴリを作らない。\n\
\n\
# カテゴリ一覧\n\
- {cat_list}\n\
\n\
# 訴求文一覧\n\
{item_list}\n"
    )
}

#[tokio::main]
async fn main() -> anyhow::Result<()> {
    let args: Vec<String> = std::env::args().collect();
    let csv_path = args
        .get(1)
        .cloned()
        .unwrap_or_else(|| "C:/Users/fuji1/Downloads/indeed-2026-07-10.csv".to_string());
    let out_path = args
        .get(2)
        .cloned()
        .unwrap_or_else(|| "appeal_axis_pilot_result.json".to_string());

    // ── 1. 実パーサーで CSV 解析 ──
    let data = std::fs::read(&csv_path)?;
    let records = parse_csv_bytes(&data, None).map_err(|e| anyhow::anyhow!(e))?;
    eprintln!("[1] parse_csv_bytes: {} 件", records.len());

    // ── 2. 実集計 (タグ分布・タグ×給与は監査済みロジックがそのまま出す) ──
    let agg = aggregate_records(&records);
    let top_tags: Vec<Value> = agg
        .by_tags
        .iter()
        .take(12)
        .map(|(t, n)| json!({"tag": t, "count": n}))
        .collect();
    let tag_salary: Vec<Value> = agg
        .by_tag_salary
        .iter()
        .take(10)
        .map(|t| {
            json!({
                "tag": t.tag, "count": t.count, "avg_salary": t.avg_salary,
                "diff_from_avg": t.diff_from_avg, "diff_percent": t.diff_percent,
            })
        })
        .collect();
    eprintln!(
        "[2] aggregate_records: tags={} tag_salary={}",
        agg.by_tags.len(),
        agg.by_tag_salary.len()
    );

    // ── 3. 頻出語・空白地帯 (コードのみ・部分一致) ──
    let text_of = |r: &rust_dashboard::handlers::survey::upload::SurveyRecord| {
        format!("{} {} {}", r.job_title, r.snippet, r.description)
    };
    let total = records.len().max(1);
    let common: Vec<Value> = COMMON_PHRASES
        .iter()
        .map(|p| {
            let n = records.iter().filter(|r| text_of(r).contains(p)).count();
            json!({"phrase": p, "count": n, "percent": (n * 100) as f64 / total as f64})
        })
        .collect();
    let whitespace: Vec<Value> = WHITESPACE_PATTERNS
        .iter()
        .map(|(label, pats)| {
            let n = records
                .iter()
                .filter(|r| {
                    let t = text_of(r);
                    pats.iter().any(|p| t.contains(p))
                })
                .count();
            json!({"label": label, "count": n, "percent": (n * 100) as f64 / total as f64})
        })
        .collect();
    eprintln!("[3] 頻出語/空白地帯カウント完了");

    // ── 4. NG 検査 (埋め込み 50 ルール、掲載点検と同一の detect) ──
    let ng = NgRules::load_from_str(include_str!("../../assets/ng_words.json"))?;
    let mut flagged = 0usize;
    let mut reason_map: HashMap<String, usize> = HashMap::new();
    for r in &records {
        let vs = ng.detect(&text_of(r));
        if !vs.is_empty() {
            flagged += 1;
            for v in vs {
                *reason_map.entry(v.reason).or_default() += 1;
            }
        }
    }
    let mut ng_reasons: Vec<(String, usize)> = reason_map.into_iter().collect();
    ng_reasons.sort_by(|a, b| b.1.cmp(&a.1));
    let ng_out: Vec<Value> = ng_reasons
        .iter()
        .take(6)
        .map(|(r, n)| json!({"reason": r, "count": n}))
        .collect();
    eprintln!("[4] NG検査: flagged={flagged}/{}", records.len());

    // ── 5. 訴求軸分類 (Gemini は振り分けのみ。merge ガードで捏造/漏れをカウント) ──
    let key = config::gemini_api_key();
    let mut axis_counts: HashMap<String, usize> = HashMap::new();
    let mut unassigned_total = 0usize;
    let mut hallucinated_total = 0usize;
    let mut classified_total = 0usize;
    if key.is_empty() {
        eprintln!("[5] GEMINI_API_KEY 未設定のため分類をスキップ");
    } else {
        // 訴求文 = snippet 優先、無ければタイトル。空は除外。ID で厳密照合する。
        let items: Vec<(String, String)> = records
            .iter()
            .enumerate()
            .filter_map(|(i, r)| {
                let text = if !r.snippet.trim().is_empty() {
                    r.snippet.trim().to_string()
                } else if !r.job_title.trim().is_empty() {
                    r.job_title.trim().to_string()
                } else {
                    return None;
                };
                let short: String = text.chars().take(120).collect();
                Some((format!("R{i:03}"), short))
            })
            .collect();
        let categories = appeal_categories();
        let schema = keyword_cluster::response_schema();
        let model = config::gemini_model();
        for chunk in items.chunks(50) {
            let prompt = build_appeal_prompt(chunk, &categories);
            let v = gemini::generate_json(&prompt, Some(&schema), &key, &model, 0.0).await?;
            // merge は ID 文字列で厳密照合 → 与えていない ID は hallucinated、
            // 返ってこなかった ID は unassigned として構造的に検出される。
            let source: Vec<(String, Option<i64>)> =
                chunk.iter().map(|(id, _)| (id.clone(), None)).collect();
            let merged = keyword_cluster::merge(&v, &source);
            unassigned_total += merged
                .get("unassigned_count")
                .and_then(Value::as_u64)
                .unwrap_or(0) as usize;
            hallucinated_total += merged
                .get("hallucinated_count")
                .and_then(Value::as_u64)
                .unwrap_or(0) as usize;
            if let Some(cats) = merged.get("categories").and_then(Value::as_array) {
                for c in cats {
                    // merge の出力契約はカテゴリ名 = "name" フィールド (keyword_cluster.rs)。
                    let name = c.get("name").and_then(Value::as_str).unwrap_or("その他");
                    let n = c
                        .get("keywords")
                        .and_then(Value::as_array)
                        .map(|a| a.len())
                        .unwrap_or(0);
                    if n > 0 {
                        *axis_counts.entry(name.to_string()).or_default() += n;
                        classified_total += n;
                    }
                }
            }
            eprintln!("[5] chunk 分類完了 ({} 件)", chunk.len());
        }
    }
    let mut axis_sorted: Vec<(String, usize)> = axis_counts.into_iter().collect();
    axis_sorted.sort_by(|a, b| b.1.cmp(&a.1));
    let axis_out: Vec<Value> = axis_sorted
        .iter()
        .map(|(c, n)| json!({"category": c, "count": n, "percent": (*n * 100) as f64 / classified_total.max(1) as f64}))
        .collect();

    let result = json!({
        "csv_path": csv_path,
        "records_total": records.len(),
        "top_tags": top_tags,
        "tag_salary": tag_salary,
        "salary_mean_overall": agg.by_tag_salary.first().map(|t| t.avg_salary - t.diff_from_avg),
        "common_phrases": common,
        "whitespace": whitespace,
        "ng_flagged_records": flagged,
        "ng_reasons": ng_out,
        "appeal_axis": axis_out,
        "appeal_classified_total": classified_total,
        "appeal_unassigned": unassigned_total,
        "appeal_hallucinated": hallucinated_total,
    });
    std::fs::write(&out_path, serde_json::to_string_pretty(&result)?)?;
    eprintln!("[done] {} に出力", out_path);
    Ok(())
}
