use super::fetch::CompanyContext;
use crate::handlers::helpers::{escape_html, format_number, get_i64, get_str, truncate_str, Row};

use std::fmt::Write as _;
/// 検索ページ（タブのシェル）
pub fn render_search_page() -> String {
    r##"<div class="space-y-6">
    <div class="flex items-center justify-between">
        <h2 class="text-xl font-bold text-white">🔎 企業検索
            <span class="text-blue-400 text-base font-normal">企業データ × ハローワーク市場データ</span>
        </h2>
        <div class="flex gap-2">
            <button class="text-xs bg-slate-700 hover:bg-slate-600 text-slate-300 px-3 py-1 rounded transition-colors" onclick="window.print()">🖨 印刷</button>
            <button class="text-xs bg-slate-700 hover:bg-slate-600 text-slate-300 px-3 py-1 rounded transition-colors" onclick="if(typeof exportAsImage==='function')exportAsImage()">📷 画像保存</button>
        </div>
    </div>
    <div class="stat-card">
        <div class="relative">
            <label class="block text-sm text-slate-400 mb-2">企業名で検索（236,000社）</label>
            <input type="text" id="company-search-input"
                   name="q"
                   placeholder="例: トヨタ、日本郵便、ヤマト運輸..."
                   class="w-full bg-slate-700 text-white text-lg rounded-lg px-4 py-3 border border-slate-600 focus:border-blue-500 focus:outline-none"
                   autocomplete="off"
                   hx-get="/api/company/search"
                   hx-trigger="keyup changed delay:300ms"
                   hx-target="#company-search-results"
                   hx-swap="innerHTML"
                   hx-indicator="#company-search-loading" />
            <div id="company-search-loading" class="htmx-indicator absolute right-4 top-12 text-blue-400 text-sm">検索中...</div>
        </div>
        <div id="company-search-results" class="mt-2"></div>
    </div>
    <div id="company-profile-area"></div>
    <div class="stat-card">
        <p class="text-slate-500 text-sm text-center py-4">企業を検索して選択すると、その企業の地域・業界の採用市場分析が表示されます。</p>
        <p class="text-slate-600 text-xs text-center">データソース: 企業属性 × ハローワーク求人 × 外部統計</p>
    </div>
</div>"##.to_string()
}

/// 検索結果ドロップダウン
pub fn render_search_results(results: &[Row]) -> String {
    if results.is_empty() {
        return r#"<p class="text-slate-500 text-sm py-2">該当する企業が見つかりません</p>"#
            .to_string();
    }

    let mut html = String::with_capacity(4096);
    html.push_str(r#"<div class="border border-slate-600 rounded-lg overflow-hidden max-h-96 overflow-y-auto">"#);

    for row in results {
        let corp = escape_html(&get_str(row, "corporate_number"));
        let name = escape_html(&get_str(row, "company_name"));
        let pref = escape_html(&get_str(row, "prefecture"));
        let ind = escape_html(&get_str(row, "sn_industry"));
        let ind2 = escape_html(&get_str(row, "sn_industry2"));
        let emp = get_str(row, "employee_count");
        let emp_display = if emp.is_empty() || emp == "0" {
            "-".to_string()
        } else {
            format!("{}名", emp)
        };
        let _range = escape_html(&get_str(row, "employee_range"));
        let credit = get_str(row, "credit_score");

        let sn_score = get_str(row, "salesnow_score");
        let listing = get_str(row, "listing_category");

        let credit_badge = if !credit.is_empty() && credit != "0" {
            let cls = credit_score_class(&credit);
            let mut s = String::new();
            s.push_str(" <span class=\"ml-2 text-xs px-1 rounded ");
            s.push_str(cls);
            s.push_str("\">");
            s.push_str(&format!("信用{}", credit));
            s.push_str("</span>");
            s
        } else {
            String::new()
        };

        let listing_badge = if !listing.is_empty() && listing != "-" {
            if listing.contains("上場") {
                format!(" <span class=\"text-xs px-1 rounded bg-amber-900/50 text-amber-300\">&#x1F3E2; {}</span>", escape_html(&listing))
            } else {
                String::new()
            }
        } else {
            String::new()
        };

        let sn_score_badge = if !sn_score.is_empty() && sn_score != "0" {
            format!(" <span class=\"text-xs px-1 rounded bg-purple-900/50 text-purple-300\">SN:{}</span>", sn_score)
        } else {
            String::new()
        };

        // hx-target に # が含まれるため push_str で構築
        html.push_str("<div class=\"px-4 py-3 hover:bg-slate-700 cursor-pointer border-b border-slate-700/50 transition-colors\" ");
        write!(html, "hx-get=\"/api/company/profile/{}\" ", corp).unwrap();
        html.push_str("hx-target=\"#company-profile-area\" hx-swap=\"innerHTML\" ");
        html.push_str(
            "onclick=\"document.getElementById('company-search-results').textContent=''\">",
        );
        write!(
            html,
            r##"<div class="flex justify-between items-start">
                    <div>
                        <span class="text-white font-medium">{name}</span>
                        <span class="text-slate-400 text-xs ml-2">{pref}</span>
                    </div>
                    <span class="text-slate-500 text-xs">{emp_display}</span>
                </div>
                <div class="text-xs text-slate-500 mt-0.5">
                    {ind}{ind2_sep}{ind2}{credit_badge}{listing_badge}{sn_score_badge}
                </div>
            </div>"##,
            name = name,
            pref = pref,
            emp_display = emp_display,
            ind = ind,
            ind2_sep = if !ind2.is_empty() { " / " } else { "" },
            ind2 = ind2,
            credit_badge = credit_badge,
            listing_badge = listing_badge,
            sn_score_badge = sn_score_badge,
        )
        .unwrap();
    }

    html.push_str("</div>");
    html
}

fn credit_score_class(score_str: &str) -> &'static str {
    let score: f64 = score_str.parse().unwrap_or(0.0);
    if score >= 70.0 {
        "bg-green-900/50 text-green-400"
    } else if score >= 50.0 {
        "bg-blue-900/50 text-blue-400"
    } else if score >= 30.0 {
        "bg-yellow-900/50 text-yellow-400"
    } else {
        "bg-slate-700 text-slate-400"
    }
}

