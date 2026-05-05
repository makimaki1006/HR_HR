//! 採用マーケットインテリジェンス データアクセス層 (Phase 3 Step 1)
//!
//! 媒体分析レポート拡張のための市区町村単位の事前集計テーブル読取専用関数群。
//!
//! ## 対象テーブル (DDL: docs/survey_market_intelligence_phase0_2_schema.sql)
//!
//! | テーブル | 用途 | 状態 |
//! |---------|------|------|
//! | `municipality_recruiting_scores` | 配信優先度スコア | 未投入 (Phase 3 後続で計算投入) |
//! | `municipality_living_cost_proxy` | 生活コスト proxy | 未投入 |
//! | `commute_flow_summary` | 通勤OD要約 (TOP N 流入元) | 未投入 (`v2_external_commute_od` でフォールバック計算) |
//! | `municipality_occupation_population` | 市区町村×職業×年齢×性別 人口 | 未投入 |
//!
//! ## 設計方針
//!
//! - **READ-ONLY**: SELECT のみ。書き込みは Phase 3 後続スコープ。
//! - **table_exists チェック**: テーブル不在時は空 Vec 返却 (フェイルセーフ)
//! - **Turso 優先**: `query_turso_or_local` で Turso V2 を正本とする
//! - **DTO 二段構成 (Step 2 で追加)**: Step 1 の `fetch_*` は `Vec<Row>` を返却 (互換維持)。
//!   Step 2 で `to_*_dto` 変換関数 + 型付き DTO を追加 (HTML 層で使う)。
//! - **Phase 0〜2 docs 準拠**: `SURVEY_MARKET_INTELLIGENCE_PHASE0_2_PREP.md` §5 Step 1〜2 の関数群を実装
//!
//! 詳細仕様: `docs/SURVEY_MARKET_INTELLIGENCE_METRICS.md`

use serde_json::Value;
use std::collections::HashMap;

use super::super::super::helpers::table_exists;
use super::query_turso_or_local;

#[allow(dead_code)]
type Db = crate::db::local_sqlite::LocalDb;
#[allow(dead_code)]
type TursoDb = crate::db::turso_http::TursoDb;
#[allow(dead_code)]
type Row = HashMap<String, Value>;

/// `top_n` の安全上限 (悪意のある呼び出しによる過大クエリを防止)。
const MAX_TOP_N: usize = 100;

/// 市区町村別 × 職業グループ別の配信優先度スコアを取得する。
///
/// `municipality_recruiting_scores` テーブルから事前集計済のスコアを読む。
/// 詳細スキーマ: `docs/survey_market_intelligence_phase0_2_schema.sql` の同名テーブル。
///
/// # 引数
/// - `municipality_codes`: 取得対象の市区町村コード一覧 (空ならスコープなし → 空 Vec)
/// - `occupation_group_code`: 職業グループコード (空文字なら全職業)
///
/// # 戻り値
/// 各レコードのカラムを保持する `Vec<Row>`。テーブル不在 or 結果なしの場合は空 Vec。
pub(crate) fn fetch_recruiting_scores_by_municipalities(
    db: &Db,
    turso: Option<&TursoDb>,
    municipality_codes: &[&str],
    occupation_group_code: &str,
) -> Vec<Row> {
    if municipality_codes.is_empty() {
        return vec![];
    }
    // Turso/local いずれにも未投入時は空 Vec
    if !table_exists(db, "municipality_recruiting_scores") && turso.is_none() {
        return vec![];
    }

    let select_cols = "municipality_code, prefecture, municipality_name, \
                       occupation_group_code, occupation_group_name, \
                       target_population, adjacent_population, \
                       media_job_count, competitor_job_count, \
                       median_salary_yen, effective_wage_index, \
                       commute_reach_score, job_competition_score, \
                       establishment_competition_score, wage_competitiveness_score, \
                       living_cost_score, effective_wage_score, \
                       distribution_priority_score, \
                       scenario_conservative_population, \
                       scenario_standard_population, \
                       scenario_aggressive_population, \
                       source_year";

    let (sql, params): (String, Vec<String>) = if occupation_group_code.is_empty() {
        // ?1..?N に municipality_codes をバインド
        let placeholders: String = (1..=municipality_codes.len())
            .map(|i| format!("?{i}"))
            .collect::<Vec<_>>()
            .join(",");
        let sql = format!(
            "SELECT {select_cols} \
             FROM municipality_recruiting_scores \
             WHERE municipality_code IN ({placeholders}) \
             ORDER BY distribution_priority_score DESC"
        );
        let params = municipality_codes.iter().map(|s| s.to_string()).collect();
        (sql, params)
    } else {
        // ?1 = occupation_group_code, ?2..?(N+1) = municipality_codes
        let placeholders: String = (2..=(municipality_codes.len() + 1))
            .map(|i| format!("?{i}"))
            .collect::<Vec<_>>()
            .join(",");
        let sql = format!(
            "SELECT {select_cols} \
             FROM municipality_recruiting_scores \
             WHERE occupation_group_code = ?1 \
               AND municipality_code IN ({placeholders}) \
             ORDER BY distribution_priority_score DESC"
        );
        let mut params: Vec<String> = vec![occupation_group_code.to_string()];
        params.extend(municipality_codes.iter().map(|s| s.to_string()));
        (sql, params)
    };

    query_turso_or_local(turso, db, &sql, &params, "municipality_recruiting_scores")
}

/// 市区町村別の生活コスト proxy データを取得する。
///
/// `municipality_living_cost_proxy` テーブルから単身向け/小世帯向け家賃 proxy・
/// 物価指数・地価などを取得する。
///
/// 詳細仕様: `docs/SURVEY_MARKET_INTELLIGENCE_METRICS.md` §7
pub(crate) fn fetch_living_cost_proxy(
    db: &Db,
    turso: Option<&TursoDb>,
    municipality_codes: &[&str],
) -> Vec<Row> {
    if municipality_codes.is_empty() {
        return vec![];
    }
    if !table_exists(db, "municipality_living_cost_proxy") && turso.is_none() {
        return vec![];
    }

    let placeholders: String = (1..=municipality_codes.len())
        .map(|i| format!("?{i}"))
        .collect::<Vec<_>>()
        .join(",");

    let sql = format!(
        "SELECT municipality_code, prefecture, municipality_name, \
                single_household_rent_proxy, small_household_rent_proxy, \
                rent_per_square_meter, retail_price_index_proxy, \
                household_spending_annual_yen, land_price_residential_per_sqm, \
                housing_cost_rank, source_year \
         FROM municipality_living_cost_proxy \
         WHERE municipality_code IN ({placeholders}) \
         ORDER BY municipality_code"
    );

    let params: Vec<String> = municipality_codes.iter().map(|s| s.to_string()).collect();

    query_turso_or_local(turso, db, &sql, &params, "municipality_living_cost_proxy")
}

