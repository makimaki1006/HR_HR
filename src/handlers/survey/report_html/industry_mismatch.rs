//! 印刷レポート: 産業ミスマッチ警戒 section (CR-9 / 2026-04-27 追加)
//!
//! ## 背景
//! ユーザー指摘「採用市場 逼迫度の次に、地域就業者構成と HW 求人構成のギャップを表で見たい」
//! ただし「ギャップを見るだけで原因解釈はしない」「採用容易性の直結ではない」前提。
//!
//! ## 入力
//! 1. **就業者構成**: `industry_employees` (国勢調査ベース、`industry_name` + `employees_total`)
//!    - ソース: `fetch_industry_structure` (handlers/analysis/fetch/subtab5_phase4_7.rs:202)
//!    - 集計コード (AS / AR / CR) は呼び出し側で除外済みであること
//! 2. **HW 求人産業分布**: `hw_industry_counts` (`(産業大分類名, 求人件数)` のスライス)
//!    - 現状 InsightContext には集計フィールド未実装のため、本 section では
//!      呼び出し側で集計済みデータを渡す前提とする (本タスクスコープ外で fetch 実装)。
//!    - 空スライスを渡すと section ごと非表示 (fail-soft)
//!
//! ## 産業マッピング
//! 国勢調査 industry_name と HW industry_raw は粒度が異なる可能性があるため、
//! 以下 12 大分類 (日本標準産業分類 大分類 A〜S をベース) でマッピング:
//! - 農林漁業 / 鉱業,採石業,砂利採取業 / 建設業 / 製造業 / 電気・ガス・熱供給・水道業
//! - 情報通信業 / 運輸業,郵便業 / 卸売業,小売業 / 金融業,保険業 / 不動産業,物品賃貸業
//! - 学術研究,専門・技術サービス業 / 宿泊業,飲食サービス業
//! - 生活関連サービス業,娯楽業 / 教育,学習支援業 / 医療,福祉
//! - 複合サービス事業 / サービス業（他に分類されないもの） / 公務（他に分類されるものを除く）
//! - 分類不能の産業
//!
//! 既存の `industry_name` 文字列を優先し、近似マッチが必要な場合は `normalize_industry_name`
//! でブレを吸収する。
//!
//! ## 出力構造
//! 表のみ (チャートなし、印刷向け簡素設計):
//! - 産業 / 就業者構成比 / HW 求人構成比 / ギャップ (pt) / 解釈
//!
//! ## 解釈ロジック
//! - ギャップ < -10pt: 「求人不足の可能性」(就業者多いのに求人少ない) - 赤系
//! - ギャップ > +10pt: 「求人過剰の可能性」(就業者少ないのに求人多い) - 緑系
//! - |ギャップ| ≤ 10pt: 「整合」 - グレー
//!
//! ## 設計原則 (memory ルール準拠)
//! - `feedback_correlation_not_causation.md`: ギャップ表示のみ、原因解釈はユーザーに委ねる
//! - `feedback_hw_data_scope.md`: 「HW 登録求人のみで全求人市場ではない」明記
//! - `feedback_test_data_validation.md`: 構成比 28% / ギャップ -16pt を assert_eq で逆証明
//! - `feedback_never_guess_data.md`: industry_name は実カラム grep 確認済み
//! - `feedback_reverse_proof_tests.md`: 構成比合計 ≈ 100 の不変条件を必須テスト
//!
//! ## 公開 API
//! - `render_section_industry_mismatch(html, industry_employees, hw_industry_counts)`

#![allow(dead_code)]

use super::super::super::helpers::{escape_html, get_i64, get_str_ref, Row};

use super::helpers::{render_figure_caption, render_read_hint, render_section_howto};

// =====================================================================
// 公開 API
// =====================================================================

/// 「産業ミスマッチ警戒」section 全体を描画
///
/// # 引数
/// - `industry_employees`: 国勢調査ベースの就業者数行 (`industry_name`, `employees_total`)。
///   呼び出し側で集計コード (AS / AR / CR) を除外しておくこと。
/// - `hw_industry_counts`: HW 求人の産業大分類別件数。空スライスなら fail-soft で section 非表示。
///
/// # Fail-soft 条件
/// - `industry_employees` が空、または 全行の employees_total 合計が 0 以下
/// - `hw_industry_counts` が空、または 件数合計が 0 以下
/// - 両者でマッチする産業が 0 件
/// 2026-04-29 (Public variant): CSV 媒体掲載求人 vs 国勢調査就業者構成 のミスマッチ
///
/// HW を介さず、ユーザーがアップロードした CSV の業種分布 (推定) と
/// 国勢調査の就業者構成を比較する。CSV 側は SurveyAggregation の `by_tags` /
/// `by_employment_type` 等から **キーワード推定** する。
///
/// # 推定優先順位 (本実装の制約)
/// SurveyAggregation には個別レコードや産業列が含まれないため、以下の信号で推定:
/// 1. `agg.by_tags` (タグ別集計、上位 30 件) を `map_keyword_to_major_industry` で分類
/// 2. fallback: 推定 0 件なら section 非表示 (fail-soft)
///
/// **注意**: 元 CSV に独立した「業種」列がある (求人ボックス等) 場合でも、
/// 現行 SurveyAggregation はそれを保持しないため、本関数では参照できない。
/// この制約は出力 HTML の caveat に明記する。
///
/// # Fail-soft 条件
/// - `industry_employees` が空 / 合計 0
/// - `agg.by_tags` から推定された産業件数合計が 0
/// - 双方 0.5% 未満の行のみ → 残行 0 件
///
/// # 解釈ロジック (HW 版とは符号反対)
/// - gap ≥ +10pt: 「CSV 重点業界」(CSV に多い)
/// - gap ≤ -10pt: 「CSV 過少」 (CSV に少ない)
/// - |gap| < 10pt: 「整合」
///
/// gap = csv_pct - emp_pct
pub(super) fn render_section_industry_mismatch_csv(
    html: &mut String,
    industry_employees: &[Row],
    agg: &super::super::aggregator::SurveyAggregation,
) {
    // ---- fail-soft: 国勢調査側 ----
    if industry_employees.is_empty() {
        return;
    }
    let employee_total: i64 = industry_employees
        .iter()
        .map(|r| get_i64(r, "employees_total"))
        .sum();
    if employee_total <= 0 {
        return;
    }

    // ---- CSV 側: タグから業種推定 ----
    let csv_industry_counts = estimate_csv_industry_counts(agg);
    let csv_total: i64 = csv_industry_counts.iter().map(|(_, c)| *c).sum();
    if csv_total <= 0 || csv_industry_counts.is_empty() {
        return;
    }

    // ---- ギャップ計算 ----
    let rows = build_csv_mismatch_rows(industry_employees, &csv_industry_counts);
    if rows.is_empty() {
        return;
    }

    // ---- HTML 出力 ----
    // Round 23 (2026-05-13): 設計メモ §18 準拠で表現中立化 + 推定信頼度警告。
    // 「産業ミスマッチ」「CSV重点業界」「ギャップ」等の強い表現を中立化し、
    // CSV 側が推定分類であることを明示する。
    html.push_str("<div class=\"section\" data-testid=\"industry-mismatch-csv-section\">\n");
    html.push_str("<h2>CSV 推定業種構成と地域産業構成の比較</h2>\n");

    // 上位 1 カテゴリ占有率 (推定信頼度判定)
    let total_csv: i64 = rows.iter().map(|r| r.csv_count).sum();
    let top_share = rows.iter().map(|r| r.csv_count).max().unwrap_or(0) as f64
        / total_csv.max(1) as f64;
    if top_share >= 0.85 {
        html.push_str(
            "<p style=\"font-size:10pt;color:#7f1d1d;background:#fef2f2;padding:8px 12px;border-left:4px solid #b91c1c;margin:8px 0;\">\
             <strong>⚠ 推定信頼度: 低</strong> — CSV 推定業種の上位カテゴリが 85% を超えており、分類ロジックまたは検索条件の偏りが強い可能性があります。\
             公的統計とのギャップ解釈は行わず、参考表示に留めることを推奨します。\
             給与判断は §3-B 給与構造クラスタ分析を主軸にしてください。\
             </p>\n",
        );
    } else if top_share >= 0.70 {
        html.push_str(
            "<p style=\"font-size:10pt;color:#78350f;background:#fef3c7;padding:8px 12px;border-left:4px solid #d97706;margin:8px 0;\">\
             <strong>⚠ 推定信頼度: 中</strong> — CSV 推定業種の上位カテゴリが 70% を超えています。\
             検索条件・媒体特性・分類キーワードの偏り、または推定誤差がある可能性があります。\
             この業種構成は参考値として扱い、給与判断には §3-B 給与構造クラスタ分析を併用してください。\
             </p>\n",
        );
    }

    render_section_howto(
        html,
        &[
            "アップロードした CSV の推定業種構成と地域の就業者構成 (国勢調査) の差分を表示します",
            "差分 ≥ +10pt: CSV 内 推定多出現 / ≤ -10pt: CSV 内 推定少数 / |差分| < 10pt: 差分小",
            "業種は CSV のタグ列・職種列・企業名からキーワード推定したもので、推定誤差を含みます。給与判断は §3-B 給与構造クラスタ分析を主軸にしてください",
        ],
    );

    render_figure_caption(
        html,
        "表 4B-1",
        "CSV 推定業種分布 vs 国勢調査就業者構成 (大分類)",
    );

    html.push_str(
        "<table class=\"sortable-table zebra\" data-testid=\"industry-mismatch-csv-table\">\n",
    );
    html.push_str(
        "<thead><tr>\
        <th>産業</th>\
        <th style=\"text-align:right\">CSV 推定構成比</th>\
        <th style=\"text-align:right\">就業者構成比</th>\
        <th style=\"text-align:right\">差分</th>\
        <th>解釈</th>\
        </tr></thead>\n<tbody>\n",
    );

    for r in &rows {
        let (interp_label, color_class) = classify_csv_gap(r.gap_pt);
        html.push_str(&format!(
            "<tr>\
                <td>{name}</td>\
                <td class=\"num\">{csv_pct:.1}% ({csv_n}件)</td>\
                <td class=\"num\">{emp_pct:.1}%</td>\
                <td class=\"num\" style=\"color:{color};font-weight:600;\" data-gap=\"{gap_raw:.1}\">{gap_sign}{gap_abs:.1}pt</td>\
                <td><span class=\"{cls}\">{interp}</span></td>\
            </tr>\n",
            name = escape_html(&r.industry_name),
            csv_pct = r.csv_pct,
            csv_n = r.csv_count,
            emp_pct = r.emp_pct,
            color = color_class,
            gap_raw = r.gap_pt,
            gap_sign = if r.gap_pt >= 0.0 { "+" } else { "-" },
            gap_abs = r.gap_pt.abs(),
            cls = match interp_label {
                "CSV 内 推定多出現" => "gap-pos",
                "CSV 内 推定少数" => "gap-neg",
                _ => "gap-neutral",
            },
            interp = interp_label,
        ));
    }
    html.push_str("</tbody></table>\n");

    // 必須 caveat (CSV 推定の限界 + 国勢調査スコープ + 因果非主張)
    html.push_str(
        "<p class=\"caveat\" style=\"font-size:9pt;color:#475569;margin-top:8px;\">\
        \u{26A0} CSV 業種は職種列・タグ列・会社名 (例: 「メディカル」「ケアセンター」「建設」等) からのキーワード推定です。元 CSV に業種列がない場合精度に限界があります。\
        就業者構成は国勢調査 (5 年に 1 回、最新 2020 年)。\
        CSV はユーザー指定の媒体掲載求人で、地域全体を代表しません。\
        ギャップは CSV の業種傾向と地域就業者の差を示すもので、採用優劣評価ではありません。\
        本表は相関の可視化であり、因果の証明ではありません。\
        </p>\n",
    );

    render_read_hint(
        html,
        "ギャップが大きい産業は、媒体掲載のしやすさ・採用ニーズ強度・職種特性などの複合要因を示唆します。\
         具体的な原因解釈は別途現場ヒアリング等で検証してください。",
    );

    html.push_str("</div>\n");
}

