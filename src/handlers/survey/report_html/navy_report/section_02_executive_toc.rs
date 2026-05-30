//! Section 02 - TOC (目次) + Section 01 - Executive Summary
//!
//! navy_report.rs の分割 (A1 Commit 3 / β Section Team / 2026-05-29) で抽出。
//!
//! 元 `navy_report/mod.rs` L48-L444 の以下を物理コピー:
//! - `render_navy_toc`     (TOC ページ, 公開 API)
//! - `push_toc_item`       (TOC ヘルパー, module-private)
//! - `render_navy_executive` (Executive Summary ページ, 公開 API)
//! - `build_findings`      (Finding 1-7 生成, module-private)
//!
//! API 表面:
//! - `pub(crate) fn render_navy_toc` (Commit 2 パターン踏襲: `pub(super)` は階層不足で E0364)
//! - `pub(crate) fn render_navy_executive` (同上)
//!
//! 内部 helper (`push_toc_item` / `build_findings`) は本ファイル内のみで使用される
//! ため、元コードと同じく `fn` (module-private) を維持。`navy_report` モジュール
//! 外への露出はない。
//!
//! common 経由参照: `push_page_head` / `push_kpi` / `severity_label` /
//! `compute_skew_severity` は `super::common::*` から直接 import。
//! (mod.rs 側の `pub(super) use common::*;` 再エクスポートには依存しない。)

#![allow(dead_code)]

// パス解析 (現在位置: survey::report_html::navy_report::section_02_executive_toc):
//   super              = navy_report
//   super::super       = report_html
//   super::super::super = survey
//   super::super::super::super = handlers
use super::super::super::super::helpers::{escape_html, format_number};
use super::super::super::super::insight::fetch::InsightContext;
use super::super::super::aggregator::{EmpTypeSalary, SurveyAggregation};
use super::super::super::job_seeker::JobSeekerAnalysis;
use super::super::salary_summary;
use super::super::ReportVariant;
use super::common::{compute_skew_severity, push_kpi, push_page_head, severity_label};

// ============================================================
// TOC
// ============================================================

pub(crate) fn render_navy_toc(html: &mut String, variant: ReportVariant) {
    let section_02 = match variant {
        ReportVariant::Full => "地域 × 求人媒体データ連携",
        _ => "地域データ補強",
    };
    html.push_str("<section class=\"page-navy toc-page\" role=\"region\" aria-label=\"目次\">\n");
    push_page_head(
        html,
        "TABLE OF CONTENTS",
        "目次",
        "本レポートは A4 縦印刷を前提に構成しています",
    );
    html.push_str("<div class=\"toc-grid\">\n");

    html.push_str("<div class=\"toc-col\">\n");
    for (no, name) in [
        ("01", "Executive Summary"),
        ("02", section_02),
        ("03", "給与分布 統計"),
        ("04", "採用市場 逼迫度"),
    ] {
        push_toc_item(html, no, name);
    }
    html.push_str("</div>\n");

    html.push_str("<div class=\"toc-col\">\n");
    for (no, name) in [
        ("05", "地域企業構造"),
        ("06", "人材デモグラフィック"),
        ("07", "最低賃金・ライフスタイル"),
        ("08", "注記・出典・免責"),
    ] {
        push_toc_item(html, no, name);
    }
    html.push_str("</div>\n");

    html.push_str("</div>\n"); // /toc-grid

    html.push_str(
        "<div class=\"toc-foot\">\
         <div class=\"tf-block\">\
         <div class=\"tf-label\">SEVERITY 凡例</div>\
         <div class=\"legend-row\">\
         <span class=\"legend-chip pos\">POSITIVE</span>\
         <span class=\"legend-chip neu\">NEUTRAL</span>\
         <span class=\"legend-chip warn\">WARN</span>\
         <span class=\"legend-chip neg\">NEGATIVE</span>\
         </div></div>\
         <div class=\"tf-block\">\
         <div class=\"tf-label\">凡例の読み方</div>\
         <p>本レポート内の指標は上記 4 段階で評価しています。NEGATIVE / WARN は\
         「改善検討」の対象、POSITIVE は「強み」として認識してください。</p>\
         </div></div>\n",
    );
    html.push_str("</section>\n");
}

