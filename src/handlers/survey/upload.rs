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
    let mut col_map = build_column_map(&headers, &source);

    // ヘッダーマッチが不十分な場合、データ内容ベースの動的検出にフォールバック（GASのdetectColumnsAutomatically移植）
    if col_map.len() < 3 {
        // サンプル行を先に読み取って動的検出
        let data_bytes = if data.starts_with(&[0xEF, 0xBB, 0xBF]) { &data[3..] } else { data };
        let mut sample_rdr = csv::ReaderBuilder::new().flexible(true).has_headers(true).from_reader(data_bytes);
        let sample_rows: Vec<csv::StringRecord> = sample_rdr.records().take(20).filter_map(|r| r.ok()).collect();
        if !sample_rows.is_empty() {
            let detected = detect_columns_from_data(&headers, &sample_rows);
            tracing::info!("Dynamic column detection: {:?}", detected.keys().collect::<Vec<_>>());
            // 検出結果でcol_mapを上書き（ヘッダーマッチより動的検出を優先）
            for (key, idx) in detected {
                col_map.insert(key, idx);
            }
        }
    }

    let mut records = Vec::new();
    let mut skipped_metadata = 0_usize;
    let mut skipped_incomplete = 0_usize;
    for (idx, result) in rdr.records().enumerate() {
        let row = match result {
            Ok(r) => r,
            Err(e) => {
                tracing::warn!("CSV行{}: パースエラー: {e}", idx + 2);
                continue;
            }
        };

        // GAS isMetadataRow() 移植: メタデータ行を除外
        let first_col = row.get(0).unwrap_or("").trim();
        if first_col.is_empty()
            || first_col.contains("この採用企業")
            || first_col.contains("優先条件")
            || first_col.contains("希望する給与")
            || first_col.contains("新しい求人")
            || first_col.contains("この採用企業の")
        {
            skipped_metadata += 1;
            continue;
        }

        let get = |key: &str| -> String {
            col_map.get(key)
                .and_then(|&col_idx| row.get(col_idx))
                .unwrap_or("")
                .trim()
                .to_string()
        };

        let job_title = get("job_title");
        let company_name = get("company_name");

        // GAS cleanDataFromSheet() 移植: タイトルと会社名の両方がない行はスキップ
        if job_title.is_empty() && company_name.is_empty() {
            skipped_incomplete += 1;
            continue;
        }
        // GAS getFirstValidValue() 移植: 行ごとに全列スキャンして最適な値を選択
        // Indeed CSVでは勤務地がcol[3]とcol[13]に分散するため、常に全列を探索
        let location_raw = {
            let mut best_val = String::new();
            let mut best_score = 0_i32;
            // まずcol_mapの列を試す
            let mapped = get("location");
            let mapped_score = score_location(&mapped);
            if mapped_score > 0 { best_val = mapped; best_score = mapped_score; }
            // 全列をスキャンしてより良い値を探す
            if best_score == 0 {
                for ci in 0..row.len() {
                    let val = row.get(ci).unwrap_or("").trim();
                    let s = score_location(val);
                    if s > best_score {
                        best_score = s;
                        best_val = val.to_string();
                    }
                }
            }
            best_val
        };
        let salary_raw = {
            let mapped = get("salary");
            if score_salary(&mapped) > 0 { mapped }
            else {
                let mut best_val = String::new();
                let mut best_score = 0_i32;
                for ci in 0..row.len() {
                    let val = row.get(ci).unwrap_or("").trim();
                    let s = score_salary(val);
                    if s > best_score { best_score = s; best_val = val.to_string(); }
                }
                best_val
            }
        };
        let employment_type = {
            let mapped = get("employment_type");
            if score_employment_type(&mapped) > 0 { mapped }
            else {
                let mut best_val = String::new();
                for ci in 0..row.len() {
                    let val = row.get(ci).unwrap_or("").trim();
                    if score_employment_type(val) > 0 { best_val = val.to_string(); break; }
                }
                best_val
            }
        };
        let employment_type = normalize_employment_type(&employment_type);
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

    tracing::info!(
        "CSV parse: {} records accepted, {} metadata rows skipped, {} incomplete rows skipped",
        records.len(), skipped_metadata, skipped_incomplete
    );

    if records.is_empty() {
        return Err(format!(
            "CSVにデータ行がありません（メタデータ除外{}行、不完全行除外{}行）",
            skipped_metadata, skipped_incomplete
        ));
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

    map
}

/// GASのColumnDetectionPatternsを移植: データ内容ベースの動的カラム検出
/// ヘッダー名ではなくデータの中身をスコアリングして最適な列を自動判定
pub fn detect_columns_from_data(
    headers: &[String],
    sample_rows: &[csv::StringRecord],
) -> std::collections::HashMap<&'static str, usize> {
    let mut scores: std::collections::HashMap<&str, Vec<(usize, i32)>> = std::collections::HashMap::new();
    for key in &["location", "salary", "company_name", "job_title", "url", "employment_type", "is_new"] {
        scores.insert(key, Vec::new());
    }

    let sample_size = sample_rows.len().min(20);
    for row in sample_rows.iter().take(sample_size) {
        for col_idx in 0..row.len().min(headers.len()) {
            let val = row.get(col_idx).unwrap_or("").trim();
            if val.is_empty() { continue; }

            // 勤務地スコア（都道府県パターン）
            let loc_score = score_location(val);
            if loc_score > 0 { scores.get_mut("location").unwrap().push((col_idx, loc_score)); }

            // 給与スコア
            let sal_score = score_salary(val);
            if sal_score > 0 { scores.get_mut("salary").unwrap().push((col_idx, sal_score)); }

            // 会社名スコア
            let comp_score = score_company(val);
            if comp_score > 0 { scores.get_mut("company_name").unwrap().push((col_idx, comp_score)); }

            // URLスコア
            if val.starts_with("http") { scores.get_mut("url").unwrap().push((col_idx, 100)); }

            // 雇用形態スコア
            let emp_score = score_employment_type(val);
            if emp_score > 0 { scores.get_mut("employment_type").unwrap().push((col_idx, emp_score)); }

            // 求人タイトルスコア
            let title_score = score_job_title(val);
            if title_score > 0 { scores.get_mut("job_title").unwrap().push((col_idx, title_score)); }

            // 新着スコア
            if val.contains("新着") || val.contains("NEW") || val.contains("日前") {
                scores.get_mut("is_new").unwrap().push((col_idx, 100));
            }
        }
    }

    // 各フィールドで最高スコアの列を選択
    let mut result = std::collections::HashMap::new();
    let mut used_cols = std::collections::HashSet::new();

    // 優先度順: URL → salary → location → company → employment → title → is_new
    for key in &["url", "salary", "location", "company_name", "employment_type", "job_title", "is_new"] {
        let mut col_totals: std::collections::HashMap<usize, i32> = std::collections::HashMap::new();
        for (col, score) in scores.get(key).unwrap() {
            *col_totals.entry(*col).or_default() += score;
        }
        if let Some((&best_col, &best_score)) = col_totals.iter()
            .filter(|(col, _)| !used_cols.contains(*col))
            .max_by_key(|(_, &score)| score)
        {
            if best_score >= 30 {
                result.insert(*key, best_col);
                used_cols.insert(best_col);
            }
        }
    }

    // タグ: locationでもsalaryでもcompanyでもない列のうち、短いテキスト（<30文字）が多い列
    if !result.contains_key("tags") {
        for col_idx in 0..headers.len() {
            if used_cols.contains(&col_idx) { continue; }
            let h = headers[col_idx].to_lowercase();
            if h.contains("tag") || h.contains("jobsearch-jobcard-tag") {
                result.insert("tags", col_idx);
                break;
            }
        }
    }

    result
}