// =====================================================================
// CSV 業種推定
// =====================================================================

/// CSV 集計結果から「産業大分類 → 件数」の概算を推定する
///
/// 信号源: `agg.by_tags` (タグ別集計、件数つき)
///
/// 各タグ文字列を `map_keyword_to_major_industry` で分類し、件数を合算する。
/// 分類不能 (どのキーワードにもマッチしない) なタグは合算対象外
/// (ノイズを「サービス業 (他)」に押し込まない設計)。
///
/// # 戻り値
/// `Vec<(産業大分類名, 件数)>`
/// 件数降順、合計 0 のときは空 Vec。
pub(crate) fn estimate_csv_industry_counts(
    agg: &super::super::aggregator::SurveyAggregation,
) -> Vec<(String, i64)> {
    let mut industry_counts: std::collections::HashMap<&'static str, i64> =
        std::collections::HashMap::new();

    // 信号 1: by_tags (タグ列ある場合の最優先信号)
    for (tag, count) in &agg.by_tags {
        if let Some(industry) = map_keyword_to_major_industry(tag) {
            *industry_counts.entry(industry).or_insert(0) += *count as i64;
        }
    }

    // 信号 2 (2026-04-30 拡張): by_company の会社名から推定
    // Indeed/求人ボックス CSV にタグ列が無い場合の主要信号源。
    // 会社名に「メディカル」「病院」「介護」「建設」等のキーワードを含む場合、
    // 当該企業の求人件数 (CompanyAgg.count) を該当大分類に加算。
    // 注意: 会社名は業種を完全に表すわけではなく推定誤差を含むため、caveat で明示する。
    for company in &agg.by_company {
        if let Some(industry) = map_keyword_to_major_industry(&company.name) {
            *industry_counts.entry(industry).or_insert(0) += company.count as i64;
        }
    }

    let mut result: Vec<(String, i64)> = industry_counts
        .into_iter()
        .map(|(k, v)| (k.to_string(), v))
        .collect();
    result.sort_by(|a, b| b.1.cmp(&a.1));
    result
}

/// キーワード (タグ・職種等) → 12 大分類マッピング
///
/// `map_hw_to_major_industry` と同じカテゴリ体系だが、CSV のタグ・職種は
/// HW の `industry_raw` と語彙が異なるため、キーワードを CSV 寄りに調整。
///
/// **不一致の場合は `None`** を返す (HW 版は fallback で「サービス業 (他)」だが、
/// CSV 推定では誤分類を避けるため None で除外する)。
///
/// メモリルール `feedback_correlation_not_causation`: マッピング誤差は caveat で言及。
pub(crate) fn map_keyword_to_major_industry(keyword: &str) -> Option<&'static str> {
    let s = keyword;
    if s.is_empty() {
        return None;
    }

    // 医療・福祉系 (専門度高、最優先)
    if s.contains("看護")
        || s.contains("准看")
        || s.contains("病院")
        || s.contains("医療")
        || s.contains("メディカル")
        || s.contains("診療")
        || s.contains("クリニック")
        || s.contains("歯科")
        || s.contains("デンタル")
        || s.contains("助産")
        || s.contains("獣医")
        || s.contains("社会福祉")
        || s.contains("児童福祉")
        || s.contains("障害者")
        || s.contains("障がい者")
        || s.contains("老人")
        || s.contains("介護")
        || s.contains("ケアセンター")
        || s.contains("ケアマネ")
        || s.contains("ケアホーム")
        || s.contains("デイケア")
        || s.contains("ヘルパー")
        || s.contains("保育")
        || s.contains("精神保健")
        || s.contains("リハビリ")
        || s.contains("理学療法")
        || s.contains("作業療法")
        || s.contains("言語聴覚")
        || s.contains("薬剤師")
        || s.contains("管理栄養士")
        || s.contains("栄養士")
        || s.contains("生活支援員")
        || s.contains("生活相談員")
        || s.contains("サービス提供責任者")
        || s.contains("サービス管理責任者")
        || s.contains("児童指導員")
        || s.contains("児童発達支援")
        || s.contains("デイサービス")
        || s.contains("グループホーム")
        || s.contains("特別養護")
        || s.contains("有料老人ホーム")
        || s.contains("訪問看護")
        || s.contains("訪問介護")
        || s.contains("通所介護")
        || s.contains("福祉")
    {
        return Some("医療，福祉");
    }
    // 建設業
    if s.contains("建設")
        || s.contains("土木")
        || s.contains("建築")
        || s.contains("総合工事")
        || s.contains("設備工事")
        || s.contains("塗装")
        || s.contains("舗装")
        || s.contains("配管")
        || s.contains("電気工事")
        || s.contains("内装")
        || s.contains("大工")
        || s.contains("左官")
        || s.contains("施工管理")
    {
        return Some("建設業");
    }
    // 製造業
    if s.contains("製造")
        || s.contains("食料品")
        || s.contains("飲料")
        || s.contains("繊維")
        || s.contains("衣服")
        || s.contains("木材")
        || s.contains("家具")
        || s.contains("印刷")
        || s.contains("化学")
        || s.contains("プラスチック")
        || s.contains("ゴム")
        || s.contains("窯業")
        || s.contains("金属加工")
        || s.contains("機械加工")
        || s.contains("輸送用機器")
        || s.contains("精密機器")
        || s.contains("組立")
        || s.contains("工場")
        || s.contains("生産工程")
        || s.contains("溶接")
        || s.contains("検品")
    {
        return Some("製造業");
    }
    // 運輸業，郵便業
    if s.contains("運輸")
        || s.contains("運送")
        || s.contains("配送")
        || s.contains("郵便")
        || s.contains("貨物")
        || s.contains("旅客")
        || s.contains("鉄道")
        || s.contains("自動車運送")
        || s.contains("倉庫")
        || s.contains("配達")
        || s.contains("ドライバー")
        || s.contains("トラック")
        || s.contains("タクシー")
        || s.contains("バス運転")
    {
        return Some("運輸業，郵便業");
    }
    // 卸売業，小売業
    if s.contains("卸売")
        || s.contains("小売")
        || s.contains("販売店")
        || s.contains("百貨店")
        || s.contains("スーパー")
        || s.contains("コンビニ")
        || s.contains("商業")
        || s.contains("レジ")
        || s.contains("店員")
        || s.contains("販売スタッフ")
        || s.contains("ショップ")
    {
        return Some("卸売業，小売業");
    }
    // 宿泊業，飲食サービス業
    if s.contains("飲食")
        || s.contains("レストラン")
        || s.contains("食堂")
        || s.contains("酒場")
        || s.contains("ビヤホール")
        || s.contains("バー")
        || s.contains("喫茶")
        || s.contains("カフェ")
        || s.contains("旅館")
        || s.contains("ホテル")
        || s.contains("宿泊")
        || s.contains("料理店")
        || s.contains("給食")
        || s.contains("ホール")
        || s.contains("キッチン")
        || s.contains("調理")
    {
        return Some("宿泊業，飲食サービス業");
    }
    // 情報通信業
    if s.contains("ソフトウェア")
        || s.contains("情報サービス")
        || s.contains("通信業")
        || s.contains("情報通信")
        || s.contains("インターネット")
        || s.contains("放送")
        || s.contains("映像")
        || s.contains("出版")
        || s.contains("新聞")
        || s.contains("Web")
        || s.contains("プログラマ")
        || s.contains("プログラマー")
        || s.contains("エンジニア")
        || s.contains("システム開発")
        || s.contains("SE")
        || s.contains("IT")
    {
        return Some("情報通信業");
    }
    // 教育，学習支援業
    if s.contains("学校")
        || s.contains("教育")
        || s.contains("学習支援")
        || s.contains("塾")
        || s.contains("予備校")
        || s.contains("教習所")
        || s.contains("学習教室")
        || s.contains("講師")
        || s.contains("教員")
    {
        return Some("教育，学習支援業");
    }
    // 不動産業，物品賃貸業
    if s.contains("不動産") || s.contains("物品賃貸") || s.contains("レンタル") {
        return Some("不動産業，物品賃貸業");
    }
    // 金融業，保険業
    // 注: 「保険」単独は「健康保険あり/労災保険あり/雇用保険あり」(福利厚生タグ) と
    //     誤マッチするため、業界実体を示す複合語のみに限定する。
    if s.contains("金融業")
        || s.contains("銀行")
        || s.contains("保険業")
        || s.contains("保険会社")
        || s.contains("生命保険")
        || s.contains("損害保険")
        || s.contains("証券会社")
        || s.contains("信用組合")
        || s.contains("信用金庫")
    {
        return Some("金融業，保険業");
    }
    // 農林漁業
    if s.contains("農業") || s.contains("林業") || s.contains("漁業") || s.contains("水産")
    {
        return Some("農林漁業");
    }
    // 鉱業
    if s.contains("鉱業") || s.contains("採石") || s.contains("砂利") {
        return Some("鉱業");
    }
    // 電気・ガス・熱供給・水道業
    if (s.contains("電気") && s.contains("供給"))
        || s.contains("ガス業")
        || s.contains("熱供給")
        || s.contains("水道業")
    {
        return Some("電気・ガス・熱供給・水道業");
    }
    // 学術研究，専門・技術サービス業
    if s.contains("学術")
        || s.contains("研究所")
        || (s.contains("専門") && s.contains("技術"))
        || s.contains("広告")
        || s.contains("デザイン")
        || s.contains("法務")
        || s.contains("会計")
        || s.contains("コンサル")
        || s.contains("経営戦略")
        || s.contains("市場調査")
    {
        return Some("学術研究，専門・技術サービス業");
    }
    // 生活関連サービス業，娯楽業
    if s.contains("理容")
        || s.contains("美容")
        || s.contains("クリーニング")
        || s.contains("浴場")
        || s.contains("娯楽")
        || s.contains("遊技場")
        || s.contains("興行")
        || s.contains("冠婚葬祭")
        || s.contains("葬儀")
        || s.contains("結婚")
        || s.contains("写真館")
        || s.contains("旅行")
        || s.contains("生活関連サービス")
    {
        return Some("生活関連サービス業，娯楽業");
    }
    // 公務
    if s.contains("公務") || s.contains("公務員") {
        return Some("公務（他に分類されるものを除く）");
    }
    // 複合サービス事業
    if s.contains("複合サービス") || s.contains("協同組合") {
        return Some("複合サービス事業");
    }
    // サービス業 (他)
    if s.contains("派遣")
        || s.contains("人材紹介")
        || s.contains("職業紹介")
        || s.contains("建物管理")
        || s.contains("ビルメンテナンス")
        || s.contains("警備")
        || s.contains("清掃")
        || s.contains("廃棄物")
        || s.contains("修理")
        || s.contains("メンテナンス")
        || s.contains("設備管理")
        || s.contains("事業サービス")
    {
        return Some("サービス業（他に分類されないもの）");
    }

    // 不明 → None (CSV 推定では誤分類を避けるため除外)
    None
}