/// 企業プロフィール全体（サブタブ構成）
pub fn render_company_profile(ctx: &CompanyContext) -> String {
    let mut html = String::with_capacity(64_000);

    // 戻るボタン（前のタブに戻れるように）
    html.push_str(r##"<div class="mb-3">
      <button class="text-xs text-slate-400 hover:text-white transition-colors px-2 py-1 rounded bg-slate-800 hover:bg-slate-700"
              onclick="if(window._lastTab){document.querySelector('.tab-btn[hx-get=&quot;'+window._lastTab+'&quot;]').click()}else{document.querySelector('.tab-btn[hx-get=&quot;/tab/company&quot;]').click()}">
        ← 戻る
      </button>
    </div>"##);

    // サブタブナビゲーション
    html.push_str(r##"<div class="flex gap-1 mb-4 flex-wrap" id="company-subtab-nav">
      <button class="px-3 py-1.5 text-xs rounded bg-blue-600 text-white font-medium transition-colors" onclick="showCompanyTab(0)" data-company-tab="0">サマリー</button>
      <button class="px-3 py-1.5 text-xs rounded bg-slate-700 text-slate-300 hover:bg-slate-600 transition-colors" onclick="showCompanyTab(1)" data-company-tab="1">人材フロー</button>
      <button class="px-3 py-1.5 text-xs rounded bg-slate-700 text-slate-300 hover:bg-slate-600 transition-colors" onclick="showCompanyTab(2)" data-company-tab="2">給与・競合</button>
      <button class="px-3 py-1.5 text-xs rounded bg-slate-700 text-slate-300 hover:bg-slate-600 transition-colors" onclick="showCompanyTab(3)" data-company-tab="3">求人詳細</button>
    </div>"##);

    // ===== サブタブ0: サマリー =====
    html.push_str(r#"<div class="company-tab-panel" data-company-panel="0">"#);
    render_sales_pitches(&mut html, ctx);
    render_header(&mut html, ctx);
    render_hiring_risk(&mut html, ctx);
    render_market_snapshot(&mut html, ctx);
    html.push_str("</div>");

    // ===== サブタブ1: 人材フロー =====
    html.push_str(
        r#"<div class="company-tab-panel" data-company-panel="1" style="display:none;">"#,
    );
    render_region_vs_company(&mut html, ctx);
    render_demographics(&mut html, ctx);
    render_insights(&mut html, ctx);
    html.push_str("</div>");

    // ===== サブタブ2: 給与・競合 =====
    html.push_str(
        r#"<div class="company-tab-panel" data-company-panel="2" style="display:none;">"#,
    );
    render_salary_gap_table(&mut html, ctx);
    render_salary_section(&mut html, ctx);
    render_competitor_section(&mut html, ctx);
    html.push_str("</div>");

    // ===== サブタブ3: 求人詳細 =====
    html.push_str(
        r#"<div class="company-tab-panel" data-company-panel="3" style="display:none;">"#,
    );
    render_hw_postings(&mut html, ctx);
    render_nearby_companies(&mut html, ctx);
    html.push_str("</div>");

    // レポートリンク（常に表示）
    write!(html,
        r#"<div class="text-right mt-4">
            <a href="/report/company/{}" target="_blank"
               class="inline-flex items-center gap-2 bg-blue-600 hover:bg-blue-500 text-white px-4 py-2 rounded-lg transition-colors text-sm">
                🖨 印刷用レポートを開く
            </a>
        </div>"#,
        escape_html(&ctx.corporate_number)
    ).unwrap();

    // サブタブ切り替えJavaScript
    html.push_str(r##"<script>
function showCompanyTab(idx) {
    // パネル切り替え
    document.querySelectorAll('.company-tab-panel').forEach(function(p) {
        p.style.display = p.getAttribute('data-company-panel') == idx.toString() ? '' : 'none';
    });
    // ボタンスタイル切り替え
    document.querySelectorAll('#company-subtab-nav button').forEach(function(b) {
        if (b.getAttribute('data-company-tab') == idx.toString()) {
            b.className = 'px-3 py-1.5 text-xs rounded bg-blue-600 text-white font-medium transition-colors';
        } else {
            b.className = 'px-3 py-1.5 text-xs rounded bg-slate-700 text-slate-300 hover:bg-slate-600 transition-colors';
        }
    });
    // EChartsの再初期化（非表示→表示でサイズがおかしくなる対策）
    var panel = document.querySelector('.company-tab-panel[data-company-panel="' + idx + '"]');
    if (panel && typeof initECharts === 'function') {
        setTimeout(function() { initECharts(panel); }, 50);
    }
}
</script>"##);

    html
}

fn render_header(html: &mut String, ctx: &CompanyContext) {
    let delta_arrow = if ctx.employee_delta_1y > 0.5 {
        format!(
            r#"<span class="text-green-400">+{:.1}%↑</span>"#,
            ctx.employee_delta_1y
        )
    } else if ctx.employee_delta_1y < -0.5 {
        format!(
            r#"<span class="text-red-400">{:.1}%↓</span>"#,
            ctx.employee_delta_1y
        )
    } else {
        format!(
            r#"<span class="text-slate-400">{:.1}%→</span>"#,
            ctx.employee_delta_1y
        )
    };

    // 成長シグナルバッジ
    let growth_badge = match ctx.growth_signal.as_str() {
        "StrongGrowth" => r#"<span class="ml-2 text-xs px-2 py-0.5 rounded bg-green-900/60 text-green-300 border border-green-700">急成長</span>"#.to_string(),
        "ModerateGrowth" => r#"<span class="ml-2 text-xs px-2 py-0.5 rounded bg-emerald-900/60 text-emerald-300 border border-emerald-700">成長中</span>"#.to_string(),
        "Contradictory" => r#"<span class="ml-2 text-xs px-2 py-0.5 rounded bg-amber-900/60 text-amber-300 border border-amber-700">要注意: 増員中だが欠員多</span>"#.to_string(),
        "SilentDecline" => r#"<span class="ml-2 text-xs px-2 py-0.5 rounded bg-red-900/60 text-red-300 border border-red-700">潜在リスク: 静かな縮小</span>"#.to_string(),
        "Declining" => r#"<span class="ml-2 text-xs px-2 py-0.5 rounded bg-red-900/60 text-red-300 border border-red-700">人員減少中</span>"#.to_string(),
        "Stagnant" => r#"<span class="ml-2 text-xs px-2 py-0.5 rounded bg-slate-700 text-slate-300 border border-slate-600">横ばい</span>"#.to_string(),
        _ => String::new(),
    };

    let hw_mapping = if !ctx.primary_hw_job_type.is_empty() {
        format!(
            r#"<span class="text-xs bg-blue-900/60 text-blue-300 px-2 py-0.5 rounded">HW: {}</span>"#,
            escape_html(&ctx.primary_hw_job_type)
        )
    } else {
        r#"<span class="text-xs bg-slate-700 text-slate-400 px-2 py-0.5 rounded">HWマッピングなし</span>"#.to_string()
    };

    // 上場バッジ
    let listing_badge = if !ctx.listing_category.is_empty() && ctx.listing_category != "-" {
        if ctx.listing_category.contains("上場") {
            format!(
                r#"<span class="ml-2 text-xs px-2 py-0.5 rounded bg-amber-900/60 text-amber-300 border border-amber-700">&#x1F3E2; {}</span>"#,
                escape_html(&ctx.listing_category)
            )
        } else {
            format!(
                r#"<span class="ml-2 text-xs px-2 py-0.5 rounded bg-slate-700 text-slate-400 border border-slate-600">{}</span>"#,
                escape_html(&ctx.listing_category)
            )
        }
    } else {
        String::new()
    };

    // BtoB/BtoC バッジ
    let tob_toc_badge = if !ctx.tob_toc.is_empty() && ctx.tob_toc != "-" {
        format!(
            r#"<span class="ml-1 text-xs px-1.5 py-0.5 rounded bg-indigo-900/50 text-indigo-300">{}</span>"#,
            escape_html(&ctx.tob_toc)
        )
    } else {
        String::new()
    };

    // 設立年と企業年齢
    let established_display = if !ctx.established_date.is_empty() && ctx.established_date != "-" {
        // 年を抽出して企業年齢を計算（YYYY-MM-DD or YYYY/MM/DD or YYYY）
        let year_str: String = ctx.established_date.chars().take(4).collect();
        let age = year_str.parse::<i32>().ok().map(|y| 2026 - y);
        match age {
            Some(a) if a > 0 => format!(
                r#"<span class="text-xs text-slate-400">設立 {} ({}年)</span>"#,
                escape_html(&ctx.established_date),
                a
            ),
            _ => format!(
                r#"<span class="text-xs text-slate-400">設立 {}</span>"#,
                escape_html(&ctx.established_date)
            ),
        }
    } else {
        String::new()
    };

    // 事業タグ（カンマ区切りをバッジ化）
    let tags_html = if !ctx.business_tags.is_empty() && ctx.business_tags != "-" {
        let tags: Vec<&str> = ctx
            .business_tags
            .split(',')
            .map(|t| t.trim())
            .filter(|t| !t.is_empty())
            .take(5) // 最大5つ表示
            .collect();
        if tags.is_empty() {
            String::new()
        } else {
            let mut s = String::from(r#"<div class="flex flex-wrap gap-1 mt-1">"#);
            for tag in &tags {
                s.push_str(&format!(
                    r#"<span class="text-xs px-1.5 py-0.5 rounded bg-slate-700/80 text-slate-300 border border-slate-600">{}</span>"#,
                    escape_html(tag)
                ));
            }
            s.push_str("</div>");
            s
        }
    } else {
        String::new()
    };

    // デルタトレンド: EChartsバーチャート（従業員数増減率）
    let deltas: [(&str, f64); 5] = [
        ("1ヶ月", ctx.employee_delta_1m),
        ("3ヶ月", ctx.employee_delta_3m),
        ("6ヶ月", ctx.employee_delta_6m),
        ("1年", ctx.employee_delta_1y),
        ("2年", ctx.employee_delta_2y),
    ];
    let has_any_delta = deltas.iter().any(|(_, v)| v.abs() > 0.01);
    let delta_trend = if has_any_delta {
        // aria-label用のフォールバックテキスト
        let aria_parts: Vec<String> = deltas
            .iter()
            .map(|(label, val)| {
                if val.abs() < 0.01 {
                    format!("{} -", label)
                } else {
                    format!("{} {:+.1}%", label, val)
                }
            })
            .collect();
        let aria_label = format!("従業員数推移: {}", aria_parts.join(", "));

        // 各バーの色: 正=緑、負=赤、ゼロ=グレー
        let bar_color = |v: f64| -> &str {
            if v.abs() < 0.01 {
                "\\u00239ca3af"
            } else if v >= 0.0 {
                "\\u002322c55e"
            } else {
                "\\u0023ef4444"
            }
        };

        // 各バーのデータ項目JSONを生成
        let data_items: Vec<String> = deltas
            .iter()
            .map(|(_, val)| {
                format!(
                    "{{\"value\":{:.1},\"itemStyle\":{{\"color\":\"{}\"}}}}",
                    val,
                    bar_color(*val)
                )
            })
            .collect();

        // ECharts chart config JSON（#はUnicodeエスケープで記述してraw string衝突を回避）
        let chart_config = format!(
            concat!(
                "{{",
                "\"xAxis\":{{\"type\":\"category\",\"data\":[\"1ヶ月\",\"3ヶ月\",\"6ヶ月\",\"1年\",\"2年\"],",
                "\"axisLabel\":{{\"color\":\"\\u002394a3b8\",\"fontSize\":11}},",
                "\"axisLine\":{{\"lineStyle\":{{\"color\":\"\\u0023334155\"}}}}}},",
                "\"yAxis\":{{\"type\":\"value\",",
                "\"axisLabel\":{{\"color\":\"\\u002394a3b8\",\"fontSize\":10,\"formatter\":\"{{value}}%\"}},",
                "\"splitLine\":{{\"lineStyle\":{{\"color\":\"\\u00231e293b\"}}}},",
                "\"axisLine\":{{\"lineStyle\":{{\"color\":\"\\u0023334155\"}}}}}},",
                "\"grid\":{{\"left\":\"12%\",\"right\":\"8%\",\"top\":\"14%\",\"bottom\":\"16%\"}},",
                "\"series\":[{{\"type\":\"bar\",\"data\":[{data}],",
                "\"barWidth\":\"40%\",",
                "\"label\":{{\"show\":true,\"position\":\"top\",\"color\":\"\\u0023e2e8f0\",\"fontSize\":11,\"formatter\":\"{{c}}%\"}},",
                "\"markLine\":{{\"silent\":true,\"symbol\":\"none\",",
                "\"lineStyle\":{{\"color\":\"\\u0023475569\",\"type\":\"dashed\"}},",
                "\"data\":[{{\"yAxis\":0}}]}}}}],",
                "\"tooltip\":{{\"trigger\":\"axis\",",
                "\"backgroundColor\":\"\\u00231e293b\",\"borderColor\":\"\\u0023334155\",",
                "\"textStyle\":{{\"color\":\"\\u0023e2e8f0\"}},",
                "\"formatter\":\"{{b}}: {{c}}%\"}}}}"
            ),
            data = data_items.join(","),
        );

        // HTML要素としてチャートとアクセシビリティを出力
        let mut trend_html = String::with_capacity(2048);
        trend_html
            .push_str("<div class=\"stat-card mt-3\" style=\"border-left:4px solid #3b82f6\">");
        trend_html
            .push_str("<h4 class=\"text-xs text-slate-400 mb-2\">従業員数推移（増減率%）</h4>");
        trend_html.push_str("<div class=\"echart\" role=\"img\" aria-label=\"");
        trend_html.push_str(&aria_label);
        trend_html.push_str("\" data-chart-config='");
        trend_html.push_str(&chart_config);
        trend_html.push_str("' style=\"height:160px;\"></div>");
        trend_html.push_str("</div>");
        trend_html
    } else {
        String::new()
    };

    // 企業スコア表示
    let sn_score_display = if ctx.salesnow_score > 0.0 {
        let sn_color = if ctx.salesnow_score >= 70.0 {
            "text-green-400"
        } else if ctx.salesnow_score >= 50.0 {
            "text-blue-400"
        } else if ctx.salesnow_score >= 30.0 {
            "text-yellow-400"
        } else {
            "text-slate-400"
        };
        format!(
            r#"<div class="text-xl font-bold {}">{:.0}<span class="text-sm text-slate-400">/100</span></div>"#,
            sn_color, ctx.salesnow_score
        )
    } else {
        r#"<div class="text-lg text-slate-600">-</div>"#.to_string()
    };

    // 資本金表示（capital_stockは円単位）
    let capital_display = if ctx.capital_stock > 0 {
        let man = ctx.capital_stock / 10_000; // 万円換算
        if man >= 10_000 {
            // 1億円以上
            let oku = man / 10_000;
            let rem = man % 10_000;
            if rem > 0 {
                format!("{}億{}万円", format_number(oku), format_number(rem))
            } else {
                format!("{}億円", format_number(oku))
            }
        } else if man > 0 {
            format!("{}万円", format_number(man))
        } else {
            format!("{}円", format_number(ctx.capital_stock))
        }
    } else if !ctx.capital_stock_range.is_empty() && ctx.capital_stock_range != "-" {
        ctx.capital_stock_range.clone()
    } else {
        "-".to_string()
    };

    // 企業URLリンク
    let url_link = if !ctx.company_url.is_empty() && ctx.company_url != "-" {
        format!(
            r#" <a href="{}" target="_blank" rel="noopener" class="text-blue-400 hover:text-blue-300 text-xs ml-1" title="企業サイト">&#x1F517;</a>"#,
            escape_html(&ctx.company_url)
        )
    } else {
        String::new()
    };

    write!(html,
        r#"<div class="stat-card border-l-4 border-blue-500">
        <div class="flex justify-between items-start mb-4">
            <div>
                <h3 class="text-2xl font-bold text-white">{name}{url_link}{listing_badge}{tob_toc_badge}</h3>
                <div class="text-sm text-slate-400 mt-1">
                    {pref} | {ind}{ind2_sep}{ind2} {hw_mapping} {established}
                </div>
                {tags}
            </div>
            <div class="text-right">
                <div class="text-xs text-slate-500">法人番号</div>
                <div class="text-sm text-slate-300 font-mono">{corp}</div>
            </div>
        </div>
        <div class="grid grid-cols-2 md:grid-cols-4 gap-3">
            <div class="bg-slate-800/50 rounded-lg p-3">
                <div class="text-xs text-slate-500">従業員数</div>
                <div class="text-xl font-bold text-white">{emp}<span class="text-sm text-slate-400">名</span></div>
                <div class="text-xs mt-1">前年比 {delta} {growth_badge}</div>
                <div class="text-[10px] text-slate-500 mt-0.5">{emp_range}</div>
                {group_emp}
            </div>
            <div class="bg-slate-800/50 rounded-lg p-3">
                <div class="text-xs text-slate-500">売上規模</div>
                <div class="text-lg font-bold text-white">{sales}</div>
                <div class="text-xs text-slate-500 mt-1">資本金: {capital}</div>
            </div>
            <div class="bg-slate-800/50 rounded-lg p-3">
                <div class="text-xs text-slate-500">与信スコア</div>
                <div class="text-xl font-bold {credit_color}">{credit}<span class="text-sm text-slate-400">/100</span></div>
            </div>
            <div class="bg-slate-800/50 rounded-lg p-3">
                <div class="text-xs text-slate-500">SNスコア</div>
                {sn_score}
            </div>
        </div>
        {delta_trend}
    </div>"#,
        name = escape_html(&ctx.company_name),
        url_link = url_link,
        listing_badge = listing_badge,
        tob_toc_badge = tob_toc_badge,
        pref = escape_html(&ctx.prefecture),
        ind = escape_html(&ctx.sn_industry),
        ind2_sep = if !ctx.sn_industry2.is_empty() { " / " } else { "" },
        ind2 = escape_html(&ctx.sn_industry2),
        hw_mapping = hw_mapping,
        established = established_display,
        tags = tags_html,
        corp = escape_html(&ctx.corporate_number),
        emp = format_number(ctx.employee_count),
        delta = delta_arrow,
        sales = if ctx.sales_range.is_empty() { "-" } else { &ctx.sales_range },
        capital = capital_display,
        credit = ctx.credit_score as i64,
        credit_color = if ctx.credit_score >= 70.0 { "text-green-400" }
                       else if ctx.credit_score >= 50.0 { "text-blue-400" }
                       else if ctx.credit_score >= 30.0 { "text-yellow-400" }
                       else { "text-red-400" },
        sn_score = sn_score_display,
        emp_range = escape_html(&ctx.employee_range),
        group_emp = if ctx.group_employee_count > 0 {
            format!(r#"<div class="text-xs text-slate-500 mt-1">グループ: {}名</div>"#, format_number(ctx.group_employee_count))
        } else {
            String::new()
        },
        growth_badge = growth_badge,
        delta_trend = delta_trend,
    ).unwrap();
}

fn render_market_snapshot(html: &mut String, ctx: &CompanyContext) {
    let salary_display = if ctx.market_avg_salary_min > 0.0 {
        format!("{:.0}円", ctx.market_avg_salary_min)
    } else {
        "-".to_string()
    };

    let salary_diff = ctx.market_avg_salary_min - ctx.national_avg_salary;
    let salary_diff_display = if ctx.market_avg_salary_min > 0.0 && ctx.national_avg_salary > 0.0 {
        if salary_diff > 0.0 {
            format!(
                r#"<span class="text-green-400 text-xs">全国比 +{:.0}円</span>"#,
                salary_diff
            )
        } else {
            format!(
                r#"<span class="text-red-400 text-xs">全国比 {:.0}円</span>"#,
                salary_diff
            )
        }
    } else {
        String::new()
    };

    write!(html,
        r#"<div class="mt-4">
        <h4 class="text-sm text-slate-400 mb-3">📊 {pref} × {ind} の採用市場</h4>
        <div class="grid grid-cols-2 md:grid-cols-4 gap-3">
            <div class="stat-card"><div class="stat-value text-blue-400">{postings}</div><div class="stat-label">求人件数</div></div>
            <div class="stat-card"><div class="stat-value text-emerald-400">{salary}</div><div class="stat-label">平均月給 {salary_diff}</div></div>
            <div class="stat-card"><div class="stat-value {vac_color}">{vacancy:.1}%</div><div class="stat-label">欠員補充率</div></div>
            <div class="stat-card"><div class="stat-value text-cyan-400">{ft_rate:.1}%</div><div class="stat-label">正社員比率</div></div>
        </div>
    </div>"#,
        pref = escape_html(&ctx.prefecture),
        ind = escape_html(&ctx.primary_hw_job_type),
        postings = format_number(ctx.market_posting_count),
        salary = salary_display,
        salary_diff = salary_diff_display,
        vacancy = ctx.market_vacancy_rate,
        vac_color = if ctx.market_vacancy_rate > 40.0 { "text-red-400" }
                    else if ctx.market_vacancy_rate > 25.0 { "text-amber-400" }
                    else { "text-green-400" },
        ft_rate = ctx.market_fulltime_rate,
    ).unwrap();
}

fn render_salary_section(html: &mut String, ctx: &CompanyContext) {
    if ctx.salary_distribution.is_empty() {
        return;
    }

    let labels: Vec<String> = ctx
        .salary_distribution
        .iter()
        .map(|(l, _)| format!("\"{}\"", l))
        .collect();
    let values: Vec<String> = ctx
        .salary_distribution
        .iter()
        .map(|(_, v)| v.to_string())
        .collect();

    write!(html,
        r##"<div class="stat-card mt-4">
        <h4 class="text-sm text-slate-400 mb-2">💰 給与帯分布（{pref} × {ind}）</h4>
        <div class="echart" style="height:300px;" data-chart-config='{{
            "tooltip": {{"trigger": "axis"}},
            "grid": {{"left": "10%", "right": "5%", "bottom": "15%", "containLabel": true}},
            "xAxis": {{"type": "category", "data": [{labels}], "axisLabel": {{"color": "#94a3b8"}}}},
            "yAxis": {{"type": "value", "name": "件数", "axisLabel": {{"color": "#94a3b8"}}}},
            "series": [{{
                "type": "bar",
                "data": [{values}],
                "itemStyle": {{"color": "#6366F1", "borderRadius": [4,4,0,0]}},
                "barWidth": "60%"
            }}]
        }}'></div>
    </div>"##,
        pref = escape_html(&ctx.prefecture),
        ind = escape_html(&ctx.primary_hw_job_type),
        labels = labels.join(","),
        values = values.join(","),
    ).unwrap();
}

fn render_competitor_section(html: &mut String, ctx: &CompanyContext) {
    html.push_str(r#"<div class="grid grid-cols-1 md:grid-cols-2 gap-4 mt-4">"#);

    // 求人理由ドーナツ
    if !ctx.recruitment_reasons.is_empty() {
        let pie_data: Vec<String> = ctx
            .recruitment_reasons
            .iter()
            .map(|(name, cnt)| {
                let color = match name.as_str() {
                    "欠員補充" => "#ef4444",
                    "増員" => "#22c55e",
                    "新設" => "#3b82f6",
                    _ => "#6b7280",
                };
                format!(
                    r#"{{"value": {}, "name": "{}", "itemStyle": {{"color": "{}"}}}}"#,
                    cnt, name, color
                )
            })
            .collect();

        write!(
            html,
            r##"<div class="stat-card">
            <h4 class="text-sm text-slate-400 mb-2">📋 求人理由</h4>
            <div class="echart" style="height:280px;" data-chart-config='{{
                "tooltip": {{"trigger": "item", "formatter": "{{b}}: {{c}}件 ({{d}}%)"}},
                "series": [{{
                    "type": "pie",
                    "radius": ["40%", "70%"],
                    "center": ["50%", "50%"],
                    "itemStyle": {{"borderRadius": 6, "borderColor": "#0f172a", "borderWidth": 2}},
                    "label": {{"show": true, "color": "#e2e8f0", "formatter": "{{b}}\n{{d}}%"}},
                    "data": [{data}]
                }}]
            }}'></div>
        </div>"##,
            data = pie_data.join(","),
        )
        .unwrap();
    }

    // 福利厚生レーダー
    if !ctx.benefit_rates.is_empty() {
        let indicators: Vec<String> = ctx
            .benefit_rates
            .iter()
            .map(|(name, _)| format!(r#"{{"name": "{}", "max": 100}}"#, name))
            .collect();
        let values: Vec<String> = ctx
            .benefit_rates
            .iter()
            .map(|(_, v)| format!("{:.1}", v))
            .collect();

        write!(html,
            r##"<div class="stat-card">
            <h4 class="text-sm text-slate-400 mb-2">🎯 福利厚生普及率（正社員）</h4>
            <div class="echart" style="height:280px;" data-chart-config='{{
                "tooltip": {{}},
                "radar": {{
                    "indicator": [{indicators}],
                    "axisName": {{"color": "#94a3b8", "fontSize": 11}},
                    "splitArea": {{"areaStyle": {{"color": ["rgba(30,41,59,0.5)", "rgba(30,41,59,0.3)"]}}}}
                }},
                "series": [{{
                    "type": "radar",
                    "data": [{{
                        "value": [{values}],
                        "name": "業界普及率",
                        "areaStyle": {{"opacity": 0.3}},
                        "lineStyle": {{"color": "#8b5cf6"}},
                        "itemStyle": {{"color": "#8b5cf6"}}
                    }}]
                }}]
            }}'></div>
        </div>"##,
            indicators = indicators.join(","),
            values = values.join(","),
        ).unwrap();
    }

    html.push_str("</div>");

    // 従業員規模分布
    if !ctx.emp_size_distribution.is_empty() {
        let labels: Vec<String> = ctx
            .emp_size_distribution
            .iter()
            .rev()
            .map(|(l, _)| format!("\"{}\"", l))
            .collect();
        let values: Vec<String> = ctx
            .emp_size_distribution
            .iter()
            .rev()
            .map(|(_, v)| v.to_string())
            .collect();

        write!(
            html,
            r##"<div class="stat-card mt-4">
            <h4 class="text-sm text-slate-400 mb-2">🏢 競合企業の従業員規模分布</h4>
            <div class="echart" style="height:250px;" data-chart-config='{{
                "tooltip": {{"trigger": "axis"}},
                "grid": {{"left": "20%", "right": "10%", "containLabel": true}},
                "yAxis": {{"type": "category", "data": [{labels}]}},
                "xAxis": {{"type": "value"}},
                "series": [{{
                    "type": "bar",
                    "data": [{values}],
                    "itemStyle": {{"color": "#0ea5e9", "borderRadius": [0,4,4,0]}},
                    "label": {{"show": true, "position": "right", "color": "#e2e8f0"}}
                }}]
            }}'></div>
        </div>"##,
            labels = labels.join(","),
            values = values.join(","),
        )
        .unwrap();
    }
}

fn render_demographics(html: &mut String, ctx: &CompanyContext) {
    if ctx.population == 0 {
        return;
    }
    write!(html,
        r#"<div class="mt-4">
        <h4 class="text-sm text-slate-400 mb-3">👥 {pref} の人口コンテキスト</h4>
        <div class="grid grid-cols-3 gap-3">
            <div class="stat-card"><div class="stat-value text-white">{pop}</div><div class="stat-label">総人口</div></div>
            <div class="stat-card"><div class="stat-value text-cyan-400">{daytime:.1}%</div><div class="stat-label">昼夜間人口比</div></div>
            <div class="stat-card"><div class="stat-value {aging_color}">{aging:.1}%</div><div class="stat-label">高齢化率</div></div>
        </div>
    </div>"#,
        pref = escape_html(&ctx.prefecture),
        pop = format_number(ctx.population),
        daytime = ctx.daytime_ratio,
        aging = ctx.aging_rate,
        aging_color = if ctx.aging_rate > 30.0 { "text-red-400" }
                      else if ctx.aging_rate > 25.0 { "text-amber-400" }
                      else { "text-green-400" },
    ).unwrap();
}

fn render_insights(html: &mut String, ctx: &CompanyContext) {
    html.push_str(r#"<div class="stat-card mt-4 border-l-4 border-emerald-500"><h4 class="text-sm text-slate-400 mb-3">💡 複合示唆</h4><div class="space-y-3">"#);

    // 1. 給与ポジショニング
    if ctx.market_avg_salary_min > 0.0 {
        let diff = ctx.market_avg_salary_min - ctx.national_avg_salary;
        let diff_pct = if ctx.national_avg_salary > 0.0 {
            diff / ctx.national_avg_salary * 100.0
        } else {
            0.0
        };
        let (sev_class, sev_label) = if diff_pct < -10.0 {
            ("bg-red-900/50 border-red-700", "注意")
        } else if diff_pct < -3.0 {
            ("bg-amber-900/50 border-amber-700", "情報")
        } else {
            ("bg-green-900/50 border-green-700", "良好")
        };
        write!(html,
            r#"<div class="border rounded-lg p-3 {cls}">
                <div class="flex items-center gap-2 mb-1">
                    <span class="text-xs font-bold px-1.5 py-0.5 rounded bg-slate-800">{label}</span>
                    <span class="text-sm text-white font-medium">給与市場ポジション</span>
                </div>
                <p class="text-xs text-slate-300">{pref}の{ind}業界の平均月給は{salary:.0}円で、全国平均（{nat:.0}円）と比べ{diff:+.0}円（{diff_pct:+.1}%）です。</p>
            </div>"#,
            cls = sev_class, label = sev_label,
            pref = escape_html(&ctx.prefecture),
            ind = escape_html(&ctx.primary_hw_job_type),
            salary = ctx.market_avg_salary_min,
            nat = ctx.national_avg_salary,
            diff = diff, diff_pct = diff_pct,
        ).unwrap();
    }

    // 2. 採用圧力
    if ctx.market_vacancy_rate > 0.0 {
        let pressure = if ctx.employee_delta_1y < -2.0 && ctx.market_vacancy_rate > 30.0 {
            ("bg-red-900/50 border-red-700", "重大", format!(
                "この企業は前年比{:.1}%の人員減少中で、地域の欠員補充率も{:.1}%と高水準。採用支援の緊急性が高い市場です。",
                ctx.employee_delta_1y, ctx.market_vacancy_rate
            ))
        } else if ctx.market_vacancy_rate > 25.0 {
            ("bg-amber-900/50 border-amber-700", "注意", format!(
                "地域の欠員補充率が{:.1}%と全国平均（{:.1}%）を上回っています。人材確保の競争が激しい市場です。",
                ctx.market_vacancy_rate, ctx.national_vacancy_rate
            ))
        } else {
            (
                "bg-blue-900/50 border-blue-700",
                "情報",
                format!(
                "地域の欠員補充率は{:.1}%で安定しています。計画的な採用活動が可能な市場環境です。",
                ctx.market_vacancy_rate
            ),
            )
        };
        write!(html,
            r#"<div class="border rounded-lg p-3 {cls}">
                <div class="flex items-center gap-2 mb-1">
                    <span class="text-xs font-bold px-1.5 py-0.5 rounded bg-slate-800">{label}</span>
                    <span class="text-sm text-white font-medium">採用圧力</span>
                </div>
                <p class="text-xs text-slate-300">{body}</p>
            </div>"#,
            cls = pressure.0, label = pressure.1, body = pressure.2,
        ).unwrap();
    }

    // 3. 高齢化リスク
    if ctx.aging_rate > 28.0 {
        write!(html,
            r#"<div class="border rounded-lg p-3 bg-amber-900/50 border-amber-700">
                <div class="flex items-center gap-2 mb-1">
                    <span class="text-xs font-bold px-1.5 py-0.5 rounded bg-slate-800">注意</span>
                    <span class="text-sm text-white font-medium">労働力供給リスク</span>
                </div>
                <p class="text-xs text-slate-300">{pref}の高齢化率は{rate:.1}%で、労働力人口の減少が見込まれます。中長期的な人材確保戦略の検討が必要です。</p>
            </div>"#,
            pref = escape_html(&ctx.prefecture),
            rate = ctx.aging_rate,
        ).unwrap();
    }

    // 4. 市場規模
    if ctx.market_posting_count > 0 {
        let competition = ctx.market_posting_count as f64 / ctx.market_facility_count.max(1) as f64;
        write!(html,
            r#"<div class="border rounded-lg p-3 bg-blue-900/50 border-blue-700">
                <div class="flex items-center gap-2 mb-1">
                    <span class="text-xs font-bold px-1.5 py-0.5 rounded bg-slate-800">情報</span>
                    <span class="text-sm text-white font-medium">市場規模</span>
                </div>
                <p class="text-xs text-slate-300">{pref}の{ind}業界には{posts}件の求人が{facs}事業所から出されています（1事業所あたり{ratio:.1}件）。</p>
            </div>"#,
            pref = escape_html(&ctx.prefecture),
            ind = escape_html(&ctx.primary_hw_job_type),
            posts = format_number(ctx.market_posting_count),
            facs = format_number(ctx.market_facility_count),
            ratio = competition,
        ).unwrap();
    }

    html.push_str("</div></div>");
}

/// 提案ポイントカード（最重要: プロフィールの最上部に表示）
fn render_sales_pitches(html: &mut String, ctx: &CompanyContext) {
    if ctx.sales_pitches.is_empty() {
        return;
    }

    html.push_str(
        r#"<div class="stat-card border-l-4 border-cyan-400 mb-4">
        <h4 class="text-sm font-bold text-cyan-400 mb-3">&#x1F4A1; 提案ポイント</h4>
        <div class="space-y-3">"#,
    );

    for (i, (headline, body)) in ctx.sales_pitches.iter().enumerate() {
        write!(
            html,
            r#"<div class="bg-slate-800/50 rounded-lg p-3">
                <div class="flex items-start gap-2">
                    <span class="text-blue-400 font-bold text-sm shrink-0">{num}.</span>
                    <div>
                        <div class="text-white text-sm font-medium">{headline}</div>
                        <p class="text-xs text-slate-300 mt-1">{body}</p>
                    </div>
                </div>
            </div>"#,
            num = i + 1,
            headline = escape_html(headline),
            body = escape_html(body),
        )
        .unwrap();
    }

    html.push_str("</div></div>");
}

/// 採用リスクゲージ
fn render_hiring_risk(html: &mut String, ctx: &CompanyContext) {
    let (grade_color, grade_bg) = match ctx.hiring_risk_grade.as_str() {
        "A" => ("text-green-400", "bg-green-900/40 border-green-700"),
        "B" => ("text-blue-400", "bg-blue-900/40 border-blue-700"),
        "C" => ("text-amber-400", "bg-amber-900/40 border-amber-700"),
        "D" => ("text-orange-400", "bg-orange-900/40 border-orange-700"),
        _ => ("text-red-400", "bg-red-900/40 border-red-700"),
    };

    let explanation = match ctx.hiring_risk_grade.as_str() {
        "A" => "採用環境は良好です。給与水準・地域特性ともに人材確保に有利な条件が揃っています。",
        "B" => "採用環境はおおむね良好ですが、一部の指標に注意が必要です。",
        "C" => "採用に中程度のリスクがあります。給与水準や地域の人口動態に改善の余地があります。",
        "D" => "採用リスクが高い状態です。複数の指標で不利な条件が重なっています。",
        _ => "採用環境は非常に厳しい状態です。早急な対策が必要です。",
    };

    write!(html,
        r#"<div class="stat-card mt-4 border {grade_bg}">
        <div class="flex items-center gap-6">
            <div class="text-center">
                <div class="text-4xl font-black {grade_color}">{grade}</div>
                <div class="text-xs text-slate-500 mt-1">採用リスク</div>
                <div class="text-sm {grade_color} font-mono">{score:.0}<span class="text-xs text-slate-500">/100</span></div>
            </div>
            <div class="flex-1">
                <p class="text-sm text-slate-300">{explanation}</p>
                <div class="flex gap-4 mt-2 text-xs text-slate-500">
                    <span>高齢化率: {aging:.1}%</span>
                    <span>欠員補充率: {vacancy:.1}%</span>
                    <span>給与: {salary_pct}パーセンタイル</span>
                    <span>与信: {credit:.0}</span>
                </div>
            </div>
        </div>
    </div>"#,
        grade_bg = grade_bg,
        grade_color = grade_color,
        grade = escape_html(&ctx.hiring_risk_grade),
        score = ctx.hiring_risk_score,
        explanation = explanation,
        aging = ctx.aging_rate,
        vacancy = ctx.market_vacancy_rate,
        salary_pct = if ctx.salary_percentile > 0.0 {
            format!("{:.0}%", ctx.salary_percentile)
        } else {
            "-".to_string()
        },
        credit = ctx.credit_score,
    ).unwrap();
}

/// 地域 vs 自社 比較セクション
fn render_region_vs_company(html: &mut String, ctx: &CompanyContext) {
    if ctx.region_industry_company_count == 0 {
        return;
    }

    let region_delta_display = if ctx.region_industry_avg_delta.abs() > 0.01 {
        format!("{:+.1}%", ctx.region_industry_avg_delta)
    } else {
        "0.0%".to_string()
    };

    let net_change_color = if ctx.region_industry_net_change > 0 {
        "text-green-400"
    } else if ctx.region_industry_net_change < 0 {
        "text-red-400"
    } else {
        "text-slate-400"
    };

    let gap_display = if ctx.company_vs_region_gap.abs() > 0.1 {
        if ctx.company_vs_region_gap > 0.0 {
            format!(
                r#"<span class="text-green-400">+{:.1}pt 上回る</span>"#,
                ctx.company_vs_region_gap
            )
        } else {
            format!(
                r#"<span class="text-red-400">{:.1}pt 下回る</span>"#,
                ctx.company_vs_region_gap
            )
        }
    } else {
        r#"<span class="text-slate-400">同水準</span>"#.to_string()
    };

    write!(html,
        r#"<div class="stat-card mt-4 border-l-4 border-purple-500">
        <h4 class="text-sm text-slate-400 mb-3">&#x1F4CA; 地域×業種 人材フロー比較</h4>
        <div class="grid grid-cols-1 md:grid-cols-2 gap-4">
            <div class="bg-slate-800/50 rounded-lg p-4">
                <div class="text-xs text-slate-500 mb-1">{pref}の{ind}業界（{cnt}社）</div>
                <div class="text-xl font-bold text-white">{total}人</div>
                <div class="text-sm {net_color} mt-1">年間 {net_change:+}人（平均 {avg_delta}）</div>
            </div>
            <div class="bg-slate-800/50 rounded-lg p-4">
                <div class="text-xs text-slate-500 mb-1">御社</div>
                <div class="text-xl font-bold text-white">{emp}人</div>
                <div class="text-sm mt-1">前年比 {delta:.1}%（地域平均比: {gap}）</div>
            </div>
        </div>
    </div>"#,
        pref = escape_html(&ctx.prefecture),
        ind = escape_html(&ctx.sn_industry),
        cnt = ctx.region_industry_company_count,
        total = format_number(ctx.region_industry_total_employees),
        net_color = net_change_color,
        net_change = ctx.region_industry_net_change,
        avg_delta = region_delta_display,
        emp = format_number(ctx.employee_count),
        delta = ctx.employee_delta_1y,
        gap = gap_display,
    ).unwrap();
}

/// 給与ギャップテーブル
fn render_salary_gap_table(html: &mut String, ctx: &CompanyContext) {
    // 自社給与データがある場合のみ表示
    if ctx.company_salary_count == 0 || ctx.market_avg_salary_min <= 0.0 {
        return;
    }

    let gap = ctx.company_avg_salary_min - ctx.market_avg_salary_min;
    let gap_color = if gap > 0.0 {
        "text-green-400"
    } else {
        "text-red-400"
    };
    let gap_display = if gap.abs() > 0.0 {
        format!(r#"<span class="{}">{:+.0}円</span>"#, gap_color, gap)
    } else {
        "±0円".to_string()
    };

    write!(html,
        r#"<div class="stat-card mt-4 border-l-4 border-amber-500">
        <h4 class="text-sm text-slate-400 mb-3">&#x1F4B0; 給与ギャップ分析（月給下限）</h4>
        <table class="w-full text-sm">
            <thead><tr class="text-slate-500 border-b border-slate-700">
                <th class="text-left py-2 px-3"></th>
                <th class="text-right py-2 px-3">月給（下限平均）</th>
                <th class="text-right py-2 px-3">市場比</th>
                <th class="text-right py-2 px-3">パーセンタイル</th>
            </tr></thead>
            <tbody>
                <tr class="border-b border-slate-800">
                    <td class="py-2 px-3 text-white font-medium">この企業</td>
                    <td class="text-right py-2 px-3 text-white">{company_sal:.0}円 <span class="text-xs text-slate-500">({cnt}件)</span></td>
                    <td class="text-right py-2 px-3">{gap}</td>
                    <td class="text-right py-2 px-3 {pct_color}">上位 {pct:.0}%</td>
                </tr>
                <tr>
                    <td class="py-2 px-3 text-slate-400">市場全体</td>
                    <td class="text-right py-2 px-3 text-slate-300">{market_sal:.0}円</td>
                    <td class="text-right py-2 px-3 text-slate-500">-</td>
                    <td class="text-right py-2 px-3 text-slate-500">-</td>
                </tr>
            </tbody>
        </table>
    </div>"#,
        company_sal = ctx.company_avg_salary_min,
        cnt = ctx.company_salary_count,
        gap = gap_display,
        pct_color = if ctx.salary_percentile > 50.0 { "text-green-400" } else { "text-amber-400" },
        pct = 100.0 - ctx.salary_percentile,
        market_sal = ctx.market_avg_salary_min,
    ).unwrap();
}

fn render_hw_postings(html: &mut String, ctx: &CompanyContext) {
    html.push_str(r##"<div class="stat-card mt-4 border-l-4 border-rose-500"><h4 class="text-sm text-slate-400 mb-3">📋 この企業のハローワーク求人</h4>"##);

    if ctx.hw_matched_postings.is_empty() {
        html.push_str(r##"<p class="text-slate-500 text-sm text-center py-4">ハローワークに求人掲載なし</p></div>"##);
        return;
    }

    let total = ctx.hw_matched_total_count;
    let shown = ctx.hw_matched_postings.len();
    let label = if total as usize > shown {
        format!(
            "企業名「{}」でマッチした求人 {}件（上位{}件表示）",
            escape_html(&ctx.company_name),
            total,
            shown
        )
    } else {
        format!(
            "企業名「{}」でマッチした求人 {}件",
            escape_html(&ctx.company_name),
            total
        )
    };
    write!(
        html,
        r##"<p class="text-xs text-slate-500 mb-2">{}</p>"##,
        label
    )
    .unwrap();

    html.push_str(
        r##"<div class="overflow-x-auto max-h-80"><table class="w-full text-xs">
        <thead><tr class="text-slate-500 border-b border-slate-700">
            <th class="text-left py-1.5 px-2">職種</th>
            <th class="text-left py-1.5 px-2">雇用形態</th>
            <th class="text-left py-1.5 px-2">勤務地</th>
            <th class="text-right py-1.5 px-2">給与</th>
            <th class="text-left py-1.5 px-2">見出し</th>
        </tr></thead><tbody>"##,
    );

    for row in &ctx.hw_matched_postings {
        let rowid = get_i64(row, "rowid");
        let job_type = get_str(row, "job_type");
        let emp_type = get_str(row, "employment_type");
        let muni = get_str(row, "municipality");
        let salary_min = get_i64(row, "salary_min");
        let salary_max = get_i64(row, "salary_max");
        let salary_type = get_str(row, "salary_type");
        let headline = get_str(row, "headline");
        let job_number = get_str(row, "job_number");
        let working_hours = get_str(row, "working_hours");
        let holidays = get_str(row, "holidays");
        let benefits = get_str(row, "benefits");
        let reason = get_str(row, "recruitment_reason");

        let salary_display = if salary_min > 0 && salary_max > 0 {
            format!(
                "{} {}-{}",
                escape_html(&salary_type),
                format_number(salary_min),
                format_number(salary_max)
            )
        } else if salary_min > 0 {
            format!(
                "{} {}~",
                escape_html(&salary_type),
                format_number(salary_min)
            )
        } else {
            "-".to_string()
        };

        let emp_color = match emp_type.as_str() {
            "正社員" => "text-green-400",
            _ => "text-slate-300",
        };

        // クリックで詳細を展開/折りたたみ
        let detail_id = format!("hw-detail-{}", rowid);
        write!(html,
            r##"<tr class="border-b border-slate-800 hover:bg-slate-700/50 cursor-pointer" onclick="var d=document.getElementById('{detail_id}');d.style.display=d.style.display==='none'?'table-row':'none'">
                <td class="py-1.5 px-2">{}</td>
                <td class="py-1.5 px-2 {}"><span class="font-medium">{}</span></td>
                <td class="py-1.5 px-2 text-slate-400">{}</td>
                <td class="text-right py-1.5 px-2 text-amber-400">{}</td>
                <td class="py-1.5 px-2 text-slate-400 max-w-xs truncate">{}</td>
            </tr>"##,
            escape_html(&job_type),
            emp_color,
            escape_html(&emp_type),
            escape_html(&muni),
            salary_display,
            escape_html(&truncate_str(&headline, 40)),
        ).unwrap();

        // 展開時の詳細行（初期非表示）
        write!(
            html,
            r##"<tr id="{detail_id}" style="display:none" class="bg-slate-800/80">
                <td colspan="5" class="px-4 py-3">
                    <div class="grid grid-cols-2 gap-x-6 gap-y-1 text-xs">"##,
        )
        .unwrap();
        if !job_number.is_empty() {
            write!(html,
                r##"<div><span class="text-slate-500">求人番号:</span> <span class="text-cyan-300 font-mono">{}</span></div>"##,
                escape_html(&job_number)).unwrap();
        }
        if !headline.is_empty() {
            write!(html,
                r##"<div class="col-span-2"><span class="text-slate-500">見出し:</span> <span class="text-white">{}</span></div>"##,
                escape_html(&headline)).unwrap();
        }
        if !working_hours.is_empty() {
            write!(html,
                r##"<div><span class="text-slate-500">勤務時間:</span> <span class="text-slate-300">{}</span></div>"##,
                escape_html(&truncate_str(&working_hours, 60))).unwrap();
        }
        if !holidays.is_empty() {
            write!(html,
                r##"<div><span class="text-slate-500">休日:</span> <span class="text-slate-300">{}</span></div>"##,
                escape_html(&truncate_str(&holidays, 60))).unwrap();
        }
        if !benefits.is_empty() {
            write!(html,
                r##"<div class="col-span-2"><span class="text-slate-500">福利厚生:</span> <span class="text-slate-300">{}</span></div>"##,
                escape_html(&truncate_str(&benefits, 100))).unwrap();
        }
        if !reason.is_empty() {
            write!(html,
                r##"<div><span class="text-slate-500">募集理由:</span> <span class="text-slate-300">{}</span></div>"##,
                escape_html(&reason)).unwrap();
        }
        html.push_str("</div></td></tr>");
    }

    html.push_str("</tbody></table></div></div>");
}

fn render_nearby_companies(html: &mut String, ctx: &CompanyContext) {
    if ctx.nearby_companies.is_empty() {
        return;
    }

    let postal_prefix = if ctx.postal_code.len() >= 3 {
        &ctx.postal_code[..3]
    } else {
        &ctx.postal_code
    };

    write!(
        html,
        r##"<div class="stat-card mt-4">
        <h4 class="text-sm text-slate-400 mb-3">🏢 近隣企業（〒{}xxx エリア、{}社）</h4>
        <div class="overflow-x-auto max-h-96"><table class="w-full text-xs">
        <thead><tr class="text-slate-500 border-b border-slate-700">
            <th class="text-left py-1.5 px-2">企業名</th>
            <th class="text-left py-1.5 px-2">業界</th>
            <th class="text-right py-1.5 px-2">従業員数</th>
            <th class="text-right py-1.5 px-2">与信</th>
            <th class="text-right py-1.5 px-2">HW求人</th>
        </tr></thead><tbody>"##,
        escape_html(postal_prefix),
        ctx.nearby_companies.len(),
    )
    .unwrap();

    for nc in &ctx.nearby_companies {
        let hw_badge = if nc.hw_posting_count > 0 {
            format!(
                r##"<span class="text-blue-400 font-medium">{}件</span>"##,
                nc.hw_posting_count
            )
        } else {
            r##"<span class="text-slate-600">-</span>"##.to_string()
        };

        // クリックで同タブ内に企業プロフィールを展開
        html.push_str(
            "<tr class=\"border-b border-slate-800 hover:bg-slate-700/50 cursor-pointer\" ",
        );
        write!(
            html,
            "hx-get=\"/api/company/profile/{}\" ",
            escape_html(&nc.corporate_number)
        )
        .unwrap();
        html.push_str("hx-target=\"#content\" hx-swap=\"innerHTML\">");

        write!(
            html,
            r##"<td class="py-1.5 px-2 text-white">{}</td>
                <td class="py-1.5 px-2 text-slate-400">{}</td>
                <td class="text-right py-1.5 px-2">{}</td>
                <td class="text-right py-1.5 px-2">{}</td>
                <td class="text-right py-1.5 px-2">{}</td>
            </tr>"##,
            escape_html(&nc.company_name),
            escape_html(&nc.sn_industry),
            if nc.employee_count > 0 {
                format_number(nc.employee_count)
            } else {
                "-".to_string()
            },
            if nc.credit_score > 0.0 {
                format!("{:.0}", nc.credit_score)
            } else {
                "-".to_string()
            },
            hw_badge,
        )
        .unwrap();
    }

    html.push_str("</tbody></table></div></div>");
}

/// 印刷用レポートHTML（フルページ）
pub fn render_company_report(ctx: &CompanyContext) -> String {
    let mut body = String::with_capacity(48_000);
    render_sales_pitches(&mut body, ctx);
    render_header(&mut body, ctx);
    render_hiring_risk(&mut body, ctx);
    render_market_snapshot(&mut body, ctx);
    render_region_vs_company(&mut body, ctx);
    render_salary_section(&mut body, ctx);
    render_salary_gap_table(&mut body, ctx);
    render_competitor_section(&mut body, ctx);
    render_demographics(&mut body, ctx);
    render_insights(&mut body, ctx);
    render_hw_postings(&mut body, ctx);

    format!(
        r##"<!DOCTYPE html>
<html lang="ja">
<head>
<meta charset="UTF-8">
<title>企業分析レポート - {name}</title>
<link rel="stylesheet" href="/static/css/tailwind-precompiled.css">
<link rel="stylesheet" href="/static/css/dashboard.css">
<script src="https://cdn.jsdelivr.net/npm/echarts@5/dist/echarts.min.js"></script>
<style>
@page {{ size: A4 landscape; margin: 10mm; }}
@media print {{
    body {{ background: #0f172a !important; -webkit-print-color-adjust: exact; print-color-adjust: exact; }}
    .no-print {{ display: none !important; }}
}}
</style>
</head>
<body class="bg-navy-900 text-slate-100 p-6">
<div class="max-w-6xl mx-auto">
    <div class="flex justify-between items-center mb-4 no-print">
        <h1 class="text-lg font-bold">企業分析レポート</h1>
        <button onclick="window.print()" class="bg-blue-600 hover:bg-blue-500 text-white px-4 py-2 rounded text-sm">🖨 印刷 / PDF保存</button>
    </div>
    <div class="text-xs text-slate-500 mb-4">生成日: <script>document.write(new Date().toLocaleDateString('ja-JP'))</script> | データソース: 企業属性 + ハローワーク求人 + 外部統計</div>
    {body}
    <div class="text-xs text-slate-600 mt-8 text-center">
        ※ ハローワーク掲載求人のみが対象です。民間求人サイト（Indeed等）の求人は含まれません。
    </div>
</div>
<script src="/static/js/charts.js"></script>
<script src="/static/js/app.js"></script>
<script>
document.addEventListener('DOMContentLoaded', function() {{
    if (typeof window.initECharts === 'function') window.initECharts(document.body);
}});
</script>
</body>
</html>"##,
        name = escape_html(&ctx.company_name),
        body = body,
    )
}