/// 目的地市区町村への通勤流入元 TOP N を取得する。
///
/// 優先順:
/// 1. `commute_flow_summary` (事前集計テーブル) があればそれを使う
/// 2. なければ `v2_external_commute_od` (国勢調査 OD 行列、実存) から動的に TOP N 計算
///
/// # 引数
/// - `dest_pref` / `dest_muni`: 目的地 (どちらか空なら空 Vec)
/// - `top_n`: 上位何件まで取るか (内部で `MAX_TOP_N` (=100) でクランプ)
///
/// # 戻り値
/// 流入元の各レコード。`commute_flow_summary` 経由なら `origin_municipality_code` 等が、
/// `v2_external_commute_od` 経由なら `origin_pref` / `origin_muni` / `total_commuters` 等が含まれる。
/// テーブル/データ不在時は空 Vec。
///
/// 詳細仕様: `docs/SURVEY_MARKET_INTELLIGENCE_METRICS.md` §6
pub(crate) fn fetch_commute_flow_summary(
    db: &Db,
    turso: Option<&TursoDb>,
    dest_pref: &str,
    dest_muni: &str,
    top_n: usize,
) -> Vec<Row> {
    if dest_pref.is_empty() || dest_muni.is_empty() {
        return vec![];
    }
    let limit = top_n.min(MAX_TOP_N).max(1);

    // 優先 1: 事前集計テーブル
    if table_exists(db, "commute_flow_summary") {
        let sql = format!(
            "SELECT origin_municipality_code, origin_prefecture, origin_municipality_name, \
                    flow_count, flow_share, target_origin_population, \
                    estimated_target_flow_conservative, \
                    estimated_target_flow_standard, \
                    estimated_target_flow_aggressive, \
                    rank_to_destination, source_year \
             FROM commute_flow_summary \
             WHERE destination_prefecture = ?1 AND destination_municipality_name = ?2 \
             ORDER BY rank_to_destination LIMIT {limit}"
        );
        let params = vec![dest_pref.to_string(), dest_muni.to_string()];
        return query_turso_or_local(turso, db, &sql, &params, "commute_flow_summary");
    }

    // 優先 2: v2_external_commute_od (国勢調査 OD) からフォールバック計算
    // ローカル table_exists チェックでローカル不在時は Turso 経由のみ試行
    let local_has = table_exists(db, "v2_external_commute_od");
    if !local_has && turso.is_none() {
        return vec![];
    }
    let sql = format!(
        "SELECT origin_pref, origin_muni, \
                total_commuters, male_commuters, female_commuters, reference_year \
         FROM v2_external_commute_od \
         WHERE dest_pref = ?1 AND dest_muni = ?2 \
           AND (origin_pref != dest_pref OR origin_muni != dest_muni) \
         ORDER BY total_commuters DESC LIMIT {limit}"
    );
    let params = vec![dest_pref.to_string(), dest_muni.to_string()];
    query_turso_or_local(turso, db, &sql, &params, "v2_external_commute_od")
}

/// 市区町村単位の職業×年齢×性別人口を取得する。
///
/// `municipality_occupation_population` テーブルから国勢調査由来の常住地ベース職業人口を取得する。
///
/// # 引数
/// - `municipality_code`: 取得対象の市区町村コード (空なら空 Vec)
/// - `basis`: `"resident"` (常住地) または `"workplace"` (従業地) (空文字なら両方)
/// - `occupation_codes`: 職業大分類コード一覧 (空なら全職業)
///
/// 詳細仕様: `docs/SURVEY_MARKET_INTELLIGENCE_METRICS.md` §3-§4
pub(crate) fn fetch_occupation_population(
    db: &Db,
    turso: Option<&TursoDb>,
    municipality_code: &str,
    basis: &str,
    occupation_codes: &[&str],
) -> Vec<Row> {
    if municipality_code.is_empty() {
        return vec![];
    }
    if !table_exists(db, "municipality_occupation_population") && turso.is_none() {
        return vec![];
    }

    // ベース WHERE: municipality_code (?1) + basis (?2、空なら省略)
    let mut params: Vec<String> = vec![municipality_code.to_string()];
    let mut where_clauses: Vec<String> = vec!["municipality_code = ?1".to_string()];

    if !basis.is_empty() {
        params.push(basis.to_string());
        where_clauses.push("basis = ?2".to_string());
    }

    // occupation_codes IN (...)
    if !occupation_codes.is_empty() {
        let start = params.len() + 1;
        let placeholders: String = (start..start + occupation_codes.len())
            .map(|i| format!("?{i}"))
            .collect::<Vec<_>>()
            .join(",");
        where_clauses.push(format!("occupation_code IN ({placeholders})"));
        for c in occupation_codes {
            params.push(c.to_string());
        }
    }

    let where_sql = where_clauses.join(" AND ");
    let sql = format!(
        "SELECT municipality_code, prefecture, municipality_name, basis, \
                occupation_code, occupation_name, age_group, gender, \
                population, source_year \
         FROM municipality_occupation_population \
         WHERE {where_sql} \
         ORDER BY occupation_code, age_group, gender"
    );

    query_turso_or_local(turso, db, &sql, &params, "municipality_occupation_population")
}

// ============================================================
// Phase 3 Step 2: 型付き DTO 層
// ============================================================
//
// `Vec<Row>` ベースの fetch 結果を、レポート実装で安全に使える型付き DTO に変換する。
// HTML 層は DTO だけを参照する設計とし、Row 直接参照は避ける。
//
// 設計:
// - 主キー文字列カラムは `String` (空文字 fallback)
// - 数値カラムは `Option<i64>` / `Option<f64>` (NULL 区別を保持)
// - Turso HTTP API の文字列表現 (e.g. `"1973395"`) は変換ヘルパーで吸収
// - `serde::Serialize` を実装し、JSON シリアライズ可能
//
// 詳細: docs/SURVEY_MARKET_INTELLIGENCE_METRICS.md

use serde::Serialize;

// -------- Phase 3 Step 5 Phase 2: データソースラベル分類 --------

/// データソース種別ラベル (basis × data_label の直積を 1 つの enum で表現)
///
/// XOR 不変条件:
/// - `*Measured` / `ResidentActual` 系: `population` のみ Some
/// - `*EstimatedBeta` 系: `estimate_index` のみ Some
/// - `AggregateParent`: 親市集約の UI 暫定表示 (生データではない)
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize)]
#[serde(rename_all = "snake_case")]
#[allow(dead_code)]
pub enum DataSourceLabel {
    /// basis=resident + measured (将来予約、現状未投入)
    ResidentActual,
    /// basis=resident + estimated_beta (Model F2 推定指数)
    ResidentEstimatedBeta,
    /// basis=workplace + measured (e-Stat 15-1 国勢調査)
    WorkplaceMeasured,
    /// basis=workplace + estimated_beta (15-1 fallback)
    WorkplaceEstimatedBeta,
    /// 親市集約表示 (UI 暫定、生データではない)
    AggregateParent,
}

// -------- Turso 文字列 ↔ 数値表現の差を吸収する Option ヘルパー --------

/// `Row` から `Option<i64>` を取得する。
///
/// JSON Number / String どちらも i64 にパース可能。
/// NULL / 未存在 / パース不能は `None` を返す (panic しない)。
#[allow(dead_code)]
pub(crate) fn opt_i64(row: &Row, key: &str) -> Option<i64> {
    let v = row.get(key)?;
    if v.is_null() {
        return None;
    }
    v.as_i64()
        .or_else(|| v.as_f64().map(|f| f as i64))
        .or_else(|| v.as_str().and_then(|s| s.parse::<i64>().ok()))
}

/// `Row` から `Option<f64>` を取得する。
///
/// JSON Number / String どちらも f64 にパース可能。
#[allow(dead_code)]
pub(crate) fn opt_f64(row: &Row, key: &str) -> Option<f64> {
    let v = row.get(key)?;
    if v.is_null() {
        return None;
    }
    v.as_f64()
        .or_else(|| v.as_i64().map(|i| i as f64))
        .or_else(|| v.as_str().and_then(|s| s.parse::<f64>().ok()))
}