fn push_toc_item(html: &mut String, no: &str, name: &str) {
    html.push_str(&format!(
        "<div class=\"toc-item\">\
         <span class=\"t-no\">{}</span>\
         <span class=\"t-name\">{}</span>\
         <span class=\"t-pg\">—</span>\
         </div>\n",
        escape_html(no),
        escape_html(name)
    ));
}

// ============================================================
// Executive Summary
// ============================================================

pub(crate) fn render_navy_executive(
    html: &mut String,
    agg: &SurveyAggregation,
    _seeker: &JobSeekerAnalysis,
    by_emp_type_salary: &[EmpTypeSalary],
    hw_context: Option<&InsightContext>,
    variant: ReportVariant,
    target_region: &str,
) {
    html.push_str("<section class=\"page-navy navy-exec\" role=\"region\" aria-labelledby=\"navy-exec-title\">\n");
    push_page_head(
        html,
        "SECTION 01",
        "Executive Summary",
        "3 分で読み切れる全体要旨と優先アクション",
    );
    html.push_str(&format!(
        "<h2 id=\"navy-exec-title\" class=\"sr-only\" style=\"position:absolute;left:-9999px;\">Executive Summary</h2>\n"
    ));

    // -- exec-headline (引用調 + 1 段落要旨)
    let total = agg.total_count;
    let salary_parse_pct = (agg.salary_parse_rate * 100.0).round() as i64;
    let new_pct = if total > 0 {
        (agg.new_count as f64 / total as f64 * 100.0).round() as i64
    } else {
        0
    };
    let dominant_emp = agg
        .by_employment_type
        .first()
        .map(|(name, c)| {
            let pct = if total > 0 {
                *c as f64 / total as f64 * 100.0
            } else {
                0.0
            };
            format!("{} ({:.0}%)", name, pct)
        })
        .unwrap_or_else(|| "—".to_string());

    // 2026-05-14: 「給与解析率」表記は撤去 (Section 03 で解析できた件数のみ提示する方針)。
    let _ = salary_parse_pct;

    // 2026-05-14: 選択地域 (target_region) と CSV 内最多地域 (dominant) が異なる場合、
    //   「御社の地域で検索したが、結果として隣接の地域の方が多い → 隣地域への応募流入 /
    //   流出が多い」観点で 1 文補足する。県境スクレイピングや広域募集では頻発する。
    //   選択地域 と dominant が一致する場合は補足なし。
    let region_divergence_note: String = {
        // dominant_pref を優先、無ければ by_prefecture[0]
        let dominant_pref_owned = agg
            .dominant_prefecture
            .clone()
            .or_else(|| agg.by_prefecture.first().map(|(p, _)| p.clone()));
        // target_region と CSV 最多が文字列含み的に一致しない場合のみ補足
        let pref_in_target = dominant_pref_owned
            .as_deref()
            .map(|d| !d.is_empty() && !target_region.contains(d))
            .unwrap_or(false);
        if pref_in_target {
            let top_pref = dominant_pref_owned.as_deref().unwrap_or("");
            let top_count = agg
                .by_prefecture
                .iter()
                .find(|(p, _)| p == top_pref)
                .map(|(_, c)| *c)
                .unwrap_or(0);
            format!(
                " ただし CSV 内に最も多く出現したのは <strong>{}</strong> ({} 件) で、対象地域より件数が多くなっています。\
                 県境スクレイピングなどで隣地域の求人/応募流入が多いケースが想定されます。",
                escape_html(top_pref),
                format_number(top_count as i64)
            )
        } else {
            String::new()
        }
    };

    // Phase 2-A (2026-05-29): 時給モードの場合は lede に「時給ベース」を明示
    //   ユーザーが「これは時給対象のレポート」と即座に認識できるよう、地域名直後に
    //   付加する。月給モードは旧文言を維持。
    let region_prefix = if agg.is_hourly {
        format!(
            "{} の <strong>時給ベース求人</strong>",
            escape_html(target_region)
        )
    } else {
        escape_html(target_region).to_string()
    };
    let headline_body = format!(
        "本レポートは <strong>{}</strong> を対象に、サンプル <strong>{} 件</strong> を分析した結果です。\
         主要雇用形態は <strong>{}</strong>、新着比率 <strong>{}%</strong>。{}\
         本ページでは <strong>KPI</strong> と <strong>Findings</strong> を提示し、末尾の <strong>SO WHAT</strong> で取るべき方針を集約します。",
        region_prefix,
        format_number(total as i64),
        escape_html(&dominant_emp),
        new_pct,
        region_divergence_note,
    );
    html.push_str(&format!(
        "<div class=\"exec-headline\">\
         <div class=\"eh-quote\" aria-hidden=\"true\">&ldquo;</div>\
         <p>{}</p>\
         </div>\n",
        headline_body
    ));

    // -- kpi-row (5 cell)
    let k1 = format!("{}", format_number(total as i64));
    let k1_dot = if total >= 30 {
        "pos"
    } else if total > 0 {
        "warn"
    } else {
        "neg"
    };
    let k1_foot = if total >= 30 {
        "n>=30 で実務判断に参照可"
    } else if total > 0 {
        "n が少なく傾向参照のみ"
    } else {
        "サンプルなし"
    };

    let k3_name = agg
        .by_employment_type
        .first()
        .map(|(n, _)| n.clone())
        .unwrap_or_default();
    let k3_pct = agg
        .by_employment_type
        .first()
        .map(|(_, c)| {
            if total > 0 {
                *c as f64 / total as f64 * 100.0
            } else {
                0.0
            }
        })
        .unwrap_or(0.0);
    let k3_value = if k3_name.is_empty() {
        "—".to_string()
    } else {
        k3_name.clone()
    };
    let k3_dot = if k3_pct >= 85.0 { "warn" } else { "neu" };
    let k3_foot = if k3_pct > 0.0 {
        format!("構成比 {:.0}%", k3_pct)
    } else {
        "—".to_string()
    };

    let salary_h = salary_summary::SalaryHeadline::from_aggregation(agg);
    let cover_hl = salary_h.cover_highlight_text();
    let _ = by_emp_type_salary;
    let _ = hw_context;
    let _ = variant;

    let k5_value = format!("{}", new_pct);
    let k5_dot = if total == 0 {
        "neu"
    } else if new_pct >= 15 {
        "pos"
    } else if new_pct < 5 {
        "warn"
    } else {
        "neu"
    };
    let k5_foot = "直近 30 日の新着求人比率";

    let k6_value = format!("{}", salary_parse_pct);
    let k6_dot = if salary_parse_pct >= 85 {
        "pos"
    } else if salary_parse_pct >= 60 {
        "warn"
    } else {
        "neg"
    };
    let k6_foot = "給与文字列から数値抽出に成功した比率";

    // 2026-05-14: 給与解析率 KPI 撤去。kpi-row → kpi-row-4 で 4 カードレイアウト。
    let _ = (k6_value, k6_dot, k6_foot);
    html.push_str("<div class=\"kpi-row kpi-row-4\">\n");
    push_kpi(html, "サンプル件数", &k1, "件", k1_dot, k1_foot, false);
    // 2026-05-14: 主要地域 = ユーザー選択地域 (handlers.rs:482 で確定済)。
    //   フッタは「件数最多」だと CSV 分布最多と混同するので「対象地域」に変更。
    //   CSV 分布最多が選択地域と異なる場合は別途 SO WHAT / 注記で扱う。
    push_kpi(
        html,
        "主要地域",
        target_region,
        "",
        "neu",
        "対象地域",
        false,
    );
    push_kpi(html, "主要雇用形態", &k3_value, "", k3_dot, &k3_foot, false);
    push_kpi(
        html,
        cover_hl.label.as_str(),
        cover_hl.value_text.as_str(),
        cover_hl.unit.as_str(),
        "neu",
        "本レポートの代表給与値",
        true,
    );
    html.push_str("</div>\n");

    // -- findings (KEY FINDINGS, 最大 7 件)
    // P1-6 (2026-05-28): hw_context 経由で業界/職種偏り Finding 2 件を追加可能。
    // hw_context=None (CSV 単体モード等) の場合は従来通り 4 件のみ。
    let findings = build_findings(agg, total, k3_pct, new_pct, salary_parse_pct, hw_context);
    // P1-6: hw_context 有無で件数 4 / 6 と動的に変わるため、固定文言ではなく実数を表示。
    // ※既存 findings は (1)サンプル件数 (2)雇用形態 (3)新着比率 (5)地域カバレッジ の 4 件
    //   (旧 #4 給与解析率は 2026-05-14 撤去済み)。
    let findings_title = format!("優先確認 {} ポイント", findings.len());
    html.push_str(&format!(
        "<div class=\"findings\">\n\
         <div class=\"findings-head\">\
         <div class=\"fh-no\">KEY FINDINGS</div>\
         <div class=\"fh-title\">{}</div>\
         </div>\n",
        escape_html(&findings_title),
    ));
    html.push_str("<ol class=\"findings-list\">\n");
    for (i, (sev_tag, title, body, refer)) in findings.iter().enumerate() {
        let no = format!("{:02}", i + 1);
        html.push_str(&format!(
            "<li>\
             <div class=\"f-no\">{}</div>\
             <div class=\"f-body\">\
             <div class=\"f-title\"><span class=\"tag tag-{}\">{}</span> &nbsp;{}</div>\
             <p>{}</p>\
             </div>\
             <div class=\"f-ref\">{}</div>\
             </li>\n",
            no,
            sev_tag,
            severity_label(sev_tag),
            escape_html(title),
            body,
            escape_html(refer),
        ));
    }
    html.push_str("</ol>\n</div>\n");

    // -- so-what
    // 2026-05-14: 給与解析率の言及を撤去。
    let new_pct_label = if total > 0 {
        format!("{}%", new_pct)
    } else {
        "—".to_string()
    };
    let so_what_body = format!(
        "サンプル件数 <strong>n={}</strong> / 新着比率 <strong>{}</strong> を踏まえ、\
         <strong>給与水準と訴求軸の再点検</strong> を起点に、<strong>不足セグメント (n<30) の補完取得</strong> を併走させてください。\
         以降のセクションで具体的な分布・市場逼迫度・地域企業構造を確認します。",
        format_number(total as i64),
        new_pct_label,
    );
    html.push_str(&format!(
        "<div class=\"so-what\">\
         <div class=\"sw-label\">SO WHAT</div>\
         <div class=\"sw-body\">{}</div>\
         </div>\n",
        so_what_body
    ));

    html.push_str("</section>\n");
}

