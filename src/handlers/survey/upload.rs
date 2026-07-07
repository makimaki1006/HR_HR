//! CSVアップロード処理
//! Indeed/求人ボックスのCSVを解析してSurveyRecordに変換

use serde::Serialize;

use super::location_parser::{parse_location, ParsedLocation};
use super::salary_parser::{parse_salary, ParsedSalary, SalaryType};

// ======== CSVソース ========

#[derive(Debug, Clone, PartialEq, Serialize)]
pub enum CsvSource {
    Indeed,
    /// 2026-06-30 追加: Indeed スマホ版スクレイピング (年間休日 + 人気タグ取得可)
    IndeedSp,
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
    pub description: String,
    // パース結果
    pub salary_parsed: ParsedSalary,
    pub location_parsed: ParsedLocation,
    pub annual_holidays: Option<i64>,
}

// ======== CSVヘッダー検出 ========

/// ヘッダーからCSVソースを自動判定
pub fn detect_csv_source(headers: &[String]) -> CsvSource {
    let header_str = headers.join(",").to_lowercase();
    // 2026-06-30 追加: Indeed (SP) スマホ版を最優先で判定
    // 固有 CSS クラス: css-u74ql7 (人気/超人気タグ列) は Indeed SP のみが出力。
    // フォールバック: css-bxyec3 (求人タイトル) + css-1vlebyu (description) の組み合わせも SP 固有。
    // 注意: 既存 Indeed (PC) は jcs-jobtitle / css-19eicqx 等の別 CSS クラスを使うため衝突しない。
    if header_str.contains("css-u74ql7")
        || (header_str.contains("css-bxyec3") && header_str.contains("css-1vlebyu"))
    {
        return CsvSource::IndeedSp;
    }
    // 求人ボックス: CSSクラス名ベースのヘッダー検出（GASのp-result_name, p-result_company, c-icon相当）
    if header_str.contains("p-result") || header_str.contains("c-icon") {
        return CsvSource::JobBox;
    }
    // 求人ボックス: 「企業名」「所在地」「賃金」
    if header_str.contains("企業名")
        || header_str.contains("所在地")
        || header_str.contains("求人ボックス")
    {
        return CsvSource::JobBox;
    }
    // Indeed: 「会社名」「勤務地」「給与」or CSSクラス名ベース（スクレイピングツール出力）
    if header_str.contains("会社名")
        || header_str.contains("勤務地")
        || header_str.contains("indeed")
    {
        return CsvSource::Indeed;
    }
    // Indeed CSSクラス名ベース: jcs-JobTitle, jobsearch-JobCard-tag
    if header_str.contains("jcs-jobtitle") || header_str.contains("jobsearch-jobcard") {
        return CsvSource::Indeed;
    }
    CsvSource::Unknown
}

// ======== CSVパース ========

/// ユーザー明示指定の給与単位 (UI の「給与単位」選択に対応)
#[derive(Debug, Clone, Copy, PartialEq)]
pub enum WageMode {
    /// 月給ベース（正社員・契約社員中心想定、時給レコードは月給換算 x160）
    Monthly,
    /// 時給ベース（パート・アルバイト中心想定、月給レコードは時給換算 /160）
    Hourly,
    /// 自動判定（従来ロジック互換: 全体多数派）
    Auto,
}

impl WageMode {
    pub fn from_str(s: &str) -> Self {
        match s {
            "monthly" => Self::Monthly,
            "hourly" => Self::Hourly,
            _ => Self::Auto,
        }
    }
    pub fn label(&self) -> &'static str {
        match self {
            Self::Monthly => "月給",
            Self::Hourly => "時給",
            Self::Auto => "自動判定",
        }
    }
}

/// ユーザー明示指定の CSV ソース (UI の「ソース媒体」選択に対応)
#[derive(Debug, Clone, Copy, PartialEq)]
pub enum UserSourceHint {
    Indeed,
    /// 2026-06-30 追加: Indeed スマホ版 (年間休日 / 人気タグ取得可)
    IndeedSp,
    JobBox,
    Other,
    Auto,
}

impl UserSourceHint {
    pub fn from_str(s: &str) -> Self {
        match s {
            "indeed" => Self::Indeed,
            "indeed_sp" => Self::IndeedSp,
            "jobbox" => Self::JobBox,
            "other" => Self::Other,
            _ => Self::Auto,
        }
    }
}

/// CSVバイト列をパースしてSurveyRecordのVecに変換
pub fn parse_csv_bytes(
    data: &[u8],
    context_pref: Option<&str>,
) -> Result<Vec<SurveyRecord>, String> {
    parse_csv_bytes_with_hints(data, context_pref, UserSourceHint::Auto)
}

/// CSV バイト列のエンコーディング検出 + UTF-8 への正規化
///
/// 2026-04-26 Fix-A: 媒体分析タブ CSV アップロードの文字化け対策。
/// 検出順 (BOM 優先 → ヒューリスティック):
/// 1. UTF-8 BOM (0xEF 0xBB 0xBF) → そのまま (BOM 除去)
/// 2. UTF-16 LE BOM (0xFF 0xFE) → UTF-16LE デコード
/// 3. UTF-16 BE BOM (0xFE 0xFF) → UTF-16BE デコード
/// 4. UTF-8 として有効 → UTF-8 採用
/// 5. Shift-JIS (CP932) として decode → 文字化け率 (U+FFFD) <= 5% なら採用
/// 6. すべて失敗時は UTF-8 (lossy) で続行
///
/// # 戻り値
/// 正規化後の UTF-8 バイト列 (Vec<u8>) と検出したエンコーディング名
pub fn decode_csv_bytes(data: &[u8]) -> (Vec<u8>, &'static str) {
    // 1. UTF-8 BOM
    if data.starts_with(&[0xEF, 0xBB, 0xBF]) {
        return (data[3..].to_vec(), "UTF-8 (BOM)");
    }
    // 2. UTF-16 LE BOM
    if data.starts_with(&[0xFF, 0xFE]) {
        let (cow, _, _) = encoding_rs::UTF_16LE.decode(&data[2..]);
        return (cow.into_owned().into_bytes(), "UTF-16LE");
    }
    // 3. UTF-16 BE BOM
    if data.starts_with(&[0xFE, 0xFF]) {
        let (cow, _, _) = encoding_rs::UTF_16BE.decode(&data[2..]);
        return (cow.into_owned().into_bytes(), "UTF-16BE");
    }
    // 4. UTF-8 valid?
    if std::str::from_utf8(data).is_ok() {
        return (data.to_vec(), "UTF-8");
    }
    // 5. Shift-JIS (CP932 superset)
    let (cow, _, had_errors) = encoding_rs::SHIFT_JIS.decode(data);
    if !had_errors {
        return (cow.into_owned().into_bytes(), "Shift-JIS");
    }
    // 文字化け率を測定: U+FFFD (\u{FFFD}) の比率が 5% 以下なら SJIS 採用
    let total_chars = cow.chars().count().max(1);
    let replacement_count = cow.chars().filter(|c| *c == '\u{FFFD}').count();
    if (replacement_count as f64 / total_chars as f64) <= 0.05 {
        return (
            cow.into_owned().into_bytes(),
            "Shift-JIS (with replacements)",
        );
    }
    // 6. fallback: UTF-8 lossy
    let utf8_lossy = String::from_utf8_lossy(data).into_owned();
    (utf8_lossy.into_bytes(), "UTF-8 (lossy fallback)")
}

/// ソース媒体の明示指定ありバージョン
pub fn parse_csv_bytes_with_hints(
    data: &[u8],
    context_pref: Option<&str>,
    source_hint: UserSourceHint,
) -> Result<Vec<SurveyRecord>, String> {
    parse_csv_bytes_inner(data, context_pref, source_hint, None)
}

/// Gemini AI 列推定の補完付きバージョン (機能 E フォールバックの再パース用)。
///
/// `col_overrides` は role キー (job_title/company_name/... / build_column_map と同じ静的キー)
/// → 列インデックスのマップ。ヘッダーマッチ + データ動的検出の**後**に上書き適用される
/// (= AI 推定は「最後の砦」)。キー未設定 (`col_overrides` 空) なら
/// [`parse_csv_bytes_with_hints`] と完全に同一の挙動になる。
pub fn parse_csv_bytes_with_col_overrides(
    data: &[u8],
    context_pref: Option<&str>,
    source_hint: UserSourceHint,
    col_overrides: &std::collections::HashMap<&'static str, usize>,
) -> Result<Vec<SurveyRecord>, String> {
    parse_csv_bytes_inner(data, context_pref, source_hint, Some(col_overrides))
}