/// `Row` から `String` を取得する (NULL / 未存在は空文字)。
///
/// 既存 `helpers::get_str` と同等。Turso の文字列値 (Value::String) と
/// JSON Number から `to_string()` の両方に対応。
#[allow(dead_code)]
pub(crate) fn str_or_empty(row: &Row, key: &str) -> String {
    match row.get(key) {
        Some(Value::String(s)) => s.clone(),
        Some(Value::Number(n)) => n.to_string(),
        Some(Value::Bool(b)) => b.to_string(),
        _ => String::new(),
    }
}

// -------- DTO: `municipality_recruiting_scores` --------

/// 市区町村 × 職業グループ別の配信優先度スコア
///
/// 詳細仕様: `docs/SURVEY_MARKET_INTELLIGENCE_METRICS.md` §2 配信優先度スコア
#[derive(Clone, Debug, Default, Serialize)]
#[allow(dead_code)]
pub struct MunicipalityRecruitingScore {
    pub municipality_code: String,
    pub prefecture: String,
    pub municipality_name: String,
    pub occupation_group_code: String,
    pub occupation_group_name: String,

    pub target_population: Option<i64>,
    pub adjacent_population: Option<i64>,
    pub media_job_count: Option<i64>,
    pub competitor_job_count: Option<i64>,

    pub median_salary_yen: Option<i64>,
    pub effective_wage_index: Option<f64>,

    /// 0〜100 指数 (高いほど通勤到達性が高い)
    pub commute_reach_score: Option<f64>,
    /// 0〜100 指数 (高いほど競合が強い、減衰寄与)
    pub job_competition_score: Option<f64>,
    /// 0〜100 指数 (高いほど競合事業所が多い)
    pub establishment_competition_score: Option<f64>,
    /// 0〜100 指数 (高いほど給与競争力が高い)
    pub wage_competitiveness_score: Option<f64>,
    /// 0〜100 指数 (高いほど生活コスト面で有利)
    pub living_cost_score: Option<f64>,
    /// 0〜100 指数
    pub effective_wage_score: Option<f64>,

    /// 0〜100 指数。METRICS.md §2.1 の `clamp(positive_score * (1 - penalty_reduction_pct/100), 0, 100)`
    pub distribution_priority_score: Option<f64>,

    pub scenario_conservative_population: Option<i64>,
    pub scenario_standard_population: Option<i64>,
    pub scenario_aggressive_population: Option<i64>,

    pub source_year: Option<i64>,
}

#[allow(dead_code)]
impl MunicipalityRecruitingScore {
    /// `Row` から DTO を構築する。NULL / パース不能なフィールドは `None` / 空文字 fallback。
    pub fn from_row(row: &Row) -> Self {
        Self {
            municipality_code: str_or_empty(row, "municipality_code"),
            prefecture: str_or_empty(row, "prefecture"),
            municipality_name: str_or_empty(row, "municipality_name"),
            occupation_group_code: str_or_empty(row, "occupation_group_code"),
            occupation_group_name: str_or_empty(row, "occupation_group_name"),
            target_population: opt_i64(row, "target_population"),
            adjacent_population: opt_i64(row, "adjacent_population"),
            media_job_count: opt_i64(row, "media_job_count"),
            competitor_job_count: opt_i64(row, "competitor_job_count"),
            median_salary_yen: opt_i64(row, "median_salary_yen"),
            effective_wage_index: opt_f64(row, "effective_wage_index"),
            commute_reach_score: opt_f64(row, "commute_reach_score"),
            job_competition_score: opt_f64(row, "job_competition_score"),
            establishment_competition_score: opt_f64(row, "establishment_competition_score"),
            wage_competitiveness_score: opt_f64(row, "wage_competitiveness_score"),
            living_cost_score: opt_f64(row, "living_cost_score"),
            effective_wage_score: opt_f64(row, "effective_wage_score"),
            distribution_priority_score: opt_f64(row, "distribution_priority_score"),
            scenario_conservative_population: opt_i64(row, "scenario_conservative_population"),
            scenario_standard_population: opt_i64(row, "scenario_standard_population"),
            scenario_aggressive_population: opt_i64(row, "scenario_aggressive_population"),
            source_year: opt_i64(row, "source_year"),
        }
    }

    /// 母集団シナリオが「保守 ≤ 標準 ≤ 強気」を満たすか。
    ///
    /// METRICS.md §9 の制約。3 値とも値があるときのみ厳密に検証し、
    /// 欠損ありなら `true` (検証不可) を返す。
    pub fn is_scenario_consistent(&self) -> bool {
        match (
            self.scenario_conservative_population,
            self.scenario_standard_population,
            self.scenario_aggressive_population,
        ) {
            (Some(c), Some(s), Some(a)) => c <= s && s <= a,
            _ => true,
        }
    }

    /// `distribution_priority_score` が `[0.0, 100.0]` の範囲内か。
    ///
    /// METRICS.md §2.1 の `clamp(..., 0, 100)` 制約。値があるときのみ検証。
    pub fn is_priority_score_in_range(&self) -> bool {
        match self.distribution_priority_score {
            Some(s) => (0.0..=100.0).contains(&s) && !s.is_nan(),
            None => true,
        }
    }
}

// -------- DTO: `municipality_living_cost_proxy` --------

/// 市区町村別の生活コスト proxy
///
/// 詳細仕様: `docs/SURVEY_MARKET_INTELLIGENCE_METRICS.md` §7 生活コスト補正後給与魅力度
#[derive(Clone, Debug, Default, Serialize)]
#[allow(dead_code)]
pub struct LivingCostProxy {
    pub municipality_code: String,
    pub prefecture: String,
    pub municipality_name: String,

    /// 円/月。公式統計から作る「単身向け相当」家賃 proxy。
    pub single_household_rent_proxy: Option<i64>,
    /// 円/月。「小世帯向け相当」。
    pub small_household_rent_proxy: Option<i64>,
    /// 円/㎡
    pub rent_per_square_meter: Option<f64>,
    /// 100 を基準値とする小売物価指数
    pub retail_price_index_proxy: Option<f64>,
    /// 円/年
    pub household_spending_annual_yen: Option<i64>,
    /// 円/㎡
    pub land_price_residential_per_sqm: Option<f64>,
    /// 全国順位 (1=住居コストが低い)
    pub housing_cost_rank: Option<i64>,

    pub source_year: Option<i64>,
}

#[allow(dead_code)]
impl LivingCostProxy {
    pub fn from_row(row: &Row) -> Self {
        Self {
            municipality_code: str_or_empty(row, "municipality_code"),
            prefecture: str_or_empty(row, "prefecture"),
            municipality_name: str_or_empty(row, "municipality_name"),
            single_household_rent_proxy: opt_i64(row, "single_household_rent_proxy"),
            small_household_rent_proxy: opt_i64(row, "small_household_rent_proxy"),
            rent_per_square_meter: opt_f64(row, "rent_per_square_meter"),
            retail_price_index_proxy: opt_f64(row, "retail_price_index_proxy"),
            household_spending_annual_yen: opt_i64(row, "household_spending_annual_yen"),
            land_price_residential_per_sqm: opt_f64(row, "land_price_residential_per_sqm"),
            housing_cost_rank: opt_i64(row, "housing_cost_rank"),
            source_year: opt_i64(row, "source_year"),
        }
    }
}

