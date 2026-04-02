use super::fetch::CompanyContext;
use crate::handlers::helpers::{escape_html, format_number, get_str, Row};

/// 検索ページ（タブのシェル）
pub fn render_search_page() -> String {
    r##"<div class="space-y-6">
    <div class="flex items-center justify-between">
        <h2 class="text-xl font-bold text-white">🔎 企業分析
            <span class="text-blue-400 text-base font-normal">SalesNow企業 × ハローワーク市場データ</span>
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
        <p class="text-slate-600 text-xs text-center">データソース: SalesNow (企業属性) × ハローワーク (求人市場469,000件) × 外部統計 (人口・労働)</p>
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
        let range = escape_html(&get_str(row, "employee_range"));
        let credit = get_str(row, "credit_score");

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

        // hx-target に # が含まれるため push_str で構築
        html.push_str("<div class=\"px-4 py-3 hover:bg-slate-700 cursor-pointer border-b border-slate-700/50 transition-colors\" ");
        html.push_str(&format!("hx-get=\"/api/company/profile/{}\" ", corp));
        html.push_str("hx-target=\"#company-profile-area\" hx-swap=\"innerHTML\" ");
        html.push_str("onclick=\"document.getElementById('company-search-results').textContent=''\">");
        html.push_str(&format!(
            r##"<div class="flex justify-between items-start">
                    <div>
                        <span class="text-white font-medium">{name}</span>
                        <span class="text-slate-400 text-xs ml-2">{pref}</span>
                    </div>
                    <span class="text-slate-500 text-xs">{emp_display}</span>
                </div>
                <div class="text-xs text-slate-500 mt-0.5">
                    {ind}{ind2_sep}{ind2}{credit_badge}
                </div>
            </div>"##,
            name = name,
            pref = pref,
            emp_display = emp_display,
            ind = ind,
            ind2_sep = if !ind2.is_empty() { " / " } else { "" },
            ind2 = ind2,
            credit_badge = credit_badge,
        ));
    }

    html.push_str("</div>");
    html
}

fn credit_score_class(score_str: &str) -> &'static str {
    let score: f64 = score_str.parse().unwrap_or(0.0);
    if score >= 70.0 { "bg-green-900/50 text-green-400" }
    else if score >= 50.0 { "bg-blue-900/50 text-blue-400" }
    else if score >= 30.0 { "bg-yellow-900/50 text-yellow-400" }
    else { "bg-slate-700 text-slate-400" }
}

/// 企業プロフィール全体
pub fn render_company_profile(ctx: &CompanyContext) -> String {
    let mut html = String::with_capacity(32_000);

    // セクションA: 企業ヘッダー
    render_header(&mut html, ctx);

    // セクションB: 市場スナップショット
    render_market_snapshot(&mut html, ctx);

    // セクションC: 給与市場ポジション
    render_salary_section(&mut html, ctx);

    // セクションD: 競合環境
    render_competitor_section(&mut html, ctx);

    // セクションE: 地域人口コンテキスト
    render_demographics(&mut html, ctx);

    // セクションF: 複合示唆
    render_insights(&mut html, ctx);

    // レポートリンク
    html.push_str(&format!(
        r#"<div class="text-right mt-4">
            <a href="/report/company/{}" target="_blank"
               class="inline-flex items-center gap-2 bg-blue-600 hover:bg-blue-500 text-white px-4 py-2 rounded-lg transition-colors text-sm">
                🖨 印刷用レポートを開く
            </a>
        </div>"#,
        escape_html(&ctx.corporate_number)
    ));

    html
}