/// CSV 推定向け Mismatch 行
#[derive(Debug, Clone, PartialEq)]
struct CsvMismatchRow {
    industry_name: String,
    csv_count: i64,
    emp_pct: f64,
    csv_pct: f64,
    gap_pt: f64, // csv_pct - emp_pct (正: CSV 重点 / 負: CSV 過少)
}

/// CSV 推定値と就業者構成から CsvMismatchRow を構築
fn build_csv_mismatch_rows(
    industry_employees: &[Row],
    csv_industry_counts: &[(String, i64)],
) -> Vec<CsvMismatchRow> {
    let employee_total: i64 = industry_employees
        .iter()
        .map(|r| get_i64(r, "employees_total"))
        .sum();
    let csv_total: i64 = csv_industry_counts.iter().map(|(_, c)| *c).sum();

    if employee_total <= 0 || csv_total <= 0 {
        return Vec::new();
    }

    // 就業者: industry_name (normalize) -> employees_total
    let mut emp_map: std::collections::HashMap<String, i64> = std::collections::HashMap::new();
    for r in industry_employees {
        let name = get_str_ref(r, "industry_name");
        if name.is_empty() {
            continue;
        }
        let emp = get_i64(r, "employees_total");
        if emp <= 0 {
            continue;
        }
        *emp_map.entry(normalize_industry_name(name)).or_insert(0) += emp;
    }

    // CSV 推定: industry_name (normalize) -> count
    let mut csv_map: std::collections::HashMap<String, i64> = std::collections::HashMap::new();
    let mut csv_display_name: std::collections::HashMap<String, String> =
        std::collections::HashMap::new();
    for (name, c) in csv_industry_counts {
        if name.is_empty() || *c <= 0 {
            continue;
        }
        let key = normalize_industry_name(name);
        *csv_map.entry(key.clone()).or_insert(0) += *c;
        csv_display_name.entry(key).or_insert_with(|| name.clone());
    }

    // Union (両側 0% でも片側存在すれば残す)
    let mut all_keys: std::collections::BTreeSet<String> = std::collections::BTreeSet::new();
    for k in emp_map.keys() {
        all_keys.insert(k.clone());
    }
    for k in csv_map.keys() {
        all_keys.insert(k.clone());
    }

    let mut rows: Vec<CsvMismatchRow> = Vec::new();
    for key in all_keys {
        let emp = *emp_map.get(&key).unwrap_or(&0);
        let csv = *csv_map.get(&key).unwrap_or(&0);
        if emp == 0 && csv == 0 {
            continue;
        }
        let emp_pct = (emp as f64) / (employee_total as f64) * 100.0;
        let csv_pct = (csv as f64) / (csv_total as f64) * 100.0;
        let gap_pt = csv_pct - emp_pct;

        // 表示名: CSV 由来優先、なければ就業者の元名称
        let display = csv_display_name
            .get(&key)
            .cloned()
            .or_else(|| {
                industry_employees.iter().find_map(|r| {
                    let n = get_str_ref(r, "industry_name");
                    if normalize_industry_name(n) == key {
                        Some(n.to_string())
                    } else {
                        None
                    }
                })
            })
            .unwrap_or_else(|| key.clone());

        rows.push(CsvMismatchRow {
            industry_name: display,
            csv_count: csv,
            emp_pct,
            csv_pct,
            gap_pt,
        });
    }

    // B9 仕様: 両側 0.5% 未満は除外
    rows.retain(|r| r.emp_pct >= 0.5 || r.csv_pct >= 0.5);

    // ギャップ絶対値降順
    rows.sort_by(|a, b| {
        b.gap_pt
            .abs()
            .partial_cmp(&a.gap_pt.abs())
            .unwrap_or(std::cmp::Ordering::Equal)
    });

    rows
}

/// CSV 版ギャップ → (解釈ラベル, 色)
///
/// HW 版の `classify_gap` とは符号の意味が逆: gap = csv_pct - emp_pct
/// - gap ≥ +10pt: CSV 重点業界 (CSV に多く、地域就業者では少ない)
/// - gap ≤ -10pt: CSV 過少 (地域就業者が多いのに CSV では少ない)
/// - |gap| < 10pt: 整合
// Round 23 (2026-05-13): 設計メモ §18.4 準拠で表現を中立化:
//   「CSV 重点業界」→「CSV 内 推定多出現」
//   「CSV 過少」    →「CSV 内 推定少数」
//   「整合」         →「差分小」(整合は意味的に強すぎる)
fn classify_csv_gap(gap_pt: f64) -> (&'static str, &'static str) {
    if gap_pt >= 10.0 {
        ("CSV 内 推定多出現", "#10b981") // 緑系
    } else if gap_pt <= -10.0 {
        ("CSV 内 推定少数", "#dc2626") // 赤系
    } else {
        ("差分小", "#64748b") // グレー
    }
}