// -------- DTO: 通勤流入元 (commute_flow_summary or v2_external_commute_od fallback) --------

/// 通勤流入元 (`commute_flow_summary` または `v2_external_commute_od` の TOP N 結果)
///
/// `fetch_commute_flow_summary` のフォールバック動作によりカラムが切り替わるため、
/// 共通カラム (origin 側 prefecture / municipality + 流入数) を中心に保持し、
/// 不在のフィールドは `None` で表現する。
///
/// 詳細仕様: `docs/SURVEY_MARKET_INTELLIGENCE_METRICS.md` §6 通勤到達性
#[derive(Clone, Debug, Default, Serialize)]
#[allow(dead_code)]
pub struct CommuteFlowSummary {
    /// 流入元の都道府県名 (`origin_prefecture` または `origin_pref`)
    pub origin_prefecture: String,
    /// 流入元の市区町村名 (`origin_municipality_name` または `origin_muni`)
    pub origin_municipality_name: String,
    /// 流入元の市区町村コード (commute_flow_summary 経由のみ)
    pub origin_municipality_code: Option<String>,

    /// 流入数 (`flow_count` または `total_commuters`)
    pub flow_count: Option<i64>,
    /// 0.0〜1.0 の流入比率 (commute_flow_summary 経由のみ)
    pub flow_share: Option<f64>,

    pub male_commuters: Option<i64>,
    pub female_commuters: Option<i64>,

    pub target_origin_population: Option<i64>,

    /// 推定流入数 (3 シナリオ、commute_flow_summary 経由のみ)
    pub estimated_flow_conservative: Option<i64>,
    pub estimated_flow_standard: Option<i64>,
    pub estimated_flow_aggressive: Option<i64>,

    /// 流入元ランク (commute_flow_summary 経由のみ、TOP1=1)
    pub rank_to_destination: Option<i64>,

    pub source_year: Option<i64>,
}

#[allow(dead_code)]
impl CommuteFlowSummary {
    /// `Row` から DTO を構築する。
    ///
    /// `commute_flow_summary` テーブル由来の場合は `origin_prefecture / origin_municipality_name` カラム名、
    /// `v2_external_commute_od` フォールバック由来の場合は `origin_pref / origin_muni / total_commuters` カラム名を使う。
    /// どちらに対応するか不明でも片方の値が取れるので両方試す。
    pub fn from_row(row: &Row) -> Self {
        // origin の名前は両ソースで異なるので、両方試す
        let origin_prefecture = if row.contains_key("origin_prefecture") {
            str_or_empty(row, "origin_prefecture")
        } else {
            str_or_empty(row, "origin_pref")
        };
        let origin_municipality_name = if row.contains_key("origin_municipality_name") {
            str_or_empty(row, "origin_municipality_name")
        } else {
            str_or_empty(row, "origin_muni")
        };

        // 流入数: flow_count か total_commuters
        let flow_count = opt_i64(row, "flow_count").or_else(|| opt_i64(row, "total_commuters"));

        Self {
            origin_prefecture,
            origin_municipality_name,
            origin_municipality_code: row
                .get("origin_municipality_code")
                .and_then(|v| v.as_str())
                .map(|s| s.to_string()),
            flow_count,
            flow_share: opt_f64(row, "flow_share"),
            male_commuters: opt_i64(row, "male_commuters"),
            female_commuters: opt_i64(row, "female_commuters"),
            target_origin_population: opt_i64(row, "target_origin_population"),
            estimated_flow_conservative: opt_i64(row, "estimated_target_flow_conservative"),
            estimated_flow_standard: opt_i64(row, "estimated_target_flow_standard"),
            estimated_flow_aggressive: opt_i64(row, "estimated_target_flow_aggressive"),
            rank_to_destination: opt_i64(row, "rank_to_destination"),
            source_year: opt_i64(row, "source_year").or_else(|| opt_i64(row, "reference_year")),
        }
    }

    /// 推定流入数の保守 ≤ 標準 ≤ 強気 検証 (3 値とも値ありの時のみ検証)。
    pub fn is_scenario_consistent(&self) -> bool {
        match (
            self.estimated_flow_conservative,
            self.estimated_flow_standard,
            self.estimated_flow_aggressive,
        ) {
            (Some(c), Some(s), Some(a)) => c <= s && s <= a,
            _ => true,
        }
    }

    /// `flow_share` が `[0.0, 1.0]` の範囲内か。
    pub fn is_flow_share_in_range(&self) -> bool {
        match self.flow_share {
            Some(s) => (0.0..=1.0).contains(&s) && !s.is_nan(),
            None => true,
        }
    }
}

// -------- DTO: `municipality_occupation_population` --------

/// 市区町村 × 職業 × 年齢 × 性別 人口 (国勢調査)
///
/// 詳細仕様: `docs/SURVEY_MARKET_INTELLIGENCE_METRICS.md` §3 対象職業人口
#[derive(Clone, Debug, Default, Serialize)]
#[allow(dead_code)]
pub struct OccupationPopulationCell {
    pub municipality_code: String,
    pub prefecture: String,
    pub municipality_name: String,
    /// "resident" (常住地) または "workplace" (従業地)
    pub basis: String,
    pub occupation_code: String,
    pub occupation_name: String,
    pub age_group: String,
    /// "male" / "female" / "total"
    pub gender: String,
    pub population: Option<i64>,
    pub source_year: Option<i64>,
}

#[allow(dead_code)]
impl OccupationPopulationCell {
    pub fn from_row(row: &Row) -> Self {
        Self {
            municipality_code: str_or_empty(row, "municipality_code"),
            prefecture: str_or_empty(row, "prefecture"),
            municipality_name: str_or_empty(row, "municipality_name"),
            basis: str_or_empty(row, "basis"),
            occupation_code: str_or_empty(row, "occupation_code"),
            occupation_name: str_or_empty(row, "occupation_name"),
            age_group: str_or_empty(row, "age_group"),
            gender: str_or_empty(row, "gender"),
            population: opt_i64(row, "population"),
            source_year: opt_i64(row, "source_year"),
        }
    }
}

// -------- Phase 3 Step 5 Phase 2: 新規 DTO 群 (Plan B 対応) --------

/// 職業別人口セル DTO (workplace measured + resident estimated_beta の XOR 表現)
///
/// 商品の核心:
/// - `data_label = "measured"` の場合は `population` のみ Some, `estimate_index` は None
/// - `data_label = "estimated_beta"` の場合は `estimate_index` のみ Some, `population` は None
/// - これは XOR 不変条件であり、UI 側で人数表示 / 指数表示を排他的に切り替える
#[derive(Debug, Clone, Default, Serialize)]
#[allow(dead_code)]
pub struct OccupationCellDto {
    pub municipality_code: String,
    pub prefecture: String,
    pub municipality_name: String,
    /// 'workplace' | 'resident'
    pub basis: String,
    pub occupation_code: String,
    pub occupation_name: String,
    pub age_class: String,
    pub gender: String,
    /// measured 時のみ Some
    pub population: Option<i64>,
    /// estimated_beta 時のみ Some
    pub estimate_index: Option<f64>,
    /// 'measured' | 'estimated_beta'
    pub data_label: String,
    pub source_name: String,
    pub source_year: i64,
    /// estimated_beta 時のみ Some
    pub weight_source: Option<String>,
}

