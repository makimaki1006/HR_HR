//! CSVアップロード処理
//! Indeed/求人ボックスのCSVを解析してSurveyRecordに変換

use serde::Serialize;

use super::salary_parser::{parse_salary, ParsedSalary, SalaryType};
use super::location_parser::{parse_location, ParsedLocation};

// ======== CSVソース ========

#[derive(Debug, Clone, PartialEq, Serialize)]
pub enum CsvSource {
    Indeed,
    JobBox,
    Unknown,
}

// ======== レコード型 ========

#[derive(Debug, Clone, Serialize)]
pub struct SurveyRecord {
    pub row_index: usize,
    pub source: CsvSource,
    pub job_title: String,
    pub company_name: String,
    pub location_raw: String,
    pub salary_raw: String,
    pub employment_type: String,
    pub tags_raw: String,
    pub url: Option<String>,
    pub is_new: bool,
    // パース結果
    pub salary_parsed: ParsedSalary,
    pub location_parsed: ParsedLocation,
}

// ======== CSVヘッダー検出 ========

/// ヘッダーからCSVソースを自動判定
pub fn detect_csv_source(headers: &[String]) -> CsvSource {
    let header_str = headers.join(",").to_lowercase();
    // 求人ボックス: 「企業名」「所在地」「賃金」
    if header_str.contains("企業名") || header_str.contains("所在地") || header_str.contains("求人ボックス") {
        return CsvSource::JobBox;
    }
    // Indeed: 「会社名」「勤務地」「給与」or CSSクラス名ベース（スクレイピングツール出力）
    if header_str.contains("会社名") || header_str.contains("勤務地") || header_str.contains("indeed") {
        return CsvSource::Indeed;
    }
    // Indeed CSSクラス名ベース: jcs-JobTitle, jobsearch-JobCard-tag
    if header_str.contains("jcs-jobtitle") || header_str.contains("jobsearch-jobcard") {
        return CsvSource::Indeed;
    }
    CsvSource::Unknown
}

// ======== CSVパース ========

/// CSVバイト列をパースしてSurveyRecordのVecに変換
pub fn parse_csv_bytes(data: &[u8], context_pref: Option<&str>) -> Result<Vec<SurveyRecord>, String> {
    // BOM除去
    let data = if data.starts_with(&[0xEF, 0xBB, 0xBF]) { &data[3..] } else { data };

    let mut rdr = csv::ReaderBuilder::new()
        .flexible(true)
        .has_headers(true)
        .from_reader(data);

    let headers: Vec<String> = rdr.headers()
        .map_err(|e| format!("ヘッダー読み取りエラー: {e}"))?
        .iter().map(|s| s.to_string()).collect();

    let source = detect_csv_source(&headers);
    let col_map = build_column_map(&headers, &source);

    let mut records = Vec::new();
    for (idx, result) in rdr.records().enumerate() {
        let row = match result {
            Ok(r) => r,
            Err(e) => {
                tracing::warn!("CSV行{}: パースエラー: {e}", idx + 2);
                continue;
            }
        };

        let get = |key: &str| -> String {
            col_map.get(key)
                .and_then(|&col_idx| row.get(col_idx))
                .unwrap_or("")
                .trim()
                .to_string()
        };

        let job_title = get("job_title");
        let company_name = get("company_name");
        let location_raw = get("location");
        let salary_raw = get("salary");
        let employment_type = get("employment_type");
        let mut tags_raw = get("tags");
        // IndeedのCSVはタグが複数カラムに分散: jobsearch-JobCard-tag, (2), (3)...
        // col_mapのtags以降の連続タグカラムを結合
        for ci in 4..12 {
            if ci < row.len() {
                let val = row.get(ci).unwrap_or("").trim();
                if !val.is_empty() && !tags_raw.contains(val) && val.len() < 30 {
                    // ヘッダーがjobsearch-JobCard-tagの場合のみ
                    if ci < headers.len() && headers[ci].contains("jobsearch-JobCard-tag") {
                        if !tags_raw.is_empty() { tags_raw.push(','); }
                        tags_raw.push_str(val);
                    }
                }
            }
        }
        let url = {
            let u = get("url");
            if u.is_empty() { None } else { Some(u) }
        };
        let is_new = {
            let v = get("is_new");
            v.contains("新着") || v.contains("NEW") || v.contains("new")
        };

        // パース
        let salary_parsed = parse_salary(&salary_raw, SalaryType::Monthly);
        let location_parsed = parse_location(&location_raw, context_pref);

        records.push(SurveyRecord {
            row_index: idx,
            source: source.clone(),
            job_title,
            company_name,
            location_raw,
            salary_raw,
            employment_type,
            tags_raw,
            url,
            is_new,
            salary_parsed,
            location_parsed,
        });
    }

    if records.is_empty() {
        return Err("CSVにデータ行がありません".to_string());
    }

    Ok(records)
}

