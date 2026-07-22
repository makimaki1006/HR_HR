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
                        if matches!(
                            t.as_str(),
                            "indeed" | "indeed_sp" | "jobbox" | "other" | "auto"
                        ) {
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

    let mut records = match result {
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

    // ===== Gemini AI フォールバック (graceful degradation) =====
    // GEMINI_API_KEY 未設定なら from_env() が None → 以下は丸ごとスキップ (= 従来動作)。
    // API エラー/タイムアウト/枠切れも generate_json が None を返すため、常に従来結果に落ちる。
    let gemini = crate::gemini::GeminiClient::from_env();
    // 機能 E: AI が列推定を補助した列数 (画面サマリー用、レポートには出さない)
    let mut ai_colmap_cols: usize = 0;
    // 機能 C 拡張: 複数属性 AI 抽出の収穫量 (集計後に agg.jobbox.ai_extracted_count へ伝播)
    let mut ai_extraction_yield = super::upload::ExtractionYield::default();
    if let Some(client) = gemini.as_ref() {
        // --- 機能 E: 列マッピングの AI 推定 (パース結果が貧弱な場合の最後の砦) ---
        if super::upload::is_parse_poor(&records) {
            if let Some((headers, samples)) = super::upload::extract_header_and_samples(&data, 2) {
                let schema = super::upload::build_colmap_schema();
                let (sys, usr) = super::upload::build_colmap_prompt(&headers, &samples);
                if let Some(resp) = client.generate_json(&sys, &usr, schema).await {
                    let overrides = super::upload::parse_colmap_from_ai(&resp, headers.len());
                    if !overrides.is_empty() {
                        // 推定結果で col_map を補完して再パース (CPU 処理なので spawn_blocking)
                        let data_c = data.clone();
                        let ctx = context_pref.map(|s| s.to_string());
                        let ov = overrides.clone();
                        let reparsed = tokio::task::spawn_blocking(move || {
                            super::upload::parse_csv_bytes_with_col_overrides(
                                &data_c,
                                ctx.as_deref(),
                                source_hint,
                                &ov,
                            )
                        })
                        .await;
                        if let Ok(Ok(new_records)) = reparsed {
                            ai_colmap_cols = overrides.len();
                            tracing::info!(
                                "Gemini: AI column mapping supplemented {} columns, re-parsed {} records",
                                ai_colmap_cols,
                                new_records.len()
                            );
                            records = new_records;
                        }
                    }
                }
            }
        }

        // --- 機能 C 拡張: 複数属性 AI 抽出 (全行対象, 上限 15 コール = 300 件) ---
        ai_extraction_yield = run_holiday_ai_extraction(client, &mut records).await;
    }

    // 集計 + 求職者分析
    let mut agg = aggregate_records_with_mode(&records, wage_mode_enum);
    // 機能 C: 年間休日の AI 抽出件数を集計結果へ伝播 (§07.5 注記「うち AI 補助 K 件」用)
    // §07.5 注記は年間休日分のみを表示するため annual_holidays のみを渡す。
    agg.jobbox.ai_extracted_count = ai_extraction_yield.annual_holidays;
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

    // 機能 E/C: AI 補助が発動した場合のみ、画面サマリーに 1 行追加 (レポートには出さない)。
    //   graceful degradation のため、AI 未発動時 (ai_colmap_cols=0 かつ全 yield=0) は
    //   従来と完全に同一の HTML を返す。
    let mut body = render_analysis_result(&agg, &seeker, &session_id);
    let y = ai_extraction_yield;
    let extraction_any = y.annual_holidays
        + y.monthly_holidays
        + y.bonus
        + y.paid_leave
        + y.weekly_type
        + y.overtime;
    if ai_colmap_cols > 0 || extraction_any > 0 {
        let mut notes = String::new();
        if ai_colmap_cols > 0 {
            notes.push_str(&format!(
                r#"<p class="text-sky-300 text-xs">AI が列推定を補助しました ({} 列)</p>"#,
                ai_colmap_cols
            ));
        }
        if extraction_any > 0 {
            // §07.5 注記の ai_extracted_count は年間休日分のみ (agg 伝播済)。
            // このプレビュー行は画面専用で全属性を一覧表示する。
            notes.push_str(&format!(
                r#"<p class="text-sky-300 text-xs">AI 整形プレビュー: 年間休日 +{} / 月間休日 {}件 / 賞与 {}件 / 有給率 {}件 / 週休形態 {}件 / 残業 {}件</p>"#,
                y.annual_holidays, y.monthly_holidays, y.bonus,
                y.paid_leave, y.weekly_type, y.overtime
            ));
        }
        body.push_str(&format!(r#"<div class="stat-card">{}</div>"#, notes));
    }
    Html(body)
}

/// 機能 C 拡張 (2026-07-07): 複数属性を Gemini AI で抽出する (全レコードバッチ処理)。
///
/// - 対象: description が 30 文字以上の **全行** (`collect_extraction_targets` を使用。
///   年間休日 None 限定の旧フローから発展させ、月間休日/賞与/有給率/週休形態/残業も同時抽出)。
/// - **20 件/1 コール**、1 アップロードあたり**最大 15 コール (=300 件)**。超過分は regex のみ。
/// - LLM の返す値は `parse_extraction_response` (属性別レンジ検証) を通してから反映。
/// - 呼び出しが 1 つでも失敗 (None) したら、そのバッチは黙ってスキップ (graceful degradation)。
/// - GEMINI_API_KEY 未設定時は呼び出し元 (handlers.rs) の `if let Some(client)` ガードで
///   この関数自体が呼ばれないため、キー未設定パスは常に従来動作を維持する。
///
/// 戻り値: [`ExtractionYield`] (属性別の実反映件数)。
async fn run_holiday_ai_extraction(
    client: &crate::gemini::GeminiClient,
    records: &mut Vec<super::upload::SurveyRecord>,
) -> super::upload::ExtractionYield {
    let targets = super::upload::collect_extraction_targets(records);
    if targets.is_empty() {
        return super::upload::ExtractionYield::default();
    }
    let mut total = super::upload::ExtractionYield::default();
    // 2026-07-11: API 予算の主用途は商談準備レポートの解説生成 (ユーザー方針)。
    //   CSV 整形の AI 補助は「regex の取りこぼし補完」に限定し、40 件/回 × 最大 2 回
    //   (80 件分) に縮小する。残りは regex のみで処理。
    //   これにより 1 操作あたりの消費は CSV 2 + レポート 2 + リトライ余地で、
    //   無料枠 15 リクエスト/分に対して常に余裕を持つ。
    for (call_idx, chunk) in targets.chunks(40).enumerate() {
        if call_idx >= 2 {
            // 上限ガード: 80 件を超える分は regex のみ (レポート解説用の API 予算を優先)
            tracing::info!(
                "Gemini: multi-attr extraction hit 2-call cap, {} targets remain, relying on regex",
                targets.len().saturating_sub(call_idx * 40)
            );
            break;
        }
        let schema = super::upload::build_extraction_schema();
        let (sys, usr) = super::upload::build_extraction_prompt(chunk);
        if let Some(resp) = client.generate_json(&sys, &usr, schema).await {
            let results = super::upload::parse_extraction_response(&resp);
            let y = super::upload::apply_extraction_results(records, &results);
            total.annual_holidays += y.annual_holidays;
            total.monthly_holidays += y.monthly_holidays;
            total.bonus += y.bonus;
            total.paid_leave += y.paid_leave;
            total.weekly_type += y.weekly_type;
            total.overtime += y.overtime;
        }
    }
    let sum = total.annual_holidays
        + total.monthly_holidays
        + total.bonus
        + total.paid_leave
        + total.weekly_type
        + total.overtime;
    if sum > 0 {
        tracing::info!(
            "Gemini: multi-attr extraction filled annual={} monthly={} bonus={} paid={} weekly={} overtime={}",
            total.annual_holidays, total.monthly_holidays, total.bonus,
            total.paid_leave, total.weekly_type, total.overtime
        );
    }
    total
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
    /// Phase 2-A (2026-05-29): 給与単位モード切替。
    /// - `"monthly"` (デフォルト) / `"hourly"` / `"auto"` (= aggregator が自動判定)
    /// 未指定時は agg.is_hourly に応じて Section 03/05/06 でモード判定 (自動)。
    /// 明示時はその値で SQL fetcher と Section 描画を制御する。
    pub wage_mode: Option<String>,
    /// 2026-07-17: 解説資料 (?variant=guide) の「貴社の現在地」用の企業名。
    /// CSV 内の企業名と部分一致で照合する。未指定なら §1 を描画しない。
    pub company: Option<String>,
    /// 2026-07-17: 解説資料の生成モード。既定 = AI パイプライン (flash-lite
    /// 作成→逆証明レビュー→修正、失敗時は決定的テンプレへ自動フォールバック)。
    /// `ai=off` で決定的テンプレを強制 (API 枠温存・比較検証用)。
    pub ai: Option<String>,
    /// 2026-07-22: レポート生成の同期モード。`sync=1` で従来の同期生成
    /// (ダウンロード・自動化スクリプト用)。未指定はジョブ化 (進捗シェル)。
    pub sync: Option<String>,
    /// 2026-07-10: 出力セクション選択 (?sections=02,03,09 の形式)。
    /// - 未指定 / 空文字列: 従来どおり variant 準拠で全セクション出力 (出力不変)。
    /// - 指定時: カンマ区切りコードのセクションのみ出力 (表紙/目次/01/08 は常時)。
    ///   有効コード: 02,03,04,05,06,07,075,076,09,10。不明コードは無視。
    pub sections: Option<String>,
    /// 2026-07-13: Ver10 専用。表2-E (都道府県別給与 — 地域比較) を表示するか (?table2e=0/1)。
    /// - 未指定 / "1" / それ以外: 表示 (既定オン)。
    /// - "0": 非表示。
    /// Ver10 以外の variant では無視される。
    pub table2e: Option<String>,
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

    // Phase 2-A (2026-05-29): wage_mode 解決 (integrate_report 用)
    //   URL query → agg.is_hourly → "monthly" (デフォルト) の優先順位
    let wage_mode_resolved: String = query
        .wage_mode
        .as_deref()
        .filter(|s| matches!(*s, "monthly" | "hourly" | "both"))
        .map(|s| s.to_string())
        .unwrap_or_else(|| {
            if agg_parsed.as_ref().map(|a| a.is_hourly).unwrap_or(false) {
                "hourly".to_string()
            } else {
                "monthly".to_string()
            }
        });
    let wage_mode_for_thread = wage_mode_resolved.clone();

    let content = tokio::task::spawn_blocking(move || {
        use super::super::company::fetch::fetch_companies_by_region;
        use super::super::insight::engine::generate_insights;
        use super::super::insight::fetch::build_insight_context_with_wage_mode;
        use super::hw_enrichment::enrich_areas;

        let ctx = build_insight_context_with_wage_mode(
            &db,
            turso.as_ref(),
            &pref2,
            &muni2,
            &wage_mode_for_thread,
        );
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

/// 媒体分析PDF/印刷用HTMLレポート (エントリ)。
///
/// 2026-07-22: 既定はジョブ化 (即時に進捗シェルを返し、重い取得〜組版は
/// バックグラウンドで進めてステージを表示する)。`&sync=1` は従来の同期生成
/// (ダウンロード・自動化スクリプト用のエスケープハッチ)。
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

    // 解説資料の AI 版は専用ジョブ (レート制限の待機込みで進捗表示)
    if query.variant.as_deref() == Some("guide") && query.ai.as_deref() != Some("off") {
        return Html(super::report_html::render_guide_progress_shell());
    }

    // 同期経路 (&sync=1): ダウンロード・自動化用
    if query.sync.as_deref() == Some("1") {
        let noop = |_: &str| {};
        return build_survey_report_inner(state, query, &noop).await;
    }

    // 既定: ジョブ化 (レポート用進捗シェル)
    Html(super::report_html::render_report_progress_shell())
}

/// レポート本体の生成 (進捗レポータ付き)。
///
/// ジョブ (`survey_report_start`) と同期経路 (`&sync=1` / ダウンロード) の両方から
/// 呼ばれる。Session には依存しない (監査は呼び出し側の責務)。
async fn build_survey_report_inner(
    state: Arc<AppState>,
    query: IntegrateQuery,
    progress: &(dyn Fn(&str) + Send + Sync),
) -> Html<String> {
    let session_id = match &query.session_id {
        Some(id) if !id.is_empty() => id.clone(),
        _ => return Html("<html><body><p>セッションIDが必要です。CSVをアップロードしてください。</p></body></html>".to_string()),
    };

    progress("集計データを読込中");
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

    progress("公的統計・地域データを取得中");
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
            // Phase 2-A (2026-05-29): wage_mode 解決
            //   優先順位: 1) URL クエリ ?wage_mode=hourly/monthly  2) agg.is_hourly フラグ
            //   silent fallback ではなく明示的に文字列で渡す ("monthly"/"hourly")。
            let wage_mode_resolved: String = query
                .wage_mode
                .as_deref()
                .filter(|s| matches!(*s, "monthly" | "hourly" | "both"))
                .map(|s| s.to_string())
                .unwrap_or_else(|| {
                    if agg.is_hourly {
                        "hourly".to_string()
                    } else {
                        "monthly".to_string()
                    }
                });
            let wage_mode_for_thread = wage_mode_resolved.clone();
            match tokio::task::spawn_blocking(move || {
                let mut ctx = super::super::insight::fetch::build_insight_context_with_wage_mode(
                    &db,
                    turso.as_ref(),
                    &pref2,
                    &muni2,
                    &wage_mode_for_thread,
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
                // P1-6 (2026-05-28): 職種偏り判定用データを取得 (Section 01 Finding 07)
                // 粒度: 産業 (hw_industry_counts) と揃えて都道府県集計 (muni="")
                ctx.hw_job_type_counts =
                    super::super::analysis::fetch::fetch_hw_job_type_counts(&db, &pref2, "");
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

    // 2026-07-17: 解説資料のテンプレ版 (?variant=guide&ai=off)。
    //   AI 版は上流でジョブ化済み (進捗シェル)。ここに来るのは ai=off のみ。
    if query.variant.as_deref() == Some("guide") {
        return Html(super::report_html::render_survey_guide_page(
            &agg,
            hw_ctx.as_ref(),
            &pref,
            &muni,
            query.company.as_deref(),
        ));
    }

    // F-2: SalesNow 企業データ取得（同じ地域の注目企業）
    // 印刷レポートにも SalesNow 企業トップリストを掲載する
    progress("地域企業データを取得中");
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
            // 2026-05-15: HW⇔SalesNow ID ずれ対策で v2_industry_mapping 逆引き使用。
            // 2026-05-15 拡張: 通勤圏 (近隣市町村 30km 圏) も含めて検索。
            //   藤岡市単独で薄い業界でも近隣 (高崎/伊勢崎/玉村等) を含めれば
            //   実用的なベンチマークが取れる。県境越え (例: 藤岡→埼玉本庄) も対応。
            let industry_handle = if let Some(ref ind) = industry_filter {
                let pref_i = pref.clone();
                let muni_i = muni.clone();
                let ind_owned = ind.clone();
                let sn_db_for_map = sn_db.clone();
                let hw_db_for_zone = hw_db.clone();
                Some(tokio::task::spawn_blocking(move || {
                    // v2_industry_mapping から SalesNow 実値リストを逆引き
                    let sn_industries =
                        super::super::company::fetch::fetch_sn_industries_for_hw_industry(
                            &sn_db_for_map,
                            &ind_owned,
                        );
                    if sn_industries.is_empty() {
                        // マッピングテーブルに該当なし → 旧 LIKE 経路フォールバック
                        return super::super::company::fetch::fetch_company_segments_by_region_with_industry(
                            &sn_db, &hw_db, &pref_i, &muni_i, Some(ind_owned.as_str()),
                        );
                    }

                    // 通勤圏取得 (距離ベース 30km、muni 指定時のみ)
                    let mut neighborhood: Vec<(String, String)> = if !muni_i.is_empty() {
                        let zone = super::super::analysis::fetch::fetch_commute_zone(
                            &hw_db_for_zone,
                            &pref_i,
                            &muni_i,
                            30.0,
                        );
                        zone.iter()
                            .map(|m| (m.prefecture.clone(), m.municipality.clone()))
                            .collect()
                    } else {
                        vec![]
                    };
                    // 自市町村も明示的に含める (zone に含まれていない場合の保険)
                    if !muni_i.is_empty()
                        && !neighborhood
                            .iter()
                            .any(|(p, m)| p == &pref_i && m == &muni_i)
                    {
                        neighborhood.insert(0, (pref_i.clone(), muni_i.clone()));
                    }

                    if !neighborhood.is_empty() {
                        super::super::company::fetch::fetch_company_segments_by_neighborhood_sn_industries(
                            &sn_db, &hw_db, &neighborhood, &sn_industries,
                        )
                    } else {
                        // muni 不在 → 通勤圏取得不能 → 単一 (pref+空 muni) で sn_industry IN
                        super::super::company::fetch::fetch_company_segments_by_region_with_sn_industries(
                            &sn_db, &hw_db, &pref_i, &muni_i, &sn_industries,
                        )
                    }
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
    // 2026-07-10: セクション選択 (?sections=02,03,09)。未指定なら variant 準拠 (出力不変)。
    let section_set =
        super::report_html::SectionSet::from_query(query.sections.as_deref(), variant);
    // 2026-07-13: Ver10 の表2-E 表示フラグ。?table2e=0 のときだけ非表示、それ以外は表示 (既定オン)。
    let table2e = query.table2e.as_deref() != Some("0");
    progress("レポートを組版中");
    let html = super::report_html::render_survey_report_page_with_sections(
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
        // 2026-07-10: セクション選択集合 (?sections=... 未指定なら variant 準拠)。
        section_set,
        // 2026-07-13: Ver10 の表2-E 表示フラグ (?table2e=0/1、既定オン)。
        table2e,
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

    // 2026-07-22: ジョブ化後もダウンロードは同期でフル HTML を得る必要があるため、
    // シェルを返す survey_report_html ではなく内部生成関数を直接呼ぶ。
    if let Some(sid) = query.session_id.clone().filter(|s| !s.is_empty()) {
        crate::audit::record_event(&state.audit, &session, "generate_survey_report", "report", &sid, "")
            .await;
    }
    let noop = |_: &str| {};
    let html_resp = build_survey_report_inner(state, query, &noop).await;
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

// ============================================================
// 解説資料 AI 版のジョブ化 (2026-07-22)
// ============================================================
//
// レート制限 (Gemini 無料枠 15req/分、リミッタは 12/分) による待機を
// 進捗表示で吸収するため、AI 版解説資料は非同期ジョブとして生成する。
// フロー: /report/survey?variant=guide → 進捗シェル (即時)
//   → JS が GET /api/survey/guide/start?{同じクエリ} → {job_id}
//   → GET /api/survey/guide/status/{job_id} を 2 秒間隔でポーリング
//   → state=done で /report/survey/guide/result/{job_id} へ遷移
// 状態と成果物はメモリキャッシュ (TTL 付き) に保持する。

fn guide_job_status_key(id: &str) -> String {
    format!("guide_job_{}", id)
}
fn guide_job_html_key(id: &str) -> String {
    format!("guide_html_{}", id)
}

fn set_guide_status(state: &AppState, id: &str, job_state: &str, message: &str) {
    state.cache.set(
        guide_job_status_key(id),
        serde_json::json!({ "state": job_state, "message": message }),
    );
}

/// 解説資料 AI 生成ジョブの開始。job_id を即時返し、生成はバックグラウンドで進む。
pub async fn survey_guide_start(
    State(state): State<Arc<AppState>>,
    session: Session,
    Query(query): Query<IntegrateQuery>,
) -> axum::response::Json<serde_json::Value> {
    use axum::response::Json;

    let Some(session_id) = query.session_id.clone().filter(|s| !s.is_empty()) else {
        return Json(serde_json::json!({ "error": "session_id が必要です" }));
    };
    let Some(agg_val) = state.cache.get(&format!("survey_agg_{}", session_id)) else {
        return Json(serde_json::json!({ "error": "分析データが期限切れです。CSVを再アップロードしてください" }));
    };
    let agg: super::aggregator::SurveyAggregation = match serde_json::from_value(agg_val) {
        Ok(a) => a,
        Err(_) => {
            return Json(serde_json::json!({ "error": "分析データの読み込みに失敗しました" }));
        }
    };

    crate::audit::record_event(
        &state.audit,
        &session,
        "generate_survey_guide",
        "report",
        &session_id,
        "",
    )
    .await;

    // pref/muni 解決 (survey_report_html と同じ優先順位: クエリ > セッションキャッシュ)
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
    let company = query.company.clone().filter(|s| !s.trim().is_empty());

    let job_id = format!("g_{}", uuid::Uuid::new_v4());
    set_guide_status(&state, &job_id, "queued", "生成を準備中");

    let state_bg = state.clone();
    let job_id_bg = job_id.clone();
    tokio::spawn(async move {
        // 進捗レポータ: ステージをキャッシュへ外部化 (シェルがポーリング表示)
        let st = state_bg.clone();
        let jid = job_id_bg.clone();
        let progress = move |msg: &str| {
            set_guide_status(&st, &jid, "running", msg);
        };
        progress("地域データを取得中");

        // 公的統計コンテキスト (解説資料が使うのは通勤 OD 等の最小セット)
        let hw_ctx = if !pref.is_empty() {
            if let Some(db) = state_bg.hw_db.clone() {
                let turso = state_bg.turso_db.clone();
                let pref2 = pref.clone();
                let muni2 = muni.clone();
                let wage_mode = if agg.is_hourly { "hourly" } else { "monthly" }.to_string();
                tokio::task::spawn_blocking(move || {
                    super::super::insight::fetch::build_insight_context_with_wage_mode(
                        &db,
                        turso.as_ref(),
                        &pref2,
                        &muni2,
                        &wage_mode,
                    )
                })
                .await
                .ok()
            } else {
                None
            }
        } else {
            None
        };

        let html = match super::report_html::render_survey_guide_page_ai(
            &agg,
            hw_ctx.as_ref(),
            &pref,
            &muni,
            company.as_deref(),
            &progress,
        )
        .await
        {
            Some(h) => h,
            None => {
                // AI 不可 (キー未設定・混雑・検証全滅) はテンプレ版に自動フォールバック
                tracing::warn!("guide AI ジョブ失敗。テンプレ版へフォールバック (job={})", job_id_bg);
                set_guide_status(&state_bg, &job_id_bg, "running", "テンプレート版で組版中");
                super::report_html::render_survey_guide_page(
                    &agg,
                    hw_ctx.as_ref(),
                    &pref,
                    &muni,
                    company.as_deref(),
                )
            }
        };
        state_bg
            .cache
            .set(guide_job_html_key(&job_id_bg), serde_json::json!(html));
        set_guide_status(&state_bg, &job_id_bg, "done", "完成しました");
    });

    Json(serde_json::json!({ "job_id": job_id }))
}

/// 解説資料ジョブの進捗取得 (シェルページがポーリング)。
pub async fn survey_guide_status(
    State(state): State<Arc<AppState>>,
    _session: Session,
    axum::extract::Path(job_id): axum::extract::Path<String>,
) -> axum::response::Json<serde_json::Value> {
    use axum::response::Json;
    match state.cache.get(&guide_job_status_key(&job_id)) {
        Some(v) => Json(v),
        None => Json(serde_json::json!({ "state": "failed", "message": "ジョブが見つからないか期限切れです" })),
    }
}

/// 解説資料ジョブの成果物 (完成 HTML)。
pub async fn survey_guide_result(
    State(state): State<Arc<AppState>>,
    _session: Session,
    axum::extract::Path(job_id): axum::extract::Path<String>,
) -> Html<String> {
    match state
        .cache
        .get(&guide_job_html_key(&job_id))
        .and_then(|v| v.as_str().map(|s| s.to_string()))
    {
        Some(html) => Html(html),
        None => Html(
            "<html><body><p>解説資料が見つからないか期限切れです。もう一度生成してください。</p></body></html>"
                .to_string(),
        ),
    }
}

/// レポート本体の生成ジョブ開始 (2026-07-22)。
///
/// 解説資料と同じジョブ+進捗シェル方式。重い取得 (公的統計・企業データ) の
/// 待ち時間をステージ表示で見える化する。
pub async fn survey_report_start(
    State(state): State<Arc<AppState>>,
    session: Session,
    Query(query): Query<IntegrateQuery>,
) -> axum::response::Json<serde_json::Value> {
    use axum::response::Json;

    let Some(session_id) = query.session_id.clone().filter(|s| !s.is_empty()) else {
        return Json(serde_json::json!({ "error": "session_id が必要です" }));
    };
    if state
        .cache
        .get(&format!("survey_agg_{}", session_id))
        .is_none()
    {
        return Json(serde_json::json!({ "error": "分析データが期限切れです。CSVを再アップロードしてください" }));
    }

    crate::audit::record_event(
        &state.audit,
        &session,
        "generate_survey_report",
        "report",
        &session_id,
        "",
    )
    .await;

    let job_id = format!("r_{}", uuid::Uuid::new_v4());
    set_guide_status(&state, &job_id, "queued", "生成を準備中");

    let state_bg = state.clone();
    let job_id_bg = job_id.clone();
    tokio::spawn(async move {
        let st = state_bg.clone();
        let jid = job_id_bg.clone();
        let progress = move |msg: &str| {
            set_guide_status(&st, &jid, "running", msg);
        };
        let html = build_survey_report_inner(state_bg.clone(), query, &progress).await;
        state_bg
            .cache
            .set(guide_job_html_key(&job_id_bg), serde_json::json!(html.0));
        set_guide_status(&state_bg, &job_id_bg, "done", "完成しました");
    });

    Json(serde_json::json!({ "job_id": job_id }))
}