#[allow(dead_code)]
impl OccupationCellDto {
    /// XOR 不変条件: data_label に応じて population/estimate_index のいずれか一方のみ Some
    pub fn is_xor_consistent(&self) -> bool {
        match self.data_label.as_str() {
            "measured" => self.population.is_some() && self.estimate_index.is_none(),
            "estimated_beta" => self.population.is_none() && self.estimate_index.is_some(),
            _ => false,
        }
    }

    /// 人数表示可否 (measured のみ true)
    pub fn can_display_population(&self) -> bool {
        self.data_label == "measured" && self.population.is_some()
    }

    /// 指数表示可否 (estimated_beta のみ true)
    pub fn can_display_index(&self) -> bool {
        self.data_label == "estimated_beta" && self.estimate_index.is_some()
    }

    /// (basis, data_label) から DataSourceLabel に変換
    pub fn label(&self) -> DataSourceLabel {
        match (self.basis.as_str(), self.data_label.as_str()) {
            ("workplace", "measured") => DataSourceLabel::WorkplaceMeasured,
            ("workplace", "estimated_beta") => DataSourceLabel::WorkplaceEstimatedBeta,
            ("resident", "measured") => DataSourceLabel::ResidentActual,
            ("resident", "estimated_beta") => DataSourceLabel::ResidentEstimatedBeta,
            _ => DataSourceLabel::AggregateParent,
        }
    }
}

/// 政令市区 (designated_ward) thickness 詳細 DTO
#[derive(Debug, Clone, Default, Serialize)]
#[allow(dead_code)]
pub struct WardThicknessDto {
    pub municipality_code: String,
    pub municipality_name: String,
    pub prefecture: String,
    /// 通常 'resident'
    pub basis: String,
    pub occupation_code: String,
    pub occupation_name: String,
    /// 0-200
    pub thickness_index: f64,
    pub rank_in_occupation: Option<i64>,
    pub rank_percentile: Option<f64>,
    /// 'A' | 'B' | 'C' | 'D'
    pub distribution_priority: Option<String>,
    pub scenario_conservative_index: Option<i64>,
    pub scenario_standard_index: Option<i64>,
    pub scenario_aggressive_index: Option<i64>,
    /// 'A-' 等
    pub estimate_grade: Option<String>,
    /// 'hypothesis_v1' 等
    pub weight_source: String,
    pub is_industrial_anchor: bool,
    pub source_year: i64,
}

/// 親市内ランキング DTO (parent_rank 優先、商品の核心)
///
/// 表示優先度: parent_rank が常に主指標、national_rank は参考のみ
#[derive(Debug, Clone, Default, Serialize)]
#[allow(dead_code)]
pub struct WardRankingRowDto {
    pub municipality_code: String,
    pub municipality_name: String,
    pub parent_code: String,
    pub parent_name: String,
    /// 主表示 (商品 UI で大きく)
    pub parent_rank: i64,
    /// 主表示 (分母)
    pub parent_total: i64,
    /// 参考表示 (UI で小さく)
    pub national_rank: i64,
    pub national_total: i64,
    pub thickness_index: f64,
    /// 'A'/'B'/'C'/'D'
    pub priority: String,
}

#[allow(dead_code)]
impl WardRankingRowDto {
    /// 表示優先度: parent_rank が常に主指標 (national は参考のみ)
    /// 商品ルール: parent_rank > national_rank の優先順位
    pub fn uses_parent_rank_primary(&self) -> bool {
        // parent_rank が有効 (1 以上、parent_total 以下) であること
        self.parent_rank >= 1
            && self.parent_total >= self.parent_rank
            && !self.parent_code.is_empty()
    }
}

/// 市区町村コードマスター DTO (結合キー用 lookup)
#[derive(Debug, Clone, Default, Serialize)]
#[allow(dead_code)]
pub struct MunicipalityCodeMasterDto {
    /// JIS 5 桁
    pub municipality_code: String,
    pub municipality_name: String,
    pub prefecture: String,
    /// 'designated_ward' | 'aggregate_city' | 'municipality' | 'special_ward' | 'aggregate_special_wards'
    pub area_type: String,
    pub parent_code: Option<String>,
}

// -------- 上位 DTO: 主要市区町村ごとの統合データ --------

/// 採用マーケットインテリジェンス分析データ (上位 DTO)
///
/// `fetch_*` 4 関数の結果を 1 つの構造体に束ねる。
/// レポート HTML 層 (Step 3 で実装) に渡す唯一の入口。
///
/// 設計:
/// - 全フィールドが `Vec<...>` (空でも良い、UI 側で欠損時非表示判定)
/// - DTO は serde::Serialize 実装済み → JSON API でもそのまま返却可能
#[derive(Clone, Debug, Default, Serialize)]
#[allow(dead_code)]
pub struct SurveyMarketIntelligenceData {
    pub recruiting_scores: Vec<MunicipalityRecruitingScore>,
    pub living_cost_proxies: Vec<LivingCostProxy>,
    pub commute_flows: Vec<CommuteFlowSummary>,
    pub occupation_populations: Vec<OccupationPopulationCell>,

    // -------- Phase 3 Step 5 Phase 2 追加 (互換維持: 既存フィールド変更なし) --------
    /// Plan B 対応 (workplace measured + resident estimated_beta)
    pub occupation_cells: Vec<OccupationCellDto>,
    /// designated_ward の thickness 詳細
    pub ward_thickness: Vec<WardThicknessDto>,
    /// parent_code 内ランキング (商品の核心)
    pub ward_rankings: Vec<WardRankingRowDto>,
    /// 結合キー用 lookup
    pub code_master: Vec<MunicipalityCodeMasterDto>,
}

#[allow(dead_code)]
impl SurveyMarketIntelligenceData {
    /// すべての構成 Vec が空かどうか。
    pub fn is_empty(&self) -> bool {
        self.recruiting_scores.is_empty()
            && self.living_cost_proxies.is_empty()
            && self.commute_flows.is_empty()
            && self.occupation_populations.is_empty()
    }

    /// 内部の各スコアが METRICS.md の不変条件を満たすか。
    /// (テストおよび monitoring 用ヘルパー)
    pub fn all_invariants_hold(&self) -> bool {
        self.recruiting_scores
            .iter()
            .all(|s| s.is_scenario_consistent() && s.is_priority_score_in_range())
            && self
                .commute_flows
                .iter()
                .all(|f| f.is_scenario_consistent() && f.is_flow_share_in_range())
    }
}

// -------- Vec<Row> → Vec<DTO> 変換ヘルパー --------

/// `Vec<Row>` を一括で `Vec<DTO>` に変換するためのトレイト風自由関数群。
/// (Step 3 でレポート HTML が呼ぶときの入口)
#[allow(dead_code)]
pub(crate) fn to_recruiting_scores(rows: &[Row]) -> Vec<MunicipalityRecruitingScore> {
    rows.iter().map(MunicipalityRecruitingScore::from_row).collect()
}

#[allow(dead_code)]
pub(crate) fn to_living_cost_proxies(rows: &[Row]) -> Vec<LivingCostProxy> {
    rows.iter().map(LivingCostProxy::from_row).collect()
}

#[allow(dead_code)]
pub(crate) fn to_commute_flows(rows: &[Row]) -> Vec<CommuteFlowSummary> {
    rows.iter().map(CommuteFlowSummary::from_row).collect()
}