/// 実体。`col_overrides` が `Some` のとき、通常の列検出後に AI 推定結果で col_map を補完する。
fn parse_csv_bytes_inner(
    data: &[u8],
    context_pref: Option<&str>,
    source_hint: UserSourceHint,
    col_overrides: Option<&std::collections::HashMap<&'static str, usize>>,
) -> Result<Vec<SurveyRecord>, String> {
    // 2026-04-26 Fix-A: BOM 検出 + Shift-JIS フォールバックで多エンコーディング対応
    let (decoded, encoding_name) = decode_csv_bytes(data);
    if encoding_name != "UTF-8" {
        tracing::info!("CSV encoding detected: {}", encoding_name);
    }
    let data: &[u8] = &decoded;

    let mut rdr = csv::ReaderBuilder::new()
        .flexible(true)
        .has_headers(true)
        .from_reader(data);

    let headers: Vec<String> = rdr
        .headers()
        .map_err(|e| format!("ヘッダー読み取りエラー: {e}"))?
        .iter()
        .map(|s| s.to_string())
        .collect();

    // ユーザー明示指定があれば優先、それ以外は自動判定
    let source = match source_hint {
        UserSourceHint::Indeed => CsvSource::Indeed,
        UserSourceHint::IndeedSp => CsvSource::IndeedSp,
        UserSourceHint::JobBox => CsvSource::JobBox,
        UserSourceHint::Other => CsvSource::Unknown,
        UserSourceHint::Auto => detect_csv_source(&headers),
    };
    let mut col_map = build_column_map(&headers, &source);

    // ヘッダーマッチが不十分な場合、データ内容ベースの動的検出にフォールバック（GASのdetectColumnsAutomatically移植）
    if col_map.len() < 3 {
        // 2026-04-26 Fix-A: data はすでに decode_csv_bytes() で BOM 除去 + UTF-8 化済
        let mut sample_rdr = csv::ReaderBuilder::new()
            .flexible(true)
            .has_headers(true)
            .from_reader(data);
        let sample_rows: Vec<csv::StringRecord> = sample_rdr
            .records()
            .take(20)
            .filter_map(|r| r.ok())
            .collect();
        if !sample_rows.is_empty() {
            let detected = detect_columns_from_data(&headers, &sample_rows);
            tracing::info!(
                "Dynamic column detection: {:?}",
                detected.keys().collect::<Vec<_>>()
            );
            // 検出結果でcol_mapを上書き（ヘッダーマッチより動的検出を優先）
            for (key, idx) in detected {
                col_map.insert(key, idx);
            }
        }
    }

    // 機能 E (2026-07-07): Gemini AI 列推定の補完 (graceful degradation の最後の砦)。
    // ヘッダーマッチ + データ動的検出でも主要列が埋まらなかった場合にのみ、
    // 呼び出し側 (handlers.rs) が AI 推定結果を col_overrides として渡す。
    // col_overrides=None (キー未設定/従来経路) のときは何もしないため挙動不変。
    if let Some(ov) = col_overrides {
        for (&key, &idx) in ov.iter() {
            col_map.insert(key, idx);
        }
    }

    // Finding #12 (2026-07-01): jobsearch-JobCard-tag 列インデックスをヘッダー解析時に
    // 1 度計算してキャッシュ。行ループ内での毎回 contains() 探索を排除。
    let jobcard_tag_indices: Vec<usize> = headers
        .iter()
        .enumerate()
        .filter(|(_, h)| h.contains("jobsearch-JobCard-tag"))
        .map(|(i, _)| i)
        .collect();

    let mut records = Vec::new();
    let mut skipped_metadata = 0_usize;
    let mut skipped_incomplete = 0_usize;
    // 2026-04-26 Fix-A: CSV 行レベル重複検出
    // hash(job_title + company_name + location_raw + salary_raw + employment_type) で
    // 完全一致行 (典型例: 同一求人を媒体内で複数日に渡って収集) を 1 件にまとめる。
    // 注意: 異なる雇用形態 (正社員/パート) は別求人として扱うため key に含める。
    let mut seen_row_hashes: std::collections::HashSet<u64> = std::collections::HashSet::new();
    let mut duplicates_removed = 0_usize;
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
            col_map
                .get(key)
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
            if mapped_score > 0 {
                best_val = mapped;
                best_score = mapped_score;
            }
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
            if score_salary(&mapped) > 0 {
                mapped
            } else {
                let mut best_val = String::new();
                let mut best_score = 0_i32;
                for ci in 0..row.len() {
                    let val = row.get(ci).unwrap_or("").trim();
                    let s = score_salary(val);
                    if s > best_score {
                        best_score = s;
                        best_val = val.to_string();
                    }
                }
                best_val
            }
        };
        let employment_type = {
            let mapped = get("employment_type");
            if score_employment_type(&mapped) > 0 {
                mapped
            } else {
                let mut best_val = String::new();
                for ci in 0..row.len() {
                    let val = row.get(ci).unwrap_or("").trim();
                    if score_employment_type(val) > 0 {
                        best_val = val.to_string();
                        break;
                    }
                }
                best_val
            }
        };
        let employment_type = normalize_employment_type(&employment_type);
        let mut tags_raw = get("tags");
        // IndeedのCSVはタグが複数カラムに分散: jobsearch-JobCard-tag, (2), (3)...
        // col_mapのtags以降の連続タグカラムを結合。
        // Finding #12: インデックスはヘッダー解析時にキャッシュ済み (jobcard_tag_indices)。
        for &ci in &jobcard_tag_indices {
            if ci < row.len() {
                let val = row.get(ci).unwrap_or("").trim();
                if !val.is_empty() && !tags_raw.contains(val) && val.len() < 30 {
                    if !tags_raw.is_empty() {
                        tags_raw.push(',');
                    }
                    tags_raw.push_str(val);
                }
            }
        }
        // 2026-06-30 Indeed (SP) 専用: css-u74ql7 列 (人気/超人気タグ) を tags_raw に明示 append。
        //   build_column_map で tags に css-u74ql7 を入れても、jobsearch-JobCard-tag が
        //   先に確定するため紐付かない。Section 07.6 (popularity) 集計が
        //   tags_raw に「人気」「超人気」が含まれることを前提とするので、
        //   ここで headers から css-u74ql7 列を直接 lookup して結合する。
        if matches!(source, CsvSource::IndeedSp) {
            if let Some(popular_idx) = headers.iter().position(|h| h == "css-u74ql7") {
                if popular_idx < row.len() {
                    let v = row.get(popular_idx).unwrap_or("").trim();
                    if !v.is_empty() && !tags_raw.contains(v) {
                        if !tags_raw.is_empty() {
                            tags_raw.push(',');
                        }
                        tags_raw.push_str(v);
                    }
                }
            }
        }
        let url = {
            let u = get("url");
            if u.is_empty() {
                None
            } else {
                Some(u)
            }
        };
        let is_new = {
            let v = get("is_new");
            v.contains("新着") || v.contains("NEW") || v.contains("new")
        };

        let description = get("description");

        // パース
        let salary_parsed = parse_salary(&salary_raw, SalaryType::Monthly);
        let location_parsed = parse_location(&location_raw, context_pref);
        let annual_holidays = extract_annual_holidays(&description);

        // 2026-06-26 求人ボックス CSV で雇用形態列 (c-icon (3) 相当) が無い場合のフォールバック:
        //   給与単位から推定 (月給/年俸 → 正社員、時給 → パート・アルバイト)。
        //   2026-06-30 Finding #10: ロジックを `infer_employment_type_for_jobbox` に抽出。
        //   2026-07-01 ユーザー方針転換で復活: IndeedSp CSV の css-1hwmqh1 列消失に対応。
        //   契約社員も正社員雇用の最初6ヶ月であるケースが多いため、月給→正社員 / 時給→パート・アルバイト
        //   の粗い区分で十分 (Commit 1 で IndeedSp 除外したが再導入)。
        let employment_type = if matches!(source, CsvSource::JobBox | CsvSource::IndeedSp)
            && employment_type.trim().is_empty()
        {
            infer_employment_type_for_jobbox(&salary_parsed.salary_type).unwrap_or(employment_type)
        } else {
            employment_type
        };

        // 2026-04-26 Fix-A: 行レベル重複検出
        // employment_type を key に含めることで「正社員/パート」が別レコード扱いになる
        // (V2 ルール: 同一施設の正社員/パートは別求人。MEMORY: feedback_dedup_rules)
        {
            use std::hash::{Hash, Hasher};
            let mut hasher = std::collections::hash_map::DefaultHasher::new();
            job_title.hash(&mut hasher);
            company_name.hash(&mut hasher);
            location_raw.hash(&mut hasher);
            salary_raw.hash(&mut hasher);
            employment_type.hash(&mut hasher);
            let row_hash = hasher.finish();
            if !seen_row_hashes.insert(row_hash) {
                duplicates_removed += 1;
                continue;
            }
        }

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
            description,
            salary_parsed,
            location_parsed,
            annual_holidays,
        });
    }

    tracing::info!(
        "CSV parse: {} records accepted, {} metadata rows skipped, {} incomplete rows skipped, {} duplicates removed",
        records.len(),
        skipped_metadata,
        skipped_incomplete,
        duplicates_removed
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
fn build_column_map(
    headers: &[String],
    source: &CsvSource,
) -> std::collections::HashMap<&'static str, usize> {
    let mut map = std::collections::HashMap::new();

    for (i, h) in headers.iter().enumerate() {
        let h = h.trim();
        match source {
            CsvSource::Indeed => {
                // 日本語ヘッダー
                if h.contains("求人名")
                    || h.contains("職種名")
                    || h.contains("タイトル")
                    || h == "title"
                {
                    map.insert("job_title", i);
                }
                if h.contains("会社名") || h.contains("企業") || h == "company" {
                    map.insert("company_name", i);
                }
                if h.contains("勤務地") || h.contains("所在地") || h == "location" {
                    map.insert("location", i);
                }
                if h.contains("給与") || h.contains("年収") || h.contains("月給") || h == "salary"
                {
                    map.insert("salary", i);
                }
                if h.contains("雇用") || h.contains("形態") || h == "type" {
                    map.insert("employment_type", i);
                }
                if h.contains("タグ") || h.contains("特徴") || h == "tags" {
                    map.insert("tags", i);
                }
                if h.contains("URL") || h.contains("url") || h.contains("リンク") {
                    map.insert("url", i);
                }
                if h.contains("新着") || h.contains("NEW") {
                    map.insert("is_new", i);
                }
                if h.contains("詳細") || h.contains("仕事内容") || h.contains("description") {
                    map.insert("description", i);
                }
                // IndeedスクレイピングツールのCSSクラス名ベースヘッダー
                // カラム順: URL, 求人名, 会社名, 勤務地, タグ×7, 給与, 雇用形態, ...
                if h == "jcs-JobTitle href" {
                    map.insert("url", i);
                }
                if h == "jcs-JobTitle" && !map.contains_key("job_title") {
                    map.insert("job_title", i);
                }
                if h == "css-19eicqx" {
                    map.insert("company_name", i);
                }
                if h == "css-1f06pz4" {
                    map.insert("location", i);
                }
                if h == "css-zydy3i" && !map.contains_key("salary") {
                    map.insert("salary", i);
                }
                if h == "css-zydy3i (2)" {
                    map.insert("employment_type", i);
                }
                if h.starts_with("jobsearch-JobCard-tag") && !map.contains_key("tags") {
                    map.insert("tags", i);
                }
            }
            CsvSource::IndeedSp => {
                // 2026-06-30 Indeed (SP) スマホ版スクレイピング用 CSS クラス名マッピング。
                // CSV ヘッダ例 (29 列): css-1hwmqh1, css-bxyec3 href, css-bxyec3, css-lx9x6g,
                //   css-14qk2ra, css-18rxko3, css-18rxko3 (2), css-ge6x3l src,
                //   jobsearch-JobCard-tag, (2)〜(11), css-1c8ncmc, css-o67di7,
                //   css-1vlebyu, css-1vlebyu (2)〜(6), css-1hwmqh1 (2), css-u74ql7
                //
                // 日本語ヘッダーフォールバック (汎用 / SP 出力でも一部混ざる可能性)
                if h.contains("求人名")
                    || h.contains("職種名")
                    || h.contains("タイトル")
                    || h == "title"
                {
                    map.insert("job_title", i);
                }
                if h.contains("会社名") || h.contains("企業") || h == "company" {
                    map.insert("company_name", i);
                }
                if h.contains("勤務地") || h.contains("所在地") || h == "location" {
                    map.insert("location", i);
                }
                if h.contains("給与") || h.contains("年収") || h.contains("月給") || h == "salary"
                {
                    map.insert("salary", i);
                }
                if h.contains("雇用") || h.contains("形態") || h == "type" {
                    map.insert("employment_type", i);
                }
                if h.contains("タグ") || h.contains("特徴") || h == "tags" {
                    map.insert("tags", i);
                }
                if h.contains("URL") || h.contains("url") || h.contains("リンク") {
                    map.insert("url", i);
                }
                if h.contains("新着") || h.contains("NEW") {
                    map.insert("is_new", i);
                }
                if h.contains("詳細") || h.contains("仕事内容") || h.contains("description") {
                    map.insert("description", i);
                }

                // Indeed (SP) 固有 CSS クラス判定 (既存 map にあれば上書きしない)
                if h == "css-bxyec3 href" && !map.contains_key("url") {
                    map.insert("url", i);
                }
                if h == "css-bxyec3" && !map.contains_key("job_title") {
                    map.insert("job_title", i);
                }
                if h == "css-14qk2ra" && !map.contains_key("company_name") {
                    map.insert("company_name", i);
                }
                if h == "css-18rxko3" && !map.contains_key("location") {
                    map.insert("location", i);
                }
                if h == "css-18rxko3 (2)" && !map.contains_key("salary") {
                    map.insert("salary", i);
                }
                // 雇用形態は col 0 (css-1hwmqh1) が主、col 26 (css-1hwmqh1 (2)) が副。
                // 副があれば優先しない (主を採用)。
                if h == "css-1hwmqh1" && !map.contains_key("employment_type") {
                    map.insert("employment_type", i);
                }
                // description: 本文 (年間休日含む)
                if h == "css-1vlebyu" && !map.contains_key("description") {
                    map.insert("description", i);
                }
                // 人気/超人気タグ (Indeed SP 固有シグナル) を tags に紐付け。
                // 既存 tags キー (jobsearch-JobCard-tag) があれば触らず、無ければ採用。
                if h == "css-u74ql7" && !map.contains_key("tags") {
                    map.insert("tags", i);
                }
                // 特徴タグ (jobsearch-JobCard-tag, (2)〜(11)) は最初の列だけ tags に採用。
                // 既存ループ (line 348-361) で複数列を連結する仕組みあり。
                if h.starts_with("jobsearch-JobCard-tag") && !map.contains_key("tags") {
                    map.insert("tags", i);
                }
                // 掲載日 (css-o67di7) は「30+日前」等の文字列。新着判定の弱いシグナルとして扱う。
                if h == "css-o67di7" && !map.contains_key("is_new") {
                    map.insert("is_new", i);
                }
            }
            CsvSource::JobBox => {
                // 日本語ベース判定 (Excel エクスポート / 手作業整形 CSV 用)
                if h.contains("職種") || h.contains("求人名") {
                    map.insert("job_title", i);
                }
                if h.contains("企業名") || h.contains("会社名") {
                    map.insert("company_name", i);
                }
                if h.contains("所在地") || h.contains("勤務地") {
                    map.insert("location", i);
                }
                if h.contains("賃金") || h.contains("給与") || h.contains("年収") {
                    map.insert("salary", i);
                }
                if h.contains("雇用") || h.contains("就業形態") {
                    map.insert("employment_type", i);
                }
                if h.contains("特徴") || h.contains("タグ") || h.contains("こだわり") {
                    map.insert("tags", i);
                }
                if h.contains("URL") || h.contains("url") {
                    map.insert("url", i);
                }
                if h.contains("新着") {
                    map.insert("is_new", i);
                }
                if h.contains("詳細") || h.contains("仕事内容") || h.contains("休日") {
                    map.insert("description", i);
                }

                // CSS クラス名ベース判定 (求人ボックスのスクレイピングツール出力)
                // ヘッダ例: p-result_title_link href, p-result_name, p-result_company,
                //           c-icon, c-icon (2), p-result_lines,
                //           p-result_tag_feature--ver2, p-result_new, p-em_kwd, ...
                // 2026-06-25 追加: テストCSV (xn--pckua2a7gp15o89zb-*.csv) で description が
                //   取れず Section 07.5 の年間休日抽出が全件失敗していたため、CSS クラス名対応を追加。
                //   既存日本語判定は維持 (既に map に入っていれば上書きしない)。
                if h == "p-result_title_link href" && !map.contains_key("url") {
                    map.insert("url", i);
                }
                if h == "p-result_name" && !map.contains_key("job_title") {
                    map.insert("job_title", i);
                }
                if h == "p-result_company" && !map.contains_key("company_name") {
                    map.insert("company_name", i);
                }
                if h == "c-icon" && !map.contains_key("location") {
                    map.insert("location", i);
                }
                if h == "c-icon (2)" && !map.contains_key("salary") {
                    map.insert("salary", i);
                }
                if h == "c-icon (3)" && !map.contains_key("employment_type") {
                    map.insert("employment_type", i);
                }
                if h == "p-result_lines" && !map.contains_key("description") {
                    map.insert("description", i);
                }
                if h == "p-result_new" && !map.contains_key("is_new") {
                    map.insert("is_new", i);
                }
                // 特徴タグ (複数列) / 検索キーワードハイライト の最初の列だけ採用
                if h.starts_with("p-result_tag_feature") && !map.contains_key("tags") {
                    map.insert("tags", i);
                }
                if h.starts_with("p-em_kwd") && !map.contains_key("tags") {
                    map.insert("tags", i);
                }
            }
            CsvSource::Unknown => {
                // 汎用マッチ
                let hl = h.to_lowercase();
                if hl.contains("title") || h.contains("職種") || h.contains("求人") {
                    map.insert("job_title", i);
                }
                if hl.contains("company") || h.contains("会社") || h.contains("企業") {
                    map.insert("company_name", i);
                }
                if hl.contains("location")
                    || h.contains("勤務地")
                    || h.contains("所在地")
                    || h.contains("住所")
                {
                    map.insert("location", i);
                }
                if hl.contains("salary")
                    || h.contains("給与")
                    || h.contains("賃金")
                    || h.contains("年収")
                {
                    map.insert("salary", i);
                }
                if hl.contains("type") || h.contains("雇用") {
                    map.insert("employment_type", i);
                }
                if hl.contains("tag") || h.contains("タグ") || h.contains("特徴") {
                    map.insert("tags", i);
                }
                if hl.contains("url") {
                    map.insert("url", i);
                }
                if hl.contains("description") || h.contains("詳細") || h.contains("仕事内容")
                {
                    map.insert("description", i);
                }
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
    let mut scores: std::collections::HashMap<&str, Vec<(usize, i32)>> =
        std::collections::HashMap::new();
    for key in &[
        "location",
        "salary",
        "company_name",
        "job_title",
        "url",
        "employment_type",
        "is_new",
    ] {
        scores.insert(key, Vec::new());
    }

    let sample_size = sample_rows.len().min(20);
    for row in sample_rows.iter().take(sample_size) {
        for col_idx in 0..row.len().min(headers.len()) {
            let val = row.get(col_idx).unwrap_or("").trim();
            if val.is_empty() {
                continue;
            }

            // SAFETY (C-3): すべてのキーは line 513-523 で初期化済 → if let Some で防御的に
            // 勤務地スコア（都道府県パターン）
            let loc_score = score_location(val);
            if loc_score > 0 {
                if let Some(v) = scores.get_mut("location") {
                    v.push((col_idx, loc_score));
                }
            }

            // 給与スコア
            let sal_score = score_salary(val);
            if sal_score > 0 {
                if let Some(v) = scores.get_mut("salary") {
                    v.push((col_idx, sal_score));
                }
            }

            // 会社名スコア
            let comp_score = score_company(val);
            if comp_score > 0 {
                if let Some(v) = scores.get_mut("company_name") {
                    v.push((col_idx, comp_score));
                }
            }

            // URLスコア
            if val.starts_with("http") {
                if let Some(v) = scores.get_mut("url") {
                    v.push((col_idx, 100));
                }
            }

            // 雇用形態スコア
            let emp_score = score_employment_type(val);
            if emp_score > 0 {
                if let Some(v) = scores.get_mut("employment_type") {
                    v.push((col_idx, emp_score));
                }
            }

            // 求人タイトルスコア
            let title_score = score_job_title(val);
            if title_score > 0 {
                if let Some(v) = scores.get_mut("job_title") {
                    v.push((col_idx, title_score));
                }
            }

            // 新着スコア
            if val.contains("新着") || val.contains("NEW") || val.contains("日前") {
                if let Some(v) = scores.get_mut("is_new") {
                    v.push((col_idx, 100));
                }
            }
        }
    }

    // 各フィールドで最高スコアの列を選択
    let mut result = std::collections::HashMap::new();
    let mut used_cols = std::collections::HashSet::new();

    // 優先度順: URL → salary → location → company → employment → title → is_new
    for key in &[
        "url",
        "salary",
        "location",
        "company_name",
        "employment_type",
        "job_title",
        "is_new",
    ] {
        let mut col_totals: std::collections::HashMap<usize, i32> =
            std::collections::HashMap::new();
        // SAFETY (C-3): static keys は上で初期化済。なくても空処理で続行
        if let Some(score_list) = scores.get(key) {
            for (col, score) in score_list {
                *col_totals.entry(*col).or_default() += score;
            }
        }
        if let Some((&best_col, &best_score)) = col_totals
            .iter()
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
            if used_cols.contains(&col_idx) {
                continue;
            }
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
    if val.starts_with("http") || val.len() > 100 {
        return 0;
    }
    let mut score = 0;
    // 都道府県
    if val.contains("都") || val.contains("道") || val.contains("府") || val.contains("県") {
        let prefs = [
            "北海道",
            "青森",
            "岩手",
            "宮城",
            "秋田",
            "山形",
            "福島",
            "茨城",
            "栃木",
            "群馬",
            "埼玉",
            "千葉",
            "東京",
            "神奈川",
            "新潟",
            "富山",
            "石川",
            "福井",
            "山梨",
            "長野",
            "岐阜",
            "静岡",
            "愛知",
            "三重",
            "滋賀",
            "京都",
            "大阪",
            "兵庫",
            "奈良",
            "和歌山",
            "鳥取",
            "島根",
            "岡山",
            "広島",
            "山口",
            "徳島",
            "香川",
            "愛媛",
            "高知",
            "福岡",
            "佐賀",
            "長崎",
            "熊本",
            "大分",
            "宮崎",
            "鹿児島",
            "沖縄",
        ];
        if prefs.iter().any(|p| val.contains(p)) {
            score += 50;
        }
    }
    // 市区町村
    if val.contains("市") || val.contains("区") || val.contains("町") || val.contains("村") {
        score += 30;
    }
    score
}

fn score_salary(val: &str) -> i32 {
    if val.starts_with("http") {
        return 0;
    }
    let mut score = 0;
    if val.contains("時給") {
        score += 80;
    }
    if val.contains("月給") {
        score += 70;
    }
    if val.contains("年収") || val.contains("年俸") {
        score += 70;
    }
    if val.contains("日給") {
        score += 60;
    }
    if val.contains("円") && val.chars().any(|c| c.is_ascii_digit()) {
        score += 40;
    }
    if val.contains("万") && val.chars().any(|c| c.is_ascii_digit()) {
        score += 30;
    }
    // 住所と混同しないように
    if val.contains("市") || val.contains("区") {
        score = 0;
    }
    score
}

fn score_company(val: &str) -> i32 {
    if val.starts_with("http") || val.contains("円") {
        return 0;
    }
    let mut score = 0;
    if val.contains("株式会社") || val.contains("有限会社") || val.contains("合同会社")
    {
        score += 60;
    }
    if val.contains("(株)") || val.contains("（株）") {
        score += 50;
    }
    if val.contains("法人") || val.contains("医療法人") || val.contains("社会福祉法人")
    {
        score += 60;
    }
    if val.len() >= 3 && val.len() <= 50 && score == 0 {
        score += 5;
    } // 短すぎず長すぎない
    score
}

fn score_employment_type(val: &str) -> i32 {
    if val.contains("正社員")
        || val.contains("契約社員")
        || val.contains("派遣社員")
        || val.contains("パート")
        || val.contains("アルバイト")
        || val.contains("業務委託")
        || val.contains("紹介予定派遣")
        || val.contains("嘱託")
        || val.contains("請負")
    {
        return 100;
    }
    0
}

/// GAS EMPLOYMENT_TYPE_MAP 移植: 雇用形態テキストを正規化
fn normalize_employment_type(val: &str) -> String {
    if val.contains("正社員") || val.contains("正職員") {
        return "正社員".into();
    }
    if val.contains("契約社員") || val.contains("嘱託") {
        return "契約社員".into();
    }
    if val.contains("紹介予定派遣") {
        return "紹介予定派遣".into();
    }
    if val.contains("派遣") {
        return "派遣社員".into();
    }
    if val.contains("パート") || val.contains("アルバイト") {
        return "パート・アルバイト".into();
    }
    if val.contains("業務委託") || val.contains("請負") {
        return "業務委託".into();
    }
    val.to_string()
}

fn score_job_title(val: &str) -> i32 {
    if val.starts_with("http") || val.contains("円") {
        return 0;
    }
    let keywords = [
        "エンジニア",
        "デザイナー",
        "営業",
        "事務",
        "経理",
        "スタッフ",
        "ドライバー",
        "看護",
        "介護",
        "製造",
        "加工",
        "販売",
        "接客",
        "マネージャー",
        "担当",
        "募集",
        "オペレーター",
    ];
    let mut score = 0;
    if keywords.iter().any(|k| val.contains(k)) {
        score += 40;
    }
    if val.len() >= 5 && val.len() <= 100 {
        score += 20;
    }
    score
}

/// 年間休日として妥当な値か (GAS Constants.js:extractAnnualHolidays と統一)
/// 70-180 日を妥当範囲とする (最低 70 日: 週休 1 日換算、上限 180 日: 完全週休 3 日超)
fn is_valid_annual_holidays(v: i64) -> bool {
    (70..=180).contains(&v)
}

/// `text` 中の `keyword` 出現位置直後にある2-3桁数字を抽出する。
fn try_extract_after(text: &str, keyword: &str) -> Option<i64> {
    let mut search_from = 0;
    while let Some(rel_pos) = text[search_from..].find(keyword) {
        let abs_after = search_from + rel_pos + keyword.len();
        if abs_after >= text.len() {
            return None;
        }
        let after = &text[abs_after..];
        let after = after.trim_start_matches(|c: char| {
            matches!(
                c,
                ':' | '：'
                    | ' '
                    | '\u{3000}'
                    | '・'
                    | '>'
                    | ']'
                    | '['
                    | '<'
                    | '\t'
                    | '\n'
                    | '\r'
                    | '('
                    | '（'
                    | 'は'
                    | 'が'
                    | '/'
                    | '／'
            )
        });
        let num_str: String = after.chars().take_while(|c| c.is_ascii_digit()).collect();
        if !num_str.is_empty() && num_str.len() <= 3 {
            if let Ok(v) = num_str.parse::<i64>() {
                if is_valid_annual_holidays(v) {
                    return Some(v);
                }
            }
        }
        search_from = abs_after;
    }
    None
}

/// `text` 中の `keyword` 出現位置直前にある「XX日」を抽出する。
fn try_extract_before(text: &str, keyword: &str) -> Option<i64> {
    let key_pos = text.find(keyword)?;
    let before = &text[..key_pos];
    let mut day_search_end = before.len();
    while day_search_end > 0 {
        let segment = &before[..day_search_end];
        let day_byte_pos = segment.rfind('日')?;
        let pre_day = &segment[..day_byte_pos];
        let tail: String = pre_day
            .chars()
            .rev()
            .take_while(|c| c.is_ascii_digit())
            .collect();
        if !tail.is_empty() && tail.len() <= 3 {
            let num_str: String = tail.chars().rev().collect();
            if let Ok(v) = num_str.parse::<i64>() {
                if is_valid_annual_holidays(v) {
                    return Some(v);
                }
            }
        }
        day_search_end = day_byte_pos;
    }
    None
}

/// description テキストから年間休日数を抽出する。
///
/// GAS `Constants.js` の `ANNUAL_HOLIDAYS_PATTERNS` (18 パターン) を網羅 + V2 拡張。
/// 優先度順に試行し、最初の妥当値 (70-99 or 100-180) を返す。
fn extract_annual_holidays(text: &str) -> Option<i64> {
    if text.is_empty() {
        return None;
    }
    const PREFIX_KEYWORDS: &[&str] = &[
        "年間休日数",
        "年間休日",
        "年間休暇",
        "休日数",
        "年休",
        "年間休",
        "年間",
    ];
    for kw in PREFIX_KEYWORDS {
        if let Some(v) = try_extract_after(text, kw) {
            return Some(v);
        }
    }
    if text.contains("休日") {
        let stripped = text.replace("年間休日", "");
        if let Some(v) = try_extract_after(&stripped, "休日") {
            return Some(v);
        }
    }
    for kw in &["年間休日", "年間", "年休"] {
        if let Some(v) = try_extract_before(text, kw) {
            return Some(v);
        }
    }
    None
}

/// 年間休日のカテゴリ分類 (GAS `Constants.js:ANNUAL_HOLIDAYS_RANGES` 移植)
pub fn annual_holidays_category(days: i64) -> &'static str {
    match days {
        i64::MIN..=89 => "～89日",
        90..=104 => "90～104日",
        105..=119 => "105～119日",
        120..=124 => "120～124日",
        125..=129 => "125～129日",
        _ => "130日～",
    }
}

/// 年間休日カテゴリの並び順 (UI 表示用、固定 6 要素)
pub const ANNUAL_HOLIDAYS_CATEGORIES: [&str; 6] = [
    "～89日",
    "90～104日",
    "105～119日",
    "120～124日",
    "125～129日",
    "130日～",
];

/// 求人ボックス CSV で雇用形態列が空の場合に、給与単位から雇用形態を推定する
/// (2026-06-30 Finding #10: `parse_csv_bytes_with_hints` から抽出)
///
/// 求人ボックスの典型的な雇用形態と給与単位の対応関係に基づく推定:
/// - `Monthly` / `Annual` → `Some("正社員")`
/// - `Hourly`             → `Some("パート・アルバイト")`
/// - `Daily` / `Weekly`   → `None` (実データ希少のため推定対象外、空文字列のまま「不明」)
pub fn infer_employment_type_for_jobbox(salary_type: &SalaryType) -> Option<String> {
    match salary_type {
        SalaryType::Monthly | SalaryType::Annual => Some("正社員".to_string()),
        SalaryType::Hourly => Some("パート・アルバイト".to_string()),
        _ => None,
    }
}

// =============================================================================
// Gemini AI フォールバック補助関数 (機能 E: 列マッピング / 機能 C: 年間休日抽出)
//
// いずれも **純関数** (ネットワーク非依存)。実際の API 呼び出しは handlers.rs が
// GeminiClient 経由で行い、graceful degradation (キー未設定/失敗 → None → 従来動作) を担う。
// ここではプロンプト構築・JSON schema・レスポンスパース・適用ロジックのみを提供し、
// CI (キー無し) でユニットテスト可能にする。
// =============================================================================

/// 機能 E: パース結果が「貧弱」か判定する。
///
/// salary / company / title の **いずれか 1 つでも全レコードで空** なら true。
/// この場合、通常の列検出 (ヘッダー + 動的検出) が主要列を取りこぼしていると見なし、
/// AI 列推定フォールバックの発動条件とする。空レコード集合は false (エラー経路で処理済)。
pub fn is_parse_poor(records: &[SurveyRecord]) -> bool {
    if records.is_empty() {
        return false;
    }
    let all_salary_empty = records.iter().all(|r| r.salary_raw.trim().is_empty());
    let all_company_empty = records.iter().all(|r| r.company_name.trim().is_empty());
    let all_title_empty = records.iter().all(|r| r.job_title.trim().is_empty());
    all_salary_empty || all_company_empty || all_title_empty
}

/// 機能 E: CSV バイト列からヘッダ行と先頭 `n_rows` データ行を抽出する (プロンプト素材)。
///
/// [`decode_csv_bytes`] でエンコーディング正規化してから読む。ヘッダ空なら `None`。
pub fn extract_header_and_samples(
    data: &[u8],
    n_rows: usize,
) -> Option<(Vec<String>, Vec<Vec<String>>)> {
    let (decoded, _) = decode_csv_bytes(data);
    let mut rdr = csv::ReaderBuilder::new()
        .flexible(true)
        .has_headers(true)
        .from_reader(&decoded[..]);
    let headers: Vec<String> = rdr.headers().ok()?.iter().map(|s| s.to_string()).collect();
    if headers.is_empty() {
        return None;
    }
    let mut samples = Vec::new();
    for rec in rdr.records().take(n_rows).flatten() {
        samples.push(rec.iter().map(|s| s.to_string()).collect());
    }
    Some((headers, samples))
}

/// 機能 E: 列マッピング推定の JSON schema。
/// `{ mappings: [{ column_index: int, role: enum }] }`
pub fn build_colmap_schema() -> serde_json::Value {
    serde_json::json!({
        "type": "object",
        "properties": {
            "mappings": {
                "type": "array",
                "items": {
                    "type": "object",
                    "properties": {
                        "column_index": { "type": "integer" },
                        "role": {
                            "type": "string",
                            "enum": [
                                "title", "company", "location", "salary",
                                "employment_type", "description", "tags", "url", "ignore"
                            ]
                        }
                    },
                    "required": ["column_index", "role"]
                }
            }
        },
        "required": ["mappings"]
    })
}

/// 機能 E: 列マッピング推定の (system, user) プロンプトを構築する。
///
/// 認証情報・接続情報は一切含めない。列値は 80 文字で切り詰める。
pub fn build_colmap_prompt(headers: &[String], samples: &[Vec<String>]) -> (String, String) {
    let system = "あなたは求人 CSV の列構造を判定するアシスタントです。\
        各列を title/company/location/salary/employment_type/description/tags/url/ignore の\
        いずれかの役割に分類してください。該当する役割が無い列は ignore にしてください。\
        出力は指定された JSON schema に厳密に従い、余計な説明を加えないでください。"
        .to_string();
    let mut user = String::from(
        "以下の求人 CSV のヘッダと先頭データ行から、各列 (0 始まり index) の役割を推定してください。\n\nヘッダ:\n",
    );
    for (i, h) in headers.iter().enumerate() {
        user.push_str(&format!("[{}] {}\n", i, h));
    }
    for (ri, row) in samples.iter().enumerate() {
        user.push_str(&format!("\nデータ行{}:\n", ri + 1));
        for (i, v) in row.iter().enumerate() {
            let trimmed: String = if v.chars().count() > 80 {
                v.chars().take(80).collect()
            } else {
                v.clone()
            };
            user.push_str(&format!("[{}] {}\n", i, trimmed));
        }
    }
    (system, user)
}

/// AI role 文字列を build_column_map と同じ静的 col_map キーに変換する。
/// `ignore` や未知の role は `None` (= 補完対象外)。
pub fn ai_role_to_colmap_key(role: &str) -> Option<&'static str> {
    match role {
        "title" => Some("job_title"),
        "company" => Some("company_name"),
        "location" => Some("location"),
        "salary" => Some("salary"),
        "employment_type" => Some("employment_type"),
        "description" => Some("description"),
        "tags" => Some("tags"),
        "url" => Some("url"),
        _ => None,
    }
}

/// 機能 E: Gemini レスポンス JSON から col_map 補完マップを構築する。
///
/// - `column_index` が `num_columns` 以上 → 無視 (範囲外を信じない)
/// - 未知 role / `ignore` → 無視
/// - 同一 role が複数 → 最初の 1 件を採用 (決定的)
pub fn parse_colmap_from_ai(
    value: &serde_json::Value,
    num_columns: usize,
) -> std::collections::HashMap<&'static str, usize> {
    let mut map = std::collections::HashMap::new();
    if let Some(arr) = value.get("mappings").and_then(|v| v.as_array()) {
        for m in arr {
            let idx = m.get("column_index").and_then(|v| v.as_u64());
            let role = m.get("role").and_then(|v| v.as_str());
            if let (Some(idx), Some(role)) = (idx, role) {
                let idx = idx as usize;
                if idx >= num_columns {
                    continue;
                }
                if let Some(key) = ai_role_to_colmap_key(role) {
                    map.entry(key).or_insert(idx);
                }
            }
        }
    }
    map
}

/// 機能 C: 年間休日 AI 抽出の対象レコードを収集する。
///
/// 条件: `annual_holidays` が `None` かつ description が 30 文字以上。
/// description は 500 文字上限でトリムする。戻り値は `(レコード index, トリム済 description)`。
pub fn collect_holiday_targets(records: &[SurveyRecord]) -> Vec<(usize, String)> {
    records
        .iter()
        .enumerate()
        .filter(|(_, r)| r.annual_holidays.is_none() && r.description.chars().count() >= 30)
        .map(|(i, r)| {
            let desc: String = r.description.chars().take(500).collect();
            (i, desc)
        })
        .collect()
}

/// 機能 C: 年間休日抽出の JSON schema。
/// `{ results: [{ index: int, annual_holidays: int|null }] }`
pub fn build_holiday_schema() -> serde_json::Value {
    serde_json::json!({
        "type": "object",
        "properties": {
            "results": {
                "type": "array",
                "items": {
                    "type": "object",
                    "properties": {
                        "index": { "type": "integer" },
                        "annual_holidays": { "type": "integer", "nullable": true }
                    },
                    "required": ["index", "annual_holidays"]
                }
            }
        },
        "required": ["results"]
    })
}

/// 機能 C: 年間休日抽出の (system, user) プロンプトを構築する。
///
/// プロンプトで厳格化: 明記された年間休日日数のみ。月間休日・有給・週休からの推測は null。
/// `batch` は `(レコード index, トリム済 description)` の並び。index はそのまま echo させる。
pub fn build_holiday_prompt(batch: &[(usize, String)]) -> (String, String) {
    let system = "あなたは求人票から年間休日数のみを厳密に抽出するアシスタントです。\
        『年間休日』として明記された日数のみを返してください。\
        月間休日・有給休暇・週休の記載からの推測は禁止で、その場合は必ず null を返してください。\
        有効な値は 70 から 180 の整数のみです。範囲外・不明・推測が必要な場合は null にしてください。\
        与えられた index はそのまま echo し、出力は JSON schema に厳密に従ってください。"
        .to_string();
    let mut user =
        String::from("以下の各求人 (index と説明文) について、年間休日数を抽出してください。\n");
    for (idx, desc) in batch {
        user.push_str(&format!("\nindex={}\n説明: {}\n", idx, desc));
    }
    (system, user)
}

/// 機能 C: Gemini レスポンス JSON から `(index, annual_holidays)` を抽出する。
///
/// **LLM の返す値を無検証で信じない**: [`is_valid_annual_holidays`] (70-180) を
/// 満たす整数のみを返す。null や範囲外は捨てる。
pub fn parse_holiday_response(value: &serde_json::Value) -> Vec<(usize, i64)> {
    let mut out = Vec::new();
    if let Some(arr) = value.get("results").and_then(|v| v.as_array()) {
        for item in arr {
            let idx = item.get("index").and_then(|v| v.as_u64());
            let days = item.get("annual_holidays").and_then(|v| v.as_i64());
            if let (Some(idx), Some(days)) = (idx, days) {
                if is_valid_annual_holidays(days) {
                    out.push((idx as usize, days));
                }
            }
        }
    }
    out
}

/// 機能 C: 検証済み `(index, days)` をレコードに反映し、適用件数を返す。
///
/// 既に `annual_holidays` が埋まっているレコード (regex 抽出成功分) は上書きしない。
/// index が範囲外・値が無効なものは適用しない (二重防御)。
pub fn apply_holiday_results(records: &mut [SurveyRecord], results: &[(usize, i64)]) -> usize {
    let mut applied = 0usize;
    for &(idx, days) in results {
        if idx < records.len()
            && records[idx].annual_holidays.is_none()
            && is_valid_annual_holidays(days)
        {
            records[idx].annual_holidays = Some(days);
            applied += 1;
        }
    }
    applied
}

// =============================================================================
// 2026-04-26 Fix-A 逆証明テスト (Shift-JIS / UTF-8 BOM / 行レベル重複)
// =============================================================================

#[cfg(test)]
mod fixa_upload_tests {
    use super::*;

    #[test]
    fn fixa_decode_utf8_bom_strips_bom() {
        // UTF-8 BOM 付きヘッダーが正しく除去される
        let bytes = b"\xEF\xBB\xBF\xE6\x9C\x88\xE7\xB5\xA6"; // BOM + "月給"
        let (out, name) = decode_csv_bytes(bytes);
        assert_eq!(name, "UTF-8 (BOM)");
        assert_eq!(String::from_utf8(out).unwrap(), "月給");
    }

    #[test]
    fn fixa_decode_plain_utf8_passes_through() {
        // BOM なし UTF-8 はそのまま
        let bytes = "会社名,給与\nA,月給25万円".as_bytes();
        let (out, name) = decode_csv_bytes(bytes);
        assert_eq!(name, "UTF-8");
        assert_eq!(out, bytes.to_vec());
    }

    #[test]
    fn fixa_decode_shift_jis_excel_save() {
        // 修正前: Shift-JIS バイト列 → csv crate が UTF-8 解釈失敗 → 全行 skip → records=0
        // 修正後: SHIFT_JIS デコードで日本語が復元される
        // 「会社名」(SJIS 6 bytes): 8E 51 8E 90 96 BC, 「月給」: 8C 8E 8B 8B
        let sjis_bytes: &[u8] = &[
            // "会社名,給与\n"
            0x89, 0xEF, 0x8E, 0xD0, 0x96, 0xBC, 0x2C, 0x8B, 0x8B, 0x97, 0x5E, 0x0A,
            // "A,月給25万円\n"
            0x41, 0x2C, 0x8C, 0x8E, 0x8B, 0x8B, 0x32, 0x35, 0x96, 0x9C, 0x89, 0x7E, 0x0A,
        ];
        // UTF-8 として無効である事を逆証明 (lint 抑止: 意図的に不正 UTF-8 を扱う)
        #[allow(invalid_from_utf8)]
        let utf8_check = std::str::from_utf8(sjis_bytes);
        assert!(utf8_check.is_err(), "fixture is non-UTF-8");
        let (out, name) = decode_csv_bytes(sjis_bytes);
        assert_eq!(name, "Shift-JIS");
        let s = String::from_utf8(out).unwrap();
        assert!(
            s.contains("会社名") || s.contains("月給"),
            "SJIS decode must contain Japanese: {}",
            s
        );
    }

    #[test]
    fn fixa_parse_csv_bytes_accepts_shift_jis() {
        // 修正前: SJIS の CSV は records=0 で「CSVにデータ行がありません」エラー
        // 修正後: 正しくパースされて records >= 1
        // SJIS encoded CSV
        let utf8_csv = "会社名,給与,雇用形態,勤務地\n株式会社A,月給25万円,正社員,東京都新宿区\n";
        let (sjis, _, _) = encoding_rs::SHIFT_JIS.encode(utf8_csv);
        let records = parse_csv_bytes(&sjis, None).expect("SJIS CSV parsed");
        assert_eq!(
            records.len(),
            1,
            "Shift-JIS CSV must produce 1 record (旧仕様: 0件で error)"
        );
        assert_eq!(records[0].company_name, "株式会社A");
        assert_eq!(records[0].employment_type, "正社員");
    }

    #[test]
    fn fixa_dedupe_removes_exact_duplicate_rows() {
        // 修正前: 完全一致行が複数あれば全て records に追加 → 集計バイアス
        // 修正後: hash(title+company+location+salary+emp_type) で 1 件にまとめる
        let csv = "会社名,給与,雇用形態,勤務地,求人名\n\
                   株式会社A,月給25万円,正社員,東京都新宿区,事務スタッフ\n\
                   株式会社A,月給25万円,正社員,東京都新宿区,事務スタッフ\n\
                   株式会社A,月給25万円,正社員,東京都新宿区,事務スタッフ\n";
        let records = parse_csv_bytes(csv.as_bytes(), None).expect("parse");
        assert_eq!(records.len(), 1, "3 件の完全重複は 1 件にまとめる");
    }

    #[test]
    fn fixa_dedupe_keeps_different_employment_type_as_separate() {
        // 同一会社・同一給与でも雇用形態が違えば別求人 (V2 ルール / MEMORY feedback_dedup_rules)
        let csv = "会社名,給与,雇用形態,勤務地,求人名\n\
                   株式会社A,月給25万円,正社員,東京都新宿区,事務\n\
                   株式会社A,月給25万円,パート,東京都新宿区,事務\n";
        let records = parse_csv_bytes(csv.as_bytes(), None).expect("parse");
        assert_eq!(
            records.len(),
            2,
            "正社員/パートは別レコード (employment_type を dedupe key に含む)"
        );
    }

    #[test]
    fn fixa_dedupe_keeps_different_location_as_separate() {
        // 同一会社・同一給与でも勤務地が違えば別求人
        let csv = "会社名,給与,雇用形態,勤務地,求人名\n\
                   株式会社A,月給25万円,正社員,東京都新宿区,事務\n\
                   株式会社A,月給25万円,正社員,東京都港区,事務\n";
        let records = parse_csv_bytes(csv.as_bytes(), None).expect("parse");
        assert_eq!(records.len(), 2, "勤務地違い → 別レコード");
    }
}

// =============================================================================
// Finding #20: extract_annual_holidays 境界テスト + infer_employment_type テスト
// =============================================================================

#[cfg(test)]
mod annual_holidays_extraction_tests {
    use super::*;

    // ---- extract_annual_holidays 境界テスト ----

    #[test]
    fn boundary_69_invalid() {
        // 69 日 → None (下限 70 の -1)
        assert_eq!(
            extract_annual_holidays("年間休日69日"),
            None,
            "69日は下限70未満 → None"
        );
    }

    #[test]
    fn boundary_70_valid() {
        // 70 日 → Some(70) (下限ぴったり)
        assert_eq!(
            extract_annual_holidays("年間休日70日"),
            Some(70),
            "70日は下限ちょうど → Some(70)"
        );
    }

    #[test]
    fn boundary_180_valid() {
        // 180 日 → Some(180) (上限ぴったり)
        assert_eq!(
            extract_annual_holidays("年間休日180日"),
            Some(180),
            "180日は上限ちょうど → Some(180)"
        );
    }

    #[test]
    fn boundary_181_invalid() {
        // 181 日 → None (上限 180 の +1)
        assert_eq!(
            extract_annual_holidays("年間休日181日"),
            None,
            "181日は上限超過 → None"
        );
    }

    #[test]
    fn empty_text() {
        // 空文字列 → None
        assert_eq!(extract_annual_holidays(""), None, "空文字列 → None");
    }

    #[test]
    fn year_word_alone_not_matched() {
        // 「年100日キャンペーン」→ None (Commit 1 で「年」単独キーワードを削除した検証)
        // PREFIX_KEYWORDS に「年」は含まれない。「年間」「年休」「年間休日」等のみ対象。
        // 「年100日キャンペーン」にはこれらが含まれないので None を返すべき。
        assert_eq!(
            extract_annual_holidays("年100日キャンペーン"),
            None,
            "「年」単独は対象外 → None"
        );
    }

    // ---- infer_employment_type_for_jobbox テスト (Commit 3 で抽出した関数) ----

    #[test]
    fn infer_emp_monthly_returns_seishain() {
        assert_eq!(
            infer_employment_type_for_jobbox(&SalaryType::Monthly),
            Some("正社員".to_string()),
            "Monthly → 正社員"
        );
    }

    #[test]
    fn infer_emp_annual_returns_seishain() {
        assert_eq!(
            infer_employment_type_for_jobbox(&SalaryType::Annual),
            Some("正社員".to_string()),
            "Annual → 正社員"
        );
    }

    #[test]
    fn infer_emp_hourly_returns_part_time() {
        assert_eq!(
            infer_employment_type_for_jobbox(&SalaryType::Hourly),
            Some("パート・アルバイト".to_string()),
            "Hourly → パート・アルバイト"
        );
    }

    #[test]
    fn infer_emp_daily_returns_none() {
        assert_eq!(
            infer_employment_type_for_jobbox(&SalaryType::Daily),
            None,
            "Daily → None (推定対象外)"
        );
    }

    #[test]
    fn infer_emp_weekly_returns_none() {
        assert_eq!(
            infer_employment_type_for_jobbox(&SalaryType::Weekly),
            None,
            "Weekly → None (推定対象外)"
        );
    }
}

// =============================================================================
// 2026-06-30 Indeed (SP) スマホ版スクレイピング検出テスト
// =============================================================================

#[cfg(test)]
mod indeed_sp_detection_tests {
    use super::*;

    /// indeed-2026-06-30 (5).csv 相当の 29 列ヘッダー (css-u74ql7 含む)
    /// → CsvSource::IndeedSp が返ること
    #[test]
    fn detect_indeed_sp_via_unique_css_u74ql7() {
        let headers: Vec<String> = vec![
            "css-1hwmqh1",
            "css-bxyec3 href",
            "css-bxyec3",
            "css-lx9x6g",
            "css-14qk2ra",
            "css-18rxko3",
            "css-18rxko3 (2)",
            "css-ge6x3l src",
            "jobsearch-JobCard-tag",
            "jobsearch-JobCard-tag (2)",
            "jobsearch-JobCard-tag (3)",
            "jobsearch-JobCard-tag (4)",
            "jobsearch-JobCard-tag (5)",
            "jobsearch-JobCard-tag (6)",
            "jobsearch-JobCard-tag (7)",
            "jobsearch-JobCard-tag (8)",
            "jobsearch-JobCard-tag (9)",
            "jobsearch-JobCard-tag (10)",
            "jobsearch-JobCard-tag (11)",
            "css-1c8ncmc",
            "css-o67di7",
            "css-1vlebyu",
            "css-1vlebyu (2)",
            "css-1vlebyu (3)",
            "css-1vlebyu (4)",
            "css-1vlebyu (5)",
            "css-1hwmqh1 (2)",
            "css-u74ql7",
            "css-1vlebyu (6)",
        ]
        .into_iter()
        .map(String::from)
        .collect();
        assert_eq!(
            detect_csv_source(&headers),
            CsvSource::IndeedSp,
            "css-u74ql7 を含む Indeed SP ヘッダーは IndeedSp と判定されるべき"
        );
    }

    /// css-u74ql7 が無くても css-bxyec3 + css-1vlebyu の組み合わせなら SP 判定
    #[test]
    fn detect_indeed_sp_via_bxyec3_and_1vlebyu_combo() {
        let headers: Vec<String> = vec!["css-bxyec3", "css-1vlebyu"]
            .into_iter()
            .map(String::from)
            .collect();
        assert_eq!(detect_csv_source(&headers), CsvSource::IndeedSp);
    }

    /// 既存 Indeed (PC) ヘッダー (jcs-JobTitle) は IndeedSp ではなく Indeed と判定 (回帰防止)
    #[test]
    fn indeed_pc_still_detected_as_indeed_not_sp() {
        let headers: Vec<String> = vec!["jcs-JobTitle href", "jcs-JobTitle", "css-19eicqx"]
            .into_iter()
            .map(String::from)
            .collect();
        assert_eq!(
            detect_csv_source(&headers),
            CsvSource::Indeed,
            "Indeed PC ヘッダーは IndeedSp 判定にすり抜けないこと"
        );
    }

    /// 求人ボックスは引き続き JobBox と判定 (回帰防止)
    #[test]
    fn jobbox_still_detected_as_jobbox() {
        let headers: Vec<String> = vec!["p-result_title_link href", "p-result_name", "c-icon"]
            .into_iter()
            .map(String::from)
            .collect();
        assert_eq!(detect_csv_source(&headers), CsvSource::JobBox);
    }

    /// UserSourceHint::from_str("indeed_sp") → IndeedSp にマッピング
    #[test]
    fn user_source_hint_from_str_maps_indeed_sp() {
        assert_eq!(
            UserSourceHint::from_str("indeed_sp"),
            UserSourceHint::IndeedSp
        );
        assert_eq!(UserSourceHint::from_str("indeed"), UserSourceHint::Indeed);
        assert_eq!(UserSourceHint::from_str("jobbox"), UserSourceHint::JobBox);
        assert_eq!(UserSourceHint::from_str("other"), UserSourceHint::Other);
        assert_eq!(UserSourceHint::from_str(""), UserSourceHint::Auto);
    }

    /// build_column_map (IndeedSp) で description が css-1vlebyu に紐付き、
    /// tags が css-u74ql7 (人気タグ) または jobsearch-JobCard-tag のいずれかに紐付くこと
    #[test]
    fn build_column_map_indeed_sp_maps_critical_columns() {
        let headers: Vec<String> = vec![
            "css-1hwmqh1",           // 0 employment_type
            "css-bxyec3 href",       // 1 url
            "css-bxyec3",            // 2 job_title
            "css-lx9x6g",            // 3
            "css-14qk2ra",           // 4 company_name
            "css-18rxko3",           // 5 location
            "css-18rxko3 (2)",       // 6 salary
            "css-ge6x3l src",        // 7
            "jobsearch-JobCard-tag", // 8 tags (最初に来るのでこちらが採用)
            "css-1vlebyu",           // 9 ... 待って position が違う
            "css-u74ql7",            // 10 人気タグ
        ]
        .into_iter()
        .map(String::from)
        .collect();
        let map = build_column_map(&headers, &CsvSource::IndeedSp);
        assert_eq!(map.get("employment_type"), Some(&0));
        assert_eq!(map.get("url"), Some(&1));
        assert_eq!(map.get("job_title"), Some(&2));
        assert_eq!(map.get("company_name"), Some(&4));
        assert_eq!(map.get("location"), Some(&5));
        assert_eq!(map.get("salary"), Some(&6));
        // tags: jobsearch-JobCard-tag (col 8) が先に出現するためそちらが採用される
        assert_eq!(map.get("tags"), Some(&8));
        // description: css-1vlebyu (col 9)
        assert_eq!(map.get("description"), Some(&9));
    }

    // =========================================================================
    // Finding #11: parse_csv_bytes_with_hints による IndeedSp 統合テスト
    //              (Commit 1 #4 / Commit 3 回帰防止)
    // =========================================================================

    /// css-u74ql7 列の値が tags_raw に重複なく append される (Commit 3 検証)
    /// ミニ CSV (IndeedSp 形式、2 行) を parse_csv_bytes_with_hints に渡し、
    /// tags_raw に "超人気" が含まれることを確認する。
    #[test]
    fn indeed_sp_popular_tag_appended_to_tags_raw() {
        // IndeedSp 最小ヘッダー: employment_type, url, job_title, company_name, location, salary,
        //   jobsearch-JobCard-tag (既存タグ列), description, css-u74ql7 (人気タグ列)
        let csv = "css-1hwmqh1,css-bxyec3 href,css-bxyec3,css-14qk2ra,css-18rxko3,css-18rxko3 (2),jobsearch-JobCard-tag,css-1vlebyu,css-u74ql7\n\
                   正社員,https://example.com/1,看護師,A病院,東京都千代田区,月給30万円〜35万円,週休2日,年間休日120日,超人気\n";
        let records = parse_csv_bytes_with_hints(csv.as_bytes(), None, UserSourceHint::IndeedSp)
            .expect("IndeedSp CSV parse");
        assert_eq!(records.len(), 1, "1 件パースされる");
        assert_eq!(records[0].source, CsvSource::IndeedSp, "source=IndeedSp");
        assert!(
            records[0].tags_raw.contains("超人気"),
            "css-u74ql7 列の '超人気' が tags_raw に含まれる: got '{}'",
            records[0].tags_raw
        );
    }

    /// Commit 1 #4 検証: IndeedSp で employment_type 空欄レコード →
    /// employment_type は空のまま (Monthly → 正社員 フォールバックが発動しない)
    /// 2026-07-01 ユーザー方針転換: IndeedSp の月給レコードで employment_type 空欄 → "正社員" にフォールバック
    /// (css-1hwmqh1 列消失に対応。契約社員も月給なら正社員扱いで十分という方針)
    #[test]
    fn indeed_sp_monthly_employment_falls_back_to_seishin() {
        // job_title を col[0] に置いてメタデータ除外を回避し、
        // employment_type 列 (css-1hwmqh1) を空欄にする
        let csv = "css-bxyec3,css-14qk2ra,css-18rxko3,css-18rxko3 (2),css-1vlebyu,css-u74ql7,css-1hwmqh1\n\
                   介護職,B施設,大阪府大阪市,月給25万円,年間休日125日,人気,\n";
        let records = parse_csv_bytes_with_hints(csv.as_bytes(), None, UserSourceHint::IndeedSp)
            .expect("IndeedSp CSV parse");
        assert_eq!(records.len(), 1, "1 件パースされる");
        assert_eq!(records[0].source, CsvSource::IndeedSp);
        // 2026-07-01 復活: IndeedSp も月給空欄 → "正社員" にフォールバック
        assert_eq!(
            records[0].employment_type, "正社員",
            "IndeedSp: Monthly + 雇用形態空欄 → '正社員' フォールバック: got '{}'",
            records[0].employment_type
        );
    }

    /// 2026-07-01 IndeedSp の時給レコードで employment_type 空欄 → "パート・アルバイト" にフォールバック
    #[test]
    fn indeed_sp_hourly_employment_falls_back_to_part() {
        // 時給形式の給与 + 雇用形態列空欄
        let csv = "css-bxyec3,css-14qk2ra,css-18rxko3,css-18rxko3 (2),css-1vlebyu,css-u74ql7,css-1hwmqh1\n\
                   介護補助,C施設,東京都新宿区,時給1200円,年間休日110日,,\n";
        let records = parse_csv_bytes_with_hints(csv.as_bytes(), None, UserSourceHint::IndeedSp)
            .expect("IndeedSp CSV parse");
        assert_eq!(records.len(), 1, "1 件パースされる");
        assert_eq!(records[0].source, CsvSource::IndeedSp);
        // 時給 + 空欄 → "パート・アルバイト"
        assert_eq!(
            records[0].employment_type, "パート・アルバイト",
            "IndeedSp: Hourly + 雇用形態空欄 → 'パート・アルバイト' フォールバック: got '{}'",
            records[0].employment_type
        );
    }

    /// JobBox の Monthly レコードで employment_type 空欄 → "正社員" にフォールバック
    /// (JobBox の挙動は不変であることを保証 — Commit 1 #4 で IndeedSp 分岐を追加したが
    ///  JobBox の既存ロジックに回帰がないことを確認)
    #[test]
    fn jobbox_employment_type_fallback_still_works() {
        // 求人ボックス形式 (日本語ヘッダー) で雇用形態列を空にする
        let csv = "企業名,給与,雇用形態,勤務地,求人名\n\
                   テスト株式会社,月給25万円〜30万円,,東京都新宿区,一般事務\n";
        let records = parse_csv_bytes_with_hints(csv.as_bytes(), None, UserSourceHint::JobBox)
            .expect("JobBox CSV parse");
        assert_eq!(records.len(), 1, "1 件パースされる");
        assert_eq!(records[0].source, CsvSource::JobBox);
        // JobBox: Monthly 空欄 → infer_employment_type_for_jobbox → "正社員"
        assert_eq!(
            records[0].employment_type, "正社員",
            "JobBox Monthly + 雇用形態空欄 → '正社員' フォールバック"
        );
    }
}

// =============================================================================
// 機能 E / C: Gemini AI フォールバック補助関数のユニットテスト (ネットワーク非依存)
// =============================================================================

#[cfg(test)]
mod gemini_fallback_tests {
    use super::*;
    use serde_json::json;

    // ---- 機能 E: is_parse_poor ----

    fn dummy_record(title: &str, company: &str, salary: &str) -> SurveyRecord {
        SurveyRecord {
            row_index: 0,
            source: CsvSource::Unknown,
            job_title: title.to_string(),
            company_name: company.to_string(),
            location_raw: String::new(),
            salary_raw: salary.to_string(),
            employment_type: String::new(),
            tags_raw: String::new(),
            url: None,
            is_new: false,
            description: String::new(),
            salary_parsed: parse_salary(salary, SalaryType::Monthly),
            location_parsed: parse_location("", None),
            annual_holidays: None,
        }
    }

    #[test]
    fn is_parse_poor_true_when_salary_all_empty() {
        let recs = vec![
            dummy_record("看護師", "A病院", ""),
            dummy_record("介護職", "B施設", ""),
        ];
        assert!(is_parse_poor(&recs), "salary が全行空 → 貧弱");
    }

    #[test]
    fn is_parse_poor_false_when_all_columns_present() {
        let recs = vec![
            dummy_record("看護師", "A病院", "月給30万円"),
            dummy_record("介護職", "B施設", "月給25万円"),
        ];
        assert!(!is_parse_poor(&recs), "全列が埋まっていれば非貧弱");
    }

    #[test]
    fn is_parse_poor_false_when_empty() {
        assert!(!is_parse_poor(&[]), "空集合はエラー経路 → 貧弱扱いしない");
    }

    // ---- 機能 E: ai_role_to_colmap_key / parse_colmap_from_ai ----

    #[test]
    fn ai_role_maps_expected_keys() {
        assert_eq!(ai_role_to_colmap_key("title"), Some("job_title"));
        assert_eq!(ai_role_to_colmap_key("company"), Some("company_name"));
        assert_eq!(ai_role_to_colmap_key("salary"), Some("salary"));
        assert_eq!(ai_role_to_colmap_key("ignore"), None);
        assert_eq!(ai_role_to_colmap_key("unknown_role"), None);
    }

    #[test]
    fn parse_colmap_from_ai_builds_map_and_filters() {
        // モック Gemini レスポンス JSON
        let resp = json!({
            "mappings": [
                { "column_index": 0, "role": "company" },
                { "column_index": 1, "role": "title" },
                { "column_index": 2, "role": "salary" },
                { "column_index": 3, "role": "ignore" },      // ignore は除外
                { "column_index": 99, "role": "location" },   // 範囲外は除外
            ]
        });
        let map = parse_colmap_from_ai(&resp, 4);
        assert_eq!(map.get("company_name"), Some(&0));
        assert_eq!(map.get("job_title"), Some(&1));
        assert_eq!(map.get("salary"), Some(&2));
        assert!(!map.contains_key("location"), "範囲外 index は補完しない");
        assert_eq!(map.len(), 3, "ignore + 範囲外を除いた 3 件");
    }

    #[test]
    fn parse_colmap_from_ai_empty_on_garbage() {
        // mappings 欠落 → 空マップ (呼び出し側は再パースをスキップ = 従来動作)
        assert!(parse_colmap_from_ai(&json!({"foo": 1}), 5).is_empty());
    }

    // ---- 機能 E: col_overrides による再パース ----

    #[test]
    fn col_overrides_supplements_columns_on_reparse() {
        // ヘッダーが CSS クラス名等で通常検出できない CSV を想定。
        // AI 推定 (col 0=company, 1=title, 2=salary, 3=location) を override として渡すと
        // 各列が正しく紐付いて 1 レコードが得られる。
        let csv = "x0,x1,x2,x3\n\
                   株式会社テスト,看護師,月給30万円,東京都新宿区\n";
        // override なしでは salary/location が取れない可能性を確認 (最低限、override で確実に取れる)
        let mut ov: std::collections::HashMap<&'static str, usize> =
            std::collections::HashMap::new();
        ov.insert("company_name", 0);
        ov.insert("job_title", 1);
        ov.insert("salary", 2);
        ov.insert("location", 3);
        let records =
            parse_csv_bytes_with_col_overrides(csv.as_bytes(), None, UserSourceHint::Other, &ov)
                .expect("override parse");
        assert_eq!(records.len(), 1);
        assert_eq!(records[0].company_name, "株式会社テスト");
        assert_eq!(records[0].job_title, "看護師");
        assert_eq!(records[0].salary_raw, "月給30万円");
    }

    #[test]
    fn empty_overrides_matches_legacy_output() {
        // override が空なら parse_csv_bytes_with_hints と完全一致 (フォールバック証明)
        let csv = "企業名,給与,雇用形態,勤務地,求人名\n\
                   株式会社A,月給25万円,正社員,東京都新宿区,事務\n";
        let legacy = parse_csv_bytes_with_hints(csv.as_bytes(), None, UserSourceHint::JobBox)
            .expect("legacy");
        let empty_ov: std::collections::HashMap<&'static str, usize> =
            std::collections::HashMap::new();
        let overridden = parse_csv_bytes_with_col_overrides(
            csv.as_bytes(),
            None,
            UserSourceHint::JobBox,
            &empty_ov,
        )
        .expect("empty override");
        assert_eq!(legacy.len(), overridden.len());
        assert_eq!(legacy[0].company_name, overridden[0].company_name);
        assert_eq!(legacy[0].salary_raw, overridden[0].salary_raw);
        assert_eq!(legacy[0].employment_type, overridden[0].employment_type);
    }

    // ---- 機能 C: collect_holiday_targets ----

    #[test]
    fn collect_holiday_targets_filters_and_trims() {
        let long_desc: String = "年".repeat(600); // 600 文字 (>= 30, かつ >500 でトリム対象)
        let mut r_target = dummy_record("t", "c", "月給20万円");
        r_target.description = long_desc;
        r_target.annual_holidays = None;

        // description 短すぎ (< 30) → 対象外
        let mut r_short = dummy_record("t", "c", "月給20万円");
        r_short.description = "短い説明".to_string();

        // 既に annual_holidays あり → 対象外
        let mut r_filled = dummy_record("t", "c", "月給20万円");
        r_filled.description = "年".repeat(50);
        r_filled.annual_holidays = Some(120);

        let recs = vec![r_target, r_short, r_filled];
        let targets = collect_holiday_targets(&recs);
        assert_eq!(targets.len(), 1, "対象は 1 件のみ (index 0)");
        assert_eq!(targets[0].0, 0, "レコード index が保持される");
        assert_eq!(
            targets[0].1.chars().count(),
            500,
            "description は 500 文字にトリムされる"
        );
    }

    // ---- 機能 C: parse_holiday_response (70-180 検証) ----

    #[test]
    fn parse_holiday_response_validates_range_and_null() {
        let resp = json!({
            "results": [
                { "index": 0, "annual_holidays": 120 },   // 有効
                { "index": 1, "annual_holidays": 69 },    // 範囲外 (下限-1)
                { "index": 2, "annual_holidays": 181 },   // 範囲外 (上限+1)
                { "index": 3, "annual_holidays": null },  // null → 除外
                { "index": 4, "annual_holidays": 70 },    // 有効 (下限)
                { "index": 5, "annual_holidays": 180 },   // 有効 (上限)
            ]
        });
        let parsed = parse_holiday_response(&resp);
        assert_eq!(
            parsed,
            vec![(0, 120), (4, 70), (5, 180)],
            "70-180 の整数のみ通過、null/範囲外は除外"
        );
    }

    #[test]
    fn parse_holiday_response_empty_on_garbage() {
        assert!(parse_holiday_response(&json!({"x": 1})).is_empty());
    }

    // ---- 機能 C: apply_holiday_results ----

    #[test]
    fn apply_holiday_results_fills_only_none_and_counts() {
        let mut recs = vec![
            dummy_record("a", "c", "月給20万円"), // index 0: None → 埋まる
            {
                let mut r = dummy_record("b", "c", "月給20万円");
                r.annual_holidays = Some(100); // index 1: 既存 → 上書きしない
                r
            },
        ];
        // index 2 は範囲外 (records.len()=2) → 無視
        let results = vec![(0, 125), (1, 130), (2, 120)];
        let applied = apply_holiday_results(&mut recs, &results);
        assert_eq!(applied, 1, "実際に反映されたのは index 0 の 1 件のみ");
        assert_eq!(recs[0].annual_holidays, Some(125));
        assert_eq!(recs[1].annual_holidays, Some(100), "既存値は上書きされない");
    }

    // ---- schema 形状 (最低限の健全性) ----

    #[test]
    fn schemas_have_expected_top_level_keys() {
        let colmap = build_colmap_schema();
        assert!(colmap["properties"]["mappings"].is_object());
        let holiday = build_holiday_schema();
        assert!(holiday["properties"]["results"].is_object());
    }
}
