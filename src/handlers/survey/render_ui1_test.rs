//! UI-1 媒体分析タブ UI 強化のコントラクトテスト (2026-04-26)
//!
//! 目的: render_upload_form / render_analysis_result が
//! ・必要な UI 要素 (DOM ID / data 属性) を含む
//! ・具体的な値（KPI 値、ヒートマップ data 配列、雇用形態100%帯）を含む
//! ・アクセシビリティ属性 (role / aria-label) を含む
//! ことを検証する。
//!
//! 原則 (MEMORY feedback_reverse_proof_tests / feedback_test_data_validation):
//! - 「要素が存在する」だけでなく「具体テキスト/属性値」を assert
//! - 期待値は手計算 / 仕様で算出
//!
//! 関連 memory:
//! - feedback_correlation_not_causation: 因果断定禁止 → "傾向" 用語の検証
//! - feedback_hw_data_scope: HW 限定性 → スコープ注意書きの検証

#![cfg(test)]

use super::aggregator::{EmpGroupNativeAgg, SurveyAggregation};
use super::job_seeker::{JobSeekerAnalysis, SalaryRangePerception};
use super::render::{render_analysis_result, render_upload_form};
use super::statistics::{EnhancedStats, QuartileStats};

// ═══════════════════════════════════════════════════════════════════
// テストヘルパー: 最小限の SurveyAggregation / JobSeekerAnalysis 構築
// ═══════════════════════════════════════════════════════════════════

fn sample_aggregation() -> SurveyAggregation {
    SurveyAggregation {
        total_count: 250,
        new_count: 60,
        salary_parse_rate: 0.85,
        location_parse_rate: 0.92,
        dominant_prefecture: Some("東京都".to_string()),
        dominant_municipality: Some("新宿区".to_string()),
        by_prefecture: vec![
            ("東京都".to_string(), 120),
            ("神奈川県".to_string(), 50),
            ("埼玉県".to_string(), 30),
            ("千葉県".to_string(), 25),
            ("大阪府".to_string(), 25),
        ],
        by_salary_range: vec![
            ("〜20万".to_string(), 30),
            ("20〜25万".to_string(), 80),
            ("25〜30万".to_string(), 90),
            ("30〜35万".to_string(), 35),
            ("35万〜".to_string(), 15),
        ],
        by_employment_type: vec![
            ("正社員".to_string(), 180),
            ("パート".to_string(), 50),
            ("派遣".to_string(), 20),
        ],
        by_tags: vec![("未経験可".to_string(), 60), ("資格不問".to_string(), 40)],
        salary_values: vec![200_000, 250_000, 280_000, 300_000, 350_000],
        enhanced_stats: Some(EnhancedStats {
            count: 5,
            mean: 276_000,
            median: 280_000,
            min: 200_000,
            max: 350_000,
            std_dev: 50_000,
            bootstrap_ci: None,
            trimmed_mean: None,
            quartiles: Some(QuartileStats {
                q1: 240_000,
                q2: 280_000,
                q3: 310_000,
                iqr: 70_000,
                lower_bound: 135_000,
                upper_bound: 415_000,
                outlier_count: 0,
                inlier_count: 5,
            }),
            reliability: "高".to_string(),
        }),
        by_company: Vec::new(),
        by_emp_type_salary: Vec::new(),
        salary_min_values: Vec::new(),
        salary_max_values: Vec::new(),
        by_tag_salary: Vec::new(),
        by_municipality_salary: Vec::new(),
        scatter_min_max: Vec::new(),
        regression_min_max: None,
        by_prefecture_salary: Vec::new(),
        is_hourly: false,
        by_emp_group_native: Vec::<EmpGroupNativeAgg>::new(),
        outliers_removed_total: 12,
        salary_values_raw_count: 262,
    }
}

fn sample_seeker() -> JobSeekerAnalysis {
    JobSeekerAnalysis {
        expected_salary: Some(245_000),
        salary_range_perception: Some(SalaryRangePerception {
            avg_range_width: 60_000,
            avg_lower: 230_000,
            avg_upper: 290_000,
            expected_point: 245_000,
            narrow_count: 30,
            medium_count: 100,
            wide_count: 20,
        }),
        inexperience_analysis: None,
        new_listings_premium: None,
        total_analyzed: 250,
    }
}

// ═══════════════════════════════════════════════════════════════════
// アップロードフォームのコントラクトテスト
// ═══════════════════════════════════════════════════════════════════