pub(super) fn render_section_industry_mismatch(
    html: &mut String,
    industry_employees: &[Row],
    hw_industry_counts: &[(String, i64)],
) {
    // ---- fail-soft ガード ----
    if industry_employees.is_empty() || hw_industry_counts.is_empty() {
        return;
    }

    let employee_total: i64 = industry_employees
        .iter()
        .map(|r| get_i64(r, "employees_total"))
        .sum();
    let hw_total: i64 = hw_industry_counts.iter().map(|(_, c)| *c).sum();
    if employee_total <= 0 || hw_total <= 0 {
        return;
    }

    // ---- ギャップ計算 ----
    let rows = build_mismatch_rows(industry_employees, hw_industry_counts);
    if rows.is_empty() {
        return;
    }

    // ---- HTML 出力 ----
    html.push_str("<div class=\"section\" data-testid=\"industry-mismatch-section\">\n");
    html.push_str("<h2>産業ミスマッチ (地域就業者構成 vs HW 求人構成)</h2>\n");

    render_section_howto(
        html,
        &[
            "対象地域の「就業者構成 (国勢調査)」と「HW 登録求人の産業構成」のギャップを表示します",
            "ギャップ < -10pt: 求人不足の可能性 / > +10pt: 求人過剰の可能性 / |ギャップ| ≤ 10pt: 整合",
            "ギャップは即・採用しやすさを意味しません。地域構造理解の参考値として活用してください",
        ],
    );

    render_figure_caption(
        html,
        "表 4B-1",
        "産業別 就業者構成比 vs HW 求人構成比 (大分類)",
    );

    html.push_str(
        "<table class=\"sortable-table zebra\" data-testid=\"industry-mismatch-table\">\n",
    );
    html.push_str(
        "<thead><tr>\
        <th>産業</th>\
        <th style=\"text-align:right\">就業者構成比</th>\
        <th style=\"text-align:right\">HW 求人構成比</th>\
        <th style=\"text-align:right\">ギャップ</th>\
        <th>解釈</th>\
        </tr></thead>\n<tbody>\n",
    );

    for r in &rows {
        let (interp_label, color_class) = classify_gap(r.gap_pt);
        html.push_str(&format!(
            "<tr>\
                <td>{name}</td>\
                <td class=\"num\">{emp_pct:.1}%</td>\
                <td class=\"num\">{hw_pct:.1}%</td>\
                <td class=\"num\" style=\"color:{color};font-weight:600;\" data-gap=\"{gap_raw:.1}\">{gap_sign}{gap_abs:.1}pt</td>\
                <td><span class=\"{cls}\">{interp}</span></td>\
            </tr>\n",
            name = escape_html(&r.industry_name),
            emp_pct = r.emp_pct,
            hw_pct = r.hw_pct,
            color = color_class,
            gap_raw = r.gap_pt,
            gap_sign = if r.gap_pt >= 0.0 { "+" } else { "-" },
            gap_abs = r.gap_pt.abs(),
            cls = match interp_label {
                "求人不足の可能性" => "gap-neg",
                "求人過剰の可能性" => "gap-pos",
                _ => "gap-neutral",
            },
            interp = interp_label,
        ));
    }
    html.push_str("</tbody></table>\n");

    // 必須 caveat (HW スコープ + 因果非主張)
    html.push_str(
        "<p class=\"caveat\" style=\"font-size:9pt;color:#475569;margin-top:8px;\">\
        \u{26A0} 就業者構成は国勢調査 (5 年に 1 回、最新 2020 年)。\
        HW 求人は HW 登録求人のみで全求人市場ではありません。\
        ギャップは即・採用しやすさを意味せず、地域構造理解の参考値です。\
        本表は相関の可視化であり、因果の証明ではありません。\
        </p>\n",
    );

    render_read_hint(
        html,
        "ギャップが大きい産業は、求人媒体ミックス (HW vs 民間 vs 紹介) の地域差を示唆する場合があります。\
         ただし「就業者が多い = 採用しやすい」ではなく、職種・条件マッチングが本質的要因です。",
    );

    html.push_str("</div>\n");
}

// =====================================================================
// 内部ロジック
// =====================================================================

/// 1 行の集計結果
#[derive(Debug, Clone, PartialEq)]
struct MismatchRow {
    industry_name: String,
    employees: i64,
    hw_count: i64,
    emp_pct: f64,
    hw_pct: f64,
    gap_pt: f64, // hw_pct - emp_pct (正: 求人過剰側 / 負: 求人不足側)
}

/// 就業者と HW の産業マッピング (12 大分類ベース、ブレ吸収)
///
/// 国勢調査の `industry_name` は通常以下のような長い名称:
///   "医療，福祉" / "卸売業，小売業" / "製造業" 等
/// HW の `industry_raw` は短縮形が多いが、本関数では呼び出し側で
/// 大分類名に正規化されている前提で文字列マッチング (normalize 後の == 比較)。
fn normalize_industry_name(raw: &str) -> String {
    // カンマ (全角／半角)、空白を除去して比較を緩める
    raw.chars()
        .filter(|c| !matches!(*c, '，' | ',' | ' ' | '　' | '・' | '、'))
        .collect()
}

/// HW の `industry_raw` (詳細分類、JSIC 小分類レベルが多い) を
/// 国勢調査 大分類 (本モジュールの 12 大分類版) にマッピング。
///
/// keyword マッチング方式。完全一致せず曖昧な場合「サービス業（他に分類されないもの）」に
/// fallback。これは雑処理だが「ギャップ表示の参考値」用途であり厳密一致は不要。
///
/// メモリルール `feedback_correlation_not_causation`: マッピング誤差は注記で言及。
///
/// keyword の評価順は **専門的→汎用** へ。例えば「建物管理」は「サービス業」より先に判定する。
/// 2026-04-27 拡張: 実 postings 上位値 (派遣/建物管理/警備/清掃/その他生活関連サービス等)
/// が「サービス業（他に分類されないもの）」一極集中する問題 (B2) を解消するため、
/// 生活関連サービス系・職業紹介派遣業 (派遣) 系を専用カテゴリに先回しで分岐。
pub(crate) fn map_hw_to_major_industry(industry_raw: &str) -> &'static str {
    let s = industry_raw;
    // 医療・福祉系 (専門度高、最優先)
    if s.contains("病院")
        || s.contains("医療")
        || s.contains("診療")
        || s.contains("歯科")
        || s.contains("助産")
        || s.contains("看護")
        || s.contains("獣医")
        || s.contains("社会保険")
        || s.contains("社会福祉")
        || s.contains("児童福祉")
        || s.contains("障害者")
        || s.contains("老人")
        || s.contains("介護")
        || s.contains("保育")
        || s.contains("精神保健")
        || s.contains("リハビリ")
        || s.contains("福祉")
    {
        return "医療，福祉";
    }
    // 建設業
    if s.contains("建設")
        || s.contains("土木")
        || s.contains("建築")
        || s.contains("総合工事")
        || s.contains("設備工事")
        || s.contains("塗装工事")
        || s.contains("舗装工事")
        || s.contains("配管工事")
        || s.contains("電気工事")
        || s.contains("内装")
    {
        return "建設業";
    }
    // 製造業
    if s.contains("製造")
        || s.contains("食料品")
        || s.contains("飲料")
        || s.contains("繊維")
        || s.contains("衣服")
        || s.contains("木材")
        || s.contains("家具")
        || s.contains("印刷")
        || s.contains("化学")
        || s.contains("プラスチック")
        || s.contains("ゴム")
        || s.contains("窯業")
        || s.contains("金属")
        || s.contains("機械")
        || s.contains("輸送用")
        || s.contains("精密")
        || s.contains("加工")
        || s.contains("工場")
        || s.contains("生産工程")
    {
        return "製造業";
    }
    // 運輸業，郵便業
    if s.contains("運輸")
        || s.contains("運送")
        || s.contains("配送")
        || s.contains("郵便")
        || s.contains("貨物")
        || s.contains("旅客")
        || s.contains("鉄道")
        || s.contains("自動車運送")
        || s.contains("倉庫")
        || s.contains("配達")
        || s.contains("ドライバー")
    {
        return "運輸業，郵便業";
    }
    // 卸売業，小売業 (「商店」は曖昧なので「販売」を含めて広めに)
    if s.contains("卸売")
        || s.contains("小売")
        || s.contains("商店")
        || s.contains("販売店")
        || s.contains("百貨店")
        || s.contains("スーパー")
        || s.contains("コンビニ")
        || s.contains("商業")
    {
        return "卸売業，小売業";
    }
    // 宿泊業，飲食サービス業
    if s.contains("飲食店")
        || s.contains("レストラン")
        || s.contains("食堂")
        || s.contains("酒場")
        || s.contains("ビヤホール")
        || s.contains("バー")
        || s.contains("喫茶")
        || s.contains("旅館")
        || s.contains("ホテル")
        || s.contains("宿泊")
        || s.contains("料理店")
        || s.contains("給食")
    {
        return "宿泊業，飲食サービス業";
    }
    // 情報通信業
    if s.contains("ソフトウェア")
        || s.contains("情報サービス")
        || s.contains("通信業")
        || s.contains("情報通信")
        || s.contains("インターネット")
        || s.contains("放送")
        || s.contains("映像")
        || s.contains("出版")
        || s.contains("新聞")
        || s.contains("Web")
    {
        return "情報通信業";
    }
    // 教育，学習支援業
    if s.contains("学校")
        || s.contains("教育")
        || s.contains("学習支援")
        || s.contains("塾")
        || s.contains("予備校")
        || s.contains("教習所")
        || s.contains("学習教室")
    {
        return "教育，学習支援業";
    }
    // 不動産業，物品賃貸業
    if s.contains("不動産") || s.contains("物品賃貸") || s.contains("レンタル") {
        return "不動産業，物品賃貸業";
    }
    // 金融業，保険業
    if s.contains("金融")
        || s.contains("銀行")
        || s.contains("保険")
        || s.contains("証券")
        || s.contains("信用組合")
        || s.contains("信用金庫")
    {
        return "金融業，保険業";
    }
    // 農林漁業
    if s.contains("農業") || s.contains("林業") || s.contains("漁業") || s.contains("水産")
    {
        return "農林漁業";
    }
    // 鉱業
    if s.contains("鉱業") || s.contains("採石") || s.contains("砂利") {
        return "鉱業";
    }
    // 電気・ガス・熱供給・水道業
    if s.contains("電気") && s.contains("供給")
        || s.contains("ガス業")
        || s.contains("熱供給")
        || s.contains("水道業")
    {
        return "電気・ガス・熱供給・水道業";
    }
    // 学術研究，専門・技術サービス業
    if s.contains("学術")
        || s.contains("研究所")
        || (s.contains("専門") && s.contains("技術"))
        || s.contains("広告")
        || s.contains("デザイン")
        || s.contains("法務")
        || s.contains("会計")
        || s.contains("コンサル")
        || s.contains("経営戦略")
        || s.contains("市場調査")
    {
        return "学術研究，専門・技術サービス業";
    }
    // 生活関連サービス業，娯楽業
    if s.contains("理容")
        || s.contains("美容")
        || s.contains("クリーニング")
        || s.contains("浴場")
        || s.contains("娯楽")
        || s.contains("遊技場")
        || s.contains("興行")
        || s.contains("冠婚葬祭")
        || s.contains("葬儀")
        || s.contains("結婚")
        || s.contains("写真館")
        || s.contains("旅行")
        || s.contains("生活関連サービス")
    {
        return "生活関連サービス業，娯楽業";
    }
    // 公務
    if s.contains("公務") {
        return "公務（他に分類されるものを除く）";
    }
    // 複合サービス事業 (郵便局・農協・生協)
    if s.contains("複合サービス") || s.contains("協同組合") {
        return "複合サービス事業";
    }
    // 「サービス業（他に分類されないもの）」専用キーワード
    // 派遣業 / 建物管理 / 警備 / 清掃 / その他事業サービス
    if s.contains("派遣")
        || s.contains("人材紹介")
        || s.contains("職業紹介")
        || s.contains("建物管理")
        || s.contains("ビルメンテナンス")
        || s.contains("警備")
        || s.contains("清掃")
        || s.contains("廃棄物")
        || s.contains("修理")
        || s.contains("メンテナンス")
        || s.contains("設備管理")
        || s.contains("事業サービス")
    {
        return "サービス業（他に分類されないもの）";
    }
    // フォールバック: 「サービス業（他に分類されないもの）」
    "サービス業（他に分類されないもの）"
}

