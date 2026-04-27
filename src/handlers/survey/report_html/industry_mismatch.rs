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
    if s.contains("病院") || s.contains("医療") || s.contains("診療") || s.contains("歯科")
        || s.contains("助産") || s.contains("看護") || s.contains("獣医")
        || s.contains("社会保険") || s.contains("社会福祉") || s.contains("児童福祉")
        || s.contains("障害者") || s.contains("老人") || s.contains("介護")
        || s.contains("保育") || s.contains("精神保健") || s.contains("リハビリ")
        || s.contains("福祉")
    {
        return "医療，福祉";
    }
    // 建設業
    if s.contains("建設") || s.contains("土木") || s.contains("建築")
        || s.contains("総合工事") || s.contains("設備工事")
        || s.contains("塗装工事") || s.contains("舗装工事") || s.contains("配管工事")
        || s.contains("電気工事") || s.contains("内装")
    {
        return "建設業";
    }
    // 製造業
    if s.contains("製造") || s.contains("食料品") || s.contains("飲料")
        || s.contains("繊維") || s.contains("衣服") || s.contains("木材")
        || s.contains("家具") || s.contains("印刷") || s.contains("化学")
        || s.contains("プラスチック") || s.contains("ゴム") || s.contains("窯業")
        || s.contains("金属") || s.contains("機械") || s.contains("輸送用")
        || s.contains("精密") || s.contains("加工") || s.contains("工場")
        || s.contains("生産工程")
    {
        return "製造業";
    }
    // 運輸業，郵便業
    if s.contains("運輸") || s.contains("運送") || s.contains("配送")
        || s.contains("郵便") || s.contains("貨物") || s.contains("旅客")
        || s.contains("鉄道") || s.contains("自動車運送") || s.contains("倉庫")
        || s.contains("配達") || s.contains("ドライバー")
    {
        return "運輸業，郵便業";
    }
    // 卸売業，小売業 (「商店」は曖昧なので「販売」を含めて広めに)
    if s.contains("卸売") || s.contains("小売") || s.contains("商店")
        || s.contains("販売店") || s.contains("百貨店") || s.contains("スーパー")
        || s.contains("コンビニ") || s.contains("商業")
    {
        return "卸売業，小売業";
    }
    // 宿泊業，飲食サービス業
    if s.contains("飲食店") || s.contains("レストラン") || s.contains("食堂")
        || s.contains("酒場") || s.contains("ビヤホール") || s.contains("バー")
        || s.contains("喫茶") || s.contains("旅館") || s.contains("ホテル")
        || s.contains("宿泊") || s.contains("料理店") || s.contains("給食")
    {
        return "宿泊業，飲食サービス業";
    }
    // 情報通信業
    if s.contains("ソフトウェア") || s.contains("情報サービス")
        || s.contains("通信業") || s.contains("情報通信")
        || s.contains("インターネット") || s.contains("放送")
        || s.contains("映像") || s.contains("出版") || s.contains("新聞")
        || s.contains("Web")
    {
        return "情報通信業";
    }
    // 教育，学習支援業
    if s.contains("学校") || s.contains("教育") || s.contains("学習支援")
        || s.contains("塾") || s.contains("予備校") || s.contains("教習所")
        || s.contains("学習教室")
    {
        return "教育，学習支援業";
    }
    // 不動産業，物品賃貸業
    if s.contains("不動産") || s.contains("物品賃貸") || s.contains("レンタル") {
        return "不動産業，物品賃貸業";
    }
    // 金融業，保険業
    if s.contains("金融") || s.contains("銀行") || s.contains("保険")
        || s.contains("証券") || s.contains("信用組合") || s.contains("信用金庫")
    {
        return "金融業，保険業";
    }
    // 農林漁業
    if s.contains("農業") || s.contains("林業") || s.contains("漁業")
        || s.contains("水産")
    {
        return "農林漁業";
    }
    // 鉱業
    if s.contains("鉱業") || s.contains("採石") || s.contains("砂利") {
        return "鉱業";
    }
    // 電気・ガス・熱供給・水道業
    if s.contains("電気") && s.contains("供給") || s.contains("ガス業")
        || s.contains("熱供給") || s.contains("水道業")
    {
        return "電気・ガス・熱供給・水道業";
    }
    // 学術研究，専門・技術サービス業
    if s.contains("学術") || s.contains("研究所")
        || (s.contains("専門") && s.contains("技術"))
        || s.contains("広告") || s.contains("デザイン") || s.contains("法務")
        || s.contains("会計") || s.contains("コンサル")
        || s.contains("経営戦略") || s.contains("市場調査")
    {
        return "学術研究，専門・技術サービス業";
    }
    // 生活関連サービス業，娯楽業
    if s.contains("理容") || s.contains("美容") || s.contains("クリーニング")
        || s.contains("浴場") || s.contains("娯楽") || s.contains("遊技場")
        || s.contains("興行") || s.contains("冠婚葬祭")
        || s.contains("葬儀") || s.contains("結婚") || s.contains("写真館")
        || s.contains("旅行") || s.contains("生活関連サービス")
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
    if s.contains("派遣") || s.contains("人材紹介") || s.contains("職業紹介")
        || s.contains("建物管理") || s.contains("ビルメンテナンス")
        || s.contains("警備") || s.contains("清掃") || s.contains("廃棄物")
        || s.contains("修理") || s.contains("メンテナンス") || s.contains("設備管理")
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
    let mut emp_map: std::collections::HashMap<String, i64> =
        std::collections::HashMap::new();
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
        ("整合", "#64748b") // グレー
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
        assert_eq!(
            map_hw_to_major_industry("一般土木建築工事業"),
            "建設業"
        );
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
        assert_eq!(
            map_hw_to_major_industry("ソフトウェア業"),
            "情報通信業"
        );
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
            "病院", "労働者派遣業", "建物総合管理業", "ソフトウェア業",
            "食堂", "理容業", "美容業", "農業", "製造業", "鉄道業",
            "金融業", "保険業", "学校教育", "公務", "未知の業種XYZ",
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
        let emp = vec![
            mk_emp("医療，福祉", 28_000),
            mk_emp("その他", 72_000),
        ];
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
        assert_eq!(classify_gap(-10.0).0, "整合"); // 境界 (-10 はちょうど整合側)
        assert_eq!(classify_gap(0.0).0, "整合");
        assert_eq!(classify_gap(8.0).0, "整合");
        assert_eq!(classify_gap(10.0).0, "整合"); // 境界 (10 はちょうど整合側)
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
        render_section_industry_mismatch(
            &mut html2,
            &[],
            &[("医療，福祉".to_string(), 100)],
        );
        assert!(html2.is_empty(), "就業者空 → section 非表示");

        // HW のみ空
        let mut html3 = String::new();
        render_section_industry_mismatch(
            &mut html3,
            &[mk_emp("医療，福祉", 28_000)],
            &[],
        );
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
        assert!(html.contains("HW 求人構成比"), "列ヘッダ HW 求人構成比 必須");
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