/// ヘッダー名→カラムインデックスのマッピング構築
fn build_column_map(headers: &[String], source: &CsvSource) -> std::collections::HashMap<&'static str, usize> {
    let mut map = std::collections::HashMap::new();

    for (i, h) in headers.iter().enumerate() {
        let h = h.trim();
        match source {
            CsvSource::Indeed => {
                // 日本語ヘッダー
                if h.contains("求人名") || h.contains("職種名") || h.contains("タイトル") || h == "title" { map.insert("job_title", i); }
                if h.contains("会社名") || h.contains("企業") || h == "company" { map.insert("company_name", i); }
                if h.contains("勤務地") || h.contains("所在地") || h == "location" { map.insert("location", i); }
                if h.contains("給与") || h.contains("年収") || h.contains("月給") || h == "salary" { map.insert("salary", i); }
                if h.contains("雇用") || h.contains("形態") || h == "type" { map.insert("employment_type", i); }
                if h.contains("タグ") || h.contains("特徴") || h == "tags" { map.insert("tags", i); }
                if h.contains("URL") || h.contains("url") || h.contains("リンク") { map.insert("url", i); }
                if h.contains("新着") || h.contains("NEW") { map.insert("is_new", i); }
                // IndeedスクレイピングツールのCSSクラス名ベースヘッダー
                // カラム順: URL, 求人名, 会社名, 勤務地, タグ×7, 給与, 雇用形態, ...
                if h == "jcs-JobTitle href" { map.insert("url", i); }
                if h == "jcs-JobTitle" && !map.contains_key("job_title") { map.insert("job_title", i); }
                if h == "css-19eicqx" { map.insert("company_name", i); }
                if h == "css-1f06pz4" { map.insert("location", i); }
                if h == "css-zydy3i" && !map.contains_key("salary") { map.insert("salary", i); }
                if h == "css-zydy3i (2)" { map.insert("employment_type", i); }
                if h.starts_with("jobsearch-JobCard-tag") && !map.contains_key("tags") { map.insert("tags", i); }
            }
            CsvSource::JobBox => {
                if h.contains("職種") || h.contains("求人名") { map.insert("job_title", i); }
                if h.contains("企業名") || h.contains("会社名") { map.insert("company_name", i); }
                if h.contains("所在地") || h.contains("勤務地") { map.insert("location", i); }
                if h.contains("賃金") || h.contains("給与") || h.contains("年収") { map.insert("salary", i); }
                if h.contains("雇用") || h.contains("就業形態") { map.insert("employment_type", i); }
                if h.contains("特徴") || h.contains("タグ") || h.contains("こだわり") { map.insert("tags", i); }
                if h.contains("URL") || h.contains("url") { map.insert("url", i); }
                if h.contains("新着") { map.insert("is_new", i); }
            }
            CsvSource::Unknown => {
                // 汎用マッチ
                let hl = h.to_lowercase();
                if hl.contains("title") || h.contains("職種") || h.contains("求人") { map.insert("job_title", i); }
                if hl.contains("company") || h.contains("会社") || h.contains("企業") { map.insert("company_name", i); }
                if hl.contains("location") || h.contains("勤務地") || h.contains("所在地") || h.contains("住所") { map.insert("location", i); }
                if hl.contains("salary") || h.contains("給与") || h.contains("賃金") || h.contains("年収") { map.insert("salary", i); }
                if hl.contains("type") || h.contains("雇用") { map.insert("employment_type", i); }
                if hl.contains("tag") || h.contains("タグ") || h.contains("特徴") { map.insert("tags", i); }
                if hl.contains("url") { map.insert("url", i); }
            }
        }
    }

    // フォールバック: ヘッダーマッチが少ない場合、IndeedのCSSクラス名パターンで位置推定
    if map.len() < 3 && headers.len() >= 14 {
        let h0 = headers[0].trim();
        if h0.contains("href") || h0.contains("indeed.com") || h0.contains("jcs-JobTitle") {
            // Indeed scraping pattern: URL, title, company, location, tags..., salary, emp_type
            if !map.contains_key("url") { map.insert("url", 0); }
            if !map.contains_key("job_title") { map.insert("job_title", 1); }
            if !map.contains_key("company_name") { map.insert("company_name", 2); }
            if !map.contains_key("location") { map.insert("location", 3); }
            if !map.contains_key("tags") { map.insert("tags", 4); }
            // 給与と雇用形態: 「円」「万」を含むカラムを探す
            for i in 10..headers.len().min(20) {
                if !map.contains_key("salary") {
                    // 最初の行のデータは見れないのでヘッダー名で判定
                    let hdr = headers[i].trim();
                    if hdr.contains("zydy3i") || hdr.contains("salary") || hdr.contains("給与") {
                        map.insert("salary", i);
                    }
                }
            }
            // 給与の次が雇用形態
            if let Some(&sal_idx) = map.get("salary") {
                if sal_idx + 1 < headers.len() && !map.contains_key("employment_type") {
                    map.insert("employment_type", sal_idx + 1);
                }
            }
        }
    }

    map
}