#[allow(dead_code)]
pub(crate) fn to_occupation_populations(rows: &[Row]) -> Vec<OccupationPopulationCell> {
    rows.iter().map(OccupationPopulationCell::from_row).collect()
}

// ============================================================
// テスト
// ============================================================

#[cfg(test)]
mod tests {
    use super::*;
    use crate::db::local_sqlite::LocalDb;

    /// 既存パターン (`db/local_sqlite.rs:131` 流用) で空の一時 SQLite DB を作成。
    fn create_test_db() -> (tempfile::NamedTempFile, LocalDb) {
        let tmp = tempfile::NamedTempFile::new().unwrap();
        let path = tmp.path().to_str().unwrap();
        let _ = rusqlite::Connection::open(path).unwrap();
        let db = LocalDb::new(path).unwrap();
        (tmp, db)
    }

    /// 全テーブルが空の状態で各 fetch 関数が空 Vec を返すこと。
    /// (テーブル不在時のフェイルセーフ動作確認)
    #[test]
    fn test_all_fetch_functions_return_empty_when_tables_missing() {
        let (_tmp, db) = create_test_db();

        // 全 fetch 関数: テーブル不在 + Turso None → 空 Vec
        assert!(
            fetch_recruiting_scores_by_municipalities(&db, None, &["01101"], "").is_empty(),
            "recruiting_scores: 空配列を期待"
        );
        assert!(
            fetch_living_cost_proxy(&db, None, &["01101"]).is_empty(),
            "living_cost_proxy: 空配列を期待"
        );
        assert!(
            fetch_commute_flow_summary(&db, None, "北海道", "札幌市", 10).is_empty(),
            "commute_flow_summary: 空配列を期待 (両テーブル不在)"
        );
        assert!(
            fetch_occupation_population(&db, None, "01101", "resident", &[]).is_empty(),
            "occupation_population: 空配列を期待"
        );
    }

    /// 必須引数が空の場合、early return で空 Vec を返すこと。
    #[test]
    fn test_required_args_empty_returns_empty() {
        let (_tmp, db) = create_test_db();

        // municipality_codes 空
        assert!(fetch_recruiting_scores_by_municipalities(&db, None, &[], "").is_empty());
        assert!(fetch_living_cost_proxy(&db, None, &[]).is_empty());

        // dest_pref / dest_muni 空
        assert!(fetch_commute_flow_summary(&db, None, "", "札幌市", 10).is_empty());
        assert!(fetch_commute_flow_summary(&db, None, "北海道", "", 10).is_empty());

        // municipality_code 空
        assert!(fetch_occupation_population(&db, None, "", "resident", &[]).is_empty());
    }

    /// `commute_flow_summary` 不在 + `v2_external_commute_od` 存在 →
    /// `v2_external_commute_od` から TOP N が取れること (フォールバック動作)。
    #[test]
    fn test_commute_flow_summary_falls_back_to_external_commute_od() {
        let (_tmp, db) = create_test_db();

        // v2_external_commute_od テーブルを作って 3 件投入
        db.execute(
            "CREATE TABLE v2_external_commute_od (
                origin_pref TEXT NOT NULL,
                origin_muni TEXT NOT NULL,
                dest_pref TEXT NOT NULL,
                dest_muni TEXT NOT NULL,
                total_commuters INTEGER NOT NULL,
                male_commuters INTEGER,
                female_commuters INTEGER,
                reference_year INTEGER,
                PRIMARY KEY (origin_pref, origin_muni, dest_pref, dest_muni)
            )",
            &[],
        )
        .expect("CREATE TABLE 失敗");

        for (op, om, dc) in [
            ("北海道", "小樽市", 5000_i64),
            ("北海道", "江別市", 8000_i64),
            ("北海道", "石狩市", 3000_i64),
            ("北海道", "札幌市", 100_i64), // self-loop は除外される想定
        ] {
            db.execute(
                "INSERT INTO v2_external_commute_od VALUES (?1, ?2, ?3, ?4, ?5, NULL, NULL, 2020)",
                &[
                    &op as &dyn rusqlite::types::ToSql,
                    &om,
                    &"北海道",
                    &"札幌市",
                    &dc,
                ],
            )
            .expect("INSERT 失敗");
        }

        let rows = fetch_commute_flow_summary(&db, None, "北海道", "札幌市", 10);
        assert_eq!(rows.len(), 3, "self-loop 除外で 3 件期待 (実際: {})", rows.len());