/// 産業大分類 (本モジュール内 fail-soft 用)
///
/// 呼び出し側で集計時に使用する想定の大分類リスト (順序固定で再現性確保)。
pub(super) const MAJOR_INDUSTRY_CATEGORIES: &[&str] = &[
    "農林漁業",
    "鉱業",
    "建設業",
    "製造業",
    "電気・ガス・熱供給・水道業",
    "情報通信業",
    "運輸業，郵便業",
    "卸売業，小売業",
    "金融業，保険業",
    "不動産業，物品賃貸業",
    "学術研究，専門・技術サービス業",
    "宿泊業，飲食サービス業",
    "生活関連サービス業，娯楽業",
    "教育，学習支援業",
    "医療，福祉",
    "複合サービス事業",
    "サービス業（他に分類されないもの）",
    "公務（他に分類されるものを除く）",
];

/// 就業者と HW 求人をマッチして MismatchRow を構築する
fn build_mismatch_rows(
    industry_employees: &[Row],
    hw_industry_counts: &[(String, i64)],
) -> Vec<MismatchRow> {
    let employee_total: i64 = industry_employees
        .iter()
        .map(|r| get_i64(r, "employees_total"))
        .sum();
    let hw_total: i64 = hw_industry_counts.iter().map(|(_, c)| *c).sum();

    if employee_total <= 0 || hw_total <= 0 {
        return Vec::new();
    }

    // 就業者: industry_name -> employees_total
    let mut emp_map: std::collections::HashMap<String, i64> = std::collections::HashMap::new();
    for r in industry_employees {
        let name = get_str_ref(r, "industry_name");
        if name.is_empty() {
            continue;
        }
        let emp = get_i64(r, "employees_total");
        if emp <= 0 {
            continue;
        }
        // 同名はマージ (合算)
        *emp_map.entry(normalize_industry_name(name)).or_insert(0) += emp;
    }

    // HW: industry_name -> count
    let mut hw_map: std::collections::HashMap<String, i64> = std::collections::HashMap::new();
    let mut hw_display_name: std::collections::HashMap<String, String> =
        std::collections::HashMap::new();
    for (name, c) in hw_industry_counts {
        if name.is_empty() || *c <= 0 {
            continue;
        }
        let key = normalize_industry_name(name);
        *hw_map.entry(key.clone()).or_insert(0) += *c;
        hw_display_name.entry(key).or_insert_with(|| name.clone());
    }

    // 双方に存在する産業のみ (ミスマッチは「両方ある」前提でないと意味がない)
    // ただし片方 0 件でも 0% として表示するため、Union を採用。
    let mut all_keys: std::collections::BTreeSet<String> = std::collections::BTreeSet::new();
    for k in emp_map.keys() {
        all_keys.insert(k.clone());
    }
    for k in hw_map.keys() {
        all_keys.insert(k.clone());
    }

    let mut rows: Vec<MismatchRow> = Vec::new();
    for key in all_keys {
        let emp = *emp_map.get(&key).unwrap_or(&0);
        let hw = *hw_map.get(&key).unwrap_or(&0);
        // 完全に両方 0 ならスキップ
        if emp == 0 && hw == 0 {
            continue;
        }
        let emp_pct = (emp as f64) / (employee_total as f64) * 100.0;
        let hw_pct = (hw as f64) / (hw_total as f64) * 100.0;
        let gap_pt = hw_pct - emp_pct;
        // 表示名は HW 由来優先 (短い)、なければ就業者名 (key を使う)
        // 国勢調査の industry_name には全角カンマが含まれるため、
        // ここでは hw_display_name 優先で表示し、なければ employees の元 name を再構築。
        let display = hw_display_name
            .get(&key)
            .cloned()
            .or_else(|| {
                industry_employees.iter().find_map(|r| {
                    let n = get_str_ref(r, "industry_name");
                    if normalize_industry_name(n) == key {
                        Some(n.to_string())
                    } else {
                        None
                    }
                })
            })
            .unwrap_or_else(|| key.clone());

        rows.push(MismatchRow {
            industry_name: display,
            employees: emp,
            hw_count: hw,
            emp_pct,
            hw_pct,
            gap_pt,
        });
    }

    // 2026-04-27 (B9): 両側 0.5% 未満の行は可読性低下 (公務 0.0% / 鉱業 0.0% 等の冗長表示)
    // のため除外。ただし片側でも 0.5% 以上ある場合は残す (情報損失防止)。
    rows.retain(|r| r.emp_pct >= 0.5 || r.hw_pct >= 0.5);

    // ギャップ絶対値の大きい順でソート (警戒度の高いものを上に)
    rows.sort_by(|a, b| {
        b.gap_pt
            .abs()
            .partial_cmp(&a.gap_pt.abs())
            .unwrap_or(std::cmp::Ordering::Equal)
    });

    rows
}

/// ギャップ pt から (解釈ラベル, 色コード) を返す
fn classify_gap(gap_pt: f64) -> (&'static str, &'static str) {
    if gap_pt < -10.0 {
        ("求人不足の可能性", "#dc2626") // 赤系
    } else if gap_pt > 10.0 {
        ("求人過剰の可能性", "#10b981") // 緑系
    } else {
        ("差分小", "#64748b") // グレー
    }
}

// =====================================================================
// テスト
// =====================================================================

#[cfg(test)]
mod tests {
    use super::*;
    use serde_json::json;
    use std::collections::HashMap;

    fn mk_emp(name: &str, total: i64) -> Row {
        let mut m: Row = HashMap::new();
        m.insert("industry_name".to_string(), json!(name));
        m.insert("employees_total".to_string(), json!(total));
        m
    }

    /// CR-9 統合 (2026-04-27): HW industry_raw → 12 大分類マッピングの逆証明
    /// 実 postings.industry_raw に存在する代表値を 12 大分類のいずれかに収束させる。
    #[test]
    fn map_hw_to_major_industry_real_world_values() {
        // 医療・福祉系
        assert_eq!(map_hw_to_major_industry("病院"), "医療，福祉");
        assert_eq!(map_hw_to_major_industry("一般診療所"), "医療，福祉");
        assert_eq!(map_hw_to_major_industry("歯科診療所"), "医療，福祉");
        assert_eq!(map_hw_to_major_industry("老人福祉・介護事業"), "医療，福祉");
        assert_eq!(map_hw_to_major_industry("障害者福祉事業"), "医療，福祉");
        assert_eq!(
            map_hw_to_major_industry("新聞保育・児童福祉事業"),
            "医療，福祉"
        );
        // 建設系
        assert_eq!(map_hw_to_major_industry("一般土木建築工事業"), "建設業");
        assert_eq!(
            map_hw_to_major_industry("土木工事業（舗装工事業を除く）"),
            "建設業"
        );
        // 運輸系
        assert_eq!(
            map_hw_to_major_industry("一般貨物自動車運送業"),
            "運輸業，郵便業"
        );
        assert_eq!(
            map_hw_to_major_industry("一般乗用旅客自動車運送業"),
            "運輸業，郵便業"
        );
        // 飲食宿泊系
        assert_eq!(
            map_hw_to_major_industry("食堂，レストラン（専門料理店を除く）"),
            "宿泊業，飲食サービス業"
        );
        assert_eq!(
            map_hw_to_major_industry("旅館，ホテル"),
            "宿泊業，飲食サービス業"
        );
        // 情報通信
        assert_eq!(map_hw_to_major_industry("ソフトウェア業"), "情報通信業");
        // 不明分類は fallback
        assert_eq!(
            map_hw_to_major_industry("他に分類されないもの"),
            "サービス業（他に分類されないもの）"
        );
    }

    /// CR-9 統合: マッピング結果がすべて 12 大分類リストに収まるドメイン不変条件
    #[test]
    fn map_hw_to_major_industry_all_outputs_in_majors() {
        let test_inputs = [
            "病院",
            "労働者派遣業",
            "建物総合管理業",
            "ソフトウェア業",
            "食堂",
            "理容業",
            "美容業",
            "農業",
            "製造業",
            "鉄道業",
            "金融業",
            "保険業",
            "学校教育",
            "公務",
            "未知の業種XYZ",
        ];
        for input in &test_inputs {
            let mapped = map_hw_to_major_industry(input);
            assert!(
                MAJOR_INDUSTRY_CATEGORIES.contains(&mapped),
                "input={:?} -> mapped={:?} は 12 大分類のいずれかに収まるべき",
                input,
                mapped
            );
        }
    }

    /// テスト 1: 構成比計算の逆証明
    /// 就業者 100k 中 医療福祉 28k → 28.0%
    #[test]
    fn industry_mismatch_emp_pct_28_percent() {
        let emp = vec![
            mk_emp("医療，福祉", 28_000),
            mk_emp("製造業", 14_000),
            mk_emp("卸売業，小売業", 16_000),
            mk_emp("その他", 42_000),
        ];
        let hw = vec![
            ("医療，福祉".to_string(), 1200),
            ("製造業".to_string(), 2200),
            ("卸売業，小売業".to_string(), 1800),
            ("その他".to_string(), 4800),
        ];

        let rows = build_mismatch_rows(&emp, &hw);
        let med = rows
            .iter()
            .find(|r| r.industry_name.contains("医療"))
            .expect("医療,福祉行が必須");
        assert!(
            (med.emp_pct - 28.0).abs() < 0.01,
            "就業者構成比 28% (実際: {})",
            med.emp_pct
        );
    }

