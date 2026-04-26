//! 統合 PDF レポートの HTML レンダリング
//!
//! 単一の `<!DOCTYPE html>` ドキュメントを返す。`window.print()` で 1 PDF。

use axum::extract::{Query, State};
use axum::response::Html;
use serde::Deserialize;
use serde_json::Value;
use std::fmt::Write as _;
use std::sync::Arc;
use tower_sessions::Session;

use crate::handlers::helpers::{
    escape_html, escape_url_attr, format_number, get_f64, get_i64, get_str_ref,
};
use crate::handlers::insight::engine::generate_insights;
use crate::handlers::insight::fetch::build_insight_context;
use crate::handlers::insight::helpers::{Insight, InsightCategory, Severity};
use crate::handlers::overview::get_session_filters;
use crate::AppState;

/// 統合レポートのクエリパラメータ
#[derive(Debug, Deserialize, Default)]
pub struct IntegratedReportQuery {
    #[serde(default)]
    pub prefecture: Option<String>,
    #[serde(default)]
    pub municipality: Option<String>,
    /// クライアントロゴ差し替え対応（任意。dangerous scheme は escape_url_attr で拒否）
    #[serde(default)]
    pub logo_url: Option<String>,
}

/// `/report/integrated` ハンドラ
pub async fn integrated_report(
    State(state): State<Arc<AppState>>,
    session: Session,
    Query(q): Query<IntegratedReportQuery>,
) -> Html<String> {
    // 監査ログ（feedback_test_data_validation.md: フィルタ条件も記録対象）
    crate::audit::record_event(
        &state.audit,
        &session,
        "generate_integrated_report",
        "report",
        "integrated",
        &format!(
            "{}_{}",
            q.prefecture.as_deref().unwrap_or(""),
            q.municipality.as_deref().unwrap_or("")
        ),
    )
    .await;

    let filters = get_session_filters(&session).await;

    // クエリ優先 → セッションフォールバック
    let pref = q
        .prefecture
        .filter(|s| !s.is_empty())
        .unwrap_or(filters.prefecture.clone());
    let muni = q
        .municipality
        .filter(|s| !s.is_empty())
        .unwrap_or(filters.municipality.clone());

    let logo_url = q
        .logo_url
        .as_deref()
        .map(escape_url_attr)
        .filter(|s| s != "#" && !s.is_empty());

    let industry_label = filters.industry_label();

    let db = match &state.hw_db {
        Some(db) => db.clone(),
        None => {
            return Html(render_no_db_page(&pref, &muni, &industry_label));
        }
    };

    let cache_key = format!(
        "integrated_report_{}_{}_{}",
        filters.industry_cache_key(),
        pref,
        muni,
    );
    if let Some(cached) = state.cache.get(&cache_key) {
        if let Some(html) = cached.as_str() {
            return Html(html.to_string());
        }
    }

    let turso = state.turso_db.clone();
    let pref_c = pref.clone();
    let muni_c = muni.clone();

    let (insights, ctx_kpis) = tokio::task::spawn_blocking(move || {
        let ctx = build_insight_context(&db, turso.as_ref(), &pref_c, &muni_c);
        let insights = generate_insights(&ctx);
        let mut kpis = extract_kpi_summary(&ctx);
        // フォールバック: v2_vacancy_rate 等の事前計算テーブルがない環境では
        // 直接 postings から件数・正社員比率・給与平均を計算する
        if kpis.posting_count == 0 {
            kpis = fallback_kpi_from_postings(&db, &pref_c, &muni_c);
        }
        (insights, kpis)
    })
    .await
    .unwrap_or_else(|e| {
        tracing::error!("integrated_report: build context failed: {e}");
        (Vec::new(), KpiSummary::default())
    });

    let html = render_integrated_html(
        &pref,
        &muni,
        &industry_label,
        logo_url.as_deref(),
        &insights,
        &ctx_kpis,
    );
    state.cache.set(cache_key, Value::String(html.clone()));
    Html(html)
}