        // 1 位は江別市 (total_commuters=8000)
        let first_origin_muni = rows[0]
            .get("origin_muni")
            .and_then(|v| v.as_str())
            .unwrap_or("");
        assert_eq!(first_origin_muni, "江別市", "TOP1 流入元は江別市");
    }

    // ============================================================
    // Phase 3 Step 2: DTO 変換テスト
    // ============================================================

    /// Turso 風の文字列値 (`Value::String("1973395")`) と JSON Number 両方を i64 に変換できること。
    #[test]
    fn test_opt_i64_handles_turso_string_and_local_number() {
        let mut row: Row = HashMap::new();
        row.insert("turso_str".into(), Value::String("1973395".into()));
        row.insert("local_num".into(), Value::from(42_i64));
        row.insert("float_str".into(), Value::String("3.14".into()));
        row.insert("float_num".into(), Value::from(2.71_f64));
        row.insert("null_val".into(), Value::Null);
        row.insert("bool_val".into(), Value::Bool(true));

        assert_eq!(opt_i64(&row, "turso_str"), Some(1_973_395));
        assert_eq!(opt_i64(&row, "local_num"), Some(42));
        // 小数文字列 "3.14" は as_f64 で None (SQLite REAL は別カラムで使う設計)、
        // parse::<i64> も "3.14" を拒否するため None。
        // 整数値のみのカラム (人口など) には影響しない。
        assert_eq!(opt_i64(&row, "float_str"), None);
        assert_eq!(opt_i64(&row, "float_num"), Some(2)); // JSON Number は as_f64 → as i64
        assert_eq!(opt_i64(&row, "null_val"), None);
        assert_eq!(opt_i64(&row, "missing"), None);
        assert_eq!(opt_i64(&row, "bool_val"), None); // bool は数値として解釈しない
    }

    /// opt_f64 も同様に Turso 文字列・JSON Number 両対応。
    #[test]
    fn test_opt_f64_handles_turso_string_and_local_number() {
        let mut row: Row = HashMap::new();
        row.insert("turso_str".into(), Value::String("3.14".into()));
        row.insert("local_num".into(), Value::from(42.5_f64));
        row.insert("int_num".into(), Value::from(100_i64));
        row.insert("null_val".into(), Value::Null);

        assert_eq!(opt_f64(&row, "turso_str"), Some(3.14));
        assert_eq!(opt_f64(&row, "local_num"), Some(42.5));
        assert_eq!(opt_f64(&row, "int_num"), Some(100.0)); // int → f64 自動
        assert_eq!(opt_f64(&row, "null_val"), None);
        assert_eq!(opt_f64(&row, "missing"), None);
    }

    /// `MunicipalityRecruitingScore::from_row` が Turso 文字列値からも DTO を構築できること。
    #[test]
    fn test_recruiting_score_from_turso_string_row() {
        // Turso v2/pipeline API は数値を文字列で返す
        let mut row: Row = HashMap::new();
        row.insert("municipality_code".into(), Value::String("01101".into()));
        row.insert("prefecture".into(), Value::String("北海道".into()));
        row.insert("municipality_name".into(), Value::String("札幌市".into()));
        row.insert("occupation_group_code".into(), Value::String("driver".into()));
        row.insert("occupation_group_name".into(), Value::String("輸送・機械運転".into()));
        row.insert("target_population".into(), Value::String("12345".into()));
        row.insert("distribution_priority_score".into(), Value::String("78.5".into()));
        row.insert("scenario_conservative_population".into(), Value::String("100".into()));
        row.insert("scenario_standard_population".into(), Value::String("300".into()));
        row.insert("scenario_aggressive_population".into(), Value::String("500".into()));

        let dto = MunicipalityRecruitingScore::from_row(&row);
        assert_eq!(dto.municipality_code, "01101");
        assert_eq!(dto.prefecture, "北海道");
        assert_eq!(dto.target_population, Some(12345));
        assert_eq!(dto.distribution_priority_score, Some(78.5));
        assert_eq!(dto.scenario_conservative_population, Some(100));
        assert_eq!(dto.scenario_standard_population, Some(300));
        assert_eq!(dto.scenario_aggressive_population, Some(500));
        // 不在カラムは None
        assert_eq!(dto.median_salary_yen, None);
        assert_eq!(dto.living_cost_score, None);
    }

    /// `from_row` が NULL / 欠損カラムでも panic しないこと。
    #[test]
    fn test_dto_from_empty_row_does_not_panic() {
        let row: Row = HashMap::new();
        // 全 DTO の from_row が空 Row でも default 値で構築される
        let s = MunicipalityRecruitingScore::from_row(&row);
        assert_eq!(s.municipality_code, "");
        assert_eq!(s.target_population, None);
        assert_eq!(s.distribution_priority_score, None);
        assert!(s.is_scenario_consistent(), "欠損ありなら true");
        assert!(s.is_priority_score_in_range(), "欠損ありなら true");

        let l = LivingCostProxy::from_row(&row);
        assert_eq!(l.municipality_code, "");
        assert_eq!(l.single_household_rent_proxy, None);

        let c = CommuteFlowSummary::from_row(&row);
        assert_eq!(c.origin_prefecture, "");
        assert_eq!(c.flow_count, None);
        assert!(c.is_scenario_consistent());
        assert!(c.is_flow_share_in_range());

        let o = OccupationPopulationCell::from_row(&row);
        assert_eq!(o.basis, "");
        assert_eq!(o.population, None);
    }

    /// `is_scenario_consistent` が「保守 ≤ 標準 ≤ 強気」の不変条件を検証すること。
    #[test]
    fn test_recruiting_score_scenario_invariant() {
        // 適合: 100 <= 300 <= 500
        let valid = MunicipalityRecruitingScore {
            scenario_conservative_population: Some(100),
            scenario_standard_population: Some(300),
            scenario_aggressive_population: Some(500),
            ..Default::default()
        };
        assert!(valid.is_scenario_consistent());

        // 適合: 等値 (100 = 100 = 100)
        let edge_equal = MunicipalityRecruitingScore {
            scenario_conservative_population: Some(100),
            scenario_standard_population: Some(100),
            scenario_aggressive_population: Some(100),
            ..Default::default()
        };
        assert!(edge_equal.is_scenario_consistent());

        // 不適合: 標準 < 保守 (順序逆転)
        let invalid_swap = MunicipalityRecruitingScore {
            scenario_conservative_population: Some(500),
            scenario_standard_population: Some(300),
            scenario_aggressive_population: Some(100),
            ..Default::default()
        };
        assert!(!invalid_swap.is_scenario_consistent());

        // 不適合: 強気 < 標準
        let invalid_partial = MunicipalityRecruitingScore {
            scenario_conservative_population: Some(100),
            scenario_standard_population: Some(500),
            scenario_aggressive_population: Some(300),
            ..Default::default()
        };
        assert!(!invalid_partial.is_scenario_consistent());

        // 欠損あり → 検証不可、true (緩いガード)
        let missing = MunicipalityRecruitingScore {
            scenario_conservative_population: Some(100),
            scenario_standard_population: None,
            scenario_aggressive_population: Some(500),
            ..Default::default()
        };
        assert!(missing.is_scenario_consistent());
    }

    /// `is_priority_score_in_range` が `[0.0, 100.0]` を検証すること。
    #[test]
    fn test_priority_score_range_invariant() {
        let cases = [
            (0.0_f64, true, "下限 0"),
            (100.0, true, "上限 100"),
            (50.5, true, "中間値"),
            (-0.1, false, "負値"),
            (100.001, false, "上限超過"),
            (f64::NAN, false, "NaN は不適合"),
        ];
        for (v, expected, label) in cases {
            let s = MunicipalityRecruitingScore {
                distribution_priority_score: Some(v),
                ..Default::default()
            };
            assert_eq!(
                s.is_priority_score_in_range(),
                expected,
                "{label} ({v}) で is_priority_score_in_range が想定外"
            );
        }

        // None は検証不可、true
        let none_score = MunicipalityRecruitingScore {
            distribution_priority_score: None,
            ..Default::default()
        };
        assert!(none_score.is_priority_score_in_range());
    }

    /// `CommuteFlowSummary::from_row` が `commute_flow_summary` テーブルのカラム名でも、
    /// `v2_external_commute_od` フォールバックのカラム名でも DTO を構築できること。
    #[test]
    fn test_commute_flow_summary_handles_both_column_naming() {
        // パターン A: commute_flow_summary 由来 (origin_prefecture, flow_count, ...)
        let mut row_a: Row = HashMap::new();
        row_a.insert("origin_prefecture".into(), Value::String("北海道".into()));
        row_a.insert(
            "origin_municipality_name".into(),
            Value::String("江別市".into()),
        );
        row_a.insert("origin_municipality_code".into(), Value::String("01217".into()));
        row_a.insert("flow_count".into(), Value::String("8000".into()));
        row_a.insert("flow_share".into(), Value::String("0.42".into()));
        row_a.insert("rank_to_destination".into(), Value::from(1_i64));

        let dto_a = CommuteFlowSummary::from_row(&row_a);
        assert_eq!(dto_a.origin_prefecture, "北海道");
        assert_eq!(dto_a.origin_municipality_name, "江別市");
        assert_eq!(dto_a.origin_municipality_code.as_deref(), Some("01217"));
        assert_eq!(dto_a.flow_count, Some(8000));
        assert_eq!(dto_a.flow_share, Some(0.42));
        assert_eq!(dto_a.rank_to_destination, Some(1));

        // パターン B: v2_external_commute_od フォールバック由来 (origin_pref, total_commuters, ...)
        let mut row_b: Row = HashMap::new();
        row_b.insert("origin_pref".into(), Value::String("北海道".into()));
        row_b.insert("origin_muni".into(), Value::String("小樽市".into()));
        row_b.insert("total_commuters".into(), Value::String("5000".into()));
        row_b.insert("male_commuters".into(), Value::from(2500_i64));
        row_b.insert("reference_year".into(), Value::from(2020_i64));

        let dto_b = CommuteFlowSummary::from_row(&row_b);
        assert_eq!(dto_b.origin_prefecture, "北海道");
        assert_eq!(dto_b.origin_municipality_name, "小樽市");
        assert_eq!(dto_b.origin_municipality_code, None); // フォールバック側にはない
        assert_eq!(dto_b.flow_count, Some(5000)); // total_commuters から取れる
        assert_eq!(dto_b.male_commuters, Some(2500));
        assert_eq!(dto_b.source_year, Some(2020)); // reference_year を fallback で吸収
    }

    /// `flow_share` が `[0.0, 1.0]` の範囲内か検証。
    #[test]
    fn test_commute_flow_share_range_invariant() {
        let in_range = CommuteFlowSummary {
            flow_share: Some(0.5),
            ..Default::default()
        };
        assert!(in_range.is_flow_share_in_range());

        let upper = CommuteFlowSummary {
            flow_share: Some(1.0),
            ..Default::default()
        };
        assert!(upper.is_flow_share_in_range());

        let over = CommuteFlowSummary {
            flow_share: Some(1.5), // 1.0 超過は不適合
            ..Default::default()
        };
        assert!(!over.is_flow_share_in_range());

        let neg = CommuteFlowSummary {
            flow_share: Some(-0.1),
            ..Default::default()
        };
        assert!(!neg.is_flow_share_in_range());
    }

    /// `to_recruiting_scores` 等が Vec<Row> → Vec<DTO> を一括変換できること。
    #[test]
    fn test_vec_row_to_vec_dto_conversion() {
        let mut row1: Row = HashMap::new();
        row1.insert("municipality_code".into(), Value::String("01101".into()));
        row1.insert("distribution_priority_score".into(), Value::String("85.0".into()));
        let mut row2: Row = HashMap::new();
        row2.insert("municipality_code".into(), Value::String("01102".into()));
        row2.insert("distribution_priority_score".into(), Value::from(70.0_f64));

        let rows = vec![row1, row2];
        let dtos = to_recruiting_scores(&rows);
        assert_eq!(dtos.len(), 2);
        assert_eq!(dtos[0].municipality_code, "01101");
        assert_eq!(dtos[0].distribution_priority_score, Some(85.0));
        assert_eq!(dtos[1].municipality_code, "01102");
        assert_eq!(dtos[1].distribution_priority_score, Some(70.0));
    }

    /// 上位 DTO の不変条件チェックが通ること。
    #[test]
    fn test_survey_market_intelligence_data_invariants() {
        let valid_score = MunicipalityRecruitingScore {
            distribution_priority_score: Some(75.0),
            scenario_conservative_population: Some(100),
            scenario_standard_population: Some(300),
            scenario_aggressive_population: Some(500),
            ..Default::default()
        };
        let valid_flow = CommuteFlowSummary {
            flow_share: Some(0.5),
            estimated_flow_conservative: Some(10),
            estimated_flow_standard: Some(50),
            estimated_flow_aggressive: Some(100),
            ..Default::default()
        };
        let data = SurveyMarketIntelligenceData {
            recruiting_scores: vec![valid_score],
            commute_flows: vec![valid_flow],
            ..Default::default()
        };
        assert!(data.all_invariants_hold());
        assert!(!data.is_empty());

        // 空 DTO は invariants 自動成立
        let empty = SurveyMarketIntelligenceData::default();
        assert!(empty.is_empty());
        assert!(empty.all_invariants_hold());

        // 不変条件違反を含むと all_invariants_hold が false
        let bad_score = MunicipalityRecruitingScore {
            distribution_priority_score: Some(150.0), // 範囲超過
            ..Default::default()
        };
        let bad_data = SurveyMarketIntelligenceData {
            recruiting_scores: vec![bad_score],
            ..Default::default()
        };
        assert!(!bad_data.all_invariants_hold());
    }
}