    /// テスト 2: ギャップ計算の逆証明
    /// 就業者 28% / HW 求人 12% → ギャップ -16pt
    #[test]
    fn industry_mismatch_gap_minus_16pt() {
        let emp = vec![mk_emp("医療，福祉", 28_000), mk_emp("その他", 72_000)];
        // HW total = 10000, 医療 1200 → 12.0%
        let hw = vec![
            ("医療，福祉".to_string(), 1200),
            ("その他".to_string(), 8800),
        ];

        let rows = build_mismatch_rows(&emp, &hw);
        let med = rows
            .iter()
            .find(|r| r.industry_name.contains("医療"))
            .expect("医療行必須");
        assert!(
            (med.hw_pct - 12.0).abs() < 0.01,
            "HW 構成比 12% (実際: {})",
            med.hw_pct
        );
        assert!(
            (med.gap_pt - (-16.0)).abs() < 0.01,
            "ギャップ -16pt (実際: {})",
            med.gap_pt
        );
    }

    /// テスト 3: 解釈分岐の逆証明
    /// -16 → 「求人不足」、+8 → 「整合」、+20 → 「求人過剰」
    #[test]
    fn industry_mismatch_classify_gap_branches() {
        assert_eq!(classify_gap(-16.0).0, "求人不足の可能性");
        assert_eq!(classify_gap(-10.5).0, "求人不足の可能性");
        assert_eq!(classify_gap(-10.0).0, "差分小"); // 境界 (-10 はちょうど整合側)
        assert_eq!(classify_gap(0.0).0, "差分小");
        assert_eq!(classify_gap(8.0).0, "差分小");
        assert_eq!(classify_gap(10.0).0, "差分小"); // 境界 (10 はちょうど整合側)
        assert_eq!(classify_gap(10.1).0, "求人過剰の可能性");
        assert_eq!(classify_gap(20.0).0, "求人過剰の可能性");
        // 色も同時検証
        assert_eq!(classify_gap(-16.0).1, "#dc2626");
        assert_eq!(classify_gap(20.0).1, "#10b981");
        assert_eq!(classify_gap(0.0).1, "#64748b");
    }

    /// テスト 4: ドメイン不変条件
    /// - 各構成比 ∈ [0, 100]
    /// - ギャップ ∈ [-100, 100]
    /// - 構成比合計 ≈ 100 (誤差 0.5 以内)
    #[test]
    fn industry_mismatch_domain_invariants() {
        let emp = vec![
            mk_emp("医療，福祉", 28_000),
            mk_emp("製造業", 14_000),
            mk_emp("卸売業，小売業", 16_000),
            mk_emp("建設業", 8_000),
            mk_emp("その他", 34_000),
        ];
        let hw = vec![
            ("医療，福祉".to_string(), 1200),
            ("製造業".to_string(), 2200),
            ("卸売業，小売業".to_string(), 1800),
            ("建設業".to_string(), 1500),
            ("その他".to_string(), 3300),
        ];

        let rows = build_mismatch_rows(&emp, &hw);
        assert!(!rows.is_empty(), "rows 非空必須");

        let mut emp_sum = 0.0;
        let mut hw_sum = 0.0;
        for r in &rows {
            assert!(
                r.emp_pct >= 0.0 && r.emp_pct <= 100.0,
                "就業者構成比 ∈ [0, 100] (実際: {})",
                r.emp_pct
            );
            assert!(
                r.hw_pct >= 0.0 && r.hw_pct <= 100.0,
                "HW 構成比 ∈ [0, 100] (実際: {})",
                r.hw_pct
            );
            assert!(
                r.gap_pt >= -100.0 && r.gap_pt <= 100.0,
                "ギャップ ∈ [-100, 100] (実際: {})",
                r.gap_pt
            );
            emp_sum += r.emp_pct;
            hw_sum += r.hw_pct;
        }
        assert!(
            (emp_sum - 100.0).abs() < 0.5,
            "就業者構成比合計 ≈ 100 (実際: {})",
            emp_sum
        );
        assert!(
            (hw_sum - 100.0).abs() < 0.5,
            "HW 構成比合計 ≈ 100 (実際: {})",
            hw_sum
        );
    }

    /// テスト 5: fail-soft (空入力)
    #[test]
    fn industry_mismatch_failsoft_empty_inputs() {
        // 両方空
        let mut html = String::new();
        render_section_industry_mismatch(&mut html, &[], &[]);
        assert!(html.is_empty(), "両方空 → section 非表示");

        // 就業者のみ空
        let mut html2 = String::new();
        render_section_industry_mismatch(&mut html2, &[], &[("医療，福祉".to_string(), 100)]);
        assert!(html2.is_empty(), "就業者空 → section 非表示");

        // HW のみ空
        let mut html3 = String::new();
        render_section_industry_mismatch(&mut html3, &[mk_emp("医療，福祉", 28_000)], &[]);
        assert!(html3.is_empty(), "HW 空 → section 非表示");

        // 就業者合計 0
        let mut html4 = String::new();
        render_section_industry_mismatch(
            &mut html4,
            &[mk_emp("医療，福祉", 0)],
            &[("医療，福祉".to_string(), 100)],
        );
        assert!(html4.is_empty(), "就業者合計 0 → section 非表示");

        // HW 合計 0 (負/0 件は build 時に除外されるため、結果的に hw_total=0)
        let mut html5 = String::new();
        render_section_industry_mismatch(
            &mut html5,
            &[mk_emp("医療，福祉", 28_000)],
            &[("医療，福祉".to_string(), 0)],
        );
        assert!(html5.is_empty(), "HW 合計 0 → section 非表示");
    }

    /// テスト 6: caveat 文言の必須要件
    /// 「国勢調査 (5 年に 1 回」「HW 登録求人のみ」「採用しやすさを意味せず」
    #[test]
    fn industry_mismatch_caveat_required_phrases() {
        let emp = vec![
            mk_emp("医療，福祉", 28_000),
            mk_emp("製造業", 14_000),
            mk_emp("その他", 58_000),
        ];
        let hw = vec![
            ("医療，福祉".to_string(), 1200),
            ("製造業".to_string(), 2200),
            ("その他".to_string(), 6600),
        ];
        let mut html = String::new();
        render_section_industry_mismatch(&mut html, &emp, &hw);

        assert!(!html.is_empty(), "section 描画必須");
        assert!(
            html.contains("国勢調査 (5 年に 1 回"),
            "「国勢調査 (5 年に 1 回」必須 caveat"
        );
        assert!(
            html.contains("HW 登録求人のみ"),
            "「HW 登録求人のみ」必須 caveat (feedback_hw_data_scope)"
        );
        assert!(
            html.contains("採用しやすさを意味せず"),
            "「採用しやすさを意味せず」必須 caveat (feedback_correlation_not_causation)"
        );
        assert!(
            html.contains("因果の証明ではありません"),
            "因果非主張 caveat 必須"
        );
    }

    /// テスト 7: HTML 構造の必須要素
    #[test]
    fn industry_mismatch_html_structure() {
        let emp = vec![
            mk_emp("医療，福祉", 28_000),
            mk_emp("製造業", 14_000),
            mk_emp("その他", 58_000),
        ];
        let hw = vec![
            ("医療，福祉".to_string(), 1200),
            ("製造業".to_string(), 2200),
            ("その他".to_string(), 6600),
        ];
        let mut html = String::new();
        render_section_industry_mismatch(&mut html, &emp, &hw);

        assert!(
            html.contains("data-testid=\"industry-mismatch-section\""),
            "section data-testid 必須"
        );
        assert!(
            html.contains("data-testid=\"industry-mismatch-table\""),
            "table data-testid 必須"
        );
        assert!(html.contains("<h2>産業ミスマッチ"), "h2 タイトル必須");
        assert!(html.contains("表 4B-1"), "図番号 4B-1 必須");
        assert!(html.contains("就業者構成比"), "列ヘッダ 就業者構成比 必須");
        assert!(
            html.contains("HW 求人構成比"),
            "列ヘッダ HW 求人構成比 必須"
        );
        assert!(html.contains("ギャップ"), "列ヘッダ ギャップ 必須");
    }

    /// テスト 8: ギャップ絶対値順ソート (警戒度の高いものが上)
    #[test]
    fn industry_mismatch_sorted_by_abs_gap_desc() {
        // 医療: emp 28% / hw 12% → -16pt (最大絶対値)
        // 製造: emp 14% / hw 22% → +8pt
        // 卸売: emp 16% / hw 18% → +2pt
        let emp = vec![
            mk_emp("医療，福祉", 28_000),
            mk_emp("製造業", 14_000),
            mk_emp("卸売業，小売業", 16_000),
            mk_emp("その他", 42_000),
        ];
        let hw = vec![
            ("医療，福祉".to_string(), 1200),
            ("製造業".to_string(), 2200),
            ("卸売業，小売業".to_string(), 1800),
            ("その他".to_string(), 4800),
        ];
        let rows = build_mismatch_rows(&emp, &hw);
        // 1 番目が絶対値最大 (医療,福祉 = -16)
        assert!(
            rows[0].industry_name.contains("医療"),
            "1 行目は最大ギャップ |16| の医療 (実際: {})",
            rows[0].industry_name
        );
        // 絶対値降順
        for w in rows.windows(2) {
            assert!(
                w[0].gap_pt.abs() >= w[1].gap_pt.abs() - 0.001,
                "ギャップ絶対値降順 (実際: {} < {})",
                w[0].gap_pt.abs(),
                w[1].gap_pt.abs()
            );
        }
    }

    // =================================================================
    // CSV Public variant tests (CR-9 / 2026-04-29)
    // =================================================================

