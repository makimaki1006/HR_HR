//! 公開ハンドラー

use axum::extract::{Query, State};
use axum::response::Html;
use serde::Deserialize;
use std::sync::Arc;
use tower_sessions::Session;

use crate::AppState;
use super::super::overview::get_session_filters;
use super::upload::parse_csv_bytes;
use super::aggregator::aggregate_records;
use super::job_seeker::analyze_job_seeker;
use super::render::{render_upload_form, render_analysis_result};

/// 競合調査タブ（初期表示: アップロードフォーム）
pub async fn tab_survey(
    State(_state): State<Arc<AppState>>,
    _session: Session,
) -> Html<String> {
    Html(render_upload_form())
}

/// CSVアップロード（multipart/form-data）
pub async fn upload_csv(
    State(state): State<Arc<AppState>>,
    session: Session,
    mut multipart: axum_extra::extract::Multipart,
) -> Html<String> {
    // ファイルデータ読み取り
    let mut csv_data: Option<Vec<u8>> = None;
    let mut filename = String::from("unknown.csv");

    while let Ok(Some(field)) = multipart.next_field().await {
        if field.name() == Some("csv_file") {
            filename = field.file_name().unwrap_or("upload.csv").to_string();
            match field.bytes().await {
                Ok(bytes) => csv_data = Some(bytes.to_vec()),
                Err(e) => {
                    return Html(format!(
                        r#"<div class="stat-card"><p class="text-red-400 text-sm">ファイル読み取りエラー: {}</p></div>"#,
                        super::super::helpers::escape_html(&e.to_string())
                    ));
                }
            }
        }
    }

    let data = match csv_data {
        Some(d) if !d.is_empty() => d,
        _ => {
            return Html(r#"<div class="stat-card"><p class="text-red-400 text-sm">CSVファイルが選択されていません</p></div>"#.to_string());
        }
    };

    // コンテキスト都道府県（セッションから取得）
    let filters = get_session_filters(&session).await;
    let context_pref = if filters.prefecture.is_empty() { None } else { Some(filters.prefecture.as_str()) };

    // CSVパース（CPU重い処理をspawn_blocking）
    let data_clone = data.clone();
    let ctx_pref = context_pref.map(|s| s.to_string());
    let result = tokio::task::spawn_blocking(move || {
        parse_csv_bytes(&data_clone, ctx_pref.as_deref())
    }).await;

    let records = match result {
        Ok(Ok(records)) => records,
        Ok(Err(e)) => {
            return Html(format!(
                r#"<div class="stat-card"><p class="text-red-400 text-sm">CSVパースエラー: {}</p></div>"#,
                super::super::helpers::escape_html(&e)
            ));
        }
        Err(e) => {
            return Html(format!(
                r#"<div class="stat-card"><p class="text-red-400 text-sm">処理エラー: {}</p></div>"#,
                super::super::helpers::escape_html(&e.to_string())
            ));
        }
    };

    // 集計 + 求職者分析
    let agg = aggregate_records(&records);
    let seeker = analyze_job_seeker(&records);

    // セッションID生成（UUID v4: 予測不可能）
    let session_id = format!("s_{}", uuid::Uuid::new_v4());

    // 集計結果をキャッシュに保存（統合レポートで再利用）
    let agg_json = serde_json::to_value(&agg).unwrap_or_default();
    let seeker_json = serde_json::to_value(&seeker).unwrap_or_default();
    state.cache.set(format!("survey_agg_{}", session_id), agg_json);
    state.cache.set(format!("survey_seeker_{}", session_id), seeker_json);

    // 主要地域もキャッシュ
    if let Some(pref) = &agg.dominant_prefecture {
        state.cache.set(
            format!("survey_pref_{}", session_id),
            serde_json::Value::String(pref.clone()),
        );
        if let Some(muni) = &agg.dominant_municipality {
            state.cache.set(
                format!("survey_muni_{}", session_id),
                serde_json::Value::String(muni.clone()),
            );
        }
    }

    tracing::info!(
        "Survey CSV uploaded: {} records from {}, dominant region: {:?}",
        records.len(), filename, agg.dominant_prefecture
    );

    Html(render_analysis_result(&agg, &seeker, &session_id))
}

#[derive(Deserialize)]
pub struct IntegrateQuery {
    pub session_id: Option<String>,
}

/// 統合レポート生成
pub async fn integrate_report(
    State(state): State<Arc<AppState>>,
    _session: Session,
    Query(query): Query<IntegrateQuery>,
) -> Html<String> {
    let session_id = match &query.session_id {
        Some(id) if !id.is_empty() => id.clone(),
        _ => return Html(r#"<p class="text-red-400 text-sm">セッションIDが必要です</p>"#.to_string()),
    };

    // キャッシュからCSV分析結果を取得
    let pref_cached = state.cache.get(&format!("survey_pref_{}", session_id));

    let pref = pref_cached
        .and_then(|v| v.as_str().map(|s| s.to_string()))
        .unwrap_or_default();

    if pref.is_empty() {
        return Html(r#"<div class="stat-card"><p class="text-amber-400 text-sm">地域が特定できませんでした。CSVに住所データが含まれていることを確認してください。</p></div>"#.to_string());
    }

    let muni = state.cache.get(&format!("survey_muni_{}", session_id))
        .and_then(|v| v.as_str().map(|s| s.to_string()))
        .unwrap_or_default();

    // HWデータ＋外部統計を取得（insightエンジンと同じフロー）
    let db = match &state.hw_db {
        Some(db) => db.clone(),
        None => return Html(r#"<p class="text-slate-400 text-sm">HWデータベース未接続のため統合分析は利用できません</p>"#.to_string()),
    };

    let turso = state.turso_db.clone();
    let salesnow = state.salesnow_db.clone();
    let pref2 = pref.clone();
    let muni2 = muni.clone();

    let content = tokio::task::spawn_blocking(move || {
        use super::super::insight::fetch::build_insight_context;
        use super::super::insight::engine::generate_insights;
        use super::super::company::fetch::fetch_companies_by_region;

        let ctx = build_insight_context(&db, turso.as_ref(), &pref2, &muni2);
        let insights = generate_insights(&ctx);

        // SalesNow企業データ取得（該当地域）
        let companies = if let Some(ref sn_db) = salesnow {
            // 業種フィルタは空（全業種）で地域の企業を取得
            fetch_companies_by_region(sn_db, &db, &pref2, &muni2, &[], 30)
        } else {
            vec![]
        };

        // 統合レポートHTML生成
        super::integration::render_integration(
            &pref2, &muni2, &insights, &ctx, &companies
        )
    }).await.unwrap_or_else(|e| {
        tracing::error!("Integration report failed: {e}");
        r#"<p class="text-red-400 text-sm">統合レポート生成に失敗しました</p>"#.to_string()
    });

    Html(content)
}

/// 分析実行（予備エンドポイント）
pub async fn analyze_survey(
    State(_state): State<Arc<AppState>>,
    _session: Session,
) -> Html<String> {
    Html(r#"<p class="text-slate-400 text-sm">CSVをアップロードしてください</p>"#.to_string())
}

/// レポートJSON API
pub async fn report_json(
    State(_state): State<Arc<AppState>>,
    _session: Session,
) -> axum::response::Json<serde_json::Value> {
    axum::response::Json(serde_json::json!({"status": "upload_csv_first"}))
}
