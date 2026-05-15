//! Excel (xlsx) レポート出力

use axum::extract::State;
use axum::response::IntoResponse;
use std::sync::Arc;
use tower_sessions::Session;

use super::super::overview::get_session_filters;
use super::engine::generate_insights;
use super::fetch::build_insight_context;
use super::helpers::*;
use crate::AppState;

/// Excel レポート出力 (/api/insight/report/xlsx)
pub async fn insight_report_xlsx(
    State(state): State<Arc<AppState>>,
    session: Session,
) -> impl IntoResponse {
    let filters = get_session_filters(&session).await;

    let db = match &state.hw_db {
        Some(db) => db.clone(),
        None => {
            return axum::response::Response::builder()
                .status(400)
                .body(axum::body::Body::from("DB未接続"))
                .unwrap_or_else(|e| {
                    tracing::error!("response builder failed: {e}");
                    axum::response::Response::new(axum::body::Body::from("Internal Error"))
                });
        }
    };

    let pref = filters.prefecture.clone();
    let muni = filters.municipality.clone();
    let turso = state.turso_db.clone();

    let xlsx_bytes = tokio::task::spawn_blocking(move || {
        let ctx = build_insight_context(&db, turso.as_ref(), &pref, &muni);
        let insights = generate_insights(&ctx);
        build_xlsx(&insights, &pref, &muni, &ctx)
    })
    .await
    .unwrap_or_else(|_| Err("処理エラー".to_string()));

    match xlsx_bytes {
        Ok(bytes) => {
            let filename = format!("hw_report_{}.xlsx", chrono::Local::now().format("%Y%m%d"));
            axum::response::Response::builder()
                .header(
                    "Content-Type",
                    "application/vnd.openxmlformats-officedocument.spreadsheetml.sheet",
                )
                .header(
                    "Content-Disposition",
                    format!("attachment; filename=\"{}\"", filename),
                )
                .body(axum::body::Body::from(bytes))
                .unwrap_or_else(|e| {
                    tracing::error!("response builder failed: {e}");
                    axum::response::Response::new(axum::body::Body::from("Internal Error"))
                })
        }
        Err(e) => axum::response::Response::builder()
            .status(500)
            .body(axum::body::Body::from(format!("Excel生成エラー: {}", e)))
            .unwrap_or_else(|e2| {
                tracing::error!("response builder failed: {e2}");
                axum::response::Response::new(axum::body::Body::from("Internal Error"))
            }),
    }
}

/// Excel ワークブック生成
fn build_xlsx(
    insights: &[Insight],
    pref: &str,
    muni: &str,
    ctx: &super::fetch::InsightContext,
) -> Result<Vec<u8>, String> {
    use rust_xlsxwriter::*;

    let mut workbook = Workbook::new();

    // フォーマット定義
    let header_fmt = Format::new()
        .set_bold()
        .set_font_size(11)
        .set_background_color(Color::RGB(0xE2E8F0));
    let title_fmt = Format::new().set_bold().set_font_size(14);
    let subtitle_fmt = Format::new()
        .set_font_size(10)
        .set_font_color(Color::RGB(0x64748B));

    let location = if !muni.is_empty() {
        format!("{} {}", pref, muni)
    } else if !pref.is_empty() {
        pref.to_string()
    } else {
        "全国".to_string()
    };

    // ======== Sheet1: サマリー ========
    let sheet1 = workbook.add_worksheet();
    sheet1.set_name("サマリー").map_err(|e| e.to_string())?;
    sheet1
        .write_string_with_format(0, 0, "ハローワーク求人市場 総合診断レポート", &title_fmt)
        .ok();
    sheet1
        .write_string_with_format(
            1,
            0,
            format!("{} | {}", location, chrono::Local::now().format("%Y年%m月")),
            &subtitle_fmt,
        )
        .ok();

    let critical = insights
        .iter()
        .filter(|i| i.severity == Severity::Critical)
        .count();
    let warning = insights
        .iter()
        .filter(|i| i.severity == Severity::Warning)
        .count();
    let info = insights
        .iter()
        .filter(|i| i.severity == Severity::Info)
        .count();
    let positive = insights
        .iter()
        .filter(|i| i.severity == Severity::Positive)
        .count();

    sheet1.write_string(3, 0, "示唆件数").ok();
    sheet1.write_string(4, 0, "重大").ok();
    sheet1.write_number(4, 1, critical as f64).ok();
    sheet1.write_string(5, 0, "注意").ok();
    sheet1.write_number(5, 1, warning as f64).ok();
    sheet1.write_string(6, 0, "情報").ok();
    sheet1.write_number(6, 1, info as f64).ok();
    sheet1.write_string(7, 0, "良好").ok();
    sheet1.write_number(7, 1, positive as f64).ok();

    // 通勤圏情報
    if ctx.commute_zone_count > 0 {
        sheet1.write_string(9, 0, "通勤圏（30km）").ok();
        sheet1.write_string(10, 0, "圏内市区町村数").ok();
        sheet1
            .write_number(10, 1, ctx.commute_zone_count as f64)
            .ok();
        sheet1.write_string(11, 0, "圏内総人口").ok();
        sheet1
            .write_number(11, 1, ctx.commute_zone_total_pop as f64)
            .ok();
        sheet1.write_string(12, 0, "通勤流入数").ok();
        sheet1
            .write_number(12, 1, ctx.commute_inflow_total as f64)
            .ok();
    }

    // ======== Sheet2: 示唆一覧 ========
    let sheet2 = workbook.add_worksheet();
    sheet2.set_name("示唆一覧").map_err(|e| e.to_string())?;

    let cols = ["ID", "重要度", "カテゴリ", "タイトル", "内容"];
    for (i, col) in cols.iter().enumerate() {
        sheet2
            .write_string_with_format(0, i as u16, *col, &header_fmt)
            .ok();
    }
    sheet2.set_column_width(3, 30).ok();
    sheet2.set_column_width(4, 60).ok();

    for (row, insight) in insights.iter().enumerate() {
        let r = (row + 1) as u32;
        sheet2.write_string(r, 0, &insight.id).ok();
        sheet2.write_string(r, 1, insight.severity.label()).ok();
        sheet2.write_string(r, 2, insight.category.label()).ok();
        sheet2.write_string(r, 3, &insight.title).ok();
        sheet2.write_string(r, 4, &insight.body).ok();
    }

    // ======== Sheet3: 通勤フロー ========
    if ctx.commute_inflow_total > 0 {
        let sheet3 = workbook.add_worksheet();
        sheet3.set_name("通勤フロー").map_err(|e| e.to_string())?;

        let flow_cols = ["流入元都道府県", "流入元市区町村", "通勤者数"];
        for (i, col) in flow_cols.iter().enumerate() {
            sheet3
                .write_string_with_format(0, i as u16, *col, &header_fmt)
                .ok();
        }

        for (row, (p, m, c)) in ctx.commute_inflow_top3.iter().enumerate() {
            let r = (row + 1) as u32;
            sheet3.write_string(r, 0, p).ok();
            sheet3.write_string(r, 1, m).ok();
            sheet3.write_number(r, 2, *c as f64).ok();
        }
    }

    // バッファに書き出し
    let buf = workbook.save_to_buffer().map_err(|e| e.to_string())?;
    Ok(buf)
}
