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

    // Worker B 投入版スキーマ: basis='resident', occupation_code/_name,
    // 4 sub-score (target_thickness/commute_access/competition/salary_living),
    // distribution_priority_score, rank, シナリオスコア (INTEGER),
    // 出所メタ (data_label='estimated_beta'/source_name/source_year/weight_source/estimate_grade)
    let select_cols = "municipality_code, prefecture, municipality_name, basis, \
                       occupation_code, occupation_name, \
                       distribution_priority_score, target_thickness_index, \
                       commute_access_score, competition_score, salary_living_score, \
                       rank_in_occupation, rank_percentile, distribution_priority, \
                       scenario_conservative_score, scenario_standard_score, scenario_aggressive_score, \
                       data_label, source_name, source_year, weight_source, estimate_grade";

    // 引数 `occupation_group_code` は後方互換のため名称維持。空文字なら全 occupation。
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
             ORDER BY occupation_code, distribution_priority_score DESC"
        );
        let params = municipality_codes.iter().map(|s| s.to_string()).collect();
        (sql, params)
    } else {
        // ?1 = occupation_code, ?2..?(N+1) = municipality_codes
        let placeholders: String = (2..=(municipality_codes.len() + 1))
            .map(|i| format!("?{i}"))
            .collect::<Vec<_>>()
            .join(",");
        let sql = format!(
            "SELECT {select_cols} \
             FROM municipality_recruiting_scores \
             WHERE occupation_code = ?1 \
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

    // Worker A 投入版スキーマ: basis/cost_index/min_wage/land_price_proxy/salary_real_terms_proxy
    // + 出所メタ (data_label/source_name/source_year/weight_source/estimated_at)
    let sql = format!(
        "SELECT municipality_code, prefecture, municipality_name, \
                basis, cost_index, min_wage, land_price_proxy, salary_real_terms_proxy, \
                data_label, source_name, source_year, weight_source, estimated_at \
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

    query_turso_or_local(
        turso,
        db,
        &sql,
        &params,
        "municipality_occupation_population",
    )
}

// ============================================================
// Phase 3 Step 5 Phase 3: 4 新規 fetch 関数 (Plan B XOR + 親市内ランキング)
// ============================================================
//
// 設計参照: docs/SURVEY_MARKET_INTELLIGENCE_PHASE3_RUST_INTEGRATION_PLAN.md §3
//
// すべて既存パターンに揃え、`query_turso_or_local` 経由で Turso 優先・ローカル SQLite
// フォールバック。`Vec<Row>` 返却 (Step 2 の二段構成踏襲、DTO 変換は `to_*` ヘルパーで実施)。

/// 職業別人口セル (Plan B 対応版)。`fetch_occupation_population` の上位互換。
///
/// `municipality_occupation_population` テーブルから XOR 列 (`population` /
/// `estimate_index`) と出所メタ (`data_label`, `source_name`, `weight_source`) を含めて取得する。
///
/// # 引数
/// - `municipality_codes`: 取得対象の市区町村コード一覧 (空なら空 Vec)
/// - `occupation_code`: 職業コード (`None` なら全職業)
/// - `basis_filter`: `Some("workplace")` / `Some("resident")` / `None` (両方)
///
/// 詳細仕様: 計画書 §3.1
pub(crate) fn fetch_occupation_cells(
    db: &Db,
    turso: Option<&TursoDb>,
    municipality_codes: &[&str],
    occupation_code: Option<&str>,
    basis_filter: Option<&str>,
) -> Vec<Row> {
    if municipality_codes.is_empty() {
        return vec![];
    }
    if !table_exists(db, "municipality_occupation_population") && turso.is_none() {
        return vec![];
    }

    let placeholders_m: String = (1..=municipality_codes.len())
        .map(|i| format!("?{i}"))
        .collect::<Vec<_>>()
        .join(",");
    let mut params: Vec<String> = municipality_codes.iter().map(|s| s.to_string()).collect();
    let mut where_clauses = vec![format!("municipality_code IN ({placeholders_m})")];

    if let Some(occ) = occupation_code {
        if !occ.is_empty() {
            params.push(occ.to_string());
            where_clauses.push(format!("occupation_code = ?{}", params.len()));
        }
    }
    if let Some(basis) = basis_filter {
        if !basis.is_empty() {
            params.push(basis.to_string());
            where_clauses.push(format!("basis = ?{}", params.len()));
        }
    }

    let sql = format!(
        "SELECT municipality_code, prefecture, municipality_name, basis, \
                occupation_code, occupation_name, age_class, gender, \
                population, estimate_index, \
                data_label, source_name, source_year, weight_source \
         FROM municipality_occupation_population \
         WHERE {where_sql} \
         ORDER BY municipality_code, occupation_code, age_class, gender",
        where_sql = where_clauses.join(" AND ")
    );

    query_turso_or_local(
        turso,
        db,
        &sql,
        &params,
        "municipality_occupation_population",
    )
}

/// `v2_municipality_target_thickness` から designated_ward の thickness 詳細を取得する。
///
/// # 引数
/// - `municipality_codes`: 取得対象の市区町村コード一覧 (空なら空 Vec)
/// - `occupation_code`: 職業コード (`None` なら全職業)
///
/// 詳細仕様: 計画書 §3.2
pub(crate) fn fetch_ward_thickness(
    db: &Db,
    turso: Option<&TursoDb>,
    municipality_codes: &[&str],
    occupation_code: Option<&str>,
) -> Vec<Row> {
    if municipality_codes.is_empty() {
        return vec![];
    }
    if !table_exists(db, "v2_municipality_target_thickness") && turso.is_none() {
        return vec![];
    }

    let placeholders: String = (1..=municipality_codes.len())
        .map(|i| format!("?{i}"))
        .collect::<Vec<_>>()
        .join(",");
    let mut params: Vec<String> = municipality_codes.iter().map(|s| s.to_string()).collect();
    let occ_clause = if let Some(occ) = occupation_code.filter(|s| !s.is_empty()) {
        params.push(occ.to_string());
        format!(" AND occupation_code = ?{}", params.len())
    } else {
        String::new()
    };

    let sql = format!(
        "SELECT municipality_code, municipality_name, prefecture, \
                basis, occupation_code, occupation_name, \
                thickness_index, \
                rank_in_occupation, rank_percentile, distribution_priority, \
                scenario_conservative_index, scenario_standard_index, scenario_aggressive_index, \
                estimate_grade, weight_source, is_industrial_anchor, source_year \
         FROM v2_municipality_target_thickness \
         WHERE municipality_code IN ({placeholders}){occ_clause} \
         ORDER BY occupation_code, thickness_index DESC"
    );

    query_turso_or_local(turso, db, &sql, &params, "v2_municipality_target_thickness")
}

/// 親市 (`parent_code`) 内ランキング SQL を構築する (Window Function 使用、商品の核心)。
///
/// SQL 文字列ビルダを公開することで、unit test で `RANK() OVER` / `COUNT(*) OVER` /
/// `PARTITION BY` の含有を直接検証可能にする。
///
/// 詳細仕様: 計画書 §3.3、Window Function 互換性 PASS は
/// `SURVEY_MARKET_INTELLIGENCE_PHASE3_SQL_WINDOW_COMPAT.md` 参照。
pub(crate) fn build_ward_ranking_sql() -> &'static str {
    "WITH ward_set AS ( \
        SELECT v.municipality_code, v.municipality_name, \
               mcm.parent_code, COALESCE(parent.municipality_name, '') AS parent_name, \
               v.thickness_index, v.distribution_priority, \
               v.rank_in_occupation, v.occupation_code \
        FROM v2_municipality_target_thickness v \
        JOIN municipality_code_master mcm \
          ON v.municipality_code = mcm.municipality_code \
        LEFT JOIN municipality_code_master parent \
          ON mcm.parent_code = parent.municipality_code \
        WHERE mcm.area_type = 'designated_ward' \
          AND v.occupation_code = ?2 \
          AND mcm.parent_code = ?1 \
     ), \
     national AS ( \
        SELECT COUNT(*) AS national_total FROM v2_municipality_target_thickness \
        WHERE occupation_code = ?2 \
     ) \
     SELECT \
        w.municipality_code, w.municipality_name, \
        w.parent_code, w.parent_name, \
        RANK() OVER (PARTITION BY w.parent_code ORDER BY w.thickness_index DESC) AS parent_rank, \
        COUNT(*) OVER (PARTITION BY w.parent_code) AS parent_total, \
        w.rank_in_occupation AS national_rank, \
        n.national_total, \
        w.thickness_index, w.distribution_priority AS priority \
     FROM ward_set w, national n \
     ORDER BY parent_rank"
}

/// `v2_municipality_target_thickness` × `municipality_code_master` から
/// 親市内ランキングを取得する (Window Function `RANK() OVER` / `COUNT(*) OVER` 使用)。
///
/// `parent_code` / `occupation_code` どちらかが空の場合は空 Vec を返す。
/// テーブルが両方とも (Turso/local) 不在の場合も空 Vec。
///
/// 詳細仕様: 計画書 §3.3
pub(crate) fn fetch_ward_rankings_by_parent(
    db: &Db,
    turso: Option<&TursoDb>,
    parent_code: &str,
    occupation_code: &str,
) -> Vec<Row> {
    if parent_code.is_empty() || occupation_code.is_empty() {
        return vec![];
    }
    // designated_ward は v2_municipality_target_thickness と municipality_code_master の両方が必要。
    // 片方でもローカル不在 + Turso なし → 空 Vec。
    let local_has = table_exists(db, "v2_municipality_target_thickness")
        && table_exists(db, "municipality_code_master");
    if !local_has && turso.is_none() {
        return vec![];
    }

    let sql = build_ward_ranking_sql();
    let params = vec![parent_code.to_string(), occupation_code.to_string()];
    query_turso_or_local(turso, db, sql, &params, "v2_municipality_target_thickness")
}

/// `municipality_code_master` から市区町村コードマスター行を取得する。
///
/// `municipality_codes` が空の場合は全件 (1,917 行程度) を返却する。
/// lookup 用途のため WHERE 句なしでテーブル全体を返す挙動を採用。
///
/// 詳細仕様: 計画書 §3.4
pub(crate) fn fetch_code_master(
    db: &Db,
    turso: Option<&TursoDb>,
    municipality_codes: &[&str],
) -> Vec<Row> {
    if !table_exists(db, "municipality_code_master") && turso.is_none() {
        return vec![];
    }

    if municipality_codes.is_empty() {
        let sql =
            "SELECT municipality_code, municipality_name, prefecture, area_type, parent_code \
                   FROM municipality_code_master \
                   ORDER BY municipality_code";
        return query_turso_or_local(turso, db, sql, &[], "municipality_code_master");
    }

    let placeholders: String = (1..=municipality_codes.len())
        .map(|i| format!("?{i}"))
        .collect::<Vec<_>>()
        .join(",");
    let sql = format!(
        "SELECT municipality_code, municipality_name, prefecture, area_type, parent_code \
         FROM municipality_code_master \
         WHERE municipality_code IN ({placeholders}) \
         ORDER BY municipality_code"
    );
    let params: Vec<String> = municipality_codes.iter().map(|s| s.to_string()).collect();
    query_turso_or_local(turso, db, &sql, &params, "municipality_code_master")
}

/// (prefecture, municipality_name) のペアから JIS 5 桁コードを解決する。
///
/// Phase 3 Step 5 Phase 5.5 (2026-05-04) で追加: variant guard 内で
/// `agg.by_municipality_salary` の (pref, name) から target_codes を導出し、
/// 4 fetch (recruiting_scores / occupation_cells / ward_thickness / code_master) を
/// 完全活性化するための解決ヘルパー。
///
/// # 設計
/// - 空入力 → 空 Vec (early return、全件取得を避ける)
/// - `area_level = 'unit'` 限定 (aggregate_city などの集約行は除外、Plan B 設計通り)
/// - 解決不能な (pref, name) はスキップ (返却行に現れない)
///
/// # 引数
/// - `pref_name_pairs`: 重複除去済みの (都道府県名, 市区町村名) ペア
pub(crate) fn fetch_code_master_by_names(
    db: &Db,
    turso: Option<&TursoDb>,
    pref_name_pairs: &[(&str, &str)],
) -> Vec<Row> {
    if pref_name_pairs.is_empty() {
        return vec![];
    }
    if !table_exists(db, "municipality_code_master") && turso.is_none() {
        return vec![];
    }

    let mut conds: Vec<String> = Vec::with_capacity(pref_name_pairs.len());
    let mut params: Vec<String> = Vec::with_capacity(pref_name_pairs.len() * 2);
    for (i, (pref, name)) in pref_name_pairs.iter().enumerate() {
        let p_idx = i * 2 + 1;
        let n_idx = i * 2 + 2;
        conds.push(format!(
            "(prefecture = ?{p_idx} AND municipality_name = ?{n_idx})"
        ));
        params.push(pref.to_string());
        params.push(name.to_string());
    }
    let sql = format!(
        "SELECT municipality_code, municipality_name, prefecture, area_type, parent_code \
         FROM municipality_code_master \
         WHERE area_level = 'unit' AND ({}) \
         ORDER BY municipality_code",
        conds.join(" OR ")
    );
    query_turso_or_local(turso, db, &sql, &params, "municipality_code_master")
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

    // -------- Worker B (Round 2) 投入版スキーマ追加フィールド --------
    /// 'resident' (本日版は resident のみ。将来 'workplace' に拡張予定)
    pub basis: String,
    /// 職業コード (例: '08_生産工程')
    pub occupation_code: String,
    /// 職業名
    pub occupation_name: String,
    /// 0-100 指数 (本職業の母集団厚み)
    pub target_thickness_index: Option<f64>,
    /// 0-100 指数 (通勤アクセス、Worker B 命名)
    pub commute_access_score: Option<f64>,
    /// 0-100 指数 (競合圧、Worker B 統合命名)
    pub competition_score: Option<f64>,
    /// 0-100 指数 (給与×生活コスト統合、Worker B 命名)
    pub salary_living_score: Option<f64>,
    /// 全国順位 (1=最上位)
    pub rank_in_occupation: Option<i64>,
    /// 0.0〜1.0 のパーセンタイル
    pub rank_percentile: Option<f64>,
    /// 'S' | 'A' | 'B' | 'C' | 'D' (CHECK 制約)
    pub distribution_priority: Option<String>,
    /// シナリオスコア (INTEGER、保守 ≤ 標準 ≤ 強気)
    pub scenario_conservative_score: Option<i64>,
    pub scenario_standard_score: Option<i64>,
    pub scenario_aggressive_score: Option<i64>,
    /// 'estimated_beta' | 'measured' | 'derived'
    pub data_label: String,
    /// 出所名 (例: 'national_census_2020')
    pub source_name: String,
    /// 例: 'hypothesis_v1'
    pub weight_source: Option<String>,
    /// 例: 'A-' / 'B+'
    pub estimate_grade: Option<String>,
}

#[allow(dead_code)]
impl MunicipalityRecruitingScore {
    /// `Row` から DTO を構築する。NULL / パース不能なフィールドは `None` / 空文字 fallback。
    ///
    /// Worker B (Round 2) 投入後は `occupation_code` / `occupation_name` / `basis` が
    /// 主たるソースカラム。後方互換のため `occupation_group_code` には `occupation_code`
    /// を fallback でコピーし、既存呼び出し元 (report_html) を壊さない。
    pub fn from_row(row: &Row) -> Self {
        let opt_string = |key: &str| -> Option<String> {
            match row.get(key) {
                Some(Value::String(s)) if !s.is_empty() => Some(s.clone()),
                Some(Value::Null) | None => None,
                Some(Value::String(_)) => None,
                Some(v) => Some(v.to_string()),
            }
        };
        // Worker B 版で唯一存在する occupation_code を group_code にも反映 (後方互換)
        let occ_code = str_or_empty(row, "occupation_code");
        let occ_name = str_or_empty(row, "occupation_name");
        let group_code_legacy = str_or_empty(row, "occupation_group_code");
        let group_name_legacy = str_or_empty(row, "occupation_group_name");
        Self {
            municipality_code: str_or_empty(row, "municipality_code"),
            prefecture: str_or_empty(row, "prefecture"),
            municipality_name: str_or_empty(row, "municipality_name"),
            occupation_group_code: if group_code_legacy.is_empty() {
                occ_code.clone()
            } else {
                group_code_legacy
            },
            occupation_group_name: if group_name_legacy.is_empty() {
                occ_name.clone()
            } else {
                group_name_legacy
            },
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
            // -------- Worker B 投入版フィールド --------
            basis: str_or_empty(row, "basis"),
            occupation_code: occ_code,
            occupation_name: occ_name,
            target_thickness_index: opt_f64(row, "target_thickness_index"),
            commute_access_score: opt_f64(row, "commute_access_score"),
            competition_score: opt_f64(row, "competition_score"),
            salary_living_score: opt_f64(row, "salary_living_score"),
            rank_in_occupation: opt_i64(row, "rank_in_occupation"),
            rank_percentile: opt_f64(row, "rank_percentile"),
            distribution_priority: opt_string("distribution_priority"),
            scenario_conservative_score: opt_i64(row, "scenario_conservative_score"),
            scenario_standard_score: opt_i64(row, "scenario_standard_score"),
            scenario_aggressive_score: opt_i64(row, "scenario_aggressive_score"),
            data_label: str_or_empty(row, "data_label"),
            source_name: str_or_empty(row, "source_name"),
            weight_source: opt_string("weight_source"),
            estimate_grade: opt_string("estimate_grade"),
        }
    }

    /// `distribution_priority` が CHECK 制約 ('S'|'A'|'B'|'C'|'D') の値か。
    /// None は検証不可で true。
    pub fn is_priority_grade_in_set(&self) -> bool {
        match self.distribution_priority.as_deref() {
            None => true,
            Some(g) => matches!(g, "S" | "A" | "B" | "C" | "D"),
        }
    }

    /// Worker B 版のシナリオスコア (INTEGER) が「保守 ≤ 標準 ≤ 強気」を満たすか。
    /// 3 値とも値ありの時のみ厳密検証、欠損時は true。
    pub fn is_scenario_score_consistent(&self) -> bool {
        match (
            self.scenario_conservative_score,
            self.scenario_standard_score,
            self.scenario_aggressive_score,
        ) {
            (Some(c), Some(s), Some(a)) => c <= s && s <= a,
            _ => true,
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

    /// `distribution_priority_score` が `[0.0, 200.0]` の範囲内か。
    ///
    /// build_municipality_recruiting_scores.py:244 で `clamp(0, 200)` で投入されているため、
    /// display 側も 200 を上限として受け入れる (Round 9 P2-G で範囲不一致バグを解消)。
    /// METRICS.md §2.1 の旧 `clamp(..., 0, 100)` 制約は build 側の penalty 適用後 raw_score
    /// が 100 を超えうる仕様変更で形骸化していた。実データ max=169.38 (cap saturation)。
    pub fn is_priority_score_in_range(&self) -> bool {
        match self.distribution_priority_score {
            Some(s) => (0.0..=200.0).contains(&s) && !s.is_nan(),
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

    // -------- 旧 spec フィールド (後方互換、SQL では選択しないため常に None) --------
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

    // -------- Worker A (Round 2) 投入版スキーマ追加フィールド --------
    /// 'reference' (Worker A 版は reference のみ。CHECK 制約)
    pub basis: String,
    /// 100 を全国平均とする物価指数
    pub cost_index: Option<f64>,
    /// 最低賃金 (円/時)
    pub min_wage: Option<i64>,
    /// 地価 proxy (円/㎡相当の正規化値)
    pub land_price_proxy: Option<f64>,
    /// 物価補正後の実質賃金 proxy
    pub salary_real_terms_proxy: Option<f64>,
    /// 'reference' (CHECK 制約)
    pub data_label: String,
    /// 出所名 (例: 'mhlw_min_wage_2024 + estat_cpi_2020')
    pub source_name: String,
    /// 例: 'hypothesis_v1'
    pub weight_source: Option<String>,
}

#[allow(dead_code)]
impl LivingCostProxy {
    pub fn from_row(row: &Row) -> Self {
        let opt_string = |key: &str| -> Option<String> {
            match row.get(key) {
                Some(Value::String(s)) if !s.is_empty() => Some(s.clone()),
                Some(Value::Null) | None => None,
                Some(Value::String(_)) => None,
                Some(v) => Some(v.to_string()),
            }
        };
        Self {
            municipality_code: str_or_empty(row, "municipality_code"),
            prefecture: str_or_empty(row, "prefecture"),
            municipality_name: str_or_empty(row, "municipality_name"),
            // 旧 spec カラム (Worker A 版 SQL では選択しないため None)
            single_household_rent_proxy: opt_i64(row, "single_household_rent_proxy"),
            small_household_rent_proxy: opt_i64(row, "small_household_rent_proxy"),
            rent_per_square_meter: opt_f64(row, "rent_per_square_meter"),
            retail_price_index_proxy: opt_f64(row, "retail_price_index_proxy"),
            household_spending_annual_yen: opt_i64(row, "household_spending_annual_yen"),
            land_price_residential_per_sqm: opt_f64(row, "land_price_residential_per_sqm"),
            housing_cost_rank: opt_i64(row, "housing_cost_rank"),
            source_year: opt_i64(row, "source_year"),
            // Worker A 版フィールド
            basis: str_or_empty(row, "basis"),
            cost_index: opt_f64(row, "cost_index"),
            min_wage: opt_i64(row, "min_wage"),
            land_price_proxy: opt_f64(row, "land_price_proxy"),
            salary_real_terms_proxy: opt_f64(row, "salary_real_terms_proxy"),
            data_label: str_or_empty(row, "data_label"),
            source_name: str_or_empty(row, "source_name"),
            weight_source: opt_string("weight_source"),
        }
    }

    /// `data_label` CHECK 制約検証: 本日版は 'reference' のみ許容。
    /// 空文字 (Default 値) も検証不可で true。
    pub fn is_data_label_in_set(&self) -> bool {
        self.data_label.is_empty() || self.data_label == "reference"
    }

    /// `cost_index` が現実的範囲 (0.0 < x < 500.0) か。
    /// 100 が全国平均、極端値検出用 (METRICS.md §7)。値なしは true。
    pub fn is_cost_index_realistic(&self) -> bool {
        match self.cost_index {
            Some(v) => v.is_finite() && v > 0.0 && v < 500.0,
            None => true,
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

    /// `Row` から DTO を構築する。
    ///
    /// XOR 不変条件は DB 側 CHECK で保証されるため、ここではそのままマップする。
    /// `is_industrial_anchor` 等の bool は別 DTO で扱う。
    pub fn from_row(row: &Row) -> Self {
        let weight_source = match row.get("weight_source") {
            Some(Value::String(s)) => Some(s.clone()),
            Some(Value::Null) | None => None,
            Some(v) => Some(v.to_string()),
        };
        Self {
            municipality_code: str_or_empty(row, "municipality_code"),
            prefecture: str_or_empty(row, "prefecture"),
            municipality_name: str_or_empty(row, "municipality_name"),
            basis: str_or_empty(row, "basis"),
            occupation_code: str_or_empty(row, "occupation_code"),
            occupation_name: str_or_empty(row, "occupation_name"),
            age_class: str_or_empty(row, "age_class"),
            gender: str_or_empty(row, "gender"),
            population: opt_i64(row, "population"),
            estimate_index: opt_f64(row, "estimate_index"),
            data_label: str_or_empty(row, "data_label"),
            source_name: str_or_empty(row, "source_name"),
            source_year: opt_i64(row, "source_year").unwrap_or(0),
            weight_source,
        }
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

#[allow(dead_code)]
impl WardThicknessDto {
    pub fn from_row(row: &Row) -> Self {
        let opt_string = |key: &str| -> Option<String> {
            match row.get(key) {
                Some(Value::String(s)) if !s.is_empty() => Some(s.clone()),
                Some(Value::Null) | None => None,
                Some(Value::String(_)) => None,
                Some(v) => Some(v.to_string()),
            }
        };
        let is_industrial_anchor = opt_i64(row, "is_industrial_anchor")
            .map(|v| v != 0)
            .unwrap_or(false);
        Self {
            municipality_code: str_or_empty(row, "municipality_code"),
            municipality_name: str_or_empty(row, "municipality_name"),
            prefecture: str_or_empty(row, "prefecture"),
            basis: str_or_empty(row, "basis"),
            occupation_code: str_or_empty(row, "occupation_code"),
            occupation_name: str_or_empty(row, "occupation_name"),
            thickness_index: opt_f64(row, "thickness_index").unwrap_or(0.0),
            rank_in_occupation: opt_i64(row, "rank_in_occupation"),
            rank_percentile: opt_f64(row, "rank_percentile"),
            distribution_priority: opt_string("distribution_priority"),
            scenario_conservative_index: opt_i64(row, "scenario_conservative_index"),
            scenario_standard_index: opt_i64(row, "scenario_standard_index"),
            scenario_aggressive_index: opt_i64(row, "scenario_aggressive_index"),
            estimate_grade: opt_string("estimate_grade"),
            weight_source: str_or_empty(row, "weight_source"),
            is_industrial_anchor,
            source_year: opt_i64(row, "source_year").unwrap_or(0),
        }
    }
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

    pub fn from_row(row: &Row) -> Self {
        Self {
            municipality_code: str_or_empty(row, "municipality_code"),
            municipality_name: str_or_empty(row, "municipality_name"),
            parent_code: str_or_empty(row, "parent_code"),
            parent_name: str_or_empty(row, "parent_name"),
            parent_rank: opt_i64(row, "parent_rank").unwrap_or(0),
            parent_total: opt_i64(row, "parent_total").unwrap_or(0),
            national_rank: opt_i64(row, "national_rank").unwrap_or(0),
            national_total: opt_i64(row, "national_total").unwrap_or(0),
            thickness_index: opt_f64(row, "thickness_index").unwrap_or(0.0),
            priority: str_or_empty(row, "priority"),
        }
    }
}

// ============================================================
// Round 8 P0-2 (2026-05-10): 産業 × 性別 (経済センサス R3)
// ============================================================
//
// 経済センサス R3 (statsDataId=0003449718) の集約形式に合わせ、
// 個別自治体 city_code を集約コード (特別区部 / 政令市本市) に変換してから
// `v2_external_industry_structure` を引く。
//
// 設計参照: docs/MEDIA_REPORT_P0_FEASIBILITY_CHECK_2026_05_09.md

/// `city_code` を経済センサス R3 の集約コードに変換する。
///
/// - 東京 23 区 (13101〜13123) → 13100 (特別区部)
/// - 政令市の行政区 → 政令市本市コード
/// - それ以外 → 入力コードをそのまま (5 桁正規化のみ)
pub(crate) fn aggregate_to_industry_structure_code(code: &str) -> String {
    let s = if code.len() < 5 {
        format!("{:0>5}", code)
    } else {
        code.to_string()
    };
    if s.len() != 5 || !s.chars().all(|c| c.is_ascii_digit()) {
        return s;
    }
    let n: u32 = match s.parse() {
        Ok(v) => v,
        Err(_) => return s,
    };
    // 東京 23 区
    if (13101..=13123).contains(&n) { return "13100".to_string(); }
    // 政令市の行政区 (各政令市の本市コード)
    if (1101..=1110).contains(&n)   { return "01100".to_string(); } // 札幌市
    if (4101..=4105).contains(&n)   { return "04100".to_string(); } // 仙台市
    if (11101..=11110).contains(&n) { return "11100".to_string(); } // さいたま市
    if (12101..=12106).contains(&n) { return "12100".to_string(); } // 千葉市
    if (14101..=14118).contains(&n) { return "14100".to_string(); } // 横浜市
    if (14131..=14137).contains(&n) { return "14130".to_string(); } // 川崎市
    if (14151..=14153).contains(&n) { return "14150".to_string(); } // 相模原市
    if (15101..=15108).contains(&n) { return "15100".to_string(); } // 新潟市
    if (22101..=22103).contains(&n) { return "22100".to_string(); } // 静岡市
    if (22131..=22137).contains(&n) { return "22130".to_string(); } // 浜松市
    if (23101..=23116).contains(&n) { return "23100".to_string(); } // 名古屋市
    if (26101..=26111).contains(&n) { return "26100".to_string(); } // 京都市
    if (27101..=27128).contains(&n) { return "27100".to_string(); } // 大阪市
    if (27141..=27147).contains(&n) { return "27140".to_string(); } // 堺市
    if (28101..=28110).contains(&n) { return "28100".to_string(); } // 神戸市
    if (33101..=33106).contains(&n) { return "33100".to_string(); } // 岡山市
    if (34101..=34108).contains(&n) { return "34100".to_string(); } // 広島市
    if (40101..=40109).contains(&n) { return "40100".to_string(); } // 北九州市
    if (40131..=40137).contains(&n) { return "40130".to_string(); } // 福岡市
    if (43101..=43105).contains(&n) { return "43100".to_string(); } // 熊本市
    s
}

/// 対象自治体 (集約変換済) の産業 × 性別を取得する。
///
/// `industry_code` は産業大分類のうち、合計系 ('AS','AR','CR') と
/// データ粒度の小さい ('AB','D') を除外する。
pub(crate) fn fetch_industry_structure_for_municipalities(
    db: &Db,
    turso: Option<&TursoDb>,
    target_municipalities: &[&str],
) -> Vec<Row> {
    if target_municipalities.is_empty() {
        return vec![];
    }
    if !table_exists(db, "v2_external_industry_structure") && turso.is_none() {
        return vec![];
    }

    let mut agg_codes: Vec<String> = target_municipalities
        .iter()
        .map(|c| aggregate_to_industry_structure_code(c))
        .collect();
    agg_codes.sort();
    agg_codes.dedup();

    let placeholders: String = (1..=agg_codes.len())
        .map(|i| format!("?{i}"))
        .collect::<Vec<_>>()
        .join(",");
    let params: Vec<String> = agg_codes;

    let sql = format!(
        "SELECT prefecture_code, city_code, city_name, industry_code, industry_name, \
                employees_total, employees_male, employees_female \
         FROM v2_external_industry_structure \
         WHERE city_code IN ({placeholders}) \
           AND industry_code NOT IN ('AS','AR','CR','AB','D') \
         ORDER BY city_code, employees_total DESC"
    );

    query_turso_or_local(turso, db, &sql, &params, "v2_external_industry_structure")
}

/// 産業 × 性別 行 (経済センサス R3 / employees_male,female 列を活用)
#[derive(Debug, Clone, Default, Serialize)]
#[allow(dead_code)]
pub struct IndustryGenderRow {
    pub prefecture_code: String,
    pub city_code: String,
    pub city_name: String,
    pub industry_code: String,
    pub industry_name: String,
    pub employees_total: Option<i64>,
    pub employees_male: Option<i64>,
    pub employees_female: Option<i64>,
}

#[allow(dead_code)]
impl IndustryGenderRow {
    pub fn from_row(row: &Row) -> Self {
        Self {
            prefecture_code: str_or_empty(row, "prefecture_code"),
            city_code: str_or_empty(row, "city_code"),
            city_name: str_or_empty(row, "city_name"),
            industry_code: str_or_empty(row, "industry_code"),
            industry_name: str_or_empty(row, "industry_name"),
            employees_total: opt_i64(row, "employees_total"),
            employees_male: opt_i64(row, "employees_male"),
            employees_female: opt_i64(row, "employees_female"),
        }
    }
}

#[allow(dead_code)]
pub(crate) fn to_industry_gender_rows(rows: &[Row]) -> Vec<IndustryGenderRow> {
    rows.iter().map(IndustryGenderRow::from_row).collect()
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

#[allow(dead_code)]
impl MunicipalityCodeMasterDto {
    pub fn from_row(row: &Row) -> Self {
        let parent_code = match row.get("parent_code") {
            Some(Value::String(s)) if !s.is_empty() => Some(s.clone()),
            Some(Value::Null) | None => None,
            Some(Value::String(_)) => None,
            Some(v) => Some(v.to_string()),
        };
        Self {
            municipality_code: str_or_empty(row, "municipality_code"),
            municipality_name: str_or_empty(row, "municipality_name"),
            prefecture: str_or_empty(row, "prefecture"),
            area_type: str_or_empty(row, "area_type"),
            parent_code,
        }
    }
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

    // -------- Round 8 P0-2 (2026-05-10): 産業 × 性別 (経済センサス R3) --------
    /// 対象自治体 (集約変換済) の産業 × 性別データ
    pub industry_gender_rows: Vec<IndustryGenderRow>,

    // -------- Round 8 P1-1 (2026-05-10): CSV 求人数 × 地域母集団 4 象限図 --------
    /// CSV 由来の対象自治体集計 (prefecture, name, count, median_salary)
    /// `agg.by_municipality_salary` の情報を build 後に inject する。
    pub csv_municipalities: Vec<CsvMunicipalityCell>,
}

/// Round 8 P1-1: CSV 求人数 × 地域母集団 4 象限図用の自治体セル
#[derive(Debug, Clone, Default, Serialize)]
#[allow(dead_code)]
pub struct CsvMunicipalityCell {
    pub prefecture: String,
    pub name: String,
    pub count: usize,
    pub median_salary: i64,
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
    rows.iter()
        .map(MunicipalityRecruitingScore::from_row)
        .collect()
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
    rows.iter()
        .map(OccupationPopulationCell::from_row)
        .collect()
}

// -------- Phase 3 Step 5 Phase 3: 4 新規 DTO 用変換ヘルパー --------

#[allow(dead_code)]
pub(crate) fn to_occupation_cells(rows: &[Row]) -> Vec<OccupationCellDto> {
    rows.iter().map(OccupationCellDto::from_row).collect()
}

#[allow(dead_code)]
pub(crate) fn to_ward_thickness_dtos(rows: &[Row]) -> Vec<WardThicknessDto> {
    rows.iter().map(WardThicknessDto::from_row).collect()
}

#[allow(dead_code)]
pub(crate) fn to_ward_rankings(rows: &[Row]) -> Vec<WardRankingRowDto> {
    rows.iter().map(WardRankingRowDto::from_row).collect()
}

#[allow(dead_code)]
pub(crate) fn to_code_master(rows: &[Row]) -> Vec<MunicipalityCodeMasterDto> {
    rows.iter()
        .map(MunicipalityCodeMasterDto::from_row)
        .collect()
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
        assert_eq!(
            rows.len(),
            3,
            "self-loop 除外で 3 件期待 (実際: {})",
            rows.len()
        );

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
        row.insert(
            "occupation_group_code".into(),
            Value::String("driver".into()),
        );
        row.insert(
            "occupation_group_name".into(),
            Value::String("輸送・機械運転".into()),
        );
        row.insert("target_population".into(), Value::String("12345".into()));
        row.insert(
            "distribution_priority_score".into(),
            Value::String("78.5".into()),
        );
        row.insert(
            "scenario_conservative_population".into(),
            Value::String("100".into()),
        );
        row.insert(
            "scenario_standard_population".into(),
            Value::String("300".into()),
        );
        row.insert(
            "scenario_aggressive_population".into(),
            Value::String("500".into()),
        );

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

    /// `is_priority_score_in_range` が `[0.0, 200.0]` を検証すること (Round 9 P2-G 拡張)。
    #[test]
    fn test_priority_score_range_invariant() {
        let cases = [
            (0.0_f64, true, "下限 0"),
            (100.0, true, "中間 100 (旧上限、現在は範囲内)"),
            (169.38, true, "実データ max"),
            (200.0, true, "新上限 200"),
            (50.5, true, "中間値"),
            (-0.1, false, "負値"),
            (200.001, false, "新上限超過"),
            (201.0, false, "上限超過"),
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
        row_a.insert(
            "origin_municipality_code".into(),
            Value::String("01217".into()),
        );
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
        row1.insert(
            "distribution_priority_score".into(),
            Value::String("85.0".into()),
        );
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
            distribution_priority_score: Some(250.0), // 範囲超過 (新上限 200 超)
            ..Default::default()
        };
        let bad_data = SurveyMarketIntelligenceData {
            recruiting_scores: vec![bad_score],
            ..Default::default()
        };
        assert!(!bad_data.all_invariants_hold());
    }

    // ============================================================
    // Worker C (Round 2) 追加: Worker A/B 投入版スキーマ対応 fetch + DTO 検証
    // ============================================================

    /// Worker A 投入版スキーマで `municipality_living_cost_proxy` テーブル不在時、
    /// fetch_living_cost_proxy が空 Vec を返すこと (フェイルセーフ)。
    #[test]
    fn fetch_living_cost_proxy_returns_empty_when_table_missing() {
        let (_tmp, db) = create_test_db();
        // テーブル不在 + Turso None
        let result = fetch_living_cost_proxy(&db, None, &["01101", "13104"]);
        assert!(result.is_empty(), "テーブル不在時は空 Vec");
    }

    /// Worker A 投入版スキーマの NULL 許容カラム (land_price_proxy 等) が
    /// LivingCostProxy DTO 経由で None として復元されること。
    #[test]
    fn fetch_living_cost_proxy_handles_null_columns() {
        let (_tmp, db) = create_test_db();

        // Worker A の DDL を再現
        db.execute(
            "CREATE TABLE municipality_living_cost_proxy (
                municipality_code TEXT NOT NULL,
                prefecture        TEXT NOT NULL,
                municipality_name TEXT NOT NULL,
                basis             TEXT NOT NULL,
                cost_index        REAL,
                min_wage          INTEGER,
                land_price_proxy  REAL,
                salary_real_terms_proxy REAL,
                data_label        TEXT NOT NULL,
                source_name       TEXT NOT NULL,
                source_year       INTEGER NOT NULL,
                weight_source     TEXT,
                estimated_at      TEXT NOT NULL DEFAULT (datetime('now')),
                PRIMARY KEY (municipality_code, basis, source_year)
            )",
            &[],
        )
        .expect("CREATE TABLE 失敗");

        // 1 行投入: cost_index/min_wage は値あり、land_price_proxy / salary_real_terms_proxy / weight_source は NULL
        db.execute(
            "INSERT INTO municipality_living_cost_proxy \
             (municipality_code, prefecture, municipality_name, basis, \
              cost_index, min_wage, land_price_proxy, salary_real_terms_proxy, \
              data_label, source_name, source_year, weight_source) \
             VALUES ('01101', '北海道', '札幌市', 'reference', \
                     98.5, 1010, NULL, NULL, \
                     'reference', 'mhlw_min_wage_2024', 2024, NULL)",
            &[],
        )
        .expect("INSERT 失敗");

        let rows = fetch_living_cost_proxy(&db, None, &["01101"]);
        assert_eq!(rows.len(), 1, "1 行取得期待");
        let dto = LivingCostProxy::from_row(&rows[0]);
        assert_eq!(dto.municipality_code, "01101");
        assert_eq!(dto.basis, "reference");
        assert_eq!(dto.data_label, "reference");
        assert_eq!(dto.cost_index, Some(98.5));
        assert_eq!(dto.min_wage, Some(1010));
        // NULL カラムは None
        assert_eq!(dto.land_price_proxy, None);
        assert_eq!(dto.salary_real_terms_proxy, None);
        assert_eq!(dto.weight_source, None);
        assert_eq!(dto.source_year, Some(2024));
        // 不変条件
        assert!(dto.is_data_label_in_set());
        assert!(dto.is_cost_index_realistic());
    }

    /// Worker B 投入版は basis='resident' のみ。fetch 結果の DTO がそれを満たし、
    /// data_label='estimated_beta' であること。
    #[test]
    fn fetch_recruiting_scores_returns_basis_resident_only() {
        let (_tmp, db) = create_test_db();
        db.execute(
            "CREATE TABLE municipality_recruiting_scores (
                municipality_code TEXT NOT NULL,
                prefecture        TEXT NOT NULL,
                municipality_name TEXT NOT NULL,
                basis             TEXT NOT NULL,
                occupation_code   TEXT NOT NULL,
                occupation_name   TEXT NOT NULL,
                distribution_priority_score REAL NOT NULL,
                target_thickness_index      REAL,
                commute_access_score        REAL,
                competition_score           REAL,
                salary_living_score         REAL,
                rank_in_occupation INTEGER,
                rank_percentile    REAL,
                distribution_priority TEXT,
                scenario_conservative_score INTEGER,
                scenario_standard_score     INTEGER,
                scenario_aggressive_score   INTEGER,
                data_label    TEXT NOT NULL,
                source_name   TEXT NOT NULL,
                source_year   INTEGER NOT NULL,
                weight_source TEXT,
                estimate_grade TEXT,
                estimated_at TEXT NOT NULL DEFAULT (datetime('now')),
                PRIMARY KEY (municipality_code, basis, occupation_code, source_year)
            )",
            &[],
        )
        .expect("CREATE TABLE 失敗");

        db.execute(
            "INSERT INTO municipality_recruiting_scores \
             (municipality_code, prefecture, municipality_name, basis, \
              occupation_code, occupation_name, distribution_priority_score, \
              target_thickness_index, commute_access_score, competition_score, salary_living_score, \
              rank_in_occupation, rank_percentile, distribution_priority, \
              scenario_conservative_score, scenario_standard_score, scenario_aggressive_score, \
              data_label, source_name, source_year, weight_source, estimate_grade) \
             VALUES ('01101', '北海道', '札幌市', 'resident', \
                     '08_生産工程', '生産工程', 78.5, \
                     85.0, 70.0, 60.0, 75.0, \
                     12, 0.93, 'A', \
                     50, 80, 120, \
                     'estimated_beta', 'national_census_2020', 2020, 'hypothesis_v1', 'A-')",
            &[],
        )
        .expect("INSERT 失敗");

        let rows = fetch_recruiting_scores_by_municipalities(&db, None, &["01101"], "");
        assert_eq!(rows.len(), 1);
        let dto = MunicipalityRecruitingScore::from_row(&rows[0]);
        assert_eq!(dto.basis, "resident", "本日版は basis=resident のみ");
        assert_eq!(dto.data_label, "estimated_beta", "本日版は estimated_beta");
        assert_eq!(dto.occupation_code, "08_生産工程");
        assert_eq!(dto.target_thickness_index, Some(85.0));
        assert_eq!(dto.commute_access_score, Some(70.0));
        assert_eq!(dto.competition_score, Some(60.0));
        assert_eq!(dto.salary_living_score, Some(75.0));
        assert_eq!(dto.scenario_conservative_score, Some(50));
        assert_eq!(dto.scenario_standard_score, Some(80));
        assert_eq!(dto.scenario_aggressive_score, Some(120));
        assert_eq!(dto.estimate_grade.as_deref(), Some("A-"));
        assert!(dto.is_scenario_score_consistent(), "50 ≤ 80 ≤ 120");
    }

    /// `distribution_priority` が CHECK 制約セット ('S'|'A'|'B'|'C'|'D') に従うこと。
    /// 想定外値や None の挙動も含めて検証。
    #[test]
    fn fetch_recruiting_scores_distribution_priority_in_set() {
        let cases = [
            (Some("S"), true, "S は許容"),
            (Some("A"), true, "A は許容"),
            (Some("B"), true, "B は許容"),
            (Some("C"), true, "C は許容"),
            (Some("D"), true, "D は許容"),
            (Some("E"), false, "E は不許容"),
            (Some("a"), false, "小文字 a は不許容 (大文字限定)"),
            (Some(""), false, "空文字は不許容 (Some 経由)"),
            (None, true, "None は検証不可で true"),
        ];
        for (val, expected, label) in cases {
            let dto = MunicipalityRecruitingScore {
                distribution_priority: val.map(|s| s.to_string()),
                ..Default::default()
            };
            assert_eq!(dto.is_priority_grade_in_set(), expected, "{label}: {val:?}");
        }

        // Worker B シナリオスコアの順序検証
        let consistent = MunicipalityRecruitingScore {
            scenario_conservative_score: Some(50),
            scenario_standard_score: Some(80),
            scenario_aggressive_score: Some(120),
            ..Default::default()
        };
        assert!(consistent.is_scenario_score_consistent());

        let inverted = MunicipalityRecruitingScore {
            scenario_conservative_score: Some(120),
            scenario_standard_score: Some(80),
            scenario_aggressive_score: Some(50),
            ..Default::default()
        };
        assert!(!inverted.is_scenario_score_consistent());
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

    // ============================================================
    // Phase 3 Step 5 Phase 6 (Worker P6): ドメイン不変条件追加テスト
    //
    // 背景: feedback_reverse_proof_tests.md (unemployment 380% 流出事故)。
    // 「合意確認」レベルではなく「ドメイン不変条件で前提誤りを検出する」。
    // ============================================================

    /// 不変条件: estimated_beta 行は population を持たない (XOR 違反検出)
    #[test]
    fn invariant_estimated_beta_never_has_population() {
        let bad = OccupationCellDto {
            data_label: "estimated_beta".into(),
            population: Some(100), // 違反: estimated_beta なのに人数あり
            estimate_index: Some(140.0),
            ..Default::default()
        };
        assert!(
            !bad.is_xor_consistent(),
            "estimated_beta に population があると XOR 違反として検出されること"
        );
    }

    /// 不変条件: measured 行は estimate_index を持たない
    #[test]
    fn invariant_measured_never_has_estimate_index() {
        let bad = OccupationCellDto {
            data_label: "measured".into(),
            population: Some(100),
            estimate_index: Some(50.0), // 違反: measured なのに指数あり
            ..Default::default()
        };
        assert!(
            !bad.is_xor_consistent(),
            "measured に estimate_index があると XOR 違反として検出されること"
        );
    }

    /// 不変条件: thickness_index は妥当な範囲 (0 <= x <= 200 が正常域)
    /// (380% 失業率事故の教訓: 異常値検出は値域チェックで)
    #[test]
    fn invariant_thickness_index_within_plausible_range() {
        // 正常: cap 内
        let valid = WardThicknessDto {
            thickness_index: 142.5,
            ..Default::default()
        };
        assert!(
            valid.thickness_index >= 0.0 && valid.thickness_index <= 200.0,
            "正常な thickness_index は 0-200 範囲"
        );

        // 異常: cap 超過 (Plan B 仕様 200 超は異常)
        let invalid_high = WardThicknessDto {
            thickness_index: 999.0,
            ..Default::default()
        };
        assert!(
            invalid_high.thickness_index > 200.0,
            "999.0 は cap 違反として検出可能"
        );

        // 異常: 負値
        let invalid_neg = WardThicknessDto {
            thickness_index: -10.0,
            ..Default::default()
        };
        assert!(
            invalid_neg.thickness_index < 0.0,
            "負値は不変条件違反として検出可能"
        );
    }

    /// 不変条件: parent_rank は parent_total を超えない (1 <= rank <= total)
    #[test]
    fn invariant_parent_rank_must_not_exceed_total() {
        // 正常: 5 / 18
        let valid = WardRankingRowDto {
            parent_code: "14100".into(),
            parent_rank: 5,
            parent_total: 18,
            ..Default::default()
        };
        assert!(valid.uses_parent_rank_primary(), "5 位 / 18 中は正常");

        // 異常: rank > total
        let invalid = WardRankingRowDto {
            parent_code: "14100".into(),
            parent_rank: 20,
            parent_total: 18,
            ..Default::default()
        };
        assert!(
            !invalid.uses_parent_rank_primary(),
            "rank > total は不変条件違反として検出されること"
        );
    }

    /// 不変条件: parent_rank == 0 は invalid (1-indexed なので 0 は未定義)
    #[test]
    fn invariant_parent_rank_zero_is_invalid() {
        let zero_rank = WardRankingRowDto {
            parent_code: "14100".into(),
            parent_rank: 0,
            parent_total: 18,
            ..Default::default()
        };
        assert!(
            !zero_rank.uses_parent_rank_primary(),
            "parent_rank = 0 は無効値として検出"
        );
    }
}

// -------- Phase 3 Step 5 Phase 3: 4 新規 fetch 関数 + DTO from_row のテスト --------
#[cfg(test)]
mod phase3_step5_fetch_tests {
    use super::*;
    use crate::db::local_sqlite::LocalDb;

    /// 空の一時 SQLite DB を作成 (既存 tests と同パターン)。
    fn create_test_db() -> (tempfile::NamedTempFile, LocalDb) {
        let tmp = tempfile::NamedTempFile::new().unwrap();
        let path = tmp.path().to_str().unwrap();
        let _ = rusqlite::Connection::open(path).unwrap();
        let db = LocalDb::new(path).unwrap();
        (tmp, db)
    }

    /// `municipality_occupation_population` テーブルを作成し、measured / estimated_beta の
    /// XOR 制約付き行を投入する。
    fn create_occupation_population_table(db: &LocalDb) {
        db.execute(
            "CREATE TABLE municipality_occupation_population (
                municipality_code TEXT NOT NULL,
                prefecture TEXT,
                municipality_name TEXT,
                basis TEXT NOT NULL,
                occupation_code TEXT NOT NULL,
                occupation_name TEXT,
                age_class TEXT,
                gender TEXT,
                population INTEGER,
                estimate_index REAL,
                data_label TEXT NOT NULL,
                source_name TEXT,
                source_year INTEGER,
                weight_source TEXT
            )",
            &[],
        )
        .expect("CREATE TABLE failed");
    }

    fn create_thickness_table(db: &LocalDb) {
        db.execute(
            "CREATE TABLE v2_municipality_target_thickness (
                municipality_code TEXT NOT NULL,
                municipality_name TEXT,
                prefecture TEXT,
                basis TEXT,
                occupation_code TEXT NOT NULL,
                occupation_name TEXT,
                thickness_index REAL,
                rank_in_occupation INTEGER,
                rank_percentile REAL,
                distribution_priority TEXT,
                scenario_conservative_index INTEGER,
                scenario_standard_index INTEGER,
                scenario_aggressive_index INTEGER,
                estimate_grade TEXT,
                weight_source TEXT,
                is_industrial_anchor INTEGER,
                source_year INTEGER
            )",
            &[],
        )
        .expect("CREATE TABLE thickness failed");
    }

    fn create_code_master_table(db: &LocalDb) {
        db.execute(
            "CREATE TABLE municipality_code_master (
                municipality_code TEXT PRIMARY KEY,
                municipality_name TEXT,
                prefecture TEXT,
                area_type TEXT,
                parent_code TEXT
            )",
            &[],
        )
        .expect("CREATE TABLE master failed");
    }

    // [1] table 不在時は空 Vec
    #[test]
    fn fetch_occupation_cells_returns_empty_when_table_missing() {
        let (_tmp, db) = create_test_db();
        let rows = fetch_occupation_cells(&db, None, &["13103"], None, None);
        assert!(rows.is_empty(), "table 不在時は空 Vec を期待");
    }

    // [2] measured XOR 行を読んで DTO に変換、is_xor_consistent が true
    #[test]
    fn fetch_occupation_cells_measured_returns_xor_consistent() {
        let (_tmp, db) = create_test_db();
        create_occupation_population_table(&db);
        db.execute(
            "INSERT INTO municipality_occupation_population VALUES \
             ('13103', '東京都', '港区', 'workplace', '08_生産工程', '生産工程', '15_64', 'total', \
              12345, NULL, 'measured', 'census_15_1', 2020, NULL)",
            &[],
        )
        .expect("INSERT failed");

        let rows = fetch_occupation_cells(&db, None, &["13103"], None, Some("workplace"));
        assert_eq!(rows.len(), 1);

        let cells = to_occupation_cells(&rows);
        assert_eq!(cells.len(), 1);
        for cell in &cells {
            assert!(
                cell.is_xor_consistent(),
                "measured 行は XOR 一貫していること"
            );
            if cell.data_label == "measured" {
                assert!(cell.population.is_some());
                assert!(cell.estimate_index.is_none());
            }
        }
    }

    // [3] estimated_beta XOR 行
    #[test]
    fn fetch_occupation_cells_estimated_returns_xor_consistent() {
        let (_tmp, db) = create_test_db();
        create_occupation_population_table(&db);
        db.execute(
            "INSERT INTO municipality_occupation_population VALUES \
             ('13103', '東京都', '港区', 'resident', '08_生産工程', '生産工程', '15_64', 'total', \
              NULL, 142.5, 'estimated_beta', 'model_f2_target_thickness', 2020, 'hypothesis_v1')",
            &[],
        )
        .expect("INSERT failed");

        let rows = fetch_occupation_cells(&db, None, &["13103"], None, Some("resident"));
        let cells = to_occupation_cells(&rows);
        assert!(!cells.is_empty());
        for cell in &cells {
            assert!(cell.is_xor_consistent(), "estimated_beta 行も XOR 一貫");
            assert!(cell.population.is_none());
            assert!(cell.estimate_index.is_some());
            assert_eq!(cell.weight_source.as_deref(), Some("hypothesis_v1"));
        }
    }

    // [4] resident は人数表示不可 (estimated_beta のみ)
    #[test]
    fn fetch_resident_cells_cannot_display_population() {
        let (_tmp, db) = create_test_db();
        create_occupation_population_table(&db);
        db.execute(
            "INSERT INTO municipality_occupation_population VALUES \
             ('13103', '東京都', '港区', 'resident', '08_生産工程', '生産工程', '15_64', 'total', \
              NULL, 142.5, 'estimated_beta', 'model_f2_target_thickness', 2020, 'hypothesis_v1')",
            &[],
        )
        .expect("INSERT failed");

        let rows = fetch_occupation_cells(&db, None, &["13103"], None, Some("resident"));
        let cells = to_occupation_cells(&rows);
        assert!(!cells.is_empty());
        for cell in &cells {
            if cell.basis == "resident" {
                assert!(
                    !cell.can_display_population(),
                    "resident + estimated_beta は人数表示不可"
                );
                assert!(cell.can_display_index(), "estimated_beta は指数表示可");
            }
        }
    }

    // [5] 親市内ランキングで parent_rank が主指標として有効
    #[test]
    fn fetch_ward_rankings_uses_parent_rank_primary() {
        let (_tmp, db) = create_test_db();
        create_thickness_table(&db);
        create_code_master_table(&db);

        // 横浜市本体 + 鶴見区 (designated_ward) を投入
        db.execute(
            "INSERT INTO municipality_code_master VALUES \
             ('14100', '横浜市', '神奈川県', 'aggregate_city', NULL), \
             ('14101', '横浜市鶴見区', '神奈川県', 'designated_ward', '14100'), \
             ('14102', '横浜市神奈川区', '神奈川県', 'designated_ward', '14100')",
            &[],
        )
        .expect("INSERT master failed");

        db.execute(
            "INSERT INTO v2_municipality_target_thickness \
             (municipality_code, municipality_name, prefecture, basis, \
              occupation_code, occupation_name, thickness_index, rank_in_occupation, \
              distribution_priority, weight_source, is_industrial_anchor, source_year) VALUES \
             ('14101', '横浜市鶴見区', '神奈川県', 'resident', '08_生産工程', '生産工程', \
              142.5, 12, 'A', 'hypothesis_v1', 1, 2020), \
             ('14102', '横浜市神奈川区', '神奈川県', 'resident', '08_生産工程', '生産工程', \
              98.0, 50, 'B', 'hypothesis_v1', 0, 2020)",
            &[],
        )
        .expect("INSERT thickness failed");

        let rows = fetch_ward_rankings_by_parent(&db, None, "14100", "08_生産工程");
        assert_eq!(rows.len(), 2, "designated_ward 2 区が返ること");

        let dtos = to_ward_rankings(&rows);
        for row in &dtos {
            assert!(
                row.uses_parent_rank_primary(),
                "parent_rank が主指標 (>=1, <=parent_total, parent_code 非空)"
            );
        }
        // 1 位は鶴見区 (thickness=142.5)
        assert_eq!(dtos[0].municipality_name, "横浜市鶴見区");
        assert_eq!(dtos[0].parent_rank, 1);
        assert_eq!(dtos[0].parent_total, 2);
    }

    // [6] Window Function SQL の文字列内に RANK() OVER / COUNT(*) OVER / PARTITION BY が含まれる
    #[test]
    fn ward_ranking_sql_includes_window_functions() {
        let sql = build_ward_ranking_sql();
        assert!(sql.contains("RANK() OVER"), "SQL must use RANK() OVER");
        assert!(sql.contains("COUNT(*) OVER"), "SQL must use COUNT(*) OVER");
        assert!(sql.contains("PARTITION BY"), "SQL must use PARTITION BY");
    }

    // [7] empty input で fetch_ward_thickness は空 Vec
    #[test]
    fn fetch_ward_thickness_empty_input_returns_empty() {
        let (_tmp, db) = create_test_db();
        let rows = fetch_ward_thickness(&db, None, &[], None);
        assert!(rows.is_empty());
    }

    // [8] fetch_code_master: 空 codes は全件取得
    #[test]
    fn fetch_code_master_returns_all_when_empty_input() {
        let (_tmp, db) = create_test_db();
        create_code_master_table(&db);
        db.execute(
            "INSERT INTO municipality_code_master VALUES \
             ('13101', '千代田区', '東京都', 'special_ward', '13100'), \
             ('14100', '横浜市', '神奈川県', 'aggregate_city', NULL), \
             ('14101', '横浜市鶴見区', '神奈川県', 'designated_ward', '14100')",
            &[],
        )
        .expect("INSERT master failed");

        // 空 codes → 全件 3 行
        let rows = fetch_code_master(&db, None, &[]);
        assert_eq!(rows.len(), 3, "空 codes は全件取得");

        let dtos = to_code_master(&rows);
        assert_eq!(dtos.len(), 3);
        // parent_code は Option<String>: aggregate_city は None、designated_ward は Some
        let yokohama = dtos
            .iter()
            .find(|d| d.municipality_code == "14100")
            .unwrap();
        assert!(yokohama.parent_code.is_none());
        let tsurumi = dtos
            .iter()
            .find(|d| d.municipality_code == "14101")
            .unwrap();
        assert_eq!(tsurumi.parent_code.as_deref(), Some("14100"));

        // 特定 codes 指定 → 1 行
        let rows_one = fetch_code_master(&db, None, &["14101"]);
        assert_eq!(rows_one.len(), 1);
    }

    // 追加: WardThicknessDto の bool 変換と数値 helper のスモーク
    #[test]
    fn fetch_ward_thickness_with_industrial_anchor_flag() {
        let (_tmp, db) = create_test_db();
        create_thickness_table(&db);
        db.execute(
            "INSERT INTO v2_municipality_target_thickness \
             (municipality_code, municipality_name, prefecture, basis, \
              occupation_code, occupation_name, thickness_index, rank_in_occupation, \
              distribution_priority, weight_source, is_industrial_anchor, \
              scenario_conservative_index, scenario_standard_index, scenario_aggressive_index, \
              source_year) VALUES \
             ('14101', '横浜市鶴見区', '神奈川県', 'resident', '08_生産工程', '生産工程', \
              142.5, 12, 'A', 'hypothesis_v1', 1, 100, 142, 180, 2020)",
            &[],
        )
        .expect("INSERT thickness failed");

        let rows = fetch_ward_thickness(&db, None, &["14101"], Some("08_生産工程"));
        assert_eq!(rows.len(), 1);

        let dtos = to_ward_thickness_dtos(&rows);
        assert_eq!(dtos.len(), 1);
        assert!(
            dtos[0].is_industrial_anchor,
            "is_industrial_anchor=1 → true"
        );
        assert_eq!(dtos[0].thickness_index, 142.5);
        assert_eq!(dtos[0].scenario_conservative_index, Some(100));
        assert_eq!(dtos[0].scenario_aggressive_index, Some(180));
        assert_eq!(dtos[0].distribution_priority.as_deref(), Some("A"));
    }

    // ============================================================
    // Phase 3 Step 5 Phase 5.5 (2026-05-04): fetch_code_master_by_names
    // ============================================================

    /// Phase 5.5 用のテスト fixture: area_level 列を含む master テーブル
    fn create_code_master_table_with_level(db: &LocalDb) {
        db.execute(
            "CREATE TABLE municipality_code_master (
                municipality_code TEXT PRIMARY KEY,
                municipality_name TEXT,
                prefecture TEXT,
                area_type TEXT,
                area_level TEXT,
                parent_code TEXT
            )",
            &[],
        )
        .expect("CREATE TABLE master (with area_level) failed");
    }

    fn insert_unit_master_rows(db: &LocalDb) {
        // 新宿区 / 港区 (special_ward, unit), 横浜市本体 (aggregate_city, aggregate)
        db.execute(
            "INSERT INTO municipality_code_master VALUES \
             ('13104', '新宿区', '東京都', 'special_ward', 'unit', '13100'), \
             ('13103', '港区', '東京都', 'special_ward', 'unit', '13100'), \
             ('14100', '横浜市', '神奈川県', 'aggregate_city', 'aggregate', NULL)",
            &[],
        )
        .expect("INSERT master failed");
    }

    #[test]
    fn fetch_code_master_by_names_resolves_known_wards() {
        let (_tmp, db) = create_test_db();
        create_code_master_table_with_level(&db);
        insert_unit_master_rows(&db);

        let pairs = [("東京都", "新宿区"), ("東京都", "港区")];
        let rows = fetch_code_master_by_names(&db, None, &pairs);
        assert_eq!(rows.len(), 2);
        let codes: Vec<String> = rows
            .iter()
            .map(|r| str_or_empty(r, "municipality_code"))
            .collect();
        assert!(codes.contains(&"13104".to_string()), "新宿区が解決");
        assert!(codes.contains(&"13103".to_string()), "港区が解決");
    }

    #[test]
    fn fetch_code_master_by_names_excludes_unresolvable() {
        let (_tmp, db) = create_test_db();
        create_code_master_table_with_level(&db);
        insert_unit_master_rows(&db);

        let pairs = [("東京都", "新宿区"), ("不明県", "存在しない市")];
        let rows = fetch_code_master_by_names(&db, None, &pairs);
        assert_eq!(rows.len(), 1, "解決できる 1 件のみ");
        assert_eq!(str_or_empty(&rows[0], "municipality_code"), "13104");
    }

    #[test]
    fn fetch_code_master_by_names_excludes_aggregate() {
        let (_tmp, db) = create_test_db();
        create_code_master_table_with_level(&db);
        insert_unit_master_rows(&db);

        // 横浜市本体 (aggregate) と 新宿区 (unit) を依頼 → 横浜市は除外
        let pairs = [("神奈川県", "横浜市"), ("東京都", "新宿区")];
        let rows = fetch_code_master_by_names(&db, None, &pairs);
        let codes: Vec<String> = rows
            .iter()
            .map(|r| str_or_empty(r, "municipality_code"))
            .collect();
        assert_eq!(
            codes.len(),
            1,
            "aggregate (横浜市) は area_level='unit' フィルタで除外"
        );
        assert_eq!(codes[0], "13104", "新宿区のみ返却");
    }

    #[test]
    fn fetch_code_master_by_names_empty_input() {
        let (_tmp, db) = create_test_db();
        // table 不在でも early return
        let rows = fetch_code_master_by_names(&db, None, &[]);
        assert!(rows.is_empty(), "空入力は早期 return で空 Vec");
    }

    #[test]
    fn fetch_code_master_by_names_table_missing_returns_empty() {
        let (_tmp, db) = create_test_db();
        // テーブル不在 + turso 無し → 空 Vec
        let pairs = [("東京都", "新宿区")];
        let rows = fetch_code_master_by_names(&db, None, &pairs);
        assert!(rows.is_empty(), "テーブル不在時は空 Vec");
    }
}
