//! 公開ハンドラー

use axum::extract::{Query, State};
use axum::response::Html;
use serde::Deserialize;
use std::sync::Arc;
use tower_sessions::Session;

use super::super::overview::get_session_filters;
use super::aggregator::{aggregate_records, aggregate_records_with_mode};
use super::job_seeker::analyze_job_seeker;
use super::render::{render_analysis_result, render_upload_form};
use super::upload::{parse_csv_bytes, parse_csv_bytes_with_hints, UserSourceHint, WageMode};
use crate::AppState;

/// 媒体分析タブ（初期表示: アップロードフォーム）
pub async fn tab_survey(State(_state): State<Arc<AppState>>, _session: Session) -> Html<String> {
    Html(render_upload_form())
}

/// multipartエラーがボディサイズ超過に起因するかを判定
fn is_body_size_error(msg: &str) -> bool {
    let lower = msg.to_lowercase();
    // axum/axum_extraがbody limit超過時に返すメッセージのバリエーションに対応
    lower.contains("length limit")
        || lower.contains("payload too large")
        || lower.contains("body limit")
        || lower.contains("request body")  // "failed to read request body..."
        || lower.contains("too large")
        // DefaultBodyLimit により multipart 入力途中で接続が打ち切られた場合、
        // axum_extra は "Error parsing `multipart/form-data` request" を返す。
        // これも body limit 超過の強いシグナルとして扱う。
        || (lower.contains("error parsing") && lower.contains("multipart/form-data"))
}

/// ボディサイズ超過時のユーザー向け日本語メッセージ
fn body_size_error_html() -> String {
    format!(
        r#"<div class="stat-card"><p class="text-red-400 text-sm">アップロード可能なファイルサイズ({}MB)を超えています。CSVを分割するか、列数を減らしてから再度お試しください。</p></div>"#,
        crate::UPLOAD_BODY_LIMIT_BYTES / (1024 * 1024)
    )
}

