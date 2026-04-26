//! Panel 4: 競合企業ランキング
//!
//! SalesNow (v2_salesnow_companies) から該当業種 × エリアの企業を
//! 従業員数・売上高でランキング (上位20社)。
//! 各社の HW 求人数を postings から付与する。
//!
//! 注意事項:
//! - 従業員数/売上高は SalesNow 時点での静的値。相関分析に留め因果は主張しない。
//! - HW 求人数は facility_name の LIKE マッチで簡易推定。正確な紐付けは未実装。
//!
//! データ範囲制約 (feedback_hw_data_scope): 企業リスト自体は SalesNow 由来で
//! 全国網羅的だが、HW 求人数は HW 掲載分のみ。未掲載企業は 0 になる。

use crate::db::local_sqlite::LocalDb;
use crate::db::turso_http::TursoDb;
use crate::handlers::company::fetch::count_hw_postings;
use crate::handlers::helpers::{get_f64, get_i64, get_str};
use crate::models::job_seeker::PREFECTURE_ORDER;
use crate::AppState;
use axum::extract::{Query, State};
use axum::Json;
use serde::Deserialize;
use serde_json::{json, Value};
use std::sync::Arc;

#[derive(Deserialize)]
pub struct CompetitorsQuery {
    /// HW 側職種名 (例: "医療")。未指定なら業種フィルタ無し。
    #[serde(default)]
    pub job_type: String,
    /// 都道府県コード (1-47)
    pub prefcode: Option<i32>,
    /// 市区町村名 (任意。postings.municipality に一致)
    #[serde(default)]
    pub municipality: String,
    /// 取得上限 (default 20)
    #[serde(default)]
    pub limit: Option<i64>,
}

/// 競合企業 1 社のレスポンス DTO
#[derive(serde::Serialize, Clone)]
pub struct CompetitorRow {
    pub corporate_number: String,
    pub name: String,
    pub prefecture: String,
    pub sn_industry: String,
    pub employees: i64,
    pub sales_amount: i64,
    pub sales_range: String,
    pub credit_score: f64,
    pub hw_postings_count: i64,
}

/// GET /api/recruitment_diag/competitors
pub async fn competitors(
    State(state): State<Arc<AppState>>,
    Query(q): Query<CompetitorsQuery>,
) -> Json<Value> {
    let prefecture = prefcode_to_name(q.prefcode).unwrap_or_default();
    let limit = q.limit.unwrap_or(20).clamp(1, 100);

    let sn_db = match &state.salesnow_db {
        Some(db) => db.clone(),
        None => {
            return Json(error_response("SalesNow DB 未接続"));
        }
    };

    let hw_db = state.hw_db.clone();
    let job_type = q.job_type.clone();
    let muni = q.municipality.clone();
    let pref_snap = prefecture.clone();

    // industry_mapping の confidence サマリ（D-2 監査 Q2.5 対応）
    let sn_db_clone = sn_db.clone();
    let job_type_for_mapping = q.job_type.clone();
    let (mapping_top_confidence, mapping_warning) = tokio::task::spawn_blocking(move || {
        build_mapping_confidence_warning(&sn_db_clone, &job_type_for_mapping)
    })
    .await
    .unwrap_or((None, None));

    let result = tokio::task::spawn_blocking(move || {
        build_competitors(&sn_db, hw_db.as_ref(), &job_type, &pref_snap, &muni, limit)
    })
    .await
    .unwrap_or_else(|_| Vec::new());

    let top20_insight = build_top20_insight(&result, &prefecture, &q.job_type);

    Json(json!({
        "prefecture": prefecture,
        "municipality": q.municipality,
        "job_type": q.job_type,
        "companies": result,
        "top20_insight": top20_insight,
        "warning": hw_data_scope_warning(),
        "mapping_confidence": mapping_top_confidence,
        "mapping_warning": mapping_warning,
    }))
}