#[test]
fn ui1_upload_form_contains_step_guide_with_4_numbered_items() {
    let html = render_upload_form();
    // 使い方ステップが 4 ステップ番号付きで表示されること
    assert!(
        html.contains(r#"id="survey-howto-steps""#),
        "使い方ステップセクション ID が必須"
    );
    // 番号 1, 2, 3, 4 がそれぞれ存在
    for n in &["1", "2", "3", "4"] {
        let pattern = format!(">{}</div>", n);
        assert!(
            html.contains(&pattern),
            "ステップ番号 {} の表示が必要 (期待: '{}')",
            n,
            pattern
        );
    }
    // 4 つのステップ見出しが具体的なラベルを持つ
    for label in &[
        "CSVエクスポート",
        "アップロード",
        "サマリ確認",
        "HW統合分析",
    ] {
        assert!(html.contains(label), "ステップラベル '{}' が必要", label);
    }
}

#[test]
fn ui1_upload_form_source_type_visualized_as_3_radio_cards() {
    let html = render_upload_form();
    // ラジオグループが視覚化されている
    assert!(
        html.contains(r#"id="source-type-cards""#),
        "ソース媒体カードグループ ID が必須"
    );
    assert!(
        html.contains(r#"role="radiogroup""#),
        "アクセシビリティ用 role=radiogroup が必要"
    );
    // 3 つのソース媒体カードが data-source 属性で識別可能
    for src in &["indeed", "jobbox", "other"] {
        let pattern = format!(r#"data-source="{}""#, src);
        assert!(
            html.contains(&pattern),
            "data-source='{}' のカードが必要",
            src
        );
    }
    // 各カードに色マーカーが存在（カラー識別）
    assert!(
        html.contains("bg-blue-500")
            && html.contains("bg-emerald-500")
            && html.contains("bg-amber-500"),
        "ソース媒体ごとに異なる色マーカーが必要"
    );
}

#[test]
fn ui1_upload_form_wage_mode_visualized_as_3_radio_cards() {
    let html = render_upload_form();
    assert!(
        html.contains(r#"id="wage-mode-cards""#),
        "給与単位カードグループ ID が必須"
    );
    for wm in &["monthly", "hourly", "auto"] {
        let pattern = format!(r#"data-wage="{}""#, wm);
        assert!(html.contains(&pattern), "data-wage='{}' のカードが必要", wm);
    }
    // 各モードの説明文が具体的
    assert!(
        html.contains("正社員・契約社員"),
        "月給ベースの想定雇用形態説明が必要"
    );
    assert!(
        html.contains("パート・アルバイト"),
        "時給ベースの想定雇用形態説明が必要"
    );
}

#[test]
fn ui1_upload_form_drop_zone_has_a11y_attributes() {
    let html = render_upload_form();
    // ドロップゾーンが大きく、アニメーション付き、a11y 対応
    assert!(html.contains(r#"id="drop-zone""#), "drop-zone ID 必須");
    assert!(
        html.contains(r#"role="button""#),
        "drop-zone に role=button 必須"
    );
    assert!(
        html.contains("animate-pulse"),
        "drop-zone のアイコンにアニメーション要"
    );
    assert!(
        html.contains("p-10"),
        "drop-zone はゆとりのあるパディング (p-10) で目立たせる"
    );
    // ファイル選択ボタンは 44x44 タッチターゲット
    assert!(
        html.contains("min-h-[44px]"),
        "ファイル選択ボタンは min-h-[44px] (タッチ a11y)"
    );
    // upload-status は aria-live
    assert!(
        html.contains(r#"aria-live="polite""#),
        "アップロード進捗は aria-live=polite"
    );
}

#[test]
fn ui1_upload_form_csv_samples_collapsible_with_indeed_and_jobbox_columns() {
    let html = render_upload_form();
    // サンプル展開セクション
    assert!(
        html.contains(r#"id="survey-csv-samples""#),
        "CSV サンプルセクション ID 必須"
    );
    assert!(html.contains("<details>"), "<details> でアコーディオン化");
    // Indeed の列名（英字）と求人ボックスの列名（日本語）が両方記載
    assert!(
        html.contains("Job Title") && html.contains("Salary"),
        "Indeed の主要列 Job Title / Salary が必要"
    );
    assert!(
        html.contains("求人タイトル") && html.contains("勤務地"),
        "求人ボックスの主要列 (日本語) が必要"
    );
}

// ═══════════════════════════════════════════════════════════════════
// 分析結果（アップロード後）のコントラクトテスト
// ═══════════════════════════════════════════════════════════════════

#[test]
fn ui1_analysis_executive_summary_kpi_cards_with_4_kpis_and_gap() {
    let agg = sample_aggregation();
    let seeker = sample_seeker();
    let html = render_analysis_result(&agg, &seeker, "test-session-001");

    // エグゼクティブサマリのコンテナ
    assert!(
        html.contains(r#"id="survey-executive-summary""#),
        "エグゼクティブサマリ ID 必須"
    );
    assert!(
        html.contains(r#"data-total="250""#),
        "data-total 属性で件数を機械可読化 (期待 250)"
    );

    // 4 つの KPI カードが存在 (region/median/expected/gap)
    for kpi in &["region", "median", "expected", "gap"] {
        let pattern = format!(r#"data-kpi="{}""#, kpi);
        assert!(html.contains(&pattern), "KPI '{}' カード必須", kpi);
    }

    // 給与中央値 280,000 円が表示
    assert!(
        html.contains("280,000円"),
        "給与中央値 280,000円 の数値表示が必要"
    );
    // 求職者期待値 245,000 円が表示
    assert!(
        html.contains("245,000円"),
        "求職者期待値 245,000円 の数値表示が必要"
    );

    // ギャップ計算: (280000 - 245000) / 245000 * 100 = 14.28...%
    // → +14.3% が表示される
    assert!(
        html.contains("+14.3%"),
        "中央値-期待値ギャップ +14.3% の表示が必要"
    );

    // ギャップが +5% 超なので emerald (良好) シグナル
    assert!(
        html.contains("text-emerald-400"),
        "ギャップ良好時の emerald カラー必要"
    );
    // 読み方吹き出し（結論先取り）
    assert!(
        html.contains("survey-summary-readout"),
        "読み方吹き出し ID 必要"
    );
    assert!(
        html.contains("応募集まりやすい給与帯"),
        "ギャップ良好時の具体的な解釈テキストが必要"
    );
}

#[test]
fn ui1_analysis_action_bar_primary_hw_integrate_emphasized() {
    let agg = sample_aggregation();
    let seeker = sample_seeker();
    let html = render_analysis_result(&agg, &seeker, "session-X-42");

    // アクションバーが session_id を保持
    assert!(
        html.contains(r#"id="survey-action-bar""#),
        "アクションバー ID 必須"
    );
    assert!(
        html.contains(r#"data-session-id="session-X-42""#),
        "session_id が data 属性で保持される必要"
    );

    // プライマリ動線: HW 統合分析ボタン
    assert!(
        html.contains(r#"id="btn-hw-integrate""#),
        "HW 統合分析ボタン ID 必須"
    );
    // gradient 強調
    assert!(
        html.contains("bg-gradient-to-r")
            && html.contains("from-blue-600")
            && html.contains("to-blue-500"),
        "プライマリボタンは blue gradient で強調"
    );
    // shadow で目立たせる
    assert!(
        html.contains("shadow-lg") || html.contains("shadow-blue"),
        "プライマリボタンに shadow 必要"
    );
    // PDF 導線は採用コンサルレポートに一本化
    assert!(
        html.contains("採用コンサルレポート PDF"),
        "採用コンサルレポート PDF ボタン必須"
    );
    assert!(
        !html.contains("HW併載版 PDF"),
        "HW併載版 PDF ボタンは通常導線から撤去"
    );
    assert!(
        !html.contains("公開データ中心版 PDF"),
        "公開データ中心版 PDF ボタンは通常導線から撤去"
    );
    assert!(html.contains("HTMLダウンロード"), "HTML DL ボタン必須");
    assert!(html.contains("別のCSVをアップロード"), "別 CSV ボタン必須");
    // タッチターゲット: 全ボタン min-h-[44px]
    let count_44 = html.matches("min-h-[44px]").count();
    assert!(
        count_44 >= 3,
        "アクションバー内のボタンは min-h-[44px] を 3 つ以上 (実際: {})",
        count_44
    );
}

#[test]
fn ui1_analysis_prefecture_heatmap_47_data_points_in_chart_config() {
    let agg = sample_aggregation();
    let seeker = sample_seeker();
    let html = render_analysis_result(&agg, &seeker, "sid");

    // ヒートマップセクション
    assert!(
        html.contains(r#"id="survey-prefecture-heatmap""#),
        "ヒートマップセクション ID 必須"
    );
    // covered 数が data 属性で取得可能 (sample_aggregation では 5 県分のデータ)
    assert!(
        html.contains(r#"data-pref-count="5""#),
        "data-pref-count が掲載のある県数 (期待 5) を反映する必要"
    );
    // 47 県が grid に並ぶ: 「47県」表示
    assert!(html.contains("47県"), "47県の総数表示が必要");

    // ECharts heatmap の data 配列に 47 件分のエントリ
    // 県名 (e.g. 北海道, 沖縄県) が data 配列に含まれている
    for pref in &["北海道", "東京都", "大阪府", "沖縄県", "福島県"] {
        assert!(
            html.contains(pref),
            "ヒートマップに県名 '{}' のエントリ必要",
            pref
        );
    }

    // visualMap (色濃度凡例) が存在
    assert!(
        html.contains(r#""visualMap""#) || html.contains("visualMap"),
        "ECharts heatmap に visualMap (色濃度凡例) 必要"
    );
    // Top 5 テーブル
    assert!(html.contains("掲載件数 Top 5"), "Top 5 テーブル必須");
    // 東京都 120 件が Top 5 に表示
    assert!(
        html.contains("120件"),
        "Top 5 に東京都 120件 が表示される必要"
    );
}

#[test]
fn ui1_analysis_salary_range_chart_iqr_shading_and_outlier_bar() {
    let agg = sample_aggregation();
    let seeker = sample_seeker();
    let html = render_analysis_result(&agg, &seeker, "sid");

    // 給与帯チャートの data-chart 属性
    assert!(
        html.contains(r#"data-chart="salary-range""#),
        "給与帯チャート data-chart 属性必須"
    );

    // 中央値 280000 と平均 276000 の markLine が ECharts config に含まれる
    assert!(
        html.contains("280000"),
        "中央値 280000 が markLine に含まれる必要"
    );
    assert!(html.contains("中央値"), "凡例ラベル '中央値' 必要");
    assert!(html.contains("平均"), "凡例ラベル '平均' 必要");

    // IQR シェード (Q1=240000, Q3=310000)
    assert!(
        html.contains("240000") && html.contains("310000"),
        "IQR Q1 240000 / Q3 310000 が markArea に含まれる必要"
    );
    assert!(html.contains("IQR"), "凡例に IQR ラベルが必要");

    // 外れ値除外バー: outliers_removed_total=12, raw=262, kept=250
    assert!(
        html.contains(r#"id="outlier-removal-bar""#),
        "外れ値除外バー ID 必須"
    );
    assert!(
        html.contains("12件除外"),
        "除外件数 (12件) のテキスト表示必要"
    );
    assert!(
        html.contains("Tukey"),
        "Tukey 法（外れ値判定法）の言及必要 (用語説明)"
    );

    // 読み方吹き出し
    assert!(
        html.contains("読み方:"),
        "給与チャート下に読み方吹き出し必須"
    );
    assert!(
        html.contains("ボリュームゾーン"),
        "中央値の意味を説明する具体的な用語が必要"
    );
}

#[test]
fn ui1_analysis_employment_type_chart_100_percent_stacked_bar() {
    let agg = sample_aggregation();
    let seeker = sample_seeker();
    let html = render_analysis_result(&agg, &seeker, "sid");

    // 雇用形態チャート
    assert!(
        html.contains(r#"data-chart="employment-type""#),
        "雇用形態チャート data-chart 属性必須"
    );
    // 100% 帯
    assert!(
        html.contains(r#"data-stack="employment-100""#),
        "100%帯 data-stack 属性必須"
    );
    // 雇用形態名がツールチップに含まれる
    assert!(
        html.contains("正社員 180件"),
        "正社員 180件 ツールチップ必要"
    );
    assert!(html.contains("パート 50件"), "パート 50件 ツールチップ必要");

    // 最多雇用形態の読み方
    // 250 件中 180 = 72.0%
    assert!(
        html.contains("72.0%"),
        "正社員シェア 72.0% の読み方表記必要"
    );
    assert!(
        html.contains("最多は「正社員」"),
        "最多雇用形態の具体的な解釈テキスト必要"
    );
    assert!(
        html.contains(r#"role="img""#),
        "100%帯は role=img でアクセシビリティ確保"
    );
}

#[test]
fn ui1_analysis_kpi_info_tooltips_explain_methodology() {
    let agg = sample_aggregation();
    let seeker = sample_seeker();
    let html = render_analysis_result(&agg, &seeker, "sid");

    // ⓘ アイコンが各 KPI に存在
    let info_count = html.matches("kpi-info").count();
    assert!(
        info_count >= 4,
        "KPI ⓘ アイコンが 4 個以上必要 (実際: {})",
        info_count
    );

    // 各 ⓘ には説明文（title 属性）
    assert!(
        html.contains("50パーセンタイル"),
        "中央値の方法論説明 (50パーセンタイル) 必要"
    );
    assert!(
        html.contains("レンジ下限"),
        "求職者期待値の方法論説明 (レンジ下限+1/3) 必要"
    );
    // IQR シェードのヒント
    assert!(html.contains("中央50%"), "IQR の意味 (中央50%) 説明必要");

    // a11y: ⓘ は role=button + aria-label
    assert!(
        html.contains(r#"role="button""#) && html.contains("aria-label="),
        "ⓘ アイコンは role=button + aria-label 必要"
    );
}