// -------- Phase 3 Step 5 Phase 2: DTO unit tests --------
#[cfg(test)]
mod phase3_step5_dto_tests {
    use super::*;

    // [1] measured: population あり / estimate_index なし
    #[test]
    fn occupation_cell_measured_xor_ok() {
        let cell = OccupationCellDto {
            data_label: "measured".into(),
            population: Some(12345),
            estimate_index: None,
            ..Default::default()
        };
        assert!(cell.is_xor_consistent());
        assert!(cell.can_display_population());
        assert!(!cell.can_display_index());
    }

    // [2] estimated_beta: population なし / estimate_index あり
    #[test]
    fn occupation_cell_estimated_xor_ok() {
        let cell = OccupationCellDto {
            data_label: "estimated_beta".into(),
            population: None,
            estimate_index: Some(142.5),
            ..Default::default()
        };
        assert!(cell.is_xor_consistent());
        assert!(!cell.can_display_population());
        assert!(cell.can_display_index());
    }

    // [3] 不正 XOR (両方 Some) は false
    #[test]
    fn occupation_cell_both_some_is_invalid() {
        let cell = OccupationCellDto {
            data_label: "measured".into(),
            population: Some(100),
            estimate_index: Some(50.0), // ← 違反
            ..Default::default()
        };
        assert!(!cell.is_xor_consistent());
    }

    // [4] 不正 XOR (両方 None) も false
    #[test]
    fn occupation_cell_both_none_is_invalid() {
        let cell = OccupationCellDto {
            data_label: "measured".into(),
            population: None,
            estimate_index: None,
            ..Default::default()
        };
        assert!(!cell.is_xor_consistent());
    }

    // [5] measured のみ人数表示 OK
    #[test]
    fn population_display_only_when_measured() {
        let m = OccupationCellDto {
            data_label: "measured".into(),
            population: Some(100),
            estimate_index: None,
            ..Default::default()
        };
        let e = OccupationCellDto {
            data_label: "estimated_beta".into(),
            population: None,
            estimate_index: Some(50.0),
            ..Default::default()
        };
        assert!(m.can_display_population());
        assert!(!e.can_display_population());
    }

    // [6] label 変換
    #[test]
    fn label_mapping_correct() {
        let workplace = OccupationCellDto {
            basis: "workplace".into(),
            data_label: "measured".into(),
            population: Some(1),
            ..Default::default()
        };
        let resident = OccupationCellDto {
            basis: "resident".into(),
            data_label: "estimated_beta".into(),
            estimate_index: Some(1.0),
            ..Default::default()
        };
        assert_eq!(workplace.label(), DataSourceLabel::WorkplaceMeasured);
        assert_eq!(resident.label(), DataSourceLabel::ResidentEstimatedBeta);
    }

    // [7] parent_rank 優先判定
    #[test]
    fn ward_ranking_uses_parent_rank_primary() {
        let valid = WardRankingRowDto {
            parent_code: "14100".into(),
            parent_rank: 3,
            parent_total: 18,
            national_rank: 12,
            national_total: 1917,
            ..Default::default()
        };
        assert!(valid.uses_parent_rank_primary());

        let invalid_no_parent = WardRankingRowDto {
            parent_code: "".into(), // 親市不在
            parent_rank: 3,
            parent_total: 18,
            ..Default::default()
        };
        assert!(!invalid_no_parent.uses_parent_rank_primary());
    }

    // [8] default が空 Vec で落ちない
    #[test]
    fn survey_data_default_is_empty_vecs() {
        let data = SurveyMarketIntelligenceData::default();
        assert!(data.occupation_cells.is_empty());
        assert!(data.ward_thickness.is_empty());
        assert!(data.ward_rankings.is_empty());
        assert!(data.code_master.is_empty());
        // 既存 Vec も空
        assert!(data.recruiting_scores.is_empty());
    }
}