/// SalesNow → HW 求人数付与までを同期実行
pub(crate) fn build_competitors(
    sn_db: &TursoDb,
    hw_db: Option<&LocalDb>,
    job_type: &str,
    prefecture: &str,
    municipality: &str,
    limit: i64,
) -> Vec<CompetitorRow> {
    // 1) job_type → sn_industry 候補を取得
    let sn_industries = fetch_sn_industries_for_job_type(sn_db, job_type);

    // 2) SalesNow から企業リスト取得
    let rows = fetch_salesnow_companies(sn_db, &sn_industries, prefecture, municipality, limit);

    // 3) DTO へ変換
    let mut companies: Vec<CompetitorRow> = rows
        .iter()
        .map(|r| CompetitorRow {
            corporate_number: get_str(r, "corporate_number"),
            name: get_str(r, "company_name"),
            prefecture: get_str(r, "prefecture"),
            sn_industry: get_str(r, "sn_industry"),
            employees: get_i64(r, "employee_count"),
            sales_amount: get_i64(r, "sales_amount"),
            sales_range: get_str(r, "sales_range"),
            credit_score: get_f64(r, "credit_score"),
            hw_postings_count: 0,
        })
        .collect();

    // 4) HW 求人数付与 (LocalDb があれば)
    if let Some(db) = hw_db {
        for c in companies.iter_mut() {
            c.hw_postings_count = count_hw_postings(db, &c.name, &c.prefecture);
        }
    }

    companies
}

/// industry_mapping の信頼度しきい値。これ未満は「精度低」とみなし UI で注記する。
///
/// D-2 監査 Q2.5 対応 / feedback_test_data_validation.md 準拠:
///   v2_industry_mapping の confidence 列は SN industry ⇄ HW job_type の
///   マッピング信頼度を 0.0-1.0 で持つ。0.7 未満のマッピングは
///   「企業の実態（病院/介護法人など）に依存して誤マッチが起き得る」ため、
///   UI で「※ マッピング精度低」注記を出す。
pub(crate) const INDUSTRY_MAPPING_CONFIDENCE_THRESHOLD: f64 = 0.7;

/// HW 職種 → SalesNow sn_industry マッピングの 1 エントリ
#[derive(Debug, Clone)]
pub(crate) struct IndustryMappingEntry {
    pub sn_industry: String,
    pub confidence: f64,
}

impl IndustryMappingEntry {
    /// confidence が信頼度しきい値以上か
    pub fn is_high_confidence(&self) -> bool {
        self.confidence >= INDUSTRY_MAPPING_CONFIDENCE_THRESHOLD
    }
}

/// HW 職種 → SalesNow sn_industry マッピング取得（confidence 付き）
fn fetch_industry_mapping_entries(sn_db: &TursoDb, job_type: &str) -> Vec<IndustryMappingEntry> {
    if job_type.is_empty() {
        return vec![];
    }
    let sql = "SELECT sn_industry, confidence \
               FROM v2_industry_mapping \
               WHERE hw_job_type = ?1 \
               ORDER BY confidence DESC";
    let params: Vec<&dyn crate::db::turso_http::ToSqlTurso> = vec![&job_type];
    match sn_db.query(sql, &params) {
        Ok(rows) => rows
            .iter()
            .map(|r| IndustryMappingEntry {
                sn_industry: get_str(r, "sn_industry"),
                confidence: get_f64(r, "confidence"),
            })
            .filter(|e| !e.sn_industry.is_empty())
            .collect(),
        Err(_) => vec![],
    }
}

/// HW 職種 → SalesNow sn_industry マッピング取得（後方互換: 名前のみ）
fn fetch_sn_industries_for_job_type(sn_db: &TursoDb, job_type: &str) -> Vec<String> {
    fetch_industry_mapping_entries(sn_db, job_type)
        .into_iter()
        .map(|e| e.sn_industry)
        .collect()
}

/// 上位マッピングの信頼度サマリ（UI 注記用）
///
/// 戻り値: (top_confidence, low_confidence_warning)
///   - top_confidence: トップマッピングの confidence。マッピングなしなら None。
///   - low_confidence_warning: top_confidence < 0.7 のとき注記文を返す。
pub(crate) fn build_mapping_confidence_warning(
    sn_db: &TursoDb,
    job_type: &str,
) -> (Option<f64>, Option<String>) {
    let entries = fetch_industry_mapping_entries(sn_db, job_type);
    if entries.is_empty() {
        return (
            None,
            Some(format!(
                "⚠️ マッピング失敗: 職種「{}」に対応する SalesNow 業種が登録されていません。\
                 unknown バケットとして集計します。",
                job_type
            )),
        );
    }
    let top = &entries[0];
    if !top.is_high_confidence() {
        return (
            Some(top.confidence),
            Some(format!(
                "※ マッピング精度低: 職種「{}」→ 業種「{}」の信頼度は {:.2}（しきい値 {:.2} 未満）。\
                 企業の実態によっては別業種が正解の可能性があります。傾向値として参照してください。",
                job_type,
                top.sn_industry,
                top.confidence,
                INDUSTRY_MAPPING_CONFIDENCE_THRESHOLD
            )),
        );
    }
    (Some(top.confidence), None)
}