/// レポート冒頭で表示する KPI 要約（InsightContext から抽出）
#[derive(Debug, Default, Clone)]
struct KpiSummary {
    /// 求人件数（HW 該当地域・産業）
    posting_count: i64,
    /// 正社員比率（0.0-1.0）
    seishain_ratio: f64,
    /// 月給下限の平均（円）
    salary_min_avg: f64,
    /// 欠員補充求人比率（0.0-1.0、可能なら）
    vacancy_rate: f64,
    /// 高齢化率 (%)
    elderly_rate: f64,
    /// 失業率 (%)
    unemployment_rate: f64,
}

/// InsightContext から KPI を抽出
fn extract_kpi_summary(ctx: &crate::handlers::insight::fetch::InsightContext) -> KpiSummary {
    let mut k = KpiSummary::default();

    // === vacancy テーブル: emp_group + posting_count + vacancy_rate + 正社員比率 ===
    // emp_group=「正社員」の行から正社員指標を取得
    if let Some(seishain_row) = ctx
        .vacancy
        .iter()
        .find(|r| get_str_ref(r, "emp_group") == "正社員")
    {
        k.vacancy_rate = get_f64(seishain_row, "vacancy_rate");
    }
    // 求人合計 = 全 emp_group の posting_count 合計
    let total: i64 = ctx
        .vacancy
        .iter()
        .map(|r| get_i64(r, "posting_count"))
        .sum();
    let seishain_count: i64 = ctx
        .vacancy
        .iter()
        .filter(|r| get_str_ref(r, "emp_group") == "正社員")
        .map(|r| get_i64(r, "posting_count"))
        .sum();
    k.posting_count = total;
    if total > 0 {
        k.seishain_ratio = seishain_count as f64 / total as f64;
    }

    // === salary_comp テーブル: avg_salary_min など ===
    // 正社員の平均月給下限（あれば）
    if let Some(row) = ctx
        .salary_comp
        .iter()
        .find(|r| get_str_ref(r, "emp_group") == "正社員")
    {
        let v = get_f64(row, "avg_salary_min");
        if v > 0.0 {
            k.salary_min_avg = v;
        }
    }

    // === ext_pyramid: 高齢化率（65歳以上 / 総人口） ===
    // ピラミッドは age_band/value 形式の場合と、aggregated カラムの場合がある
    // 安全側: 65 歳以上の合計を sum
    let total_pop: f64 = ctx.ext_pyramid.iter().map(|r| get_f64(r, "value")).sum();
    let elderly: f64 = ctx
        .ext_pyramid
        .iter()
        .filter(|r| {
            let band = get_str_ref(r, "age_band");
            band.starts_with("65")
                || band.starts_with("70")
                || band.starts_with("75")
                || band.starts_with("80")
                || band.starts_with("85")
                || band.starts_with("90")
                || band.starts_with("95")
                || band == "100歳以上"
        })
        .map(|r| get_f64(r, "value"))
        .sum();
    if total_pop > 0.0 {
        k.elderly_rate = elderly / total_pop * 100.0;
    }

    // === ext_labor_force: 失業率 ===
    if let Some(row) = ctx.ext_labor_force.first() {
        k.unemployment_rate = get_f64(row, "unemployment_rate");
    }

    k
}