/// CSVアップロード（multipart/form-data）
pub async fn upload_csv(
    State(state): State<Arc<AppState>>,
    session: Session,
    mut multipart: axum_extra::extract::Multipart,
) -> Html<String> {
    // 監査: CSV アップロード (バイト数は後で判明するので最初に記録)
    crate::audit::record_event(
        &state.audit,
        &session,
        "upload_survey_csv",
        "upload",
        "start",
        "",
    )
    .await;
    // ファイルデータ読み取り + ユーザー明示指定
    let mut csv_data: Option<Vec<u8>> = None;
    let mut filename = String::from("unknown.csv");
    let mut source_type = String::from("auto"); // "indeed" | "jobbox" | "other" | "auto"
    let mut wage_mode = String::from("auto"); // "monthly" | "hourly" | "auto"

    loop {
        match multipart.next_field().await {
            Ok(Some(field)) => {
                let field_name = field.name().unwrap_or("").to_string();
                if field_name == "csv_file" {
                    filename = field.file_name().unwrap_or("upload.csv").to_string();
                    match field.bytes().await {
                        Ok(bytes) => csv_data = Some(bytes.to_vec()),
                        Err(e) => {
                            let msg = e.to_string();
                            if is_body_size_error(&msg) {
                                tracing::warn!("Upload rejected (size exceeded): {}", msg);
                                return Html(body_size_error_html());
                            }
                            return Html(format!(
                                r#"<div class="stat-card"><p class="text-red-400 text-sm">ファイル読み取りエラー: {}</p></div>"#,
                                super::super::helpers::escape_html(&msg)
                            ));
                        }
                    }
                } else if field_name == "source_type" {
                    if let Ok(s) = field.text().await {
                        let t = s.trim().to_lowercase();
                        if matches!(t.as_str(), "indeed" | "jobbox" | "other" | "auto") {
                            source_type = t;
                        }
                    }
                } else if field_name == "wage_mode" {
                    if let Ok(s) = field.text().await {
                        let t = s.trim().to_lowercase();
                        if matches!(t.as_str(), "monthly" | "hourly" | "auto") {
                            wage_mode = t;
                        }
                    }
                }
            }
            Ok(None) => break,
            Err(e) => {
                // next_field() が body size 超過で失敗するケース
                let msg = e.to_string();
                if is_body_size_error(&msg) {
                    tracing::warn!("Upload rejected (size exceeded at next_field): {}", msg);
                    return Html(body_size_error_html());
                }
                return Html(format!(
                    r#"<div class="stat-card"><p class="text-red-400 text-sm">アップロード解析エラー: {}</p></div>"#,
                    super::super::helpers::escape_html(&msg)
                ));
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
    let context_pref = if filters.prefecture.is_empty() {
        None
    } else {
        Some(filters.prefecture.as_str())
    };

    // ユーザー明示指定
    let source_hint = UserSourceHint::from_str(&source_type);
    let wage_mode_enum = WageMode::from_str(&wage_mode);

    // CSVパース（CPU重い処理をspawn_blocking）
    let data_clone = data.clone();
    let ctx_pref = context_pref.map(|s| s.to_string());
    let result = tokio::task::spawn_blocking(move || {
        parse_csv_bytes_with_hints(&data_clone, ctx_pref.as_deref(), source_hint)
    })
    .await;
    let _ = parse_csv_bytes; // silence unused-import (kept for backward API compat)

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
    let agg = aggregate_records_with_mode(&records, wage_mode_enum);
    let _ = aggregate_records; // silence unused-import (backward API compat)
    let seeker = analyze_job_seeker(&records);

    // セッションID生成（UUID v4: 予測不可能）
    let session_id = format!("s_{}", uuid::Uuid::new_v4());

    // 集計結果をキャッシュに保存（統合レポートで再利用）
    let agg_json = serde_json::to_value(&agg).unwrap_or_default();
    let seeker_json = serde_json::to_value(&seeker).unwrap_or_default();
    state
        .cache
        .set(format!("survey_agg_{}", session_id), agg_json);
    state
        .cache
        .set(format!("survey_seeker_{}", session_id), seeker_json);

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
        records.len(),
        filename,
        agg.dominant_prefecture
    );

    Html(render_analysis_result(&agg, &seeker, &session_id))
}

#[derive(Deserialize)]
pub struct IntegrateQuery {
    pub session_id: Option<String>,
    /// レポートバリアント切替 (2026-04-29 追加)
    /// - `full` (デフォルト): HW データ併載 (既存仕様)
    /// - `public`: HW 最小化 + 公開オープンデータ + 地域比較強化
    pub variant: Option<String>,
    /// 業界絞込フィルタ (2026-04-29 追加)
    /// HW industry_raw (詳細分類) または HW 大分類名を受ける。
    /// SalesNow / e-Stat 業界別データを当該業界に絞り込む。
    /// 未指定 (None) または空文字列 → 絞り込まない (異業種ベンチマーク用途)
    /// 指定時 → **同業界 + 全業界 の両方を併記**して提示する
    /// 内部で `map_hw_to_major_industry` により 12 大分類に正規化してから利用。
    pub industry: Option<String>,
    /// グローバルフィルタの都道府県上書き (2026-04-29 追加)
    /// 指定時はキャッシュの dominant_prefecture より優先
    pub pref: Option<String>,
    /// グローバルフィルタの市区町村上書き (2026-04-29 追加)
    pub muni: Option<String>,
    /// レポートデザインテーマ切替 (2026-05-01 追加)
    /// - 未指定 / `default`: 既存スタイル
    /// - `v8`: Statistical Working Paper 風
    /// - `v7a`: Editorial 風
    /// 同じ CSV 分析結果を異なるデザインで出力するため、現場で見た目を比較できる。
    pub theme: Option<String>,
}

/// 統合レポート生成
pub async fn integrate_report(
    State(state): State<Arc<AppState>>,
    _session: Session,
    Query(query): Query<IntegrateQuery>,
) -> Html<String> {
    let session_id = match &query.session_id {
        Some(id) if !id.is_empty() => id.clone(),
        _ => {
            return Html(
                r#"<p class="text-red-400 text-sm">セッションIDが必要です</p>"#.to_string(),
            )
        }
    };

    // キャッシュからCSV分析結果を取得
    let pref_cached = state.cache.get(&format!("survey_pref_{}", session_id));

    let pref = pref_cached
        .and_then(|v| v.as_str().map(|s| s.to_string()))
        .unwrap_or_default();

    if pref.is_empty() {
        return Html(r#"<div class="stat-card"><p class="text-amber-400 text-sm">地域が特定できませんでした。CSVに住所データが含まれていることを確認してください。</p></div>"#.to_string());
    }

    let muni = state
        .cache
        .get(&format!("survey_muni_{}", session_id))
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

    // CSV集計結果をキャッシュから復元（地域×HWデータ連携用の pref/muni ペア取得）
    let agg_cached = state.cache.get(&format!("survey_agg_{}", session_id));
    let agg_parsed: Option<super::aggregator::SurveyAggregation> = agg_cached
        .and_then(|v| serde_json::from_value::<super::aggregator::SurveyAggregation>(v).ok());
    let pref_muni_pairs: Vec<(String, String)> = agg_parsed
        .as_ref()
        .map(|a| {
            a.by_municipality_salary
                .iter()
                .map(|m| (m.prefecture.clone(), m.name.clone()))
                .collect()
        })
        .unwrap_or_default();
    // Impl-1 #6: CSV 上位 3 都道府県（件数降順）
    let top3_prefs: Vec<String> = agg_parsed
        .as_ref()
        .map(|a| {
            a.by_prefecture
                .iter()
                .filter(|(p, _)| !p.is_empty())
                .take(3)
                .map(|(p, _)| p.clone())
                .collect()
        })
        .unwrap_or_default();

    // 2026-04-26 (Granularity): CSV 件数 上位 30 市区町村（ヒートマップ用）+ 上位 3（レーダー用）
    // 媒体分析タブの主役は「都道府県」ではなく CSV に登場する市区町村。
    let top_munis_30 = agg_parsed
        .as_ref()
        .map(|a| super::granularity::top_municipalities(a, 30))
        .unwrap_or_default();
    let top_munis_3 = agg_parsed
        .as_ref()
        .map(|a| super::granularity::top_municipalities(a, 3))
        .unwrap_or_default();

    let content = tokio::task::spawn_blocking(move || {
        use super::super::company::fetch::fetch_companies_by_region;
        use super::super::insight::engine::generate_insights;
        use super::super::insight::fetch::build_insight_context;
        use super::hw_enrichment::enrich_areas;

        let ctx = build_insight_context(&db, turso.as_ref(), &pref2, &muni2);
        let insights = generate_insights(&ctx);

        // 地域注目企業データ取得（該当地域）
        let companies = if let Some(ref sn_db) = salesnow {
            // 業種フィルタは空（全業種）で地域の企業を取得
            fetch_companies_by_region(sn_db, &db, &pref2, &muni2, 50)
        } else {
            vec![]
        };

        // CSV内の pref/muni ペアごとに HW DB を突合
        let enrichment_map = enrich_areas(&db, turso.as_ref(), &pref_muni_pairs);
        let mut hw_enrichments: Vec<_> = enrichment_map.into_values().collect();
        hw_enrichments.sort_by(|a, b| b.hw_posting_count.cmp(&a.hw_posting_count));

        // Impl-1 + Granularity (2026-04-26): 媒体分析データ活用 #6 / D-3 + 市区町村粒度
        let ext = build_survey_extension_data(
            &db,
            turso.as_ref(),
            &pref2,
            &top3_prefs,
            &top_munis_3,
            &top_munis_30,
        );

        // 統合レポートHTML生成
        super::integration::render_integration_with_ext(
            &pref2,
            &muni2,
            &insights,
            &ctx,
            &companies,
            &hw_enrichments,
            &ext,
        )
    })
    .await
    .unwrap_or_else(|e| {
        tracing::error!("Integration report failed: {e}");
        r#"<p class="text-red-400 text-sm">統合レポート生成に失敗しました</p>"#.to_string()
    });

    Html(content)
}

/// Impl-1 + Granularity (2026-04-26) 用の拡張データを取得
///
/// - `top3_prefs`: CSV 件数上位 3 都道府県（最大 3 件）。空なら都道府県レーダー非表示。
/// - `top_munis_3`: CSV 件数上位 3 市区町村 (主役レーダー)。空なら市区町村レーダー非表示。
/// - `top_munis_30`: CSV 件数上位 30 市区町村 (ヒートマップ)。
/// - dominant `pref` から industry_structure Top10 を取得。
fn build_survey_extension_data(
    db: &crate::db::local_sqlite::LocalDb,
    _turso: Option<&crate::db::turso_http::TursoDb>,
    pref: &str,
    top3_prefs: &[String],
    top_munis_3: &[(String, String, usize)],
    top_munis_30: &[(String, String, usize)],
) -> super::integration::SurveyExtensionData {
    use super::super::analysis::fetch as af;
    use super::super::helpers::get_i64;

    // #6: 主要 3 都道府県の region_benchmark (後方互換維持)
    let top3_region_benchmark = if top3_prefs.is_empty() {
        Vec::new()
    } else {
        af::fetch_region_benchmarks_for_prefs(db, top3_prefs)
    };

    // 2026-04-26 Granularity: 主要 3 市区町村の region_benchmark (主役)
    let top3_municipality_benchmark = if top_munis_3.is_empty() {
        Vec::new()
    } else {
        super::granularity::fetch_region_benchmarks_for_municipalities(db, top_munis_3)
    };

    // D-3: dominant pref の産業別就業者構成 Top10
    // prefecture_code が必要なので geo::pref_name_to_code でマップ
    let industry_structure_top10 = if pref.is_empty() {
        Vec::new()
    } else {
        let code_map = crate::geo::pref_name_to_code();
        let pref_code = code_map.get(pref).copied().unwrap_or("");
        if pref_code.is_empty() {
            Vec::new()
        } else {
            // turso 渡しは Option パラメタ仕様。本来は state.turso.as_ref() を渡すべきだが
            // ローカル DB フォールバックでも動作する設計のため None でも可。
            let rows = af::fetch_industry_structure(db, _turso, pref_code);
            rows.into_iter()
                .filter(|r| get_i64(r, "employees_total") > 0)
                .take(10)
                .collect()
        }
    };

    super::integration::SurveyExtensionData {
        top3_region_benchmark,
        industry_structure_top10,
        top3_municipality_benchmark,
        top_municipalities_heatmap: top_munis_30.to_vec(),
    }
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

/// 媒体分析PDF/印刷用HTMLレポート
pub async fn survey_report_html(
    State(state): State<Arc<AppState>>,
    session: Session,
    Query(query): Query<IntegrateQuery>,
) -> Html<String> {
    let session_id = match &query.session_id {
        Some(id) if !id.is_empty() => id.clone(),
        _ => return Html("<html><body><p>セッションIDが必要です。CSVをアップロードしてください。</p></body></html>".to_string()),
    };

    // 監査: 媒体分析レポート生成
    crate::audit::record_event(
        &state.audit,
        &session,
        "generate_survey_report",
        "report",
        &session_id,
        "",
    )
    .await;

    // キャッシュから集計データを復元
    let agg_cached = state.cache.get(&format!("survey_agg_{}", session_id));
    let seeker_cached = state.cache.get(&format!("survey_seeker_{}", session_id));

    let agg: super::aggregator::SurveyAggregation = match agg_cached {
        Some(v) => serde_json::from_value(v).unwrap_or_default(),
        None => return Html("<html><body><p>分析データが期限切れです。CSVを再アップロードしてください。</p></body></html>".to_string()),
    };
    let seeker: super::job_seeker::JobSeekerAnalysis = seeker_cached
        .and_then(|v| serde_json::from_value(v).ok())
        .unwrap_or_default();

    // 企業別・雇用形態別の集計はレコードキャッシュから再計算が必要だが、
    // 現時点ではaggの既存フィールドのみで生成（企業別集計はレコード不要の仮実装）
    let by_company = agg.by_company.clone();
    let by_emp_type_salary = agg.by_emp_type_salary.clone();
    let salary_min_values = agg.salary_min_values.clone();
    let salary_max_values = agg.salary_max_values.clone();

    // HWデータ＋外部統計を取得（オプション。失敗・未接続時もレポート生成は継続）
    // 2026-04-29: グローバルフィルタからの URL クエリ (?pref=&muni=) を優先採用。
    //   未指定時のみキャッシュの dominant_prefecture/municipality にフォールバック。
    let pref = query
        .pref
        .clone()
        .filter(|s| !s.is_empty() && s != "全国")
        .unwrap_or_else(|| {
            state
                .cache
                .get(&format!("survey_pref_{}", session_id))
                .and_then(|v| v.as_str().map(|s| s.to_string()))
                .unwrap_or_default()
        });
    let muni = query
        .muni
        .clone()
        .filter(|s| !s.is_empty() && s != "すべて")
        .unwrap_or_else(|| {
            state
                .cache
                .get(&format!("survey_muni_{}", session_id))
                .and_then(|v| v.as_str().map(|s| s.to_string()))
                .unwrap_or_default()
        });

    let hw_ctx = if !pref.is_empty() {
        if let Some(db) = state.hw_db.clone() {
            let turso = state.turso_db.clone();
            let pref2 = pref.clone();
            // 2026-05-14: 旧コメント「都道府県レベル(muni="")で取得してマクロ比較を優先」を
            //   撤回。muni を捨てると通勤圏 / OD 流入流出 / 労働力率市町村粒度 が
            //   永久に取れず、ユーザーが「市区町村まで選択しても OD 出ない」と
            //   訴える原因になっていた (v16 PDF 検証で発覚)。
            //   ユーザー選択 muni をそのまま渡し、市区町村粒度を優先する。
            //   都道府県マクロ指標 (人口/最低賃金) は muni 指定時も
            //   build_insight_context 内で pref ベースのカラムから取得されるため影響なし。
            let muni2 = muni.clone();
            // 2026-04-30 (T2): 業界フィルタを ext_turnover に適用するため closure に渡す
            let industry_for_ext = query
                .industry
                .as_ref()
                .filter(|s| !s.is_empty())
                .map(|raw| {
                    super::report_html::industry_mismatch::map_hw_to_major_industry(raw).to_string()
                });
            match tokio::task::spawn_blocking(move || {
                let mut ctx = super::super::insight::fetch::build_insight_context(
                    &db,
                    turso.as_ref(),
                    &pref2,
                    &muni2,
                );
                // T2 (2026-04-30): industry_filter があれば ext_turnover を業界別に上書き
                // 既存挙動 (産業計) は industry_filter=None で保持。マッチ 0 件は産業計にフォールバック。
                if let (Some(ind), Some(t)) = (industry_for_ext.as_deref(), turso.as_ref()) {
                    ctx.ext_turnover = super::super::trend::fetch::fetch_ext_turnover_with_industry(
                        t,
                        &pref2,
                        Some(ind),
                    );
                }
                // CR-9 (2026-04-28): 産業ミスマッチ専用の遅いフェッチ
                // build_insight_context から分離し、survey_report_html でのみ実行。
                // integrate エンドポイントが影響を受けないように設計。
                //
                // **粒度統一**: 就業者構成 (fetch_industry_structure) と
                // HW 求人 (fetch_hw_industry_counts) は **両方とも都道府県粒度** で集計する
                // (fetch_industry_structure は prefecture_code のみで集計、市区町村フィルタなし)。
                // 過去 (commit c7f7cff) で HW 側のみ市区町村粒度にしてしまい
                // 同じ表内で粒度が混在 (就業者=都道府県 / HW=市区町村) してギャップが歪んだバグを修正。
                use crate::geo::pref_name_to_code;
                let pref_code = pref_name_to_code()
                    .get(pref2.as_str())
                    .map(|s| s.to_string())
                    .unwrap_or_default();
                if !pref_code.is_empty() {
                    ctx.ext_industry_employees =
                        super::super::analysis::fetch::fetch_industry_structure(
                            &db,
                            turso.as_ref(),
                            &pref_code,
                        );
                }
                // muni="" で都道府県集計 (就業者構成と粒度を揃える)
                ctx.hw_industry_counts =
                    super::super::analysis::fetch::fetch_hw_industry_counts(&db, &pref2, "");
                ctx
            })
            .await
            {
                Ok(ctx) => Some(ctx),
                Err(e) => {
                    tracing::warn!("HW context build failed for survey report: {e}");
                    None
                }
            }
        } else {
            None
        }
    } else {
        None
    };

    // F-2: SalesNow 企業データ取得（同じ地域の注目企業）
    // 印刷レポートにも SalesNow 企業トップリストを掲載する
    let salesnow_companies = if !pref.is_empty() {
        if let (Some(sn_db), Some(hw_db)) = (state.salesnow_db.clone(), state.hw_db.clone()) {
            let pref2 = pref.clone();
            let muni2 = muni.clone();
            tokio::task::spawn_blocking(move || {
                super::super::company::fetch::fetch_companies_by_region(
                    &sn_db, &hw_db, &pref2, &muni2, 30,
                )
            })
            .await
            .unwrap_or_default()
        } else {
            Vec::new()
        }
    } else {
        Vec::new()
    };

    // F-2b (2026-04-29): SalesNow 4 セグメント企業 (規模上位 / 中規模 / 人員拡大 / 求人積極)
    // ユーザー指摘「今は地元の大手しか表示されてない」「業界絞込/絞らない 両方表示したい」に対応。
    //
    // 業界指定時: 全業界版 + 同業界版 の **両方** を取得し、render 側で併記表示する。
    // 業界未指定時: 全業界版のみ取得 (異業種ベンチマーク用途)。
    // 業界フィルタ: HW industry_raw (例「病院」) または 大分類 (例「医療,福祉」) を受け、
    // 12 大分類に正規化して downstream に渡す。
    // map_hw_to_major_industry は industry_raw → 大分類のキーワードマッピング。
    let industry_filter = query
        .industry
        .as_ref()
        .filter(|s| !s.is_empty())
        .map(|raw| {
            super::report_html::industry_mismatch::map_hw_to_major_industry(raw).to_string()
        });

    let (salesnow_segments_all, salesnow_segments_industry) = if !pref.is_empty() {
        if let (Some(sn_db), Some(hw_db)) = (state.salesnow_db.clone(), state.hw_db.clone()) {
            let pref_a = pref.clone();
            let muni_a = muni.clone();
            let sn_db_a = sn_db.clone();
            let hw_db_a = hw_db.clone();
            // 全業界版 (常に取得)
            let all_handle = tokio::task::spawn_blocking(move || {
                super::super::company::fetch::fetch_company_segments_by_region(
                    &sn_db_a, &hw_db_a, &pref_a, &muni_a,
                )
            });
            // 同業界版 (業界指定時のみ)
            let industry_handle = if let Some(ref ind) = industry_filter {
                let pref_i = pref.clone();
                let muni_i = muni.clone();
                let ind_owned = ind.clone();
                Some(tokio::task::spawn_blocking(move || {
                    super::super::company::fetch::fetch_company_segments_by_region_with_industry(
                        &sn_db,
                        &hw_db,
                        &pref_i,
                        &muni_i,
                        Some(ind_owned.as_str()),
                    )
                }))
            } else {
                None
            };
            let all = all_handle.await.unwrap_or_default();
            let industry = if let Some(h) = industry_handle {
                h.await.unwrap_or_default()
            } else {
                Default::default()
            };
            (all, industry)
        } else {
            (Default::default(), Default::default())
        }
    } else {
        (Default::default(), Default::default())
    };
    // 後方互換: 既存の `salesnow_segments` は全業界版を指す
    let salesnow_segments = salesnow_segments_all.clone();

    // HW enrichment map: CSV の (pref, muni) ごとに postings 実件数 + 時系列推移 + 欠員率
    let hw_enrichment_map = if let Some(hw_db) = state.hw_db.clone() {
        let turso = state.turso_db.clone();
        let pairs: Vec<(String, String)> = agg
            .by_municipality_salary
            .iter()
            .filter(|m| !m.prefecture.is_empty() && !m.name.is_empty())
            .map(|m| (m.prefecture.clone(), m.name.clone()))
            .collect();
        tokio::task::spawn_blocking(move || {
            super::hw_enrichment::enrich_areas(&hw_db, turso.as_ref(), &pairs)
        })
        .await
        .unwrap_or_default()
    } else {
        std::collections::HashMap::new()
    };

    // 2026-04-26 Granularity: CSV 上位 N 市区町村別デモグラフィック
    // ユーザー指摘「都道府県単位は参考にならない」に対応。市区町村粒度のピラミッド・労働力・教育施設等を取得
    let municipality_demographics = if let Some(hw_db) = state.hw_db.clone() {
        let turso = state.turso_db.clone();
        let top_munis = super::granularity::top_municipalities(&agg, 5);
        if top_munis.is_empty() {
            Vec::new()
        } else {
            tokio::task::spawn_blocking(move || {
                super::granularity::fetch_municipality_demographics(
                    &hw_db,
                    turso.as_ref(),
                    &top_munis,
                )
            })
            .await
            .unwrap_or_default()
        }
    } else {
        Vec::new()
    };

    let _ = (&pref, &muni);
    // 2026-04-29: variant 切替 (?variant=full|public)
    let variant = super::report_html::ReportVariant::from_query(query.variant.as_deref());
    // 2026-05-01: theme 切替 (?theme=v8|v7a|default)
    let theme = super::report_html::ReportTheme::from_query(query.theme.as_deref());
    let html = super::report_html::render_survey_report_page_with_variant_v3_themed(
        &agg,
        &seeker,
        &by_company,
        &by_emp_type_salary,
        &salary_min_values,
        &salary_max_values,
        hw_ctx.as_ref(),
        &salesnow_companies,
        &salesnow_segments,
        &salesnow_segments_industry,
        industry_filter.as_deref(),
        &hw_enrichment_map,
        &municipality_demographics,
        variant,
        theme,
        // Phase 3 Step 5 Phase 5 (2026-05-04): MarketIntelligence variant の実 fetch 用 DB。
        // 既存 Full / Public 経路では variant guard により未使用。
        // L264 の `db` / L269 の `turso` は L311 の spawn_blocking に move 済のため、
        // ここでは state から再取得する。
        state.hw_db.as_ref(),
        state.turso_db.as_ref(),
        // 2026-05-14: ヘッダーフィルタで選択された地域を「主要地域」として優先表示。
        //   未選択 (空文字列) の場合のみ CSV 内 dominant にフォールバック。
        &pref,
        &muni,
    );

    Html(html)
}

/// 媒体分析レポートを HTML ファイルとしてダウンロード
///
/// GAS 踏襲: ユーザーが HTML をローカルに保存 → 手元で文言編集 → ブラウザで開き
/// 直して印刷 → PDF 保存、というワークフローを支援する。
///
/// `/report/survey/download` エンドポイント。
/// Content-Type: text/html; charset=utf-8
/// Content-Disposition: attachment; filename="hellowork_report_YYYY-MM-DD.html"
pub async fn survey_report_download(
    State(state): State<Arc<AppState>>,
    session: Session,
    Query(query): Query<IntegrateQuery>,
) -> axum::response::Response {
    use axum::http::{header, StatusCode};
    use axum::response::IntoResponse;

    // 内部で survey_report_html と同じロジックを再利用するため、先に HTML を生成
    let html_resp = survey_report_html(State(state), session, Query(query)).await;
    let html_body = html_resp.0;

    // ダウンロードファイル名: 日付付きで上書き衝突を避ける
    let today = chrono::Local::now().format("%Y-%m-%d").to_string();
    let filename = format!("hellowork_report_{}.html", today);

    (
        StatusCode::OK,
        [
            (header::CONTENT_TYPE, "text/html; charset=utf-8".to_string()),
            (
                header::CONTENT_DISPOSITION,
                format!(r#"attachment; filename="{}""#, filename),
            ),
        ],
        html_body,
    )
        .into_response()
}
