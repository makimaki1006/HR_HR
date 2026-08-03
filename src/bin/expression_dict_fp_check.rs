//! 表現レビュー辞書の誤検出計測 (2026-08-03)。
//!
//! 目的: `assets/expression_review_rules.json` (19グループ/severity=warning) を
//! **実在の求人テキスト**に当て、ルール別の検出件数と誤検出候補を実測する。
//! 設計時に「誤検出率は実データ未計測」として残っていた課題への回答を作るための計測用バイナリ。
//!
//! LLM は使わない。辞書検出 (`NgRules::detect`) は決定論的なので Gemini/SerpApi は一切呼ばない。
//!
//! 流用している実ロジック (パイロット `appeal_axis_pilot.rs` と同じ流儀):
//! - CSV 解析: `handlers::survey::upload::parse_csv_bytes` (エンコーディング検出込み、本番と同一)
//! - 検出: `job_gen::ng_words::NgRules` (掲載点検と同一エンジン。本バイナリはエンジンを変更しない)
//!
//! 実行:
//! ```text
//! cargo run --release --bin expression_dict_fp_check -- <CSVパス|ディレクトリ>... [--out <JSONパス>]
//! ```
//! 引数を省略した場合は `C:/Users/fuji1/Downloads` 配下の `indeed-*.csv` を対象にする。
//!
//! # 誤検出の機械分類
//!
//! 人手判定を挟まずに済むよう、文字パターンだけで誤検出候補を切り分ける。判定基準:
//!
//! - `C1_命令形ではない活用形`: 命令形ルール (major が「外せ/やめろ/…」、または minor が
//!   「外せ/外してください」) の一致で、命令形語幹の直後1文字が活用語尾
//!   {る れ ら な ま ん ず ば た て} のもの。「外せない」「外せる」「従えば」等は命令ではない。
//! - `C2_測定可能な労働条件の事実記述`: 「なし/ゼロ/皆無/ありません」系の minor を持つルールの
//!   一致で、matched が {残業 夜勤 転勤 休日出勤 持ち込み} で始まるもの。求人条件としての
//!   事実表示であり、表現の当否を問う対象ではない。
//! - 上記以外は `TP候補_要人手確認` (真陽性候補) とする。
//!
//! 分類は標本ではなく**全ヒットに対して**行う。

use rust_dashboard::handlers::survey::upload::{parse_csv_bytes, SurveyRecord};
use rust_dashboard::job_gen::ng_words::NgRules;
use serde_json::{json, Value};
use std::collections::{BTreeMap, HashSet};
use std::path::{Path, PathBuf};

/// 埋め込み表現レビュー辞書 (掲載点検 `job_gen::handlers` と同じアセットを参照)。
const EXPRESSION_RULES_JSON: &str = include_str!("../../assets/expression_review_rules.json");

/// 検出箇所の前後に付ける文脈の文字数。
const CONTEXT_CHARS: usize = 30;

/// 命令形ルールの判定対象 (major 原文 / minor 原文)。
const IMPERATIVE_MAJOR_HEADS: [&str; 2] = ["外せ", "やめろ"];
/// 命令形の語幹。この直後の1文字で命令形か活用形かを判定する。
const IMPERATIVE_STEMS: [&str; 6] = ["外せ", "やめろ", "捨てろ", "従え", "黙れ", "謝れ"];
/// 活用語尾。命令形語幹の直後にこれが来るなら命令形ではない (可能/否定/仮定/丁寧)。
const CONJUGATION_TAILS: [char; 10] = ['る', 'れ', 'ら', 'な', 'ま', 'ん', 'ず', 'ば', 'た', 'て'];
/// 測定可能な労働条件語 (これで始まる「〜なし/ゼロ」は条件表示)。
const MEASURABLE_CONDITIONS: [&str; 5] = ["残業", "夜勤", "転勤", "休日出勤", "持ち込み"];
/// 「無い」ことを述べる minor 原文。
const ZERO_MINORS: [&str; 6] = [
    "なし",
    "ゼロ",
    "皆無",
    "ありません",
    "一切ない/一切ありません",
    "全くない/全くありません",
];

/// 求人1件のレビュー対象テキスト (職種名 + 訴求文 + 説明文)。
fn review_text(r: &SurveyRecord) -> String {
    [
        r.job_title.as_str(),
        r.snippet.as_str(),
        r.description.as_str(),
    ]
    .iter()
    .filter(|s| !s.trim().is_empty())
    .cloned()
    .collect::<Vec<_>>()
    .join(" ")
}