/// 事前集計テーブルがない環境向けの postings 直接集計フォールバック
///
/// `v2_vacancy_rate` 等の事前計算テーブルが投入されていない環境（テスト・初期環境）では
/// 直接 postings から KPI を再計算する。
fn fallback_kpi_from_postings(
    db: &crate::db::local_sqlite::LocalDb,
    pref: &str,
    muni: &str,
) -> KpiSummary {
    let mut k = KpiSummary::default();
    let mut where_parts: Vec<String> = vec!["1=1".to_string()];
    let mut params: Vec<String> = Vec::new();
    if !pref.is_empty() {
        where_parts.push("prefecture = ?1".to_string());
        params.push(pref.to_string());
    }
    if !muni.is_empty() {
        where_parts.push(format!("municipality = ?{}", params.len() + 1));
        params.push(muni.to_string());
    }
    let where_clause = where_parts.join(" AND ");
    let sql = format!(
        "SELECT COUNT(*) AS cnt, \
                SUM(CASE WHEN employment_type = '正社員' THEN 1 ELSE 0 END) AS seishain_cnt, \
                AVG(CASE WHEN salary_type = '月給' AND salary_min > 0 THEN salary_min END) AS sal_avg \
         FROM postings WHERE {where_clause}"
    );
    let bind_refs: Vec<&dyn rusqlite::types::ToSql> = params
        .iter()
        .map(|s| s as &dyn rusqlite::types::ToSql)
        .collect();
    if let Ok(rows) = db.query(&sql, &bind_refs) {
        if let Some(r) = rows.first() {
            k.posting_count = get_i64(r, "cnt");
            let seishain = get_i64(r, "seishain_cnt");
            if k.posting_count > 0 {
                k.seishain_ratio = seishain as f64 / k.posting_count as f64;
            }
            k.salary_min_avg = get_f64(r, "sal_avg");
        }
    }
    k
}