/// SalesNow 企業取得 (業種 × 都道府県 × 任意で市区町村)
fn fetch_salesnow_companies(
    sn_db: &TursoDb,
    sn_industries: &[String],
    prefecture: &str,
    municipality: &str,
    limit: i64,
) -> Vec<crate::handlers::helpers::Row> {
    let base_cols = "corporate_number, company_name, prefecture, sn_industry, \
                     employee_count, sales_amount, sales_range, credit_score";

    // sn_industries を IN 句にせず OR で展開 (Turso は IN のバインドに制約あり)
    // 件数が多いと遅くなるので上位 5 件のみ使う
    let top_inds: Vec<&String> = sn_industries.iter().take(5).collect();

    // 動的 SQL 構築: 業種フィルタは placeholder
    let mut where_clauses: Vec<String> = Vec::new();
    let mut params_own: Vec<String> = Vec::new();
    let mut idx: usize = 1;

    if !prefecture.is_empty() {
        where_clauses.push(format!("prefecture = ?{}", idx));
        params_own.push(prefecture.to_string());
        idx += 1;
    }

    if !municipality.is_empty() {
        where_clauses.push(format!("address LIKE ?{}", idx));
        params_own.push(format!("%{}%", municipality));
        idx += 1;
    }

    if !top_inds.is_empty() {
        let placeholders: Vec<String> = top_inds
            .iter()
            .enumerate()
            .map(|(i, _)| format!("?{}", idx + i))
            .collect();
        where_clauses.push(format!("sn_industry IN ({})", placeholders.join(",")));
        for s in &top_inds {
            params_own.push((*s).clone());
        }
        idx += top_inds.len();
    }

    where_clauses.push("employee_count > 0".to_string());

    let where_sql = if where_clauses.is_empty() {
        "1=1".to_string()
    } else {
        where_clauses.join(" AND ")
    };

    let sql = format!(
        "SELECT {cols} FROM v2_salesnow_companies \
         WHERE {where_sql} \
         ORDER BY employee_count DESC, sales_amount DESC \
         LIMIT ?{idx}",
        cols = base_cols,
        where_sql = where_sql,
        idx = idx
    );
    params_own.push(limit.to_string());

    // ToSqlTurso 配列へ変換 (String は &str 経由)
    let params: Vec<&dyn crate::db::turso_http::ToSqlTurso> = params_own
        .iter()
        .map(|s| s as &dyn crate::db::turso_http::ToSqlTurso)
        .collect();

    sn_db.query(&sql, &params).unwrap_or_default()
}

/// 上位 20 社の解釈テキスト生成 (feedback_hypothesis_driven)
fn build_top20_insight(rows: &[CompetitorRow], prefecture: &str, job_type: &str) -> String {
    if rows.is_empty() {
        return format!(
            "{}の{}業界は SalesNow 登録企業が少なく、競合ランキングを生成できませんでした。",
            if prefecture.is_empty() {
                "全国"
            } else {
                prefecture
            },
            if job_type.is_empty() {
                "該当"
            } else {
                job_type
            }
        );
    }

    let total_emp: i64 = rows.iter().map(|r| r.employees).sum();
    let hw_active = rows.iter().filter(|r| r.hw_postings_count > 0).count();
    let total_postings: i64 = rows.iter().map(|r| r.hw_postings_count).sum();

    let top_name = rows.first().map(|r| r.name.as_str()).unwrap_or("");
    let top_emp = rows.first().map(|r| r.employees).unwrap_or(0);

    let hw_share_pct = if !rows.is_empty() {
        hw_active as f64 / rows.len() as f64 * 100.0
    } else {
        0.0
    };

    format!(
        "上位{n}社合計従業員数 {total_emp}人。首位は{top_name}（{top_emp}人）。\
        HW に求人掲載中は{hw_active}社 ({hw_share:.0}%)、合計 {total_postings}件の求人。\
        掲載率が低い場合は HW 未使用企業へのアプローチ機会がある傾向。\
        ※HW 求人数は facility_name 部分一致による推定値で正確な紐付けではない。",
        n = rows.len(),
        total_emp = total_emp,
        top_name = top_name,
        top_emp = top_emp,
        hw_active = hw_active,
        hw_share = hw_share_pct,
        total_postings = total_postings,
    )
}

/// 都道府県コード (1-47) → 名称
pub(crate) fn prefcode_to_name(code: Option<i32>) -> Option<String> {
    let c = code?;
    if !(1..=47).contains(&c) {
        return None;
    }
    Some(PREFECTURE_ORDER[(c - 1) as usize].to_string())
}