/// `text` 中の `matched` の前後 [`CONTEXT_CHARS`] 文字を含む文脈を返す (文字境界安全)。
/// 併せて matched 直後の1文字を返す (命令形判定に使う)。
fn context_and_next(text: &str, matched: &str) -> (String, Option<char>) {
    let chars: Vec<char> = text.chars().collect();
    let pat: Vec<char> = matched.chars().collect();
    if pat.is_empty() || pat.len() > chars.len() {
        return (matched.to_string(), None);
    }
    let pos = (0..=chars.len() - pat.len()).find(|&i| chars[i..i + pat.len()] == pat[..]);
    match pos {
        Some(i) => {
            let a = i.saturating_sub(CONTEXT_CHARS);
            let b = (i + pat.len() + CONTEXT_CHARS).min(chars.len());
            let next = chars.get(i + pat.len()).copied();
            (chars[a..b].iter().collect(), next)
        }
        None => (matched.to_string(), None),
    }
}

/// 誤検出候補を機械的に分類する。判定基準はモジュール doc コメントを参照。
fn classify(major: &str, minor: &str, matched: &str, next_char: Option<char>) -> &'static str {
    // --- C1: 命令形ルールで、命令形語幹の直後が活用語尾 ---
    let is_imperative_rule =
        IMPERATIVE_MAJOR_HEADS.iter().any(|h| major.contains(h)) || minor.starts_with("外せ");
    if is_imperative_rule {
        for stem in IMPERATIVE_STEMS {
            if let Some(idx) = matched.find(stem) {
                // matched 内で語幹の次に来る文字。無ければ元テキスト側の次文字を見る。
                let after: Option<char> = matched[idx + stem.len()..].chars().next().or(next_char);
                if let Some(c) = after {
                    if CONJUGATION_TAILS.contains(&c) {
                        return "C1_命令形ではない活用形";
                    }
                }
            }
        }
    }
    // --- C2: 「無い」系 minor で、測定可能な労働条件が主語 ---
    if ZERO_MINORS.contains(&minor) && MEASURABLE_CONDITIONS.iter().any(|w| matched.starts_with(w))
    {
        return "C2_測定可能な労働条件の事実記述";
    }
    "TP候補_要人手確認"
}

/// 引数からCSVパスを集める (ディレクトリなら配下の *.csv を走査)。
fn collect_paths(args: &[String]) -> Vec<PathBuf> {
    let mut out = Vec::new();
    for a in args {
        let p = Path::new(a);
        if p.is_dir() {
            if let Ok(rd) = std::fs::read_dir(p) {
                let mut found: Vec<PathBuf> = rd
                    .filter_map(|e| e.ok())
                    .map(|e| e.path())
                    .filter(|p| {
                        p.extension().and_then(|e| e.to_str()).unwrap_or("") == "csv"
                            && p.file_name()
                                .and_then(|n| n.to_str())
                                .unwrap_or("")
                                .starts_with("indeed-")
                    })
                    .collect();
                found.sort();
                out.extend(found);
            }
        } else if p.is_file() {
            out.push(p.to_path_buf());
        } else {
            eprintln!("[warn] 見つからないパスをスキップ: {a}");
        }
    }
    out
}

/// 1ルール(reason/major/minor 単位)の集計。
#[derive(Default)]
struct RuleStat {
    hits: usize,
    rows: HashSet<String>,
    class_counts: BTreeMap<&'static str, usize>,
    examples: Vec<Value>,
}