    fn mk_agg_with_tags(
        tags: &[(&str, usize)],
    ) -> super::super::super::aggregator::SurveyAggregation {
        let mut agg = super::super::super::aggregator::SurveyAggregation::default();
        agg.by_tags = tags.iter().map(|(t, c)| (t.to_string(), *c)).collect();
        agg
    }

    /// 2026-04-30: by_company を信号源に追加 (タグ列なし CSV 対応)
    fn mk_agg_with_companies(
        companies: &[(&str, usize)],
    ) -> super::super::super::aggregator::SurveyAggregation {
        use super::super::super::aggregator::CompanyAgg;
        let mut agg = super::super::super::aggregator::SurveyAggregation::default();
        agg.by_company = companies
            .iter()
            .map(|(name, count)| CompanyAgg {
                name: name.to_string(),
                count: *count,
                ..Default::default()
            })
            .collect();
        agg
    }

    /// CSV テスト (2026-04-30): by_company の会社名から業種推定
    /// Indeed/求人ボックス CSV にタグ列がないとき、会社名で代替推定する
    #[test]
    fn csv_industry_estimate_from_company_name() {
        let agg = mk_agg_with_companies(&[
            ("メディカル株式会社01", 5),
            ("ケアセンター東京", 3),
            ("新宿病院", 2),
            ("製造工場サンプル", 4),
            ("カフェチェーン01", 2),
        ]);
        let counts = estimate_csv_industry_counts(&agg);
        // 会社名「メディカル」「ケアセンター」「病院」→ 医療,福祉 に集約 (5+3+2=10)
        let medical = counts
            .iter()
            .find(|(k, _)| k == "医療，福祉")
            .map(|(_, v)| *v)
            .unwrap_or(0);
        assert_eq!(medical, 10, "医療系会社名 3 社合計 = 10 件");
        // 製造業 (製造工場): 4
        let manufacturing = counts
            .iter()
            .find(|(k, _)| k == "製造業")
            .map(|(_, v)| *v)
            .unwrap_or(0);
        assert_eq!(manufacturing, 4);
        // 宿泊業,飲食サービス業 (カフェ): 2
        let food = counts
            .iter()
            .find(|(k, _)| k.contains("飲食"))
            .map(|(_, v)| *v)
            .unwrap_or(0);
        assert_eq!(food, 2);
    }

    /// CSV テスト 1: タグ「看護師」「介護スタッフ」が医療,福祉に分類される (具体値検証)
    #[test]
    fn csv_industry_keyword_medical_welfare() {
        // 医療・福祉系キーワード
        assert_eq!(map_keyword_to_major_industry("看護師"), Some("医療，福祉"));
        assert_eq!(
            map_keyword_to_major_industry("介護スタッフ"),
            Some("医療，福祉")
        );
        assert_eq!(
            map_keyword_to_major_industry("ヘルパー"),
            Some("医療，福祉")
        );
        assert_eq!(map_keyword_to_major_industry("保育士"), Some("医療，福祉"));
        assert_eq!(
            map_keyword_to_major_industry("理学療法士"),
            Some("医療，福祉")
        );
        // 飲食
        assert_eq!(
            map_keyword_to_major_industry("カフェスタッフ"),
            Some("宿泊業，飲食サービス業")
        );
        // 不明はNone
        assert_eq!(map_keyword_to_major_industry("経験不問"), None);
        assert_eq!(map_keyword_to_major_industry(""), None);
    }

    /// CSV テスト 2 (ドメイン不変条件): 構成比合計 ≈ 100% (誤差 0.5pt 以内)
    #[test]
    fn csv_industry_pct_sums_to_100() {
        // CSV: 医療 60件、製造 30件、飲食 10件 (合計 100件)
        let agg = mk_agg_with_tags(&[
            ("看護師", 40),
            ("介護スタッフ", 20),
            ("製造工場スタッフ", 30),
            ("カフェスタッフ", 10),
        ]);
        let emp = vec![
            mk_emp("医療，福祉", 28_000),
            mk_emp("製造業", 14_000),
            mk_emp("宿泊業，飲食サービス業", 6_000),
            mk_emp("その他", 52_000),
        ];

        let csv_counts = estimate_csv_industry_counts(&agg);
        let total: i64 = csv_counts.iter().map(|(_, c)| *c).sum();
        assert_eq!(total, 100, "推定合計件数 = 100 (実際: {})", total);

        let rows = build_csv_mismatch_rows(&emp, &csv_counts);
        let csv_sum: f64 = rows.iter().map(|r| r.csv_pct).sum();
        let emp_sum: f64 = rows.iter().map(|r| r.emp_pct).sum();
        // CSV 側は推定対象 (医療/製造/飲食) のみで合計 100% に達するべき
        assert!(
            (csv_sum - 100.0).abs() < 0.5,
            "CSV 構成比合計 ≈ 100 (実際: {})",
            csv_sum
        );
        // 就業者側は (medical + manuf + 飲食 = 28+14+6 = 48k / 100k = 48%) の部分のみ rows に出る
        // (「その他」はマッチせず emp_map に key:"その他" として残るが、CSV 側 0 件のため
        //  csv_pct=0、 emp_pct=52% で出現する → 合計 100%)
        assert!(
            (emp_sum - 100.0).abs() < 0.5,
            "就業者構成比合計 ≈ 100 (実際: {})",
            emp_sum
        );
    }

    /// CSV テスト 3 (ドメイン不変条件): ギャップ ∈ [-100, 100]
    #[test]
    fn csv_industry_gap_invariant_range() {
        let agg = mk_agg_with_tags(&[
            ("看護師", 80), // 医療 80件
            ("製造工場スタッフ", 10),
            ("カフェスタッフ", 10),
        ]);
        let emp = vec![
            mk_emp("医療，福祉", 10_000),
            mk_emp("製造業", 14_000),
            mk_emp("宿泊業，飲食サービス業", 6_000),
            mk_emp("その他", 70_000),
        ];

        let csv_counts = estimate_csv_industry_counts(&agg);
        let rows = build_csv_mismatch_rows(&emp, &csv_counts);
        assert!(!rows.is_empty(), "rows 非空必須");

        for r in &rows {
            assert!(
                r.csv_pct >= 0.0 && r.csv_pct <= 100.0,
                "CSV 構成比 ∈ [0,100] (実際: {})",
                r.csv_pct
            );
            assert!(
                r.emp_pct >= 0.0 && r.emp_pct <= 100.0,
                "就業者構成比 ∈ [0,100] (実際: {})",
                r.emp_pct
            );
            assert!(
                r.gap_pt >= -100.0 && r.gap_pt <= 100.0,
                "ギャップ ∈ [-100,100] (実際: {})",
                r.gap_pt
            );
        }

        // 医療: csv=80%, emp=10% → gap=+70 (CSV重点)
        let med = rows
            .iter()
            .find(|r| r.industry_name.contains("医療"))
            .expect("医療行必須");
        assert!(
            (med.csv_pct - 80.0).abs() < 0.01,
            "CSV 構成比 80% (実際: {})",
            med.csv_pct
        );
        assert!(
            (med.gap_pt - 70.0).abs() < 0.01,
            "ギャップ +70pt (実際: {})",
            med.gap_pt
        );
    }

    /// CSV テスト 4: 0件業種は除外 (B9 仕様)
    #[test]
    fn csv_industry_excludes_zero_rows() {
        // CSV: 医療 100件のみ (他はゼロ)
        let agg = mk_agg_with_tags(&[("看護師", 100)]);
        // 就業者: 医療 28k, 鉱業 0 (両側0%), その他 72k
        let emp = vec![
            mk_emp("医療，福祉", 28_000),
            mk_emp("鉱業", 0), // 両側0% → 除外対象
            mk_emp("その他", 72_000),
        ];
        let csv_counts = estimate_csv_industry_counts(&agg);
        let rows = build_csv_mismatch_rows(&emp, &csv_counts);

        // 鉱業は両側0なので含まれないこと
        for r in &rows {
            assert!(
                !r.industry_name.contains("鉱業"),
                "両側 0% の鉱業は除外されるべき"
            );
        }
    }

    /// CSV テスト 5: fail-soft (推定業種が全て不明な場合 section 非表示)
    #[test]
    fn csv_industry_failsoft_unknown_tags_only() {
        // タグが全て不明 → 推定 0 件
        let agg = mk_agg_with_tags(&[("経験不問", 50), ("週休2日", 30), ("交通費支給", 20)]);
        let emp = vec![mk_emp("医療，福祉", 28_000), mk_emp("その他", 72_000)];

        let csv_counts = estimate_csv_industry_counts(&agg);
        assert!(csv_counts.is_empty(), "全タグ不明 → 推定 0 件");

        let mut html = String::new();
        render_section_industry_mismatch_csv(&mut html, &emp, &agg);
        assert!(
            html.is_empty(),
            "推定 0 件 → section 非表示 (実際: {} bytes)",
            html.len()
        );

        // 就業者ゼロでも section 非表示
        let agg2 = mk_agg_with_tags(&[("看護師", 100)]);
        let mut html2 = String::new();
        render_section_industry_mismatch_csv(&mut html2, &[], &agg2);
        assert!(html2.is_empty(), "就業者空 → section 非表示");
    }