/// HW データ範囲制約の注意書き (feedback_hw_data_scope 準拠)
pub(crate) fn hw_data_scope_warning() -> String {
    "⚠️ HW求人データの限界:\n\
     - HW掲載求人のみを対象。全求人市場ではない。\n\
     - HW求人は市場実勢より給与を低めに設定する慣習あり。\n\
     - 外部統計 (賃金構造基本統計調査など) との直接比較は注意。\n\
     - 相関は示せても因果は証明できない。傾向として解釈すること。"
        .to_string()
}

fn error_response(msg: &str) -> Value {
    json!({ "error": msg, "companies": [], "top20_insight": "" })
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn prefcode_tokyo() {
        assert_eq!(prefcode_to_name(Some(13)).as_deref(), Some("東京都"));
    }

    #[test]
    fn prefcode_hokkaido_edge() {
        assert_eq!(prefcode_to_name(Some(1)).as_deref(), Some("北海道"));
    }

    #[test]
    fn prefcode_okinawa_edge() {
        assert_eq!(prefcode_to_name(Some(47)).as_deref(), Some("沖縄県"));
    }

    #[test]
    fn prefcode_invalid_zero() {
        assert!(prefcode_to_name(Some(0)).is_none());
    }

    #[test]
    fn prefcode_invalid_48() {
        assert!(prefcode_to_name(Some(48)).is_none());
    }

    #[test]
    fn prefcode_none() {
        assert!(prefcode_to_name(None).is_none());
    }

    #[test]
    fn warning_contains_required_text() {
        let w = hw_data_scope_warning();
        assert!(w.contains("HW掲載求人のみ"));
        assert!(w.contains("因果は証明できない"));
    }

    #[test]
    fn insight_empty_rows() {
        let msg = build_top20_insight(&[], "東京都", "医療");
        assert!(msg.contains("SalesNow 登録企業"));
        assert!(msg.contains("東京都"));
    }

    // ========================================================
    // Fix-B (D-2 監査 Q2.5): industry_mapping 信頼度ロジックテスト
    // feedback_test_data_validation.md 準拠
    // ========================================================

    #[test]
    fn fixb_mapping_confidence_threshold_is_07() {
        assert_eq!(
            INDUSTRY_MAPPING_CONFIDENCE_THRESHOLD, 0.7,
            "信頼度しきい値は 0.7 で固定 (D-2 Q2.5)"
        );
    }

    #[test]
    fn fixb_high_confidence_entry_passes() {
        let e = IndustryMappingEntry {
            sn_industry: "病院".into(),
            confidence: 0.85,
        };
        assert!(e.is_high_confidence(), "0.85 は高信頼度として通る");
    }

    #[test]
    fn fixb_low_confidence_entry_flagged() {
        let e = IndustryMappingEntry {
            sn_industry: "介護".into(),
            confidence: 0.65,
        };
        assert!(!e.is_high_confidence(), "0.65 は低信頼度として UI 注記対象");
    }

    #[test]
    fn fixb_threshold_boundary_inclusive() {
        // しきい値ちょうど 0.7 は通る
        let e = IndustryMappingEntry {
            sn_industry: "医療".into(),
            confidence: 0.7,
        };
        assert!(e.is_high_confidence(), "境界値 0.7 は高信頼度扱い");
    }

    #[test]
    fn insight_with_data() {
        let rows = vec![
            CompetitorRow {
                corporate_number: "1".into(),
                name: "Company A".into(),
                prefecture: "東京都".into(),
                sn_industry: "病院".into(),
                employees: 1000,
                sales_amount: 0,
                sales_range: String::new(),
                credit_score: 0.0,
                hw_postings_count: 5,
            },
            CompetitorRow {
                corporate_number: "2".into(),
                name: "Company B".into(),
                prefecture: "東京都".into(),
                sn_industry: "病院".into(),
                employees: 500,
                sales_amount: 0,
                sales_range: String::new(),
                credit_score: 0.0,
                hw_postings_count: 0,
            },
        ];
        let msg = build_top20_insight(&rows, "東京都", "医療");
        // 逆証明: 合計従業員数 = 1500, 首位 = Company A (1000人), HW掲載 = 1/2 = 50%, 合計求人 = 5
        assert!(msg.contains("1500"));
        assert!(msg.contains("Company A"));
        assert!(msg.contains("1000"));
        assert!(msg.contains("50%"));
        assert!(msg.contains("5件"));
    }
}