fn main() -> anyhow::Result<()> {
    let raw: Vec<String> = std::env::args().skip(1).collect();
    let mut out_path = "expression_dict_fp_check_result.json".to_string();
    let mut inputs: Vec<String> = Vec::new();
    let mut i = 0usize;
    while i < raw.len() {
        if raw[i] == "--out" {
            if let Some(v) = raw.get(i + 1) {
                out_path = v.clone();
            }
            i += 2;
        } else {
            inputs.push(raw[i].clone());
            i += 1;
        }
    }
    if inputs.is_empty() {
        inputs.push("C:/Users/fuji1/Downloads".to_string());
    }
    let paths = collect_paths(&inputs);
    if paths.is_empty() {
        anyhow::bail!("対象CSVが1件も見つかりません: {inputs:?}");
    }

    let rules = NgRules::load_from_str(EXPRESSION_RULES_JSON)?;

    let mut total_records = 0usize;
    let mut total_chars = 0usize;
    let mut flagged_rows: HashSet<String> = HashSet::new();
    let mut stats: BTreeMap<(String, String, String), RuleStat> = BTreeMap::new();
    let mut per_file: Vec<Value> = Vec::new();

    for path in &paths {
        let name = path
            .file_name()
            .and_then(|n| n.to_str())
            .unwrap_or("?")
            .to_string();
        let data = std::fs::read(path)?;
        let records = match parse_csv_bytes(&data, None) {
            Ok(r) => r,
            Err(e) => {
                eprintln!("[warn] {name}: パース失敗のためスキップ: {e}");
                continue;
            }
        };
        let mut file_records = 0usize;
        let mut file_chars = 0usize;
        let mut file_flagged = 0usize;

        for (i, r) in records.iter().enumerate() {
            let text = review_text(r);
            if text.trim().is_empty() {
                continue;
            }
            file_records += 1;
            file_chars += text.chars().count();
            let row_id = format!("{name}#{i}");
            let violations = rules.detect(&text);
            if !violations.is_empty() {
                file_flagged += 1;
                flagged_rows.insert(row_id.clone());
            }
            for v in violations {
                let (context, next) = context_and_next(&text, &v.matched);
                let class = classify(&v.major, &v.minor, &v.matched, next);
                let key = (v.reason.clone(), v.major.clone(), v.minor.clone());
                let st = stats.entry(key).or_default();
                st.hits += 1;
                st.rows.insert(row_id.clone());
                *st.class_counts.entry(class).or_default() += 1;
                st.examples.push(json!({
                    "file": name,
                    "record_index": i,
                    "matched": v.matched,
                    "severity": v.severity,
                    "context": context,
                    "class": class,
                }));
            }
        }
        total_records += file_records;
        total_chars += file_chars;
        per_file.push(json!({
            "file": name,
            "records_with_text": file_records,
            "chars": file_chars,
            "flagged_records": file_flagged,
        }));
        eprintln!("[csv] {name}: レビュー対象 {file_records} 件 / {file_chars} 字 / 検出 {file_flagged} 件");
    }

    // ルール別の出力 (件数降順)。
    let mut rule_rows: Vec<Value> = stats
        .iter()
        .map(|((reason, major, minor), st)| {
            json!({
                "reason": reason,
                "major": major,
                "minor": minor,
                "hits": st.hits,
                "records": st.rows.len(),
                "record_percent": (st.rows.len() * 10000 / total_records.max(1)) as f64 / 100.0,
                "class_counts": st.class_counts.iter().map(|(k, v)| (k.to_string(), *v)).collect::<BTreeMap<_, _>>(),
                "examples": st.examples,
            })
        })
        .collect();
    rule_rows.sort_by_key(|v| std::cmp::Reverse(v["hits"].as_u64().unwrap_or(0)));

    let mut grand: BTreeMap<String, usize> = BTreeMap::new();
    for st in stats.values() {
        for (k, v) in &st.class_counts {
            *grand.entry(k.to_string()).or_default() += v;
        }
    }
    let total_hits: usize = stats.values().map(|s| s.hits).sum();

    let result = json!({
        "rules_asset": "assets/expression_review_rules.json (コンパイル時埋め込み)",
        "csv_files": paths.iter().map(|p| p.file_name().and_then(|n| n.to_str()).unwrap_or("?")).collect::<Vec<_>>(),
        "records_with_text": total_records,
        "total_chars": total_chars,
        "flagged_records": flagged_rows.len(),
        "flagged_percent": (flagged_rows.len() * 10000 / total_records.max(1)) as f64 / 100.0,
        "total_hits": total_hits,
        // grand / rule_rows は下のサマリ出力でも読むので参照で渡す。
        "class_totals": &grand,
        "per_file": per_file,
        "rules": &rule_rows,
    });
    std::fs::write(&out_path, serde_json::to_string_pretty(&result)?)?;

    eprintln!("--------------------------------------------------");
    eprintln!(
        "求人 {total_records} 件 / {total_chars} 字 / 検出行 {} 件",
        flagged_rows.len()
    );
    eprintln!("検出 {total_hits} 件の内訳: {grand:?}");
    for v in &rule_rows {
        eprintln!(
            "  {} major={} minor={} hits={} records={}",
            v["reason"], v["major"], v["minor"], v["hits"], v["records"]
        );
    }
    eprintln!("[done] {out_path} に出力");
    Ok(())
}