fn score_location(val: &str) -> i32 {
    if val.starts_with("http") || val.len() > 100 { return 0; }
    let mut score = 0;
    // 都道府県
    if val.contains("都") || val.contains("道") || val.contains("府") || val.contains("県") {
        let prefs = ["北海道","青森","岩手","宮城","秋田","山形","福島","茨城","栃木","群馬",
            "埼玉","千葉","東京","神奈川","新潟","富山","石川","福井","山梨","長野","岐阜",
            "静岡","愛知","三重","滋賀","京都","大阪","兵庫","奈良","和歌山","鳥取","島根",
            "岡山","広島","山口","徳島","香川","愛媛","高知","福岡","佐賀","長崎","熊本",
            "大分","宮崎","鹿児島","沖縄"];
        if prefs.iter().any(|p| val.contains(p)) { score += 50; }
    }
    // 市区町村
    if val.contains("市") || val.contains("区") || val.contains("町") || val.contains("村") { score += 30; }
    score
}

fn score_salary(val: &str) -> i32 {
    if val.starts_with("http") { return 0; }
    let mut score = 0;
    if val.contains("時給") { score += 80; }
    if val.contains("月給") { score += 70; }
    if val.contains("年収") || val.contains("年俸") { score += 70; }
    if val.contains("日給") { score += 60; }
    if val.contains("円") && val.chars().any(|c| c.is_ascii_digit()) { score += 40; }
    if val.contains("万") && val.chars().any(|c| c.is_ascii_digit()) { score += 30; }
    // 住所と混同しないように
    if val.contains("市") || val.contains("区") { score = 0; }
    score
}

fn score_company(val: &str) -> i32 {
    if val.starts_with("http") || val.contains("円") { return 0; }
    let mut score = 0;
    if val.contains("株式会社") || val.contains("有限会社") || val.contains("合同会社") { score += 60; }
    if val.contains("(株)") || val.contains("（株）") { score += 50; }
    if val.contains("法人") || val.contains("医療法人") || val.contains("社会福祉法人") { score += 60; }
    if val.len() >= 3 && val.len() <= 50 && score == 0 { score += 5; } // 短すぎず長すぎない
    score
}

fn score_employment_type(val: &str) -> i32 {
    if val.contains("正社員") || val.contains("契約社員") || val.contains("派遣社員")
        || val.contains("パート") || val.contains("アルバイト") || val.contains("業務委託")
        || val.contains("紹介予定派遣") || val.contains("嘱託") || val.contains("請負") { return 100; }
    0
}

/// GAS EMPLOYMENT_TYPE_MAP 移植: 雇用形態テキストを正規化
fn normalize_employment_type(val: &str) -> String {
    if val.contains("正社員") || val.contains("正職員") { return "正社員".into(); }
    if val.contains("契約社員") || val.contains("嘱託") { return "契約社員".into(); }
    if val.contains("紹介予定派遣") { return "紹介予定派遣".into(); }
    if val.contains("派遣") { return "派遣社員".into(); }
    if val.contains("パート") || val.contains("アルバイト") { return "パート・アルバイト".into(); }
    if val.contains("業務委託") || val.contains("請負") { return "業務委託".into(); }
    val.to_string()
}

fn score_job_title(val: &str) -> i32 {
    if val.starts_with("http") || val.contains("円") { return 0; }
    let keywords = ["エンジニア","デザイナー","営業","事務","経理","スタッフ","ドライバー",
        "看護","介護","製造","加工","販売","接客","マネージャー","担当","募集","オペレーター"];
    let mut score = 0;
    if keywords.iter().any(|k| val.contains(k)) { score += 40; }
    if val.len() >= 5 && val.len() <= 100 { score += 20; }
    score
}