fn build_findings(
    agg: &SurveyAggregation,
    total: usize,
    dom_emp_pct: f64,
    new_pct: i64,
    salary_parse_pct: i64,
    hw_context: Option<&InsightContext>,
) -> Vec<(&'static str, String, String, String)> {
    let mut v: Vec<(&'static str, String, String, String)> = Vec::new();

    // 1) サンプル件数の信頼区間
    let (sev, body) = if total == 0 {
        (
            "neg",
            "サンプル 0 件のため統計値を提示できません。CSV 取得範囲の見直しが必要です。"
                .to_string(),
        )
    } else if total < 30 {
        ("warn", format!("サンプル <strong>n={}</strong> は統計的信頼性が低く、外れ値の影響が大きい状態です。傾向参照に留め、母集団の追加取得を推奨します。", total))
    } else {
        ("pos", format!("サンプル <strong>n={}</strong> は実務判断に十分な水準です。後続セクションの統計値はそのまま参照できます。", total))
    };
    v.push((
        sev,
        "サンプル件数".to_string(),
        body,
        "§2 統計信頼性".to_string(),
    ));

    // 2) 主要雇用形態の偏り
    let (sev, body) = if dom_emp_pct >= 85.0 {
        ("warn", format!("主要雇用形態が <strong>{:.0}%</strong> を占め、特定雇用形態への<strong>構成集約</strong>が見られます。他雇用形態の追加分析が有効です。", dom_emp_pct))
    } else if dom_emp_pct >= 70.0 {
        ("neu", format!("主要雇用形態の構成比は <strong>{:.0}%</strong>。やや偏り気味ですが、他雇用形態への展開余地もある水準です。", dom_emp_pct))
    } else {
        (
            "pos",
            format!(
                "主要雇用形態の構成比は <strong>{:.0}%</strong> で、バランスの取れた構成です。",
                dom_emp_pct
            ),
        )
    };
    v.push((
        sev,
        "雇用形態構成".to_string(),
        body,
        "§3 雇用形態分析".to_string(),
    ));

    // 3) 新着比率
    let (sev, body) = if total == 0 {
        ("neu", "サンプルなしのため新着比率の評価不能。".to_string())
    } else if new_pct >= 15 {
        (
            "pos",
            format!(
                "直近 30 日の新着比率 <strong>{}%</strong> は活発な採用活動を示唆します。",
                new_pct
            ),
        )
    } else if new_pct < 5 {
        ("warn", format!("新着比率 <strong>{}%</strong> は<strong>低位水準</strong>で、人材定着または求人更新サイクル長期化の可能性があります。", new_pct))
    } else {
        (
            "neu",
            format!(
                "新着比率は <strong>{}%</strong>。標準的な水準です。",
                new_pct
            ),
        )
    };
    v.push((sev, "新着比率".to_string(), body, "§3 求人動向".to_string()));

    // 2026-05-14: 「給与解析率」finding 撤去 (内部運用情報のため)。
    let _ = salary_parse_pct;

    // 5) 地域カバレッジ
    let pref_count = agg.by_prefecture.len();
    let (sev, body) = if pref_count == 0 {
        (
            "neu",
            "地域情報の抽出ができませんでした。CSV のアクセス列を確認してください。".to_string(),
        )
    } else if pref_count == 1 {
        ("neu", format!("カバー都道府県は <strong>1</strong> 都道府県。単一エリアの深掘り分析として参照可能です。"))
    } else {
        ("neu", format!("カバー都道府県は <strong>{}</strong>。複数地域比較は本レポート後半セクションで詳述します。", pref_count))
    };
    v.push((
        sev,
        "地域カバレッジ".to_string(),
        body,
        "§5 地域分析".to_string(),
    ));

    // ============================================================
    // P1-6 (2026-05-28): 極端な分類偏り警告
    // ------------------------------------------------------------
    // HW 求人 (postings) の業界/職種分布が単一カテゴリに集中している場合、
    // データ代表性 (本レポート全体の解釈) に影響する。CSV 単体モードや
    // HW context が無い場合は Finding 06/07 をスキップ (Finding 数は <=5)。
    //
    // 閾値 (compute_skew_severity 参照):
    //   - max_share > 85% → WARN (顕著)
    //   - max_share > 70% → NEU  (偏りあり、要注意)
    //   - それ以下        → POS  (バランス良好)
    //   - empty / total<=0 → NEU "データなし"
    //
    // 用語ガード (DISPLAY_SPEC v1.0 §2): 「件数」「占有率」のみ使用。
    // 「人数」「target_count」「推定母集団」等は禁止。
    // ============================================================
    if let Some(ctx) = hw_context {
        // Finding 06: 業界 (12 大分類) の偏り
        let (sev_ind, body_ind) = compute_skew_severity(&ctx.hw_industry_counts, "産業大分類");
        v.push((
            sev_ind,
            "産業構成 偏り".to_string(),
            escape_html(&body_ind),
            "§5 産業構成".to_string(),
        ));

        // Finding 07: 職種 (job_type) の偏り
        let (sev_job, body_job) = compute_skew_severity(&ctx.hw_job_type_counts, "職種");
        v.push((
            sev_job,
            "職種構成 偏り".to_string(),
            escape_html(&body_job),
            "§4 採用市場".to_string(),
        ));
    }

    v
}