/// 統合レポート HTML 生成（単一 `<!DOCTYPE html>`）
fn render_integrated_html(
    pref: &str,
    muni: &str,
    industry_label: &str,
    logo_url: Option<&str>,
    insights: &[Insight],
    kpis: &KpiSummary,
) -> String {
    let location = if muni.is_empty() {
        if pref.is_empty() {
            "全国".to_string()
        } else {
            pref.to_string()
        }
    } else {
        format!("{} {}", pref, muni)
    };
    let today = chrono::Local::now().format("%Y年%m月%d日").to_string();

    // Top findings (Critical / Warning 優先)
    let top_findings: Vec<&Insight> = {
        let mut v: Vec<&Insight> = insights
            .iter()
            .filter(|i| {
                matches!(i.severity, Severity::Critical | Severity::Warning)
                    && i.category != InsightCategory::ActionProposal
            })
            .collect();
        v.sort_by_key(|i| match i.severity {
            Severity::Critical => 0,
            Severity::Warning => 1,
            _ => 2,
        });
        v.into_iter().take(3).collect()
    };

    let action_proposals: Vec<&Insight> = insights
        .iter()
        .filter(|i| i.category == InsightCategory::ActionProposal)
        .take(5)
        .collect();

    let mut html = String::with_capacity(40_000);

    html.push_str(STYLE_HEAD);

    write!(
        html,
        r#"<title>採用市場統合レポート - {loc}</title>
</head>
<body>
<button class="print-btn no-print" onclick="window.print()" aria-label="印刷またはPDFで保存">印刷 / PDF保存</button>
"#,
        loc = escape_html(&location)
    )
    .unwrap();

    // === 表紙（page 1） ===
    html.push_str(r#"<section class="cover-page" role="region" aria-label="表紙">"#);
    if let Some(lu) = logo_url {
        write!(
            html,
            r#"<img class="cover-logo-img" src="{}" alt="クライアントロゴ" />"#,
            lu
        )
        .unwrap();
    } else {
        html.push_str(r#"<div class="cover-logo">F-A-C 株式会社</div>"#);
    }
    write!(
        html,
        r#"
<div class="cover-title">採用市場 統合レポート</div>
<div class="cover-sub">{loc} ／ 産業: {ind}</div>
<div class="cover-sub" style="font-size:13px;color:#666">作成日: {today}</div>
<div class="cover-confidential">
    本レポートは <strong>ハローワーク掲載求人データ</strong> および公的統計（e-Stat / Agoop 人流）に基づきます。<br>
    民間求人サイト（Indeed・マイナビ等）は含まれません。<br>
    集計値は「傾向」を示すものであり、因果関係を主張するものではありません。
</div>
<div class="cover-footer-cov">機密情報 ／ 取扱注意</div>
</section>
"#,
        loc = escape_html(&location),
        ind = escape_html(industry_label),
        today = escape_html(&today),
    )
    .unwrap();

    // === Executive Summary (TL;DR) ===
    html.push_str(r#"<section class="report-page"><h1>Executive Summary（TL;DR）</h1>"#);
    write!(
        html,
        r#"<div class="subtitle">{loc} ／ 産業: {ind} ／ {today}</div>"#,
        loc = escape_html(&location),
        ind = escape_html(industry_label),
        today = escape_html(&today),
    )
    .unwrap();

    // KPI 6 枚
    html.push_str(r#"<h2>主要 KPI</h2><div class="kpi-grid">"#);
    write_kpi_card(
        &mut html,
        "求人件数",
        &format_number(kpis.posting_count),
        "件",
    );
    write_kpi_card(
        &mut html,
        "正社員比率",
        &format!("{:.1}", kpis.seishain_ratio * 100.0),
        "%",
    );
    if kpis.salary_min_avg > 0.0 {
        write_kpi_card(
            &mut html,
            "月給下限平均",
            &format_number(kpis.salary_min_avg.round() as i64),
            "円",
        );
    } else {
        write_kpi_card(&mut html, "月給下限平均", "-", "");
    }
    if kpis.vacancy_rate > 0.0 {
        write_kpi_card(
            &mut html,
            "欠員補充率",
            &format!("{:.1}", kpis.vacancy_rate * 100.0),
            "%",
        );
    } else {
        write_kpi_card(&mut html, "欠員補充率", "-", "");
    }
    if kpis.elderly_rate > 0.0 {
        write_kpi_card(
            &mut html,
            "高齢化率（65 歳以上）",
            &format!("{:.1}", kpis.elderly_rate),
            "%",
        );
    } else {
        write_kpi_card(&mut html, "高齢化率（65 歳以上）", "-", "");
    }
    if kpis.unemployment_rate > 0.0 {
        write_kpi_card(
            &mut html,
            "失業率",
            &format!("{:.2}", kpis.unemployment_rate),
            "%",
        );
    } else {
        write_kpi_card(&mut html, "失業率", "-", "");
    }
    html.push_str("</div>");

    // 主要 So What 3 件
    if !top_findings.is_empty() {
        html.push_str(r#"<h2>主要な発見（上位 3 件）</h2><ol class="findings-list">"#);
        for ins in &top_findings {
            let cls = match ins.severity {
                Severity::Critical => "critical",
                Severity::Warning => "warning",
                Severity::Info => "info",
                Severity::Positive => "positive",
            };
            write!(
                html,
                r#"<li class="finding-{cls}"><strong>{title}</strong> — {body}</li>"#,
                cls = cls,
                title = escape_html(&ins.title),
                body = escape_html(&ins.body),
            )
            .unwrap();
        }
        html.push_str("</ol>");
    }

    // 主要アクション
    if !action_proposals.is_empty() {
        html.push_str(r#"<h2>推奨アクション（最大 5 件）</h2><ol class="findings-list">"#);
        for a in &action_proposals {
            write!(
                html,
                r#"<li><strong>{title}</strong> — {body}</li>"#,
                title = escape_html(&a.title),
                body = escape_html(&a.body),
            )
            .unwrap();
        }
        html.push_str("</ol>");
    }
    html.push_str("</section>");

    // === 第 1 章: 採用診断 KPI ===
    html.push_str(r#"<div class="page-break"></div>"#);
    html.push_str(r#"<section class="report-page"><h1>第 1 章 採用診断（HW 求人 KPI）</h1>"#);
    write!(
        html,
        r#"<div class="subtitle">{loc} ／ 産業: {ind}</div>"#,
        loc = escape_html(&location),
        ind = escape_html(industry_label),
    )
    .unwrap();
    html.push_str(
        r#"<table class="flow-table">
        <thead><tr><th>指標</th><th>値</th><th>解釈</th></tr></thead>
        <tbody>"#,
    );
    write!(
        html,
        r#"<tr><td>求人件数</td><td>{} 件</td><td>当該地域・産業の HW 掲載求人合計</td></tr>"#,
        format_number(kpis.posting_count)
    )
    .unwrap();
    write!(
        html,
        r#"<tr><td>正社員比率</td><td>{:.1}%</td><td>正社員区分の求人割合（HW 掲載分）</td></tr>"#,
        kpis.seishain_ratio * 100.0
    )
    .unwrap();
    if kpis.salary_min_avg > 0.0 {
        write!(
            html,
            r#"<tr><td>月給下限平均</td><td>{} 円</td><td>正社員 月給制求人の salary_min 平均</td></tr>"#,
            format_number(kpis.salary_min_avg.round() as i64)
        )
        .unwrap();
    }
    if kpis.vacancy_rate > 0.0 {
        write!(
            html,
            r#"<tr><td>欠員補充率</td><td>{:.1}%</td><td>recruitment_reason に欠員/退職を含む求人比率</td></tr>"#,
            kpis.vacancy_rate * 100.0
        )
        .unwrap();
    }
    html.push_str("</tbody></table>");

    // 採用構造インサイト
    let hiring: Vec<&Insight> = insights
        .iter()
        .filter(|i| i.category == InsightCategory::HiringStructure)
        .take(5)
        .collect();
    if !hiring.is_empty() {
        html.push_str(r#"<h2>採用構造の主な示唆</h2>"#);
        for ins in &hiring {
            html.push_str(&render_insight_card(ins));
        }
    }
    html.push_str("</section>");

    // === 第 2 章: 地域カルテ KPI ===
    html.push_str(r#"<div class="page-break"></div>"#);
    html.push_str(r#"<section class="report-page"><h1>第 2 章 地域カルテ（構造指標）</h1>"#);
    write!(
        html,
        r#"<div class="subtitle">{loc} の人口・労働・福祉 KPI</div>"#,
        loc = escape_html(&location),
    )
    .unwrap();
    html.push_str(
        r#"<table class="flow-table">
        <thead><tr><th>指標</th><th>値</th><th>出典</th></tr></thead>
        <tbody>"#,
    );
    if kpis.elderly_rate > 0.0 {
        write!(
            html,
            r#"<tr><td>高齢化率（65 歳以上）</td><td>{:.1}%</td><td>e-Stat / SSDSE-A 人口ピラミッド</td></tr>"#,
            kpis.elderly_rate
        )
        .unwrap();
    } else {
        html.push_str(
            r#"<tr><td>高齢化率（65 歳以上）</td><td>未投入</td><td>e-Stat / SSDSE-A 人口ピラミッド</td></tr>"#,
        );
    }
    if kpis.unemployment_rate > 0.0 {
        write!(
            html,
            r#"<tr><td>失業率</td><td>{:.2}%</td><td>SSDSE-A 労働力</td></tr>"#,
            kpis.unemployment_rate
        )
        .unwrap();
    } else {
        html.push_str(r#"<tr><td>失業率</td><td>未投入</td><td>SSDSE-A 労働力</td></tr>"#);
    }
    html.push_str("</tbody></table>");

    // 地域比較インサイト
    let regional: Vec<&Insight> = insights
        .iter()
        .filter(|i| i.category == InsightCategory::RegionalCompare)
        .take(5)
        .collect();
    if !regional.is_empty() {
        html.push_str(r#"<h2>地域比較の主な示唆</h2>"#);
        for ins in &regional {
            html.push_str(&render_insight_card(ins));
        }
    } else {
        html.push_str(
            r#"<p class="narrative">対象地域の比較インサイトは未生成です（データ未投入の可能性）。</p>"#,
        );
    }
    html.push_str("</section>");

    // === 第 3 章: So What 示唆（Forecast / Structural / Regional 全カテゴリ） ===
    html.push_str(r#"<div class="page-break"></div>"#);
    html.push_str(r#"<section class="report-page"><h1>第 3 章 So What 示唆</h1>"#);
    let categories = [
        (InsightCategory::Forecast, "将来予測"),
        (InsightCategory::StructuralContext, "構造的課題"),
    ];
    let mut emitted_any = false;
    for (cat, label) in &categories {
        let cat_insights: Vec<&Insight> = insights
            .iter()
            .filter(|i| i.category == *cat)
            .take(5)
            .collect();
        if cat_insights.is_empty() {
            continue;
        }
        emitted_any = true;
        write!(html, r#"<h2>{}</h2>"#, escape_html(label)).unwrap();
        for ins in &cat_insights {
            html.push_str(&render_insight_card(ins));
        }
    }
    if !emitted_any {
        html.push_str(
            r#"<p class="narrative">対象地域の So What 示唆は未生成です（データ未投入の可能性）。</p>"#,
        );
    }
    html.push_str("</section>");

    // === 第 4 章: 推奨アクション ===
    if !action_proposals.is_empty() {
        html.push_str(r#"<div class="page-break"></div>"#);
        html.push_str(r#"<section class="report-page"><h1>第 4 章 推奨アクション</h1>"#);
        for a in &action_proposals {
            html.push_str(&render_insight_card(a));
        }
        html.push_str("</section>");
    }

    // === 巻末: 出典・免責 ===
    html.push_str(r#"<div class="page-break"></div>"#);
    html.push_str(r#"<section class="report-page"><h1>巻末</h1>"#);
    html.push_str(
        r#"<h2>データソース</h2>
        <ul class="findings-list">
            <li>ハローワーク求人データ（hellowork.db）— 求人件数・給与・雇用形態</li>
            <li>e-Stat / SSDSE-A — 人口・労働力・高齢化率・教育/医療施設</li>
            <li>Agoop 人流データ — 昼夜人口比率・流入流出</li>
        </ul>
        <h2>データスコープ</h2>
        <p class="narrative">
            本レポートに記載される求人指標は、ハローワークに掲載された求人のみを対象としています。
            民間求人サイト（Indeed、マイナビ、リクナビ等）に掲載される求人は含まれません。
            したがって、地域全体の採用市場全容ではなく、HW 掲載求人を通じた採用活動に限定された分析結果です。
        </p>
        <h2>免責事項</h2>
        <p class="narrative">
            本レポートに含まれる集計値・指標は、データ間の傾向を示すものであり、
            因果関係を主張するものではありません。意思決定に当たっては、
            外部要因（業界動向・経営戦略・労働関連法令等）も併せてご検討ください。
        </p>
        <h2>更新履歴</h2>
        <p class="narrative">"#,
    );
    write!(
        html,
        r#"生成日時: {today} ／ ペルソナ A 統合レポート v1.0"#,
        today = escape_html(&today)
    )
    .unwrap();
    html.push_str(r#"</p></section>"#);

    html.push_str("</body></html>");
    html
}

fn write_kpi_card(html: &mut String, label: &str, value: &str, unit: &str) {
    write!(
        html,
        r#"<div class="kpi-card">
            <div class="kpi-value">{val} <span style="font-size:11px;color:#888">{unit}</span></div>
            <div class="kpi-label">{label}</div>
        </div>"#,
        val = escape_html(value),
        unit = escape_html(unit),
        label = escape_html(label),
    )
    .unwrap();
}

fn render_insight_card(ins: &Insight) -> String {
    let cls = match ins.severity {
        Severity::Critical => "critical",
        Severity::Warning => "warning",
        Severity::Info => "info",
        Severity::Positive => "positive",
    };
    // Insight には body のみ。Evidence を 1-2 件添えて根拠表示。
    let evidence_html = if !ins.evidence.is_empty() {
        let parts: Vec<String> = ins
            .evidence
            .iter()
            .take(2)
            .map(|e| {
                format!(
                    r#"<span style="font-size:9px;color:#888">{}: {} {}{}</span>"#,
                    escape_html(&e.metric),
                    e.value,
                    escape_html(&e.unit),
                    if e.context.is_empty() {
                        String::new()
                    } else {
                        format!("（{}）", escape_html(&e.context))
                    }
                )
            })
            .collect();
        format!(
            r#"<div class="evidence" style="margin-top:6px">{}</div>"#,
            parts.join(" / ")
        )
    } else {
        String::new()
    };
    format!(
        r#"<div class="insight-card {cls}">
            <div class="insight-title">{title}</div>
            <div class="insight-body">{body}</div>
            {evidence}
        </div>"#,
        cls = cls,
        title = escape_html(&ins.title),
        body = escape_html(&ins.body),
        evidence = evidence_html,
    )
}

fn render_no_db_page(pref: &str, muni: &str, industry_label: &str) -> String {
    let location = if muni.is_empty() {
        if pref.is_empty() {
            "全国".to_string()
        } else {
            pref.to_string()
        }
    } else {
        format!("{} {}", pref, muni)
    };
    format!(
        r#"<!DOCTYPE html>
<html lang="ja">
<head>
<meta charset="UTF-8">
<title>統合レポート - データ未接続</title>
<style>body{{font-family:"Yu Gothic","Meiryo",sans-serif;padding:40px;color:#333}}h1{{color:#dc2626}}</style>
</head>
<body>
<h1>統合レポート: データベース未接続</h1>
<p>地域: {loc} ／ 産業: {ind}</p>
<p>hellowork.db にアクセスできないため、レポートを生成できません。</p>
<p>システム管理者へお問い合わせください。</p>
</body>
</html>"#,
        loc = escape_html(&location),
        ind = escape_html(industry_label),
    )
}

/// 統合レポート用 CSS（A4 縦印刷最適化）
const STYLE_HEAD: &str = r#"<!DOCTYPE html>
<html lang="ja">
<head>
<meta charset="UTF-8">
<style>
* { margin: 0; padding: 0; box-sizing: border-box; }
body { font-family: "Yu Gothic","Meiryo","Hiragino Sans",sans-serif; font-size: 11px; color: #1a1a2e; background: #fff; padding: 20px; }
h1 { font-size: 22px; color: #1a5276; border-bottom: 3px solid #1a5276; padding-bottom: 8px; margin-bottom: 8px; }
h2 { font-size: 14px; color: #2c3e50; margin: 14px 0 8px 0; border-bottom: 1px solid #ddd; padding-bottom: 4px; }
.subtitle { color: #666; font-size: 12px; margin-bottom: 12px; }
.cover-page {
  min-height: 240mm;
  display: flex; flex-direction: column;
  justify-content: center; align-items: center;
  text-align: center; padding: 20mm 15mm;
  page-break-after: always;
  border: 1px solid #e0e0e0; border-radius: 8px;
  background: linear-gradient(135deg, #f5f9ff 0%, #fff 100%);
  position: relative; margin-bottom: 16px;
}
.cover-logo, .cover-logo-img {
  width: 200px; height: 70px;
  display: flex; align-items: center; justify-content: center;
  color: #888; font-size: 14px;
  border: 1px dashed #ccc; border-radius: 6px;
  margin-bottom: 30px; object-fit: contain;
}
.cover-title { font-size: 30px; font-weight: 700; color: #1a5276; margin: 8px 0 8px; letter-spacing: 0.04em; }
.cover-sub { font-size: 16px; color: #1a1a2e; margin-bottom: 16px; }
.cover-confidential { margin-top: 40px; font-size: 11px; color: #555; border: 1px solid #f0f0f0; background: #fafafa; padding: 14px 20px; border-radius: 6px; line-height: 1.7; max-width: 80%; }
.cover-footer-cov { position: absolute; bottom: 10mm; left: 0; right: 0; font-size: 10px; color: #999; }
.kpi-grid { display: grid; grid-template-columns: repeat(3, 1fr); gap: 10px; margin: 12px 0 16px; }
.kpi-card { border: 1px solid #e0e0e0; border-radius: 6px; padding: 14px; text-align: center; position: relative; overflow: hidden; background: #fff; box-shadow: 0 1px 3px rgba(0,0,0,0.06); }
.kpi-card::before { content: ''; position: absolute; top: 0; left: 0; right: 0; height: 3px; background: linear-gradient(90deg, #1a5276, #2874a6); }
.kpi-value { font-size: 20px; font-weight: bold; color: #1a5276; }
.kpi-label { font-size: 10px; color: #666; margin-top: 4px; }
.findings-list { margin: 8px 0 12px; padding-left: 22px; }
.findings-list li { font-size: 11px; line-height: 1.8; color: #333; margin-bottom: 4px; }
.findings-list li.finding-critical { color: #dc2626; }
.findings-list li.finding-warning { color: #b45309; }
.flow-table { width: 100%; border-collapse: collapse; font-size: 11px; margin: 8px 0 12px; }
.flow-table th { background: #2c3e50; color: #fff; padding: 6px 8px; text-align: left; font-size: 10px; }
.flow-table td { padding: 6px 8px; border-bottom: 1px solid #eee; }
.flow-table tr:nth-child(even) { background: #f8f9fa; }
.insight-card { padding: 10px 14px; margin-bottom: 8px; border-radius: 0 4px 4px 0; }
.insight-card.critical { border-left: 6px solid #ef4444; background: #fef2f2; }
.insight-card.warning { border-left: 4px solid #f59e0b; background: #fffbeb; }
.insight-card.info { border-left: 2px solid #93c5fd; background: #f8fafc; }
.insight-card.positive { border-left: 2px solid #6ee7b7; background: #f0fdf4; }
.insight-title { font-weight: bold; margin-bottom: 4px; font-size: 12px; }
.insight-card.critical .insight-title { color: #dc2626; }
.insight-card.warning .insight-title { color: #92400e; }
.insight-card.info .insight-title { color: #1e40af; }
.insight-card.positive .insight-title { color: #065f46; }
.insight-body { font-size: 10px; color: #555; line-height: 1.6; }
.so-what { font-size: 10px; color: #1a5276; font-weight: bold; margin-top: 4px; }
.narrative { background: #f8f9fa; border-left: 3px solid #1a5276; padding: 10px 14px; margin: 8px 0 12px; font-size: 11px; line-height: 1.7; color: #444; }
.report-page { page-break-after: always; }
.report-page:last-child { page-break-after: auto; }
.page-break { page-break-before: always; }
.print-btn { position: fixed; top: 10px; right: 10px; padding: 8px 16px; background: #2563eb; color: #fff; border: none; border-radius: 6px; cursor: pointer; font-size: 12px; z-index: 100; }
.print-btn:hover { background: #1d4ed8; }
@page { size: A4 portrait; margin: 14mm 12mm 16mm 12mm; }
@media print {
    -webkit-print-color-adjust: exact !important;
    print-color-adjust: exact !important;
    .print-btn, .no-print { display: none !important; }
    body { padding: 0; }
    .cover-page { page-break-after: always; min-height: 90vh; }
    .insight-card, .kpi-card, .flow-table { page-break-inside: avoid; break-inside: avoid; }
    h1, h2 { page-break-after: avoid; break-after: avoid; }
    .kpi-grid { grid-template-columns: repeat(3, 1fr); }
}
</style>
"#;

#[cfg(test)]
mod inline_tests {
    use super::*;

    #[test]
    fn render_no_db_page_escapes_input() {
        let html = render_no_db_page("<script>", "muni", "<x>");
        assert!(!html.contains("<script>"));
        assert!(html.contains("&lt;script&gt;"));
    }

    #[test]
    fn write_kpi_card_renders_value() {
        let mut s = String::new();
        write_kpi_card(&mut s, "求人件数", "1,234", "件");
        assert!(s.contains("1,234"));
        assert!(s.contains("求人件数"));
        assert!(s.contains("件"));
    }
}