    /// CSV テスト 6: caveat 必須文言の存在
    #[test]
    fn csv_industry_caveat_required_phrases() {
        let agg = mk_agg_with_tags(&[
            ("看護師", 50),
            ("製造工場スタッフ", 30),
            ("カフェスタッフ", 20),
        ]);
        let emp = vec![
            mk_emp("医療，福祉", 28_000),
            mk_emp("製造業", 14_000),
            mk_emp("宿泊業，飲食サービス業", 6_000),
            mk_emp("その他", 52_000),
        ];
        let mut html = String::new();
        render_section_industry_mismatch_csv(&mut html, &emp, &agg);

        assert!(!html.is_empty(), "section 描画必須");
        // CSV 推定の限界
        assert!(
            html.contains("CSV 業種は職種列・タグ列・会社名"),
            "CSV 推定限界 caveat 必須"
        );
        // 国勢調査スコープ
        assert!(
            html.contains("国勢調査 (5 年に 1 回"),
            "国勢調査 caveat 必須"
        );
        // CSV 範囲限定
        assert!(
            html.contains("CSV はユーザー指定の媒体掲載求人で、地域全体を代表しません"),
            "CSV スコープ caveat 必須 (feedback_hw_data_scope と同等の範囲制約)"
        );
        // 採用優劣評価ではない
        assert!(
            html.contains("採用優劣評価ではありません"),
            "採用優劣否定 caveat 必須 (feedback_correlation_not_causation)"
        );
        // 因果非主張
        assert!(
            html.contains("因果の証明ではありません"),
            "因果非主張 caveat 必須"
        );
    }

    /// CSV テスト 7: HTML 構造の必須要素
    #[test]
    fn csv_industry_html_structure() {
        let agg = mk_agg_with_tags(&[("看護師", 60), ("カフェスタッフ", 40)]);
        let emp = vec![
            mk_emp("医療，福祉", 28_000),
            mk_emp("宿泊業，飲食サービス業", 6_000),
            mk_emp("その他", 66_000),
        ];
        let mut html = String::new();
        render_section_industry_mismatch_csv(&mut html, &emp, &agg);

        assert!(
            html.contains("data-testid=\"industry-mismatch-csv-section\""),
            "section data-testid 必須"
        );
        assert!(
            html.contains("data-testid=\"industry-mismatch-csv-table\""),
            "table data-testid 必須"
        );
        // Round 23: タイトル中立化 (設計メモ §18.3)
        assert!(
            html.contains("<h2>CSV 推定業種構成と地域産業構成の比較"),
            "h2 タイトル必須"
        );
        assert!(html.contains("表 4B-1"), "図番号 4B-1 必須");
        assert!(
            html.contains("CSV 推定構成比"),
            "列ヘッダ CSV 推定構成比 必須 (§18.4 ラベル中立化)"
        );
        assert!(html.contains("就業者構成比"), "列ヘッダ 就業者構成比 必須");
        assert!(html.contains("ギャップ"), "列ヘッダ ギャップ 必須");
    }

    /// CSV テスト 8: 解釈分岐の逆証明
    /// gap = csv_pct - emp_pct
    /// ≥ +10 → CSV 重点 / ≤ -10 → CSV 過少 / それ以外 → 整合
    #[test]
    fn csv_industry_classify_gap_branches() {
        assert_eq!(classify_csv_gap(20.0).0, "CSV 内 推定多出現");
        assert_eq!(classify_csv_gap(10.0).0, "CSV 内 推定多出現"); // 境界 10 は重点側
        assert_eq!(classify_csv_gap(9.9).0, "差分小");
        assert_eq!(classify_csv_gap(0.0).0, "差分小");
        assert_eq!(classify_csv_gap(-9.9).0, "差分小");
        assert_eq!(classify_csv_gap(-10.0).0, "CSV 内 推定少数"); // 境界 -10 は過少側
        assert_eq!(classify_csv_gap(-30.0).0, "CSV 内 推定少数");

        // 色も検証
        assert_eq!(classify_csv_gap(20.0).1, "#10b981");
        assert_eq!(classify_csv_gap(-30.0).1, "#dc2626");
        assert_eq!(classify_csv_gap(0.0).1, "#64748b");
    }

    /// CSV テスト 9: industry_raw > tags_raw > job_type の優先順位 (本実装の制約説明)
    ///
    /// 現行 SurveyAggregation は `by_tags` のみ集約しており、
    /// `industry_raw` / `job_title` が直接アクセスできないため、
    /// 本実装では tags ベースのキーワード推定に統一されている。
    /// → caveat に「業種列がない場合精度に限界」と明記されていることをテスト。
    #[test]
    fn csv_industry_priority_documented_in_caveat() {
        let agg = mk_agg_with_tags(&[("看護師", 100)]);
        let emp = vec![mk_emp("医療，福祉", 28_000), mk_emp("その他", 72_000)];

        let mut html = String::new();
        render_section_industry_mismatch_csv(&mut html, &emp, &agg);
        assert!(
            html.contains("元 CSV に業種列がない場合精度に限界があります"),
            "業種推定限界の caveat 必須 (priority 説明の一部)"
        );

        // タグから推定された医療件数は 100 件 (タグ count をそのまま流用)
        let csv_counts = estimate_csv_industry_counts(&agg);
        assert_eq!(csv_counts.len(), 1, "1 産業のみ推定");
        assert_eq!(csv_counts[0].0, "医療，福祉");
        assert_eq!(csv_counts[0].1, 100);
    }

    /// CSV サンプル: 実出力 HTML の内容を eprintln で確認 (`cargo test -- --nocapture` で表示)
    /// 介護系 CSV を想定 (看護師 + 介護スタッフ + 保育士) の典型出力例。
    #[test]
    fn csv_industry_sample_output_for_review() {
        let agg = mk_agg_with_tags(&[
            ("看護師", 30),
            ("介護スタッフ", 25),
            ("保育士", 15),
            ("製造工場スタッフ", 10),
            ("カフェスタッフ", 8),
            ("販売スタッフ", 7),
            ("経験不問", 50), // 不明 → 推定対象外
            ("週休2日", 30),  // 不明 → 推定対象外
        ]);
        let emp = vec![
            mk_emp("医療，福祉", 9_700),             // 9.7%
            mk_emp("製造業", 14_000),                // 14%
            mk_emp("卸売業，小売業", 16_000),        // 16%
            mk_emp("宿泊業，飲食サービス業", 6_000), // 6%
            mk_emp("建設業", 8_000),
            mk_emp("情報通信業", 4_000),
            mk_emp("教育，学習支援業", 5_300),
            mk_emp("公務（他に分類されるものを除く）", 3_000),
            mk_emp("運輸業，郵便業", 6_000),
            mk_emp("その他", 28_000),
        ];

        let mut html = String::new();
        render_section_industry_mismatch_csv(&mut html, &emp, &agg);

        eprintln!("=== CSV industry mismatch sample HTML ===");
        eprintln!("{}", html);

        assert!(!html.is_empty(), "サンプル出力 必須");
        assert!(
            html.contains("CSV 内 推定多出現"),
            "CSV 内 推定多出現の解釈ラベル必須 (医療,福祉)"
        );
    }

    /// CSV テスト 10: ソート順 (ギャップ絶対値降順)
    #[test]
    fn csv_industry_sorted_by_abs_gap_desc() {
        let agg = mk_agg_with_tags(&[
            ("看護師", 70),           // 医療 70件
            ("製造工場スタッフ", 20), // 製造 20件
            ("カフェスタッフ", 10),   // 飲食 10件
        ]);
        // CSV: 医療 70%, 製造 20%, 飲食 10%
        // 就業者 emp_total = 90k:
        //   医療 28k → emp_pct = 31.1% → gap = 70 - 31.1 = +38.9 (最大絶対値)
        //   製造 14k → emp_pct = 15.6% → gap = +4.4
        //   飲食 6k → emp_pct = 6.7% → gap = +3.3
        //   その他 42k → emp_pct = 46.7%, csv_pct = 0 → gap = -46.7 (これが最大!)
        // → 「その他」がトップになるが、実データでは「その他」分類は estimate で出ず emp_map のみに出る
        // この場合は意図的に emp 側の合計を CSV と相互排他的に絞る
        let emp = vec![
            mk_emp("医療，福祉", 25_000),             // 25%
            mk_emp("製造業", 25_000),                 // 25%
            mk_emp("宿泊業，飲食サービス業", 25_000), // 25%
            mk_emp("教育，学習支援業", 25_000),       // 25%
        ];
        // CSV: 医療 70 / 製造 20 / 飲食 10
        // 比較対象 (両側出現する):
        //   医療: csv=70%, emp=25% → gap=+45 (max)
        //   製造: csv=20%, emp=25% → gap=-5
        //   飲食: csv=10%, emp=25% → gap=-15
        //   教育: csv=0%, emp=25% → gap=-25

        let csv_counts = estimate_csv_industry_counts(&agg);
        let rows = build_csv_mismatch_rows(&emp, &csv_counts);

        assert!(
            rows[0].industry_name.contains("医療"),
            "1 行目は最大ギャップ |+45| の医療 (実際: {} gap={})",
            rows[0].industry_name,
            rows[0].gap_pt
        );
        for w in rows.windows(2) {
            assert!(
                w[0].gap_pt.abs() >= w[1].gap_pt.abs() - 0.001,
                "ギャップ絶対値降順 (実際: {} < {})",
                w[0].gap_pt.abs(),
                w[1].gap_pt.abs()
            );
        }
    }

    /// テスト 9: 産業名正規化 (全角カンマ / 半角カンマブレ吸収)
    #[test]
    fn industry_mismatch_normalize_industry_name() {
        // 全角カンマ vs 半角カンマで同一視されること
        assert_eq!(
            normalize_industry_name("医療，福祉"),
            normalize_industry_name("医療,福祉")
        );
        assert_eq!(
            normalize_industry_name("卸売業，小売業"),
            normalize_industry_name("卸売業 小売業")
        );

        // 就業者「医療，福祉」と HW 「医療,福祉」(半角) がマッチすること
        let emp = vec![mk_emp("医療，福祉", 28_000), mk_emp("その他", 72_000)];
        let hw = vec![
            ("医療,福祉".to_string(), 1200), // 半角カンマ
            ("その他".to_string(), 8800),
        ];
        let rows = build_mismatch_rows(&emp, &hw);
        let med = rows
            .iter()
            .find(|r| r.industry_name.contains("医療"))
            .expect("医療行マッチ必須 (カンマブレ吸収)");
        assert_eq!(med.employees, 28_000);
        assert_eq!(med.hw_count, 1200);
    }
}