fn render_header(html: &mut String, ctx: &CompanyContext) {
    let delta_arrow = if ctx.employee_delta_1y > 0.5 {
        format!(r#"<span class="text-green-400">+{:.1}%↑</span>"#, ctx.employee_delta_1y)
    } else if ctx.employee_delta_1y < -0.5 {
        format!(r#"<span class="text-red-400">{:.1}%↓</span>"#, ctx.employee_delta_1y)
    } else {
        format!(r#"<span class="text-slate-400">{:.1}%→</span>"#, ctx.employee_delta_1y)
    };

    let hw_mapping = if !ctx.primary_hw_job_type.is_empty() {
        format!(
            r#"<span class="text-xs bg-blue-900/60 text-blue-300 px-2 py-0.5 rounded">HW: {}</span>"#,
            escape_html(&ctx.primary_hw_job_type)
        )
    } else {
        r#"<span class="text-xs bg-slate-700 text-slate-400 px-2 py-0.5 rounded">HWマッピングなし</span>"#.to_string()
    };

    html.push_str(&format!(
        r#"<div class="stat-card border-l-4 border-blue-500">
        <div class="flex justify-between items-start mb-4">
            <div>
                <h3 class="text-2xl font-bold text-white">{name}</h3>
                <div class="text-sm text-slate-400 mt-1">
                    {pref} | {ind}{ind2_sep}{ind2} {hw_mapping}
                </div>
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
                <div class="text-xs mt-1">前年比 {delta}</div>
            </div>
            <div class="bg-slate-800/50 rounded-lg p-3">
                <div class="text-xs text-slate-500">売上規模</div>
                <div class="text-lg font-bold text-white">{sales}</div>
            </div>
            <div class="bg-slate-800/50 rounded-lg p-3">
                <div class="text-xs text-slate-500">与信スコア</div>
                <div class="text-xl font-bold {credit_color}">{credit}<span class="text-sm text-slate-400">/100</span></div>
            </div>
            <div class="bg-slate-800/50 rounded-lg p-3">
                <div class="text-xs text-slate-500">従業員規模</div>
                <div class="text-sm font-medium text-white">{emp_range}</div>
            </div>
        </div>
    </div>"#,
        name = escape_html(&ctx.company_name),
        pref = escape_html(&ctx.prefecture),
        ind = escape_html(&ctx.sn_industry),
        ind2_sep = if !ctx.sn_industry2.is_empty() { " / " } else { "" },
        ind2 = escape_html(&ctx.sn_industry2),
        hw_mapping = hw_mapping,
        corp = escape_html(&ctx.corporate_number),
        emp = format_number(ctx.employee_count),
        delta = delta_arrow,
        sales = if ctx.sales_range.is_empty() { "-" } else { &ctx.sales_range },
        credit = ctx.credit_score as i64,
        credit_color = if ctx.credit_score >= 70.0 { "text-green-400" }
                       else if ctx.credit_score >= 50.0 { "text-blue-400" }
                       else if ctx.credit_score >= 30.0 { "text-yellow-400" }
                       else { "text-red-400" },
        emp_range = escape_html(&ctx.employee_range),
    ));
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
            format!(r#"<span class="text-green-400 text-xs">全国比 +{:.0}円</span>"#, salary_diff)
        } else {
            format!(r#"<span class="text-red-400 text-xs">全国比 {:.0}円</span>"#, salary_diff)
        }
    } else {
        String::new()
    };

    html.push_str(&format!(
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
    ));
}

fn render_salary_section(html: &mut String, ctx: &CompanyContext) {
    if ctx.salary_distribution.is_empty() {
        return;
    }

    let labels: Vec<String> = ctx.salary_distribution.iter()
        .map(|(l, _)| format!("\"{}\"", l))
        .collect();
    let values: Vec<String> = ctx.salary_distribution.iter()
        .map(|(_, v)| v.to_string())
        .collect();

    html.push_str(&format!(
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
    ));
}

fn render_competitor_section(html: &mut String, ctx: &CompanyContext) {
    html.push_str(r#"<div class="grid grid-cols-1 md:grid-cols-2 gap-4 mt-4">"#);

    // 求人理由ドーナツ
    if !ctx.recruitment_reasons.is_empty() {
        let pie_data: Vec<String> = ctx.recruitment_reasons.iter()
            .map(|(name, cnt)| {
                let color = match name.as_str() {
                    "欠員補充" => "#ef4444",
                    "増員" => "#22c55e",
                    "新設" => "#3b82f6",
                    _ => "#6b7280",
                };
                format!(r#"{{"value": {}, "name": "{}", "itemStyle": {{"color": "{}"}}}}"#, cnt, name, color)
            })
            .collect();

        html.push_str(&format!(
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
        ));
    }

    // 福利厚生レーダー
    if !ctx.benefit_rates.is_empty() {
        let indicators: Vec<String> = ctx.benefit_rates.iter()
            .map(|(name, _)| format!(r#"{{"name": "{}", "max": 100}}"#, name))
            .collect();
        let values: Vec<String> = ctx.benefit_rates.iter()
            .map(|(_, v)| format!("{:.1}", v))
            .collect();

        html.push_str(&format!(
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
        ));
    }

    html.push_str("</div>");

    // 従業員規模分布
    if !ctx.emp_size_distribution.is_empty() {
        let labels: Vec<String> = ctx.emp_size_distribution.iter().rev()
            .map(|(l, _)| format!("\"{}\"", l))
            .collect();
        let values: Vec<String> = ctx.emp_size_distribution.iter().rev()
            .map(|(_, v)| v.to_string())
            .collect();

        html.push_str(&format!(
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
        ));
    }
}

fn render_demographics(html: &mut String, ctx: &CompanyContext) {
    if ctx.population == 0 {
        return;
    }
    html.push_str(&format!(
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
    ));
}

fn render_insights(html: &mut String, ctx: &CompanyContext) {
    html.push_str(r#"<div class="stat-card mt-4"><h4 class="text-sm text-slate-400 mb-3">💡 複合示唆</h4><div class="space-y-3">"#);

    // 1. 給与ポジショニング
    if ctx.market_avg_salary_min > 0.0 {
        let diff = ctx.market_avg_salary_min - ctx.national_avg_salary;
        let diff_pct = if ctx.national_avg_salary > 0.0 { diff / ctx.national_avg_salary * 100.0 } else { 0.0 };
        let (sev_class, sev_label) = if diff_pct < -10.0 {
            ("bg-red-900/50 border-red-700", "注意")
        } else if diff_pct < -3.0 {
            ("bg-amber-900/50 border-amber-700", "情報")
        } else {
            ("bg-green-900/50 border-green-700", "良好")
        };
        html.push_str(&format!(
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
        ));
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
            ("bg-blue-900/50 border-blue-700", "情報", format!(
                "地域の欠員補充率は{:.1}%で安定しています。計画的な採用活動が可能な市場環境です。",
                ctx.market_vacancy_rate
            ))
        };
        html.push_str(&format!(
            r#"<div class="border rounded-lg p-3 {cls}">
                <div class="flex items-center gap-2 mb-1">
                    <span class="text-xs font-bold px-1.5 py-0.5 rounded bg-slate-800">{label}</span>
                    <span class="text-sm text-white font-medium">採用圧力</span>
                </div>
                <p class="text-xs text-slate-300">{body}</p>
            </div>"#,
            cls = pressure.0, label = pressure.1, body = pressure.2,
        ));
    }

    // 3. 高齢化リスク
    if ctx.aging_rate > 28.0 {
        html.push_str(&format!(
            r#"<div class="border rounded-lg p-3 bg-amber-900/50 border-amber-700">
                <div class="flex items-center gap-2 mb-1">
                    <span class="text-xs font-bold px-1.5 py-0.5 rounded bg-slate-800">注意</span>
                    <span class="text-sm text-white font-medium">労働力供給リスク</span>
                </div>
                <p class="text-xs text-slate-300">{pref}の高齢化率は{rate:.1}%で、労働力人口の減少が見込まれます。中長期的な人材確保戦略の検討が必要です。</p>
            </div>"#,
            pref = escape_html(&ctx.prefecture),
            rate = ctx.aging_rate,
        ));
    }

    // 4. 市場規模
    if ctx.market_posting_count > 0 {
        let competition = ctx.market_posting_count as f64 / ctx.market_facility_count.max(1) as f64;
        html.push_str(&format!(
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
        ));
    }

    html.push_str("</div></div>");
}

/// 印刷用レポートHTML（フルページ）
pub fn render_company_report(ctx: &CompanyContext) -> String {
    let mut body = String::with_capacity(32_000);
    render_header(&mut body, ctx);
    render_market_snapshot(&mut body, ctx);
    render_salary_section(&mut body, ctx);
    render_competitor_section(&mut body, ctx);
    render_demographics(&mut body, ctx);
    render_insights(&mut body, ctx);

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
    <div class="text-xs text-slate-500 mb-4">生成日: <script>document.write(new Date().toLocaleDateString('ja-JP'))</script> | データソース: SalesNow + ハローワーク求人 + 外部統計</div>
    {body}
    <div class="text-xs text-slate-600 mt-8 text-center">
        ※ ハローワーク掲載求人のみが対象です。民間求人サイト（Indeed等）の求人は含まれません。
    </div>
</div>
<script src="/static/js/charts.js"></script>
<script>
document.addEventListener('DOMContentLoaded', function() {{
    if (typeof initECharts === 'function') initECharts(document.body);
}});
</script>
</body>
</html>"##,
        name = escape_html(&ctx.company_name),
        body = body,
    )
}
