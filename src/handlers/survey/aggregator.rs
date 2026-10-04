//! 集計モジュール
//! パース済みレコードを地域別・給与帯別・雇用形態別・タグ別に集計

use serde::{Deserialize, Serialize};
use std::collections::HashMap;

use super::statistics::enhanced_salary_statistics;
use super::upload::SurveyRecord;

// ======== 月給換算定数（F1 #2 修正、2026-04-26 / C-3 統一、2026-04-26）========
//
// 旧定数 (F1 前): 月160h（= 8h × 20日）。
// 新定数: 月167h（= 8h × 20.875日）— 厚労省「就業条件総合調査 2024」の
// 1企業平均所定労働時間 169.0h を保守側に丸めた値。
//
// **C-3 統一 (2026-04-26)**: salary_parser.rs::HOURLY_TO_MONTHLY も 167.0 (旧 173.8) に統一。
// salary_parser::DAILY_TO_MONTHLY も 21.0 (旧 21.7) に統一。
// 統一後は parse_salary 経由 / aggregator 直変換 の両経路で月給換算値が一致。
// GAS 互換性は V2 HW Dashboard の要件外と判断 (V2 は独立リポ)。
//
// 影響: 給与表示の数値が aggregator 経路で約 4.4% (167/160) 上昇、
// salary_parser 経路では時給で約 -3.9% (167/173.8) 低下、日給で約 -3.2% (21/21.7) 低下。
//
// Phase 2-A (2026-05-29): 換算定数を pub に変更。navy_report.rs (Section 03 表 3-E /
// 図 3-5 で時給モード時の月給→時給逆換算) から `super::aggregator::HOURLY_TO_MONTHLY_HOURS`
// として参照するため可視性を上げる。値は変更なし。
/// 時給→月給 換算係数 (時間/月)
pub const HOURLY_TO_MONTHLY_HOURS: i64 = 167;
/// 日給→月給 換算係数 (日/月) — 20.875 を整数丸め
pub const DAILY_TO_MONTHLY_DAYS: i64 = 21;
/// 1日所定労働時間 (時間) — 日給→時給で使用
pub const DAILY_HOURS: i64 = 8;
/// 週給→月給 換算: scale 433 / 100 = 4.33 (= 52週/12月)
pub const WEEKLY_TO_MONTHLY_NUM: i64 = 433;
pub const WEEKLY_TO_MONTHLY_DEN: i64 = 100;
/// 週所定労働時間 (時間) — 週給→時給で使用
pub const WEEKLY_HOURS: i64 = 40;

// ======== 給与×年間休日 散布図 軸定数 (Finding #14, 2026-06-30) ========
/// 給与×年間休日 散布図 X 軸 (月給円) 最小値
pub const SCATTER_X_MIN: i64 = 150_000;

// ======== 給与フィルタ閾値定数 (Finding #12, 2026-07-01) ========
/// 月給フィルタ下限: 5 万円未満は異常値 / 誤抽出として除外
pub const MIN_MONTHLY_SALARY: i64 = 50_000;
/// 月給換算の上限ガード。2026-08-03: 説明文の「491万6000円/30歳」(年収実績) が
/// 月給4,916,000円として salary_min_values に混入し、最低給与平均を+5.7%押し上げた
/// 実測を受けて追加。journey.rs の集計フィルタ (50_000..=2_000_000) と同じ値にし、
/// 同一アプリ内で外れ値ガードの基準を揃える。
pub const MAX_MONTHLY_SALARY: i64 = 2_000_000;
/// 給与×年間休日 散布図 Y 軸 (年間休日) 最小値
pub const SCATTER_Y_MIN: i64 = 70;
/// 給与×年間休日 散布図 Y 軸 (年間休日) 最大値
pub const SCATTER_Y_MAX: i64 = 180;

/// カード単位の観測 (解説資料 Phase 2a、2026-07-20)
///
/// 解説資料 (?variant=guide&company=) が依頼企業の求人カードそのものを
/// 市場分布に重ねるための最小フィールド。全レコード分を保持する
/// (1,000 件規模 × 小フィールドでキャッシュ JSON 負荷は軽微)。
#[derive(Debug, Clone, Serialize, Deserialize, Default)]
pub struct CardBrief {
    pub company: String,
    /// 求人タイトル (先頭 60 文字)
    pub title: String,
    /// 月給レコードか (時給・日給等は false)
    pub is_monthly: bool,
    /// 給与欄の下限 (パース生値、円)
    pub salary_min: Option<i64>,
    /// 給与欄の上限 (パース生値、円)。None または min と同値なら「単一値表示」
    pub salary_max: Option<i64>,
    /// 年間休日 (説明文からの規則ベース抽出)
    pub annual_holidays: Option<i64>,
    /// 新着表示の有無
    pub is_new: bool,
    /// 説明文中の給与記載 (「月収35万円」等の万円値)。給与欄との乖離検出用
    pub desc_salary_man: Option<i64>,
}

/// 説明文から「月収◯万」「月給◯万」の万円値を抽出する (最大値)。
///
/// 給与欄が単一値なのに説明文に高い到達額が書かれているケース
/// (訴求文と給与欄の乖離) を検出するための規則ベース抽出。
/// 対象は 10〜99 万のみ (誤抽出防止: 1桁は「3万円分の手当」等と混同しやすい)。
pub fn extract_desc_salary_man(description: &str) -> Option<i64> {
    let text = description;
    let mut best: Option<i64> = None;
    for key in ["月収", "月給"] {
        let mut start = 0;
        while let Some(pos) = text[start..].find(key) {
            let abs = start + pos + key.len();
            // キー直後の数字列 (最大3桁) を読む (ASCII 数字のみなのでバイト境界は安全)
            let digits: String = text[abs..]
                .chars()
                .take_while(|c| c.is_ascii_digit())
                .take(3)
                .collect();
            let after = &text[abs + digits.len()..];
            if !digits.is_empty() && after.starts_with('万') {
                if let Ok(v) = digits.parse::<i64>() {
                    if (10..=99).contains(&v) && best.map_or(true, |b| v > b) {
                        best = Some(v);
                    }
                }
            }
            start = abs;
        }
    }
    best
}

/// 企業別集計
#[derive(Debug, Clone, Serialize, Deserialize, Default)]
pub struct CompanyAgg {
    pub name: String,
    pub count: usize,
    pub avg_salary: i64,
    pub median_salary: i64,
}

/// タグ別給与集計
#[derive(Debug, Clone, Serialize, Deserialize, Default)]
pub struct TagSalaryAgg {
    pub tag: String,
    pub count: usize,
    pub avg_salary: i64,
    pub diff_from_avg: i64, // 全体平均との差分（円）
    pub diff_percent: f64,  // 差分率（%）
}

/// 市区町村別給与集計
#[derive(Debug, Clone, Serialize, Deserialize, Default)]
pub struct MunicipalitySalaryAgg {
    pub name: String,
    pub prefecture: String,
    pub count: usize,
    pub avg_salary: i64,
    pub median_salary: i64,
}

/// 都道府県別給与集計
#[derive(Debug, Clone, Serialize, Deserialize, Default)]
pub struct PrefectureSalaryAgg {
    pub name: String,
    pub count: usize,
    pub avg_salary: i64,
    /// 下限給与の平均 (月給換算、円/月)。時給×167 / 日給×21、年俸・週給・不明と
    /// 5万〜200万円の範囲外は除外 (salary_min_values と同じ規則)
    pub avg_min_salary: i64,
    /// 時給求人の件数 (時給モードの表2-E 用、2026-09-29)
    #[serde(default)]
    pub hourly_count: usize,
    /// 時給求人の代表時給 (下限・上限の中間、上限なしは下限) の平均 (円/時)
    #[serde(default)]
    pub avg_hourly_salary: i64,
}

/// 市区町村単位の公的統計 (表 2-D 市区町村化用, 2026-07-27)。
///
/// 「都道府県平均比較」を「指定市区町村 + 同一県内近隣」比較へ切り替えるための
/// 行データ。値は SSDSE-A (v2_external_labor_force / v2_external_households) を
/// `fetch_municipality_mean` で市区町村粒度に SUM したもの。データ欠損時は None。
/// `is_base = true` はアプリで選択した基準市区町村を示す。
#[derive(Debug, Clone, Default)]
pub struct RegionMuniStat {
    pub prefecture: String,
    pub municipality: String,
    /// アプリで選択した基準市区町村なら true。
    pub is_base: bool,
    /// 失業率 (%)。0-100 域外 / 欠損は None。
    pub unemployment_rate: Option<f64>,
    /// 単身世帯率 (%)。0-100 域外 / 欠損は None。
    pub single_rate: Option<f64>,
}

/// 散布図データ点
#[derive(Debug, Clone, Serialize, Deserialize, Default)]
pub struct ScatterPoint {
    pub x: i64,
    pub y: i64,
}

/// 回帰分析結果
#[derive(Debug, Clone, Serialize, Deserialize, Default)]
pub struct RegressionResult {
    pub slope: f64,
    pub intercept: f64,
    pub r_squared: f64,
}

/// 雇用形態別給与
#[derive(Debug, Clone, Serialize, Deserialize, Default)]
pub struct EmpTypeSalary {
    pub emp_type: String,
    pub count: usize,
    /// 月給換算 (unified_monthly) の平均
    pub avg_salary: i64,
    /// 月給換算 (unified_monthly) の中央値
    pub median_salary: i64,
    /// 時給求人の件数 (時給モードの表3-B 用、2026-09-29)
    #[serde(default)]
    pub hourly_count: usize,
    /// 時給求人の代表時給 (下限・上限の中間、上限なしは下限) の平均 (円/時)
    #[serde(default)]
    pub avg_hourly_salary: i64,
    /// 同 中央値 (円/時)
    #[serde(default)]
    pub median_hourly_salary: i64,
}

#[derive(Debug, Clone, Serialize, Deserialize, Default)]
pub struct SurveyAggregation {
    pub total_count: usize,
    pub new_count: usize,
    pub salary_parse_rate: f64,
    pub location_parse_rate: f64,
    pub dominant_prefecture: Option<String>,
    pub dominant_municipality: Option<String>,
    pub by_prefecture: Vec<(String, usize)>,
    pub by_salary_range: Vec<(String, usize)>,
    pub by_employment_type: Vec<(String, usize)>,
    pub by_tags: Vec<(String, usize)>,
    pub salary_values: Vec<i64>,
    pub enhanced_stats: Option<super::statistics::EnhancedStats>,
    // レポート用追加フィールド
    pub by_company: Vec<CompanyAgg>,
    pub by_emp_type_salary: Vec<EmpTypeSalary>,
    pub salary_min_values: Vec<i64>,
    pub salary_max_values: Vec<i64>,
    pub by_tag_salary: Vec<TagSalaryAgg>,
    pub by_municipality_salary: Vec<MunicipalitySalaryAgg>,
    pub scatter_min_max: Vec<ScatterPoint>,
    pub regression_min_max: Option<RegressionResult>,
    pub by_prefecture_salary: Vec<PrefectureSalaryAgg>,
    pub is_hourly: bool,
    /// 2026-04-24 Phase 2: 雇用形態グループ別 ネイティブ単位集計
    /// 正社員系 → 月給 / パート系 → 時給 で別々に集計
    #[serde(default)]
    pub by_emp_group_native: Vec<EmpGroupNativeAgg>,
    /// 2026-04-24 全体 IQR 外れ値除外 (raw salary_values → filtered) で除外された件数
    #[serde(default)]
    pub outliers_removed_total: usize,
    /// IQR 除外前の raw 件数
    #[serde(default)]
    pub salary_values_raw_count: usize,
    // ============================================================
    // Phase 2-A (2026-05-29): 時給モード対応 ネイティブ単位給与値
    // ------------------------------------------------------------
    // 既存 salary_min_values / salary_max_values / scatter_min_max は
    // 「月給換算強制」(時給×167h / 日給×21日) で集計しているが、
    // 時給モードでは円/時のネイティブ値で表示する必要がある。
    //
    // 命名規則:
    //   - salary_min_values_native:  Hourly レコードは 円/時 のまま、
    //                                Monthly レコードは 円/月 のまま push。
    //                                Daily / Weekly は Phase 2-A では対象外
    //                                (時給モード時に意味のある換算先がないため未 push)。
    //   - scatter_min_max_native:    (下限, 上限) のペア (i64, i64)。
    //                                ScatterPoint と異なり struct ではなく tuple。
    //
    // 既存フィールドとの併存:
    //   - 月給モード時は既存 salary_min_values をそのまま使用 (動作不変)。
    //   - 時給モード時は agg.is_hourly = true なので、navy_report.rs は
    //     これら _native フィールドを優先参照する。
    //   - 後方互換: 既存 caller (E2E / 旧 test fixture) で新フィールドが
    //     空 Vec でも、月給モードでは旧フィールドを使うため動作崩れなし。
    // ============================================================
    /// 下限給与 (ネイティブ単位、換算なし)。2026-09-29 以降はモードの主単位のみ:
    /// is_hourly=true なら Hourly の円/時だけ、false なら Monthly の円/月だけ (混在させない)
    #[serde(default)]
    pub salary_min_values_native: Vec<i64>,
    /// 上限給与 (ネイティブ単位)。単位の扱いは salary_min_values_native と同じ
    #[serde(default)]
    pub salary_max_values_native: Vec<i64>,
    /// 散布図用 (下限, 上限) ペア。ネイティブ単位 (時給モード=Hourly の円/時のみ、月給モード=Monthly の円/月のみ)
    #[serde(default)]
    pub scatter_min_max_native: Vec<(i64, i64)>,

    // ========================================================================
    // 2026-06-24 求人ボックス年間休日分析機能 (GAS Aggregator.js 移植 + 拡張)
    // 2026-06-30 Finding #12: Section 07.5 関連 11 フィールドを JobboxAnalysis に集約
    // ========================================================================
    /// Section 07.5 (求人ボックス年間休日分析) 関連集計の集約サブ構造体
    #[serde(default)]
    pub jobbox: JobboxAnalysis,

    // ========================================================================
    // 2026-06-30 Section 07.6: Indeed (SP) 人気/超人気 タグ集計
    // ========================================================================
    /// Section 07.6 (Indeed SP の人気度シグナル) 集計サブ構造体
    #[serde(default)]
    pub popularity: PopularityAnalysis,

    /// 2026-07-20 Phase 2a: カード単位の観測 (解説資料の依頼企業カード解析用)。
    /// serde default で旧キャッシュ JSON と後方互換 (欠損時は空 Vec)。
    #[serde(default)]
    pub card_briefs: Vec<CardBrief>,

    /// 2026-07-28: アップロード CSV に1件以上出現する全 (都道府県, 市区町村) の集合。
    ///
    /// `by_municipality_salary` は上位15件で truncate されるため、SP レポートの
    /// 0件ハードブロック判定 (`evaluate_selection_gate`) に使うと 16 位以下の
    /// 市区町村を明示選択した際に誤ブロックされる。この集合は truncate 前の
    /// 全在否を保持し、給与の有無に関わらず location_parsed ベースで判定する。
    /// 同名市区町村 (伊達市: 北海道/福島県、府中市: 東京都/広島県) は
    /// (都道府県, 市区町村) ペアで区別するため、都道府県違いを取り違えない。
    ///
    /// serde default (空集合) で旧キャッシュ JSON と後方互換。旧 JSON 復元時は
    /// 空集合となり `has_municipality` が常に false を返すが、その挙動は
    /// 「明示選択 muni は CSV に無い扱い」となり従来 (truncate 済みリスト参照) と
    /// 同等以上に保守的なため安全。次回集計時に正しい集合が入る。
    #[serde(default)]
    pub municipality_presence: std::collections::HashSet<(String, String)>,

    /// 2026-09-29: 競合調査章 (Indeed 掲載求人) 用の集計。
    /// serde default で旧キャッシュ JSON と後方互換 (欠損時は空 = 章を出さない)。
    #[serde(default)]
    pub competitor: CompetitorAnalysis,
}

impl SurveyAggregation {
    /// 明示選択された (都道府県, 市区町村) がアップロード CSV に1件以上存在するか。
    ///
    /// `by_municipality_salary` は上位15件で truncate されるため 0件ゲート判定には
    /// 使えない。truncate の影響を受けない `municipality_presence` 集合で在否を判定する。
    /// 同名市区町村 (伊達市: 北海道/福島県 等) は (都道府県, 市区町村) ペアで区別する。
    pub fn has_municipality(&self, prefecture: &str, municipality: &str) -> bool {
        self.municipality_presence
            .contains(&(prefecture.to_string(), municipality.to_string()))
    }
}

/// 競合調査章で「検索上位 N 件」として保持する最大件数 (N の上限と同じ)。
pub const COMPETITOR_HEAD_MAX: usize = 200;

/// 下限・上限それぞれの件数 / 平均 / 中央値 (競合調査章の人気比較用、2026-09-29)。
///
/// 単位は `CompetitorAnalysis::pop_is_hourly` に従う (true=円/時、false=円/月)。
/// 下限と上限は独立に集計する (上限なしの求人は上限の n に入らない)。
#[derive(Debug, Default, Clone, Serialize, Deserialize, PartialEq)]
pub struct BoundStats {
    pub min_n: usize,
    pub min_mean: Option<i64>,
    pub min_median: Option<i64>,
    pub max_n: usize,
    pub max_mean: Option<i64>,
    pub max_median: Option<i64>,
}

impl BoundStats {
    /// 下限値・上限値の配列から作る。平均は整数に四捨五入 (小数を捨てると時給 1262.5 が 1262 になるため)。
    pub fn from_values(mins: &[i64], maxs: &[i64]) -> Self {
        fn mean_opt(v: &[i64]) -> Option<i64> {
            if v.is_empty() {
                None
            } else {
                let sum: i128 = v.iter().map(|&x| x as i128).sum();
                let n = v.len() as i128;
                Some(((2 * sum + n).div_euclid(2 * n)) as i64)
            }
        }
        fn median_opt(v: &[i64]) -> Option<i64> {
            if v.is_empty() {
                None
            } else {
                Some(median_of(v))
            }
        }
        Self {
            min_n: mins.len(),
            min_mean: mean_opt(mins),
            min_median: median_opt(mins),
            max_n: maxs.len(),
            max_mean: mean_opt(maxs),
            max_median: median_opt(maxs),
        }
    }
}

/// 競合調査章 (Indeed 掲載求人のスクレイプ) 用の集計 (2026-09-29)。
///
/// レコード本体はキャッシュされないため、レポート生成時に N (検索上位件数) を
/// 変えられるよう、取り込み順の先頭 `COMPETITOR_HEAD_MAX` 件のタグ列を保持する。
#[derive(Debug, Default, Clone, Serialize, Deserialize)]
pub struct CompetitorAnalysis {
    /// Indeed (PC / SP) 由来の件数 (重複排除後)。0 なら章を出さない。
    pub indeed_count: usize,
    /// 取り込み順 (row_index 昇順、重複排除後) の先頭 `COMPETITOR_HEAD_MAX` 件のタグ列。
    /// 分解規則は by_tags と同じ (`split_tags`) だが、1 求人内の重複タグは 1 つにまとめる (`record_tags`)。
    pub head_tags: Vec<Vec<String>>,
    /// タグを付けた求人の件数 (求人単位で重複排除、上位での切り詰めなし)。件数降順 → タグ名昇順。
    /// by_tags (出現回数) とは異なり、件数 ≤ 求人数 が常に成り立つ (占有率 ≤ 100%)。
    pub tag_counts_all: Vec<(String, usize)>,
    /// 人気比較の単位。true = 時給求人のみ (円/時)、false = 月給求人のみ (円/月)。
    /// SurveyAggregation::is_hourly と同じ値 (ネイティブ単位配列と同じ方針)。
    pub pop_is_hourly: bool,
    /// Indeed (SP) 由来の全件 (人気・超人気・タグなし) の下限・上限統計
    pub pop_all: BoundStats,
    /// Indeed (SP) 由来のうち「人気」または「超人気」タグ付きの下限・上限統計
    pub pop_popular: BoundStats,
    /// 実額の最頻値。同数の場合は低い金額を採用する。
    #[serde(default)]
    pub salary_modes: [Option<i64>; 4],
}

/// tags_raw をタグに分解する (by_tags / 競合調査章で共通)。
/// 区切り `,` `、` `/` タブ。危険 URL プレフィックスを除去し、空と 20 文字超は捨てる。
pub(crate) fn split_tags(tags_raw: &str) -> Vec<String> {
    use super::super::helpers::sanitize_tag_text;
    if tags_raw.is_empty() {
        return Vec::new();
    }
    tags_raw
        .split([',', '、', '/', '\t'])
        .map(sanitize_tag_text)
        .filter(|t| !t.is_empty() && t.chars().count() <= 20)
        .collect()
}

/// 1 求人のタグ (split_tags と同じ分解、同じタグは初出の 1 つだけ残す)。
/// 競合調査章の占有率 (= そのタグを付けた求人の割合) の分子に使う。
pub(crate) fn record_tags(tags_raw: &str) -> Vec<String> {
    let mut out: Vec<String> = Vec::new();
    for t in split_tags(tags_raw) {
        if !out.contains(&t) {
            out.push(t);
        }
    }
    out
}

/// §05 人気度と同じ判定 (`,` 区切り + 厳密一致)。戻り値 (超人気, 人気)。
fn popularity_signal(tags_raw: &str) -> (bool, bool) {
    let tokens: Vec<&str> = tags_raw.split(',').map(|s| s.trim()).collect();
    (tokens.contains(&"超人気"), tokens.contains(&"人気"))
}

/// 競合調査章の集計。`is_hourly` は SurveyAggregation::is_hourly と同じ値を渡す。
fn compute_competitor(records: &[SurveyRecord], is_hourly: bool) -> CompetitorAnalysis {
    use super::upload::CsvSource;
    let indeed_count = records
        .iter()
        .filter(|r| matches!(r.source, CsvSource::Indeed | CsvSource::IndeedSp))
        .count();

    // 取り込み順 (row_index) の先頭 N 件。records は通常すでにこの順だが明示的に並べる。
    let mut order: Vec<&SurveyRecord> = records.iter().collect();
    order.sort_by_key(|r| r.row_index);
    let head_tags: Vec<Vec<String>> = order
        .iter()
        .take(COMPETITOR_HEAD_MAX)
        .map(|r| record_tags(&r.tags_raw))
        .collect();

    let mut tag_map: HashMap<String, usize> = HashMap::new();
    for r in records {
        for t in record_tags(&r.tags_raw) {
            *tag_map.entry(t).or_default() += 1;
        }
    }
    let mut tag_counts_all: Vec<(String, usize)> = tag_map.into_iter().collect();
    tag_counts_all.sort_by(|a, b| b.1.cmp(&a.1).then_with(|| a.0.cmp(&b.0)));

    // 人気比較: §05 と同じく Indeed (SP) 由来のみが母数。単位はモードの主単位だけ
    // (native_value_for_mode: 時給モード=時給求人の円/時、月給モード=月給求人の円/月)。
    let (mut all_min, mut all_max, mut pop_min, mut pop_max) =
        (Vec::new(), Vec::new(), Vec::new(), Vec::new());
    for r in records {
        if !matches!(r.source, CsvSource::IndeedSp) {
            continue;
        }
        let (is_super, is_popular) = popularity_signal(&r.tags_raw);
        let st = &r.salary_parsed.salary_type;
        let lo = r
            .salary_parsed
            .min_value
            .and_then(|v| native_value_for_mode(v, st, is_hourly));
        let hi = r
            .salary_parsed
            .max_value
            .and_then(|v| native_value_for_mode(v, st, is_hourly));
        if let Some(v) = lo {
            all_min.push(v);
            if is_super || is_popular {
                pop_min.push(v);
            }
        }
        if let Some(v) = hi {
            all_max.push(v);
            if is_super || is_popular {
                pop_max.push(v);
            }
        }
    }

    CompetitorAnalysis {
        indeed_count,
        head_tags,
        tag_counts_all,
        pop_is_hourly: is_hourly,
        pop_all: BoundStats::from_values(&all_min, &all_max),
        pop_popular: BoundStats::from_values(&pop_min, &pop_max),
        salary_modes: [&all_min, &all_max, &pop_min, &pop_max].map(|values| {
            let mut counts = std::collections::BTreeMap::new();
            for value in values {
                *counts.entry(*value).or_insert(0usize) += 1;
            }
            counts
                .into_iter()
                .fold(None, |best: Option<(i64, usize)>, item| {
                    if best.is_none_or(|(_, count)| item.1 > count) {
                        Some(item)
                    } else {
                        best
                    }
                })
                .map(|(value, _)| value)
        }),
    }
}

/// Section 07.6 用集計の集約サブ構造体 (Indeed SP のみ取得可能)
///
/// Indeed (SP) スマートフォン版 CSV の `css-u74ql7` 列に格納される
/// 「人気」「超人気」タグを集計する。Indeed (SP) 以外のソースでは
/// 全件 `none_count` となり、`popular_count + super_popular_count == 0`
/// の場合は描画側で section ごとスキップする想定。
///
/// 中央値計算対象:
/// - `popular_salary_median` / `non_popular_salary_median`: Monthly 給与のみ
///   (時給/年俸/日給を混ぜると単位が混在するため除外)
/// - `popular_holidays_median` / `non_popular_holidays_median`:
///   `annual_holidays.is_some()` のレコードのみ
#[derive(Debug, Default, Clone, Serialize, Deserialize)]
pub struct PopularityAnalysis {
    /// 人気タグ付き件数
    pub popular_count: usize,
    /// 超人気タグ付き件数
    pub super_popular_count: usize,
    /// タグなし件数
    pub none_count: usize,
    /// IndeedSp 件数に占める人気タグ比率 (popular + super_popular) / indeed_sp_total
    /// 2026-07-01 Finding #2 修正: 分母を IndeedSp 由来件数に限定 (旧: 全 source total)
    pub popular_ratio: f64,
    /// 集計母集団: Indeed (SP) 由来レコード数 (popular_ratio 等の分母)
    /// 2026-07-01 Finding #2 で追加。serde default=0 で旧 JSON 互換。
    #[serde(default)]
    pub indeed_sp_total: usize,
    /// 人気タグ付き求人の月給中央値 (円、Monthly のみ)
    pub popular_salary_median: Option<i64>,
    /// 人気タグなし求人の月給中央値 (円、Monthly のみ)
    pub non_popular_salary_median: Option<i64>,
    /// 人気タグ付き求人の年間休日中央値 (日)
    pub popular_holidays_median: Option<i64>,
    /// 人気タグなし求人の年間休日中央値 (日)
    pub non_popular_holidays_median: Option<i64>,
    /// 月給中央値算出に使用したサンプル数 (人気タグあり、Monthly のみ)
    /// Finding #5 (2026-07-01): n < 5 の場合は表示側で "— (n不足)" に差し替える
    #[serde(default)]
    pub popular_n_salary: usize,
    /// 月給中央値算出に使用したサンプル数 (人気タグなし、Monthly のみ)
    #[serde(default)]
    pub non_popular_n_salary: usize,
    /// 年間休日中央値算出に使用したサンプル数 (人気タグあり)
    #[serde(default)]
    pub popular_n_holidays: usize,
    /// 年間休日中央値算出に使用したサンプル数 (人気タグなし)
    #[serde(default)]
    pub non_popular_n_holidays: usize,
    /// 人気タグ付き求人 (IndeedSp、Monthly のみ) の給与下限・上限統計
    /// 2026-07-01 追加。
    #[serde(default)]
    pub popular_salary_stats: SalaryStats,
    /// 超人気タグ付き求人 (IndeedSp、Monthly のみ) の給与下限・上限統計
    /// 2026-07-01 追加。
    #[serde(default)]
    pub super_popular_salary_stats: SalaryStats,
    /// 人気タグなし求人 (IndeedSp、Monthly のみ) の給与下限・上限統計
    /// 2026-07-01 追加。
    #[serde(default)]
    pub non_popular_salary_stats: SalaryStats,
    /// 超人気タグ付き求人 (IndeedSp) の年間休日統計 (件数・中央値・平均・P25/P75)
    /// 2026-07-28 追加。annual_holidays が Some のレコードのみを分母とする。
    /// `#[serde(default)]` により旧 JSON キャッシュは n=0 で復元 (§07.6-4 スキップ)。
    #[serde(default)]
    pub super_popular_holiday_stats: HolidayStats,
    /// 人気タグ付き求人 (IndeedSp) の年間休日統計
    /// 2026-07-28 追加。
    #[serde(default)]
    pub popular_holiday_stats: HolidayStats,
    /// 人気タグなし求人 (IndeedSp) の年間休日統計
    /// 2026-07-28 追加。
    #[serde(default)]
    pub non_popular_holiday_stats: HolidayStats,
}

/// Section 07.5 用集計の集約サブ構造体 (2026-06-30 Finding #12)
///
/// 旧 `SurveyAggregation` 直下の 11 フィールドをここに集約。
/// `#[serde(default)]` 付きで `SurveyAggregation::jobbox` に保持されるため、
/// 旧 JSON (フィールド未存在) でも default で復元可能。
#[derive(Debug, Clone, Default, Serialize, Deserialize)]
pub struct JobboxAnalysis {
    /// 抽出に成功した年間休日値の生データ (全 source 対象、70-180 範囲内の抽出値のみ)
    #[serde(default)]
    pub annual_holidays_values: Vec<i64>,
    /// 年間休日カテゴリ分布 (`upload::ANNUAL_HOLIDAYS_CATEGORIES` 順)
    #[serde(default)]
    pub annual_holidays_category_distribution: Vec<(String, usize)>,
    /// 給与×年間休日 散布図データ (x=月給換算円、y=年間休日日)
    #[serde(default)]
    pub salary_vs_holidays_scatter: Vec<ScatterPoint>,
    /// 個別求人レコード一覧 (求人ボックスCSV のみ、年間休日抽出成功分)
    #[serde(default)]
    pub jobbox_records: Vec<JobBoxRecord>,
    /// 年間休日 120日以上比率 (週休2日+祝日達成率、0.0-1.0)
    #[serde(default)]
    pub holiday_pct_ge_120: f64,
    /// 年間休日 125日以上比率 (完全週休2日+α達成率、0.0-1.0)
    #[serde(default)]
    pub holiday_pct_ge_125: f64,
    /// 年間休日の標準偏差
    #[serde(default)]
    pub holiday_stddev: f64,
    /// 年間休日の第3四分位 (Q3)
    #[serde(default)]
    pub holiday_q3: i64,
    /// 給与×年間休日 散布図データ (雇用形態付き) - 色分け用
    /// (x=月給換算円、y=年間休日日、emp_type="正社員"|"パート・アルバイト"|"その他")
    #[serde(default)]
    pub salary_vs_holidays_scatter_emp: Vec<(i64, i64, String)>,
    /// 給与×年間休日 相関係数 (Pearson r、-1.0〜1.0)
    /// None = 算出不可 (データ点数 < 3)
    #[serde(default)]
    pub salary_holidays_correlation: Option<f64>,
    /// 給与×年間休日 線形回帰 (slope, intercept)
    /// 月給(円)を入力とする回帰式: holidays = slope * salary + intercept
    #[serde(default)]
    pub salary_holidays_regression: Option<(f64, f64)>,
    /// 年間休日カテゴリ別 給与統計 (Monthly のみ、求人ボックスのみ)
    ///
    /// カテゴリ順は `upload::ANNUAL_HOLIDAYS_CATEGORIES` に一致 (固定 6 要素)。
    /// 各カテゴリの Monthly 求人の (salary_min, salary_max) を集計。
    /// カテゴリに該当するレコードがない場合は `SalaryStats::default()` (n=0)。
    /// 2026-07-01 追加。
    #[serde(default)]
    pub salary_stats_by_holiday_category: Vec<(String, SalaryStats)>,
    /// 機能 C (2026-07-07): 年間休日を Gemini AI フォールバックで抽出できた件数。
    ///
    /// regex (`extract_annual_holidays`) で拾えず、AI 抽出 (70-180 で再検証済) に
    /// 成功したレコード数。集計自体 (`annual_holidays_values` 等) には regex 分と合算されるが、
    /// この件数だけは注記の透明性のために別途保持する。
    /// aggregator は常に 0 で構築し、AI 抽出を行う handlers.rs が集計後に上書きする。
    /// `#[serde(default)]` により旧 JSON / AI 非経路では 0 (= §07.5 注記は従来文言)。
    #[serde(default)]
    pub ai_extracted_count: usize,
}

/// 散布図用 雇用形態 3 値分類 (2026-06-30 Finding #13)
///
/// `salary_vs_holidays_scatter_emp` の色分け用ラベルを生成する。
/// 戻り値は固定 3 種: `"正社員"` / `"パート・アルバイト"` / `"その他"`。
///
/// 判定順は狭→広 (正職員 → 正社員 → 契約 → 派遣 → パート/アルバイト)。
/// 契約社員/派遣はシンプル優先で "その他" にマッピング。
///
/// **注意**: section_07_5_jobbox_detail.rs の `render_emp_badge` は表示用に 5+ 種の
/// カラーリングを行うため、本関数とは粒度が異なる別関数のまま (粒度差は意図的)。
pub fn classify_employment_for_scatter(et: &str) -> &'static str {
    if et.contains("正職員") || et.contains("正社員") {
        "正社員"
    } else if et.contains("契約") {
        "その他"
    } else if et.contains("派遣") {
        "その他"
    } else if et.contains("パート") || et.contains("アルバイト") || et.contains("バイト")
    {
        "パート・アルバイト"
    } else {
        "その他"
    }
}

/// 求人ボックス個別求人レコード (2026-06-24 追加、Section 07.5 用)
///
/// 年間休日抽出成功 + 求人ボックスソースのレコードのみを保持する。
/// 「企業名×年間休日×給与」テーブル用。
/// salary_unit は月給固定のため除外。salary_raw / url は描画未参照のため除外。
#[derive(Debug, Clone, Serialize, Deserialize, Default)]
pub struct JobBoxRecord {
    pub company_name: String,
    pub job_title: String,
    pub location: String,
    pub employment_type: String,
    pub annual_holidays: i64,
    pub salary_min: Option<i64>,
    pub salary_max: Option<i64>,
}

/// 雇用形態グループ別 ネイティブ単位集計
///
/// Phase 2: 正社員・契約社員・業務委託 → 月給ベース
///          パート・アルバイト・派遣パート → 時給ベース
///          派遣社員 → グループ内多数派で動的決定
#[derive(Debug, Clone, Serialize, Deserialize, Default)]
pub struct EmpGroupNativeAgg {
    /// グループラベル: "正社員" / "パート" / "派遣・その他"
    pub group_label: String,
    /// 表示単位: "月給" / "時給"
    pub native_unit: String,
    /// そのグループの件数（IQR 除外後）
    pub count: usize,
    /// そのグループに含まれる雇用形態の内訳（表示用）
    pub included_emp_types: Vec<String>,
    /// ネイティブ単位の給与値 (円)
    /// native_unit=月給 なら月給値、native_unit=時給 なら時給値
    pub median: i64,
    pub mean: i64,
    pub min: i64,
    pub max: i64,
    /// ヒストグラム描画用 (IQR 除外後)
    pub values: Vec<i64>,
    /// 2026-04-24 グループ内 IQR で除外された件数
    #[serde(default)]
    pub outliers_removed: usize,
    /// IQR 除外前の件数（count + outliers_removed）
    #[serde(default)]
    pub raw_count: usize,
}

/// スライスの中央値を計算（コピー＆ソートする）
/// - 空: 0
/// - 奇数件: 中央要素
/// - 偶数件: 中央2要素の平均（整数割り算）
/// `enhanced_salary_statistics` の定義と整合。
///
/// 2026-06-30 Finding #18: section_07_5_jobbox_detail.rs の重複実装統合のため pub に昇格。
pub fn median_of(values: &[i64]) -> i64 {
    if values.is_empty() {
        return 0;
    }
    let mut sorted: Vec<i64> = values.to_vec();
    sorted.sort();
    let n = sorted.len();
    if n.is_multiple_of(2) {
        (sorted[n / 2 - 1] + sorted[n / 2]) / 2
    } else {
        sorted[n / 2]
    }
}

/// R-7 method (Excel/numpy 既定) によるパーセンタイル算出 — 線形補間
/// 2026-06-30 Finding #11 修正用。`sorted` は昇順ソート済みであること。
/// 小サンプル (n < 20) では呼び出し側で nearest-rank を継続使用する想定。
fn percentile_r7(sorted: &[i64], p: f64) -> i64 {
    if sorted.is_empty() {
        return 0;
    }
    if sorted.len() == 1 {
        return sorted[0];
    }
    let h = p * (sorted.len() - 1) as f64;
    let lo = h.floor() as usize;
    let hi = h.ceil() as usize;
    let frac = h - lo as f64;
    let lo_v = sorted[lo] as f64;
    let hi_v = sorted[hi] as f64;
    (lo_v + (hi_v - lo_v) * frac).round() as i64
}

// ============================================================
// 2026-07-01 SalaryStats: 給与下限/上限の平均・中央値・最頻値統計
// Section 07.5-2 (年間休日カテゴリ別) と Section 07.6 (人気タグ別) で
// 共通利用するため独立関数として提供する。
// ============================================================

/// 給与下限・上限の統計値 (平均/中央値/最頻値)
///
/// 単位: 円 (Monthly のみを想定、呼び出し側でフィルタする)。
/// - `min_*` は給与下限、`max_*` は給与上限の統計。
/// - `*_mode` は 5 万円刻みビン (val / 50_000 * 50_000) の最頻値。
///   複数タイなら小さい方 (ビン下端の小さい方) を返す。
/// - サンプル 0 件のとき全フィールド `None`、`n = 0`。
#[derive(Debug, Default, Clone, Serialize, Deserialize, PartialEq)]
pub struct SalaryStats {
    /// サンプル数 (下限・上限のいずれか Some のペア数)
    pub n: usize,
    /// 給与下限 平均 (整数丸め)
    pub min_mean: Option<i64>,
    /// 給与下限 中央値
    pub min_median: Option<i64>,
    /// 給与下限 最頻値 (5 万円刻みビン)
    pub min_mode: Option<i64>,
    /// 給与上限 平均 (整数丸め)
    pub max_mean: Option<i64>,
    /// 給与上限 中央値
    pub max_median: Option<i64>,
    /// 給与上限 最頻値 (5 万円刻みビン)
    pub max_mode: Option<i64>,
}

/// 給与下限・上限の (Option<i64>, Option<i64>) ペア列から `SalaryStats` を計算する
///
/// - `salaries`: `(salary_min, salary_max)` のペア列
/// - `n`: 下限・上限のいずれか `Some` のペア数
/// - 平均/中央値/最頻値はそれぞれ `Some` の値のみを集計 (下限と上限は独立)
/// - 最頻値は 5 万円刻みビン (`val / 50_000 * 50_000`) の最頻ビン下端。
///   複数タイなら小さい方 (小さいビン下端) を返す。
///
/// **注意**: 呼び出し側で Monthly のみを渡すこと (Hourly/Annual を混ぜると単位混在)。
pub fn compute_salary_stats(salaries: &[(Option<i64>, Option<i64>)]) -> SalaryStats {
    // n: 下限・上限どちらかが Some のペア数
    let n = salaries
        .iter()
        .filter(|(mn, mx)| mn.is_some() || mx.is_some())
        .count();
    if n == 0 {
        return SalaryStats::default();
    }

    let mins: Vec<i64> = salaries.iter().filter_map(|(mn, _)| *mn).collect();
    let maxs: Vec<i64> = salaries.iter().filter_map(|(_, mx)| *mx).collect();

    fn mean_opt(v: &[i64]) -> Option<i64> {
        if v.is_empty() {
            None
        } else {
            let sum: i128 = v.iter().map(|&x| x as i128).sum();
            Some((sum / v.len() as i128) as i64)
        }
    }
    fn median_opt(v: &[i64]) -> Option<i64> {
        if v.is_empty() {
            None
        } else {
            Some(median_of(v))
        }
    }
    /// 5 万円刻みビン (`val / 50_000 * 50_000`) の最頻値。
    /// タイなら小さい方 (ビン下端の小さい方) を返す。
    fn mode_bin_50k(v: &[i64]) -> Option<i64> {
        if v.is_empty() {
            return None;
        }
        let mut counts: HashMap<i64, usize> = HashMap::new();
        for &x in v {
            let bin = (x / 50_000) * 50_000;
            *counts.entry(bin).or_insert(0) += 1;
        }
        // 最大 count を取得 → その中で最小 bin を返す
        let max_count = counts.values().copied().max().unwrap_or(0);
        counts
            .into_iter()
            .filter(|&(_, c)| c == max_count)
            .map(|(b, _)| b)
            .min()
    }

    SalaryStats {
        n,
        min_mean: mean_opt(&mins),
        min_median: median_opt(&mins),
        min_mode: mode_bin_50k(&mins),
        max_mean: mean_opt(&maxs),
        max_median: median_opt(&maxs),
        max_mode: mode_bin_50k(&maxs),
    }
}

// ============================================================
// 2026-07-28 HolidayStats: 年間休日の件数・中央値・平均・四分位統計
// Section 07.6 (人気タグ 3 区分別) で使用する。給与側の SalaryStats に対応する
// 年間休日版。給与は下限/上限の 2 系列だが年間休日は単一系列 (annual_holidays)
// のため、フィールド構成は独立に定義する。
// ============================================================

/// 年間休日 (日) の分布統計 (件数・中央値・平均・P25・P75)
///
/// - `n`: 年間休日を抽出できたレコード数 (`annual_holidays.is_some()` のみ)。
///   `annual_holidays == None` は呼び出し側で除外され、分母 (n) に含めない。
/// - `median` / `mean`: 中央値 / 平均 (整数丸め、日)。
/// - `p25` / `p75`: 第 1・第 3 四分位 (R-7 法、日)。n=1 のときは両者とも唯一値。
/// - サンプル 0 件のとき全 `Option` フィールドは `None`、`n = 0`。
#[derive(Debug, Default, Clone, Serialize, Deserialize, PartialEq)]
pub struct HolidayStats {
    /// サンプル数 (年間休日を抽出できたレコード数)
    pub n: usize,
    /// 年間休日 中央値 (日)
    pub median: Option<i64>,
    /// 年間休日 平均 (整数丸め、日)
    pub mean: Option<i64>,
    /// 年間休日 第 1 四分位 P25 (R-7 法、日)
    pub p25: Option<i64>,
    /// 年間休日 第 3 四分位 P75 (R-7 法、日)
    pub p75: Option<i64>,
}

/// 年間休日値の列から `HolidayStats` を計算する
///
/// - `values`: `annual_holidays` を抽出できたレコードの値列 (None は呼び出し側で除外済み)
/// - `n`: `values.len()` (= 抽出できた件数)。分母の透明性のためそのまま保持する。
/// - median は `median_of`、P25/P75 は `percentile_r7` (Excel/numpy 既定の R-7 法) を使用。
pub fn compute_holiday_stats(values: &[i64]) -> HolidayStats {
    let n = values.len();
    if n == 0 {
        return HolidayStats::default();
    }
    let mut sorted = values.to_vec();
    sorted.sort();
    let sum: i128 = sorted.iter().map(|&x| x as i128).sum();
    let mean = (sum / n as i128) as i64;
    HolidayStats {
        n,
        median: Some(median_of(&sorted)),
        mean: Some(mean),
        p25: Some(percentile_r7(&sorted, 0.25)),
        p75: Some(percentile_r7(&sorted, 0.75)),
    }
}

/// 給与 1 値を月給換算する (salary_min_values / salary_max_values / 都道府県別下限平均で共通)。
/// 時給×167、日給×21、月給はそのまま。年俸・週給・不明は対象外 (None)。
/// 5万〜200万円の範囲外は異常値として None。
fn monthly_equiv_in_range(v: i64, salary_type: &super::salary_parser::SalaryType) -> Option<i64> {
    use super::salary_parser::SalaryType;
    let m = match salary_type {
        SalaryType::Hourly => v * HOURLY_TO_MONTHLY_HOURS,
        SalaryType::Daily => v * DAILY_TO_MONTHLY_DAYS,
        SalaryType::Monthly => v,
        _ => return None, // Annual / Weekly / Unknown は除外 (設計メモ §5 準拠)
    };
    (MIN_MONTHLY_SALARY..=MAX_MONTHLY_SALARY)
        .contains(&m)
        .then_some(m)
}

/// ネイティブ単位配列に入れる値。時給モードは Hourly (100 円/時以上) のみ、
/// 月給モードは Monthly (MIN_MONTHLY_SALARY 以上) のみ。単位の違う値は混ぜない。
fn native_value_for_mode(
    v: i64,
    salary_type: &super::salary_parser::SalaryType,
    is_hourly: bool,
) -> Option<i64> {
    use super::salary_parser::SalaryType;
    match salary_type {
        SalaryType::Hourly if is_hourly && v >= 100 => Some(v),
        SalaryType::Monthly if !is_hourly && v >= MIN_MONTHLY_SALARY => Some(v),
        _ => None,
    }
}

/// 時給求人の代表時給 (円/時)。下限・上限があれば中間値、片方のみならその値
/// (salary_parser の unified_monthly と同じ基準値を換算前で取る)。100 円/時未満は除外。
fn hourly_native_mid(r: &SurveyRecord) -> Option<i64> {
    use super::salary_parser::SalaryType;
    if r.salary_parsed.salary_type != SalaryType::Hourly {
        return None;
    }
    let base = match (r.salary_parsed.min_value, r.salary_parsed.max_value) {
        (Some(lo), Some(hi)) => (lo + hi) / 2,
        (Some(v), None) | (None, Some(v)) => v,
        (None, None) => return None,
    };
    (base >= 100).then_some(base)
}

/// パース済みレコードを集計
/// 後方互換: 自動判定モードで集計
pub fn aggregate_records(records: &[SurveyRecord]) -> SurveyAggregation {
    aggregate_records_with_mode(records, super::upload::WageMode::Auto)
}

/// ユーザー明示の給与単位モードで集計
///
/// - Monthly: 全レコードを月給換算で扱う（時給×160）
/// - Hourly:  全レコードを時給換算で扱う（月給/160）
/// - Auto:    多数派で自動判定（従来動作）
pub fn aggregate_records_with_mode(
    records: &[SurveyRecord],
    wage_mode: super::upload::WageMode,
) -> SurveyAggregation {
    use super::upload::WageMode;
    let forced_hourly = matches!(wage_mode, WageMode::Hourly);
    let forced_monthly = matches!(wage_mode, WageMode::Monthly);
    // forced_* は後段で is_hourly を上書きする際に使う
    let _ = (forced_hourly, forced_monthly);
    aggregate_records_core(records, wage_mode)
}

fn aggregate_records_core(
    records: &[SurveyRecord],
    wage_mode: super::upload::WageMode,
) -> SurveyAggregation {
    let total = records.len();
    if total == 0 {
        return SurveyAggregation::default();
    }

    let new_count = records.iter().filter(|r| r.is_new).count();

    // パース成功率
    let salary_ok = records
        .iter()
        .filter(|r| r.salary_parsed.min_value.is_some())
        .count();
    let location_ok = records
        .iter()
        .filter(|r| r.location_parsed.prefecture.is_some())
        .count();

    // 都道府県別
    // Finding #17 (2026-06-30): 都道府県数 ≤ 47 + 「不明」を見越して 50 予約
    let mut pref_map: HashMap<String, usize> = HashMap::with_capacity(50);
    for r in records {
        if let Some(pref) = &r.location_parsed.prefecture {
            *pref_map.entry(pref.clone()).or_default() += 1;
        }
    }
    let mut by_prefecture: Vec<(String, usize)> = pref_map.into_iter().collect();
    by_prefecture.sort_by(|a, b| b.1.cmp(&a.1));

    let dominant_prefecture = by_prefecture.first().map(|(p, _)| p.clone());

    // 市区町村別（最多を特定）
    // Finding #17 (2026-06-30): 平均 5 件/市区町村 を想定し records/5 を初期容量に
    // 2026-08-03: 2つの修正。
    //   (1) キーを (都道府県, 市区町村) にする。市区町村名だけだと東京都府中市と
    //       広島県府中市のような同名自治体が合算されていた。
    //   (2) 同数タイの勝者を (件数降順, 名前昇順) で決定的にする。HashMap の反復順は
    //       実行ごとに変わるため、max_by_key だけではレポートの表示地域が実行ごとに
    //       変わることがあった (実測: 16ファイル中9件で最多または15位に同数タイ)。
    let mut muni_map: HashMap<(String, String), usize> =
        HashMap::with_capacity(records.len() / 5 + 1);
    for r in records {
        if let Some(muni) = &r.location_parsed.municipality {
            let pref = r.location_parsed.prefecture.clone().unwrap_or_default();
            *muni_map.entry((pref, muni.clone())).or_default() += 1;
        }
    }
    let dominant_municipality = muni_map
        .into_iter()
        .max_by(|((_, muni_a), count_a), ((_, muni_b), count_b)| {
            count_a.cmp(count_b).then(muni_b.cmp(muni_a))
        })
        .map(|((_, muni), _)| muni);

    // 給与レンジ別
    // Finding #17 (2026-06-30): range_category は 10 種程度想定
    let mut salary_range_map: HashMap<String, usize> = HashMap::with_capacity(10);
    for r in records {
        if let Some(cat) = &r.salary_parsed.range_category {
            *salary_range_map.entry(cat.clone()).or_default() += 1;
        }
    }
    let mut by_salary_range: Vec<(String, usize)> = salary_range_map.into_iter().collect();
    by_salary_range.sort_by(|a, b| a.0.cmp(&b.0));

    // 雇用形態別
    // Finding #17 (2026-06-30): 雇用形態は 10 種程度想定 (正社員/正職員/契約/派遣/パート/アルバイト/業務委託 等)
    let mut emp_map: HashMap<String, usize> = HashMap::with_capacity(10);
    for r in records {
        let emp = if r.employment_type.is_empty() {
            "不明".to_string()
        } else {
            r.employment_type.clone()
        };
        *emp_map.entry(emp).or_default() += 1;
    }
    let mut by_employment_type: Vec<(String, usize)> = emp_map.into_iter().collect();
    by_employment_type.sort_by(|a, b| b.1.cmp(&a.1));

    // タグ別（カンマ/スペース区切りで分解、危険URLプレフィックスをサニタイズ）
    // 2026-09-29: 分解規則を split_tags に切り出し (競合調査章と共通)。同数のタグは
    //   タグ名昇順で並べる (旧: HashMap 順で同数の並びが実行ごとに変わっていた)。
    use super::super::helpers::sanitize_tag_text;
    // Finding #17 (2026-06-30): タグは 1 レコード平均 3 種程度を想定
    let mut tag_map: HashMap<String, usize> = HashMap::with_capacity(records.len() / 3 + 1);
    for r in records {
        for tag in split_tags(&r.tags_raw) {
            *tag_map.entry(tag).or_default() += 1;
        }
    }
    let mut by_tags: Vec<(String, usize)> = tag_map.into_iter().collect();
    by_tags.sort_by(|a, b| b.1.cmp(&a.1).then_with(|| a.0.cmp(&b.0)));
    by_tags.truncate(30); // 上位30タグ

    // 給与統計
    // 2026-04-24: IQR 法 (Q±1.5IQR) で外れ値除外後に統計計算
    let salary_values_raw: Vec<i64> = records
        .iter()
        .filter_map(|r| r.salary_parsed.unified_monthly)
        .collect();
    let (salary_values, outliers_removed_total) =
        super::statistics::filter_outliers_iqr(&salary_values_raw, 1.5);
    let enhanced_stats = enhanced_salary_statistics(&salary_values);
    // タグ集計用の外れ値判定域。filtered は「IQR 域内の raw 値すべて」なので、
    // [min, max] 域内判定は IQR 域内判定と同値になる。
    let tag_salary_bounds: Option<(i64, i64)> = match (
        salary_values.iter().min().copied(),
        salary_values.iter().max().copied(),
    ) {
        (Some(lo), Some(hi)) => Some((lo, hi)),
        _ => None,
    };

    // タグ別給与集計（サニタイズ済みタグで集計）
    //
    // 2026-07-21 数値監査対応: タグ別平均は外れ値未除去、比較対象の市場平均は
    // IQR 除去後という母集団の不一致があり、高額外れ値がタグ平均だけを持ち上げて
    // 「市場平均比 +28%」等の方向誤りを生んでいた (実データで有資格者歓迎タグが
    // +28% と表示されたが、同一母集団で再計算すると -6%)。市場平均と同じ
    // IQR 域内の値のみをタグ集計に使い、比較の母集団を揃える。
    let mut tag_salary_map: HashMap<String, Vec<i64>> = HashMap::new();
    for r in records {
        if let Some(sal) = r.salary_parsed.unified_monthly {
            let in_bounds = tag_salary_bounds.map_or(false, |(lo, hi)| sal >= lo && sal <= hi);
            if sal > 0 && in_bounds && !r.tags_raw.is_empty() {
                for tag in r.tags_raw.split([',', '、', '/', '\t']) {
                    let sanitized = sanitize_tag_text(tag);
                    if !sanitized.is_empty() && sanitized.chars().count() <= 20 {
                        tag_salary_map.entry(sanitized).or_default().push(sal);
                    }
                }
            }
        }
    }

    // タグ別給与差分の計算
    let overall_mean = enhanced_stats.as_ref().map(|s| s.mean).unwrap_or(0);
    let mut by_tag_salary: Vec<TagSalaryAgg> = tag_salary_map
        .into_iter()
        .filter(|(_, salaries)| salaries.len() >= 3) // 3件以上のタグのみ
        .map(|(tag, salaries)| {
            let count = salaries.len();
            let avg_salary = salaries.iter().sum::<i64>() / count as i64;
            let diff_from_avg = avg_salary - overall_mean;
            let diff_percent = if overall_mean > 0 {
                diff_from_avg as f64 / overall_mean as f64 * 100.0
            } else {
                0.0
            };
            TagSalaryAgg {
                tag,
                count,
                avg_salary,
                diff_from_avg,
                diff_percent,
            }
        })
        .collect();
    by_tag_salary.sort_by(|a, b| b.diff_from_avg.cmp(&a.diff_from_avg));
    by_tag_salary.truncate(20);

    // 時給モード判定
    // 2026-09-29: ネイティブ単位配列と都道府県別集計で使うため、給与配列の計算前に移動。
    // 時給レコードが過半数（半数超）の場合 true。
    // 境界値（同数、例: 5-5）は整数割り算のため strict 比較で false となり、
    // Monthly として扱う（より保守的な挙動）。
    let hourly_count = records
        .iter()
        .filter(|r| r.salary_parsed.salary_type == super::salary_parser::SalaryType::Hourly)
        .count();
    let total_with_salary = records
        .iter()
        .filter(|r| r.salary_parsed.min_value.is_some())
        .count();
    use super::upload::WageMode;
    let is_hourly = match wage_mode {
        WageMode::Hourly => true,
        WageMode::Monthly => false,
        WageMode::Auto => total_with_salary > 0 && hourly_count > total_with_salary / 2,
    };

    // 下限/上限給与（レポート用、月給換算）
    // Round 22 (2026-05-13): 設計メモ §5「salary_type はユーザー側で事前指定する前提」準拠。
    // Annual (年俸) は salary_type が大きく異なるため除外し、Monthly / Hourly / Daily のみ採用。
    // Hourly は 160h、Daily は 20 日で月給相当に換算。
    // Annual 求人は通常 700-1500万円 など極端に大きいため、月換算で混入するとクラスタ Y 軸が歪む。
    use super::salary_parser::SalaryType;
    let salary_min_values: Vec<i64> = records
        .iter()
        .filter_map(|r| {
            monthly_equiv_in_range(r.salary_parsed.min_value?, &r.salary_parsed.salary_type)
        })
        .collect();
    let salary_max_values: Vec<i64> = records
        .iter()
        .filter_map(|r| {
            monthly_equiv_in_range(r.salary_parsed.max_value?, &r.salary_parsed.salary_type)
        })
        .collect();

    // Phase 2-A (2026-05-29): ネイティブ単位 (時給=円/時、月給=円/月) の下限/上限給与
    //
    // 換算しない方針:
    //   - Hourly レコードは円/時のまま (例: 1200円/時)
    //   - Monthly レコードは円/月のまま (例: 250_000円/月)
    //   - Daily / Weekly / Annual / Unknown は Phase 2-A スコープ外 (skip)
    //
    // フィルタ閾値:
    //   - Hourly: 100 円/時 以上 (深夜の最低賃金 800円台より低い値は誤抽出疑い)
    //   - Monthly: MIN_MONTHLY_SALARY 円/月 以上 (既存 salary_min_values と同じ)
    //
    // is_hourly モード時の散布図軸範囲 (800-2500 円/時) との整合性を確保するため、
    // Hourly 値は filter 後にそのまま push (×167 換算しない)。
    //
    // 2026-09-29: 単位混在の修正。以前は Hourly (円/時) と Monthly (円/月) を同じ配列に
    // 入れていたため、時給モードの平均・分位点に月給値がそのまま混ざっていた
    // (時給 1,100/1,200/1,300 + 月給 20万/30万 で平均 100,720「円/時」)。
    // 配列はモードの主単位だけを持つ: 時給モード = Hourly のみ、月給モード = Monthly のみ。
    let salary_min_values_native: Vec<i64> = records
        .iter()
        .filter_map(|r| {
            native_value_for_mode(
                r.salary_parsed.min_value?,
                &r.salary_parsed.salary_type,
                is_hourly,
            )
        })
        .collect();
    let salary_max_values_native: Vec<i64> = records
        .iter()
        .filter_map(|r| {
            native_value_for_mode(
                r.salary_parsed.max_value?,
                &r.salary_parsed.salary_type,
                is_hourly,
            )
        })
        .collect();

    // 企業別集計
    // count/avg/median の意味論一致のため、給与情報（unified_monthly > 0）があるレコードのみ集計。
    // これにより count == 集計対象件数 となり、avg/median の計算母集団と一致する。
    // 表示上は「給与情報のある求人数」として扱う。
    // Finding #17 (2026-06-30): 平均 2 件/企業 を想定し records/2 を初期容量に
    let mut company_map: HashMap<String, Vec<i64>> = HashMap::with_capacity(records.len() / 2 + 1);
    for r in records {
        if !r.company_name.is_empty() {
            if let Some(sal) = r.salary_parsed.unified_monthly {
                if sal > 0 {
                    company_map
                        .entry(r.company_name.clone())
                        .or_default()
                        .push(sal);
                }
            }
        }
    }
    let mut by_company: Vec<CompanyAgg> = company_map
        .into_iter()
        .map(|(name, salaries)| {
            let count = salaries.len();
            let avg_salary = if salaries.is_empty() {
                0
            } else {
                salaries.iter().sum::<i64>() / count as i64
            };
            let median_salary = median_of(&salaries);
            CompanyAgg {
                name,
                count,
                avg_salary,
                median_salary,
            }
        })
        .collect();
    by_company.sort_by(|a, b| b.count.cmp(&a.count));

    // 雇用形態別給与
    // 2026-09-29: 時給モードの表示用に時給求人だけの代表時給 (円/時) も別系列で持つ。
    //   月給換算値 (unified_monthly) と同じ配列には入れない。
    let mut emp_salary_map: HashMap<String, (Vec<i64>, Vec<i64>)> = HashMap::new();
    for r in records {
        let emp = if r.employment_type.is_empty() {
            "不明".to_string()
        } else {
            r.employment_type.clone()
        };
        if let Some(sal) = r.salary_parsed.unified_monthly {
            let entry = emp_salary_map.entry(emp).or_default();
            entry.0.push(sal);
            if let Some(h) = hourly_native_mid(r) {
                entry.1.push(h);
            }
        }
    }
    let mut by_emp_type_salary: Vec<EmpTypeSalary> = emp_salary_map
        .into_iter()
        .map(|(emp_type, (salaries, hourly_values))| {
            let count = salaries.len();
            let avg_salary = if salaries.is_empty() {
                0
            } else {
                salaries.iter().sum::<i64>() / count as i64
            };
            let median_salary = median_of(&salaries);
            let hourly_count = hourly_values.len();
            let avg_hourly_salary = if hourly_count == 0 {
                0
            } else {
                hourly_values.iter().sum::<i64>() / hourly_count as i64
            };
            EmpTypeSalary {
                emp_type,
                count,
                avg_salary,
                median_salary,
                hourly_count,
                avg_hourly_salary,
                median_hourly_salary: median_of(&hourly_values),
            }
        })
        .collect();
    by_emp_type_salary.sort_by(|a, b| b.avg_salary.cmp(&a.avg_salary));

    // 都道府県別給与集計（最低賃金比較用）
    // 2026-09-29: min_values は月給換算 (salary_min_values と同じ規則: 時給×167 / 日給×21 /
    //   年俸・週給・不明は除外 / 5万〜200万円の範囲外は除外)。以前は円/時・円/月・円/年を
    //   単位のまま平均していた。hourly_values は時給求人の代表時給 (円/時、時給モード表示用)。
    #[derive(Default)]
    struct PrefAcc {
        unified: Vec<i64>,
        min_values: Vec<i64>,
        hourly_values: Vec<i64>,
    }
    let mut pref_salary_map: HashMap<String, PrefAcc> = HashMap::new();
    for r in records {
        if let Some(pref) = &r.location_parsed.prefecture {
            let entry = pref_salary_map.entry(pref.clone()).or_default();
            if let Some(sal) = r.salary_parsed.unified_monthly {
                if sal > 0 {
                    entry.unified.push(sal);
                }
            }
            if let Some(min_sal) = r.salary_parsed.min_value {
                if let Some(m) = monthly_equiv_in_range(min_sal, &r.salary_parsed.salary_type) {
                    entry.min_values.push(m);
                }
            }
            if let Some(h) = hourly_native_mid(r) {
                entry.hourly_values.push(h);
            }
        }
    }
    let mut by_prefecture_salary: Vec<PrefectureSalaryAgg> = pref_salary_map
        .into_iter()
        .map(|(name, acc)| {
            let PrefAcc {
                unified: salaries,
                min_values: min_salaries,
                hourly_values,
            } = acc;
            let count = salaries.len();
            let avg_salary = if salaries.is_empty() {
                0
            } else {
                salaries.iter().sum::<i64>() / count as i64
            };
            let avg_min_salary = if min_salaries.is_empty() {
                0
            } else {
                min_salaries.iter().sum::<i64>() / min_salaries.len() as i64
            };
            let hourly_count = hourly_values.len();
            let avg_hourly_salary = if hourly_count == 0 {
                0
            } else {
                hourly_values.iter().sum::<i64>() / hourly_count as i64
            };
            PrefectureSalaryAgg {
                name,
                count,
                avg_salary,
                avg_min_salary,
                hourly_count,
                avg_hourly_salary,
            }
        })
        .collect();
    by_prefecture_salary.sort_by(|a, b| b.count.cmp(&a.count));

    // 散布図データ（下限 vs 上限）
    // Round 22: クラスタ分析と整合させるため Annual / Unknown を除外し、Monthly/Hourly/Daily のみ採用。
    // Hourly は月給換算、Daily も月給換算、Monthly はそのまま。
    let scatter_min_max: Vec<ScatterPoint> = records
        .iter()
        .filter_map(|r| {
            let raw_min = r.salary_parsed.min_value?;
            let raw_max = r.salary_parsed.max_value?;
            let (min, max) = match r.salary_parsed.salary_type {
                SalaryType::Hourly => (
                    raw_min * HOURLY_TO_MONTHLY_HOURS,
                    raw_max * HOURLY_TO_MONTHLY_HOURS,
                ),
                SalaryType::Daily => (
                    raw_min * DAILY_TO_MONTHLY_DAYS,
                    raw_max * DAILY_TO_MONTHLY_DAYS,
                ),
                SalaryType::Monthly => (raw_min, raw_max),
                _ => return None,
            };
            if min > 0 && max > 0 && max >= min {
                Some(ScatterPoint { x: min, y: max })
            } else {
                None
            }
        })
        .collect();
    let regression_min_max = linear_regression_points(&scatter_min_max);

    // Phase 2-A (2026-05-29): ネイティブ単位 散布図ペア (時給=円/時、月給=円/月)
    //
    // 既存 scatter_min_max が月給換算済 (Hourly→ ×167) なのに対し、
    // 本フィールドは換算しない値を保持。is_hourly モード時の図 3-6 散布図で
    // 800-2500 円/時 の軸範囲に直接対応する。
    let scatter_min_max_native: Vec<(i64, i64)> = records
        .iter()
        .filter_map(|r| {
            let raw_min = r.salary_parsed.min_value?;
            let raw_max = r.salary_parsed.max_value?;
            // 2026-09-29: ネイティブ配列と同じくモードの主単位のみ (単位混在を防ぐ)
            native_value_for_mode(raw_min, &r.salary_parsed.salary_type, is_hourly)?;
            let (min, max) = (raw_min, raw_max);
            if min > 0 && max > 0 && max >= min {
                Some((min, max))
            } else {
                None
            }
        })
        .collect();

    // 市区町村在否集合 (2026-07-28)
    // by_municipality_salary は下段で上位15件に truncate されるため、0件ゲート判定
    // (handlers.rs::evaluate_selection_gate) には使えない。CSV に1件以上出現する
    // 全 (都道府県, 市区町村) を給与の有無に関わらず location_parsed から収集する。
    // 誤ブロック防止のため、給与未パースの市区町村も在否ありとして残す。
    let mut municipality_presence: std::collections::HashSet<(String, String)> =
        std::collections::HashSet::new();
    for r in records {
        if let (Some(pref), Some(muni)) = (
            &r.location_parsed.prefecture,
            &r.location_parsed.municipality,
        ) {
            if !pref.is_empty() && !muni.is_empty() {
                municipality_presence.insert((pref.clone(), muni.clone()));
            }
        }
    }

    // 市区町村別給与集計
    let mut muni_salary_map: HashMap<(String, String), Vec<i64>> = HashMap::new();
    for r in records {
        if let (Some(pref), Some(muni)) = (
            &r.location_parsed.prefecture,
            &r.location_parsed.municipality,
        ) {
            if let Some(sal) = r.salary_parsed.unified_monthly {
                if sal > 0 {
                    muni_salary_map
                        .entry((pref.clone(), muni.clone()))
                        .or_default()
                        .push(sal);
                }
            }
        }
    }
    let mut by_municipality_salary: Vec<MunicipalitySalaryAgg> = muni_salary_map
        .into_iter()
        .map(|((pref, name), salaries)| {
            let count = salaries.len();
            let avg_salary = salaries.iter().sum::<i64>() / count as i64;
            let median_salary = median_of(&salaries);
            MunicipalitySalaryAgg {
                name,
                prefecture: pref,
                count,
                avg_salary,
                median_salary,
            }
        })
        .collect();
    // 2026-08-03: 同数タイの並びを名前で決定的にする。件数のみのソートは HashMap の
    // 反復順の影響を受け、15位近辺に同数タイがあると上位15件の顔ぶれが実行ごとに
    // 変わっていた (実測: indeed-2026-04-13.csv で15位=2件に11自治体が同数タイ)。
    by_municipality_salary.sort_by(|a, b| b.count.cmp(&a.count).then(a.name.cmp(&b.name)));
    by_municipality_salary.truncate(15);

    // ============================================================
    // 2026-06-24 年間休日 / 求人ボックス個別求人 集計 (Section 07.5)
    // GAS Aggregator.js:createAnnualHolidaysAggregation 移植 + V2 拡張
    // ============================================================
    use super::salary_parser::SalaryType as ST;
    use super::upload::{annual_holidays_category, CsvSource, ANNUAL_HOLIDAYS_CATEGORIES};

    let annual_holidays_values: Vec<i64> =
        records.iter().filter_map(|r| r.annual_holidays).collect();

    let annual_holidays_category_distribution: Vec<(String, usize)> = {
        let mut counts: std::collections::HashMap<&'static str, usize> =
            ANNUAL_HOLIDAYS_CATEGORIES.iter().map(|&c| (c, 0)).collect();
        for &v in &annual_holidays_values {
            *counts.entry(annual_holidays_category(v)).or_insert(0) += 1;
        }
        ANNUAL_HOLIDAYS_CATEGORIES
            .iter()
            .map(|&cat| (cat.to_string(), counts.get(cat).copied().unwrap_or(0)))
            .collect()
    };

    // Finding #15 (2026-06-30): scatter / scatter_emp / Pearson の 4 pass を 1 pass に統合。
    //   1. records を 1 回だけ走査し (x, y, emp_label) を salary_vs_holidays_scatter_emp に集約
    //   2. salary_vs_holidays_scatter は emp 版から (x,y) 抜き出しで派生 (アロケート 1 回)
    //   3. Pearson と回帰はアキュムレータ (sum_x/sum_y/sum_xx/sum_yy/sum_xy) で計算し、
    //      中間 Vec<f64>×2 を廃止
    //
    // 雇用形態 3 値分類は `classify_employment_for_scatter` に一元化 (狭→広判定)。
    // section_07_5_jobbox_detail.rs の `render_emp_badge` は表示用 5+ 種なので別関数のまま。
    let salary_vs_holidays_scatter_emp: Vec<(i64, i64, String)> = records
        .iter()
        .filter_map(|r| {
            let holidays = r.annual_holidays?;
            let min_val = r.salary_parsed.min_value?;
            let monthly = match r.salary_parsed.salary_type {
                ST::Monthly => min_val,
                ST::Annual => min_val / 12,
                _ => return None,
            };
            let emp_label = classify_employment_for_scatter(&r.employment_type).to_string();
            Some((monthly, holidays, emp_label))
        })
        .collect();
    let salary_vs_holidays_scatter: Vec<ScatterPoint> = salary_vs_holidays_scatter_emp
        .iter()
        .map(|(x, y, _)| ScatterPoint { x: *x, y: *y })
        .collect();

    // 2026-06-25 重複排除: company + title + location + 年間休日 を空白・句読点除去して正規化
    //   実 CSV (求人ボックス) で同一求人が複数ページ収集されて重複していたため。
    fn normalize_for_dedup(s: &str) -> String {
        s.chars()
            .filter(|c| c.is_alphanumeric())
            .flat_map(|c| c.to_lowercase())
            .collect()
    }
    let mut seen_keys: std::collections::HashSet<String> = std::collections::HashSet::new();
    let mut jobbox_records: Vec<JobBoxRecord> = records
        .iter()
        // 2026-07-01 IndeedSp も対象に追加 (§07.5-4 具体例 / §07.5-5 セグメント別 給与統計 に反映)。
        //   Indeed (PC) は description が短くて年間休日抽出できないので対象外のまま。
        //   求人ボックス + IndeedSp は description に「年間休日◯◯日」の記載が期待できる。
        .filter(|r| matches!(r.source, CsvSource::JobBox | CsvSource::IndeedSp))
        .filter_map(|r| {
            let holidays = r.annual_holidays?;
            // 2026-06-25 個別求人「具体例」テーブルには企業名ありのレコードのみ採用。
            if r.company_name.trim().is_empty() {
                return None;
            }
            // 2026-06-26 表示対象を「月給制 + 給与記載あり」のみに限定
            //   理由: 年俸を月給換算 (÷12) すると大企業の数値が他の月給と並んで違和感、
            //         mini bar スケールも歪む。給与未記載 (両 None) も表に意味がない。
            //   集計 (annual_holidays_values / category_distribution / scatter) は影響なし。
            if !matches!(r.salary_parsed.salary_type, ST::Monthly) {
                return None;
            }
            if r.salary_parsed.min_value.is_none() && r.salary_parsed.max_value.is_none() {
                return None;
            }
            // 重複排除キー: 正規化 (company + title + location) + 年間休日 + 給与文字列 + 雇用形態
            // 2026-06-30 V2 dedup ルール準拠 (CLAUDE.md): 同一施設の経験別/雇用形態別求人は別レコード。
            //   salary_raw を含めることで「月給25-30万 vs 月給30-40万」のような経験別求人を別レコードとして残す。
            //   employment_type を含めることで正社員/パートを別レコードとして残す。
            let dedup_key = format!(
                "{}|{}|{}|{}|{}|{}",
                normalize_for_dedup(&r.company_name),
                normalize_for_dedup(&r.job_title),
                normalize_for_dedup(&r.location_raw),
                holidays,
                normalize_for_dedup(&r.salary_raw),
                normalize_for_dedup(&r.employment_type),
            );
            if !seen_keys.insert(dedup_key) {
                return None;
            }
            Some(JobBoxRecord {
                company_name: r.company_name.clone(),
                job_title: r.job_title.clone(),
                location: r.location_raw.clone(),
                employment_type: r.employment_type.clone(),
                annual_holidays: holidays,
                salary_min: r.salary_parsed.min_value,
                salary_max: r.salary_parsed.max_value,
            })
        })
        .collect();
    jobbox_records.sort_by(|a, b| {
        b.annual_holidays
            .cmp(&a.annual_holidays)
            .then_with(|| a.company_name.cmp(&b.company_name))
    });

    // ============================================================
    // 2026-07-01 年間休日カテゴリ別 給与統計 (Section 07.5-2)
    // 求人ボックス Monthly のみを対象に、年間休日カテゴリごとに
    // 給与下限・上限の平均/中央値/最頻値を計算する。
    // 集計対象は jobbox_records と同じ dedup 済みレコード。
    // ============================================================
    let salary_stats_by_holiday_category: Vec<(String, SalaryStats)> = {
        let mut buckets: std::collections::HashMap<&'static str, Vec<(Option<i64>, Option<i64>)>> =
            ANNUAL_HOLIDAYS_CATEGORIES
                .iter()
                .map(|&c| (c, Vec::new()))
                .collect();
        for r in &jobbox_records {
            let cat = annual_holidays_category(r.annual_holidays);
            buckets
                .entry(cat)
                .or_default()
                .push((r.salary_min, r.salary_max));
        }
        ANNUAL_HOLIDAYS_CATEGORIES
            .iter()
            .map(|&cat| {
                let pairs = buckets.remove(cat).unwrap_or_default();
                (cat.to_string(), compute_salary_stats(&pairs))
            })
            .collect()
    };

    // ============================================================
    // 2026-06-26 Section 07.5 UI/UX 改善 用追加集計
    // ============================================================
    let n_ah = annual_holidays_values.len();
    let (holiday_pct_ge_120, holiday_pct_ge_125, holiday_stddev, holiday_q3) = if n_ah > 0 {
        let mut sorted = annual_holidays_values.clone();
        sorted.sort_unstable();
        let count_120 = sorted.iter().filter(|&&v| v >= 120).count() as f64;
        let count_125 = sorted.iter().filter(|&&v| v >= 125).count() as f64;
        let pct_120 = count_120 / n_ah as f64;
        let pct_125 = count_125 / n_ah as f64;
        let mean = sorted.iter().sum::<i64>() as f64 / n_ah as f64;
        let variance = sorted
            .iter()
            .map(|&v| (v as f64 - mean).powi(2))
            .sum::<f64>()
            / n_ah as f64;
        let stddev = variance.sqrt();
        // 2026-06-30 Q3 算出を nearest-rank / R-7 hybrid に変更。
        //   n >= 20: R-7 (線形補間) で安定推定
        //   n < 20 : nearest-rank (小サンプルでは補間の意味が薄い)
        let q3 = if n_ah >= 20 {
            percentile_r7(&sorted, 0.75)
        } else {
            let q3_idx = (n_ah * 3 / 4).min(n_ah - 1);
            sorted[q3_idx]
        };
        (pct_120, pct_125, stddev, q3)
    } else {
        (0.0, 0.0, 0.0, 0)
    };

    // 給与×年間休日 相関係数 (Pearson r) と線形回帰 (最小二乗法)
    // Finding #15 (2026-06-30): アキュムレータベースで 1 pass 計算。中間 Vec<f64>×2 廃止。
    //   数学的恒等式:
    //     var_x = sum (x - mean_x)^2 = sum_xx - n*mean_x^2 = sum_xx - sum_x * mean_x
    //     cov   = sum (x - mean_x)(y - mean_y) = sum_xy - sum_x * mean_y
    //   旧 powi(2) 集計と数値的に同等 (誤差は f64 丸めの範囲)。
    let (salary_holidays_correlation, salary_holidays_regression) = {
        let mut n: f64 = 0.0;
        let (mut sx, mut sy, mut sxx, mut syy, mut sxy) = (0.0_f64, 0.0, 0.0, 0.0, 0.0);
        for (x, y, _) in &salary_vs_holidays_scatter_emp {
            let xf = *x as f64;
            let yf = *y as f64;
            n += 1.0;
            sx += xf;
            sy += yf;
            sxx += xf * xf;
            syy += yf * yf;
            sxy += xf * yf;
        }
        if n >= 3.0 {
            let mean_x = sx / n;
            let mean_y = sy / n;
            let var_x = sxx - sx * mean_x;
            let var_y = syy - sy * mean_y;
            let cov = sxy - sx * mean_y;
            let denom = (var_x * var_y).sqrt();
            let r = if denom > 0.0 { Some(cov / denom) } else { None };
            let reg = if var_x > 0.0 {
                let slope = cov / var_x;
                let intercept = mean_y - slope * mean_x;
                Some((slope, intercept))
            } else {
                None
            };
            (r, reg)
        } else {
            (None, None)
        }
    };

    // ============================================================
    // 2026-06-30 Section 07.6 集計: Indeed (SP) 「人気」「超人気」タグ
    // 2026-07-01 Finding #1-#3 修正: 集計母集団を IndeedSp 限定に。
    //   - 全 source 走査だと Indeed (PC)/求人ボックスの「人気の検索ワード」等の
    //     セル値に "人気" 部分文字列が含まれ誤発火していた。
    //   - 部分文字列マッチ (contains) → split(',') + 厳密一致に変更。
    //     "超人気" が "人気" を部分文字列として含むことによる二重カウントも防止。
    //   - popular_ratio の分母も IndeedSp 由来件数のみに限定。全 source 合算では
    //     IndeedSp 件数が機械的に薄まり営業資料として誤誘導 (Simpson's paradox)。
    //   - non_popular 母集団も同じく IndeedSp 限定。Indeed (PC)/求人ボックスは
    //     定義上 100% non_popular のため、混入させると「人気タグなし側」の中央値が
    //     他媒体の特性で大きく歪む。
    // ============================================================
    // 判定順:
    //   1. tags_raw を ',' で split し各トークンを trim
    //   2. "超人気" と厳密一致するトークンがあれば super_popular
    //   3. 上記以外で "人気" と厳密一致するトークンがあれば popular
    //   4. いずれもなければ none
    // popular_salary_median / non_popular_salary_median: Monthly のみ採用
    //   (時給/年俸/日給を混ぜると単位が混在し中央値が無意味になるため)
    let popularity = {
        let mut popular_count = 0usize;
        let mut super_popular_count = 0usize;
        let mut none_count = 0usize;
        let mut indeed_sp_total = 0usize;
        let mut popular_salaries: Vec<i64> = Vec::new();
        let mut non_popular_salaries: Vec<i64> = Vec::new();
        let mut popular_holidays: Vec<i64> = Vec::new();
        let mut non_popular_holidays: Vec<i64> = Vec::new();
        // 2026-07-28: 年間休日 3 区分別 (超人気/人気/タグなし) の値列 (HolidayStats 用)。
        // 既存の 2 区分 (popular_holidays / non_popular_holidays) は互換のため保持し、
        // 3 区分統計はこの 3 本で別途集計する。
        let mut super_popular_holiday_values: Vec<i64> = Vec::new();
        let mut popular_holiday_values: Vec<i64> = Vec::new();
        let mut non_popular_holiday_values: Vec<i64> = Vec::new();
        // 2026-07-01: 下限/上限ペア (SalaryStats 用) — Monthly かつ MIN_MONTHLY_SALARY 以上のみ
        let mut popular_salary_pairs: Vec<(Option<i64>, Option<i64>)> = Vec::new();
        let mut super_popular_salary_pairs: Vec<(Option<i64>, Option<i64>)> = Vec::new();
        let mut non_popular_salary_pairs: Vec<(Option<i64>, Option<i64>)> = Vec::new();

        for r in records {
            // Finding #1/#3: Indeed (SP) 由来レコードのみを集計対象にする。
            if !matches!(r.source, CsvSource::IndeedSp) {
                continue;
            }
            indeed_sp_total += 1;

            // Finding #1: split + 厳密一致 (部分文字列マッチを廃止)。
            let (is_super, is_popular) = popularity_signal(&r.tags_raw);
            let has_popular_signal = is_super || is_popular;
            // 判定順: 超人気 → 人気 (1 record は超人気 or 人気 のいずれか 1 つだけ計上)
            if is_super {
                super_popular_count += 1;
            } else if is_popular {
                popular_count += 1;
            } else {
                none_count += 1;
            }

            // 月給 (Monthly のみ) 中央値の母集団
            if matches!(r.salary_parsed.salary_type, SalaryType::Monthly) {
                if let Some(v) = r.salary_parsed.min_value {
                    if v >= MIN_MONTHLY_SALARY {
                        if has_popular_signal {
                            popular_salaries.push(v);
                        } else {
                            non_popular_salaries.push(v);
                        }
                    }
                }

                // 2026-07-01 下限/上限ペア (SalaryStats 用)
                // 下限が Some のとき MIN_MONTHLY_SALARY 未満なら異常値としてペアごと除外。
                // 下限が None でも上限が Some ならペアを採用 (上限のみ統計に寄与)。
                let min_ok = match r.salary_parsed.min_value {
                    Some(v) => v >= MIN_MONTHLY_SALARY,
                    None => true,
                };
                if min_ok
                    && (r.salary_parsed.min_value.is_some() || r.salary_parsed.max_value.is_some())
                {
                    let pair = (r.salary_parsed.min_value, r.salary_parsed.max_value);
                    if is_super {
                        super_popular_salary_pairs.push(pair);
                    } else if is_popular {
                        popular_salary_pairs.push(pair);
                    } else {
                        non_popular_salary_pairs.push(pair);
                    }
                }
            }

            // 年間休日 中央値の母集団
            if let Some(h) = r.annual_holidays {
                if has_popular_signal {
                    popular_holidays.push(h);
                } else {
                    non_popular_holidays.push(h);
                }
                // 2026-07-28: 3 区分別 (超人気/人気/タグなし)。判定順は給与側と同一
                // (超人気 → 人気 → タグなし)。annual_holidays が None のレコードは
                // ここに到達せず、各区分の分母 (n) から自動的に除外される。
                if is_super {
                    super_popular_holiday_values.push(h);
                } else if is_popular {
                    popular_holiday_values.push(h);
                } else {
                    non_popular_holiday_values.push(h);
                }
            }
        }

        // Finding #2: 分母を IndeedSp 由来件数に限定 (全 source 合算で薄めない)。
        let popular_ratio = if indeed_sp_total > 0 {
            (popular_count + super_popular_count) as f64 / indeed_sp_total as f64
        } else {
            0.0
        };
        // Finding #5 (2026-07-01): サンプル数を記録し、表示側で n < 5 を "— (n不足)" にする。
        let popular_n_salary = popular_salaries.len();
        let non_popular_n_salary = non_popular_salaries.len();
        let popular_n_holidays = popular_holidays.len();
        let non_popular_n_holidays = non_popular_holidays.len();
        let median_opt = |v: &[i64]| -> Option<i64> {
            if v.is_empty() {
                None
            } else {
                Some(median_of(v))
            }
        };

        PopularityAnalysis {
            popular_count,
            super_popular_count,
            none_count,
            popular_ratio,
            indeed_sp_total,
            popular_salary_median: median_opt(&popular_salaries),
            non_popular_salary_median: median_opt(&non_popular_salaries),
            popular_holidays_median: median_opt(&popular_holidays),
            non_popular_holidays_median: median_opt(&non_popular_holidays),
            popular_n_salary,
            non_popular_n_salary,
            popular_n_holidays,
            non_popular_n_holidays,
            // 2026-07-01 給与下限/上限の平均/中央値/最頻値 (Monthly のみ)
            popular_salary_stats: compute_salary_stats(&popular_salary_pairs),
            super_popular_salary_stats: compute_salary_stats(&super_popular_salary_pairs),
            non_popular_salary_stats: compute_salary_stats(&non_popular_salary_pairs),
            // 2026-07-28 年間休日 3 区分別 統計 (件数・中央値・平均・P25/P75)
            super_popular_holiday_stats: compute_holiday_stats(&super_popular_holiday_values),
            popular_holiday_stats: compute_holiday_stats(&popular_holiday_values),
            non_popular_holiday_stats: compute_holiday_stats(&non_popular_holiday_values),
        }
    };

    // Phase 2a (2026-07-20): カード単位の観測。企業名があるレコードのみ保持。
    let card_briefs: Vec<CardBrief> = records
        .iter()
        .filter(|r| !r.company_name.trim().is_empty())
        .map(|r| CardBrief {
            company: r.company_name.clone(),
            title: r.job_title.chars().take(60).collect(),
            is_monthly: r.salary_parsed.salary_type == SalaryType::Monthly,
            salary_min: r.salary_parsed.min_value,
            salary_max: r.salary_parsed.max_value,
            annual_holidays: r.annual_holidays,
            is_new: r.is_new,
            // 説明文 + 訴求スニペット (css-lx9x6g) の両方から抽出。
            // スニペット側に「月収35万円以上も可能」等の到達額訴求が入ることが多い。
            desc_salary_man: extract_desc_salary_man(&format!("{} {}", r.snippet, r.description)),
        })
        .collect();

    SurveyAggregation {
        total_count: total,
        new_count,
        salary_parse_rate: salary_ok as f64 / total as f64,
        location_parse_rate: location_ok as f64 / total as f64,
        dominant_prefecture,
        dominant_municipality,
        by_prefecture,
        by_salary_range,
        by_employment_type,
        by_tags,
        salary_values,
        enhanced_stats,
        by_company,
        by_emp_type_salary,
        salary_min_values,
        salary_max_values,
        by_tag_salary,
        by_municipality_salary,
        scatter_min_max,
        regression_min_max,
        by_prefecture_salary,
        is_hourly,
        by_emp_group_native: aggregate_by_emp_group_native(records),
        outliers_removed_total,
        salary_values_raw_count: salary_values_raw.len(),
        // Phase 2-A (2026-05-29): ネイティブ単位フィールド
        salary_min_values_native,
        salary_max_values_native,
        scatter_min_max_native,
        // 2026-06-24: Section 07.5 用 (年間休日 / 求人ボックス個別求人)
        // 2026-06-30 Finding #12: JobboxAnalysis sub-struct に集約
        jobbox: JobboxAnalysis {
            annual_holidays_values,
            annual_holidays_category_distribution,
            salary_vs_holidays_scatter,
            jobbox_records,
            holiday_pct_ge_120,
            holiday_pct_ge_125,
            holiday_stddev,
            holiday_q3,
            salary_vs_holidays_scatter_emp,
            salary_holidays_correlation,
            salary_holidays_regression,
            // 2026-07-01 年間休日カテゴリ別 給与統計 (Section 07.5-2)
            salary_stats_by_holiday_category,
            // 機能 C (2026-07-07): AI 抽出件数は集計時は 0。handlers.rs が集計後に上書きする。
            ai_extracted_count: 0,
        },
        // 2026-06-30 Section 07.6: Indeed (SP) 人気度シグナル
        popularity,
        // 2026-07-20 Phase 2a: カード単位の観測
        card_briefs,
        // 2026-07-28: 市区町村在否集合 (0件ゲートの truncate 非依存判定用)
        municipality_presence,
        // 2026-09-29: 競合調査章 (タグ全体 / 検索上位 N 件 / 人気比較)
        competitor: compute_competitor(records, is_hourly),
    }
}

/// 雇用形態グループ別にネイティブ単位で集計する
///
/// グループ分類 (2026-04-26 Fix-A: `crate::handlers::emp_classifier::classify` に統一):
/// - **正社員**: 「正社員」「正職員」(「以外」を含まない場合)
///   → 月給ベース (月給/年俸/日給は月給換算)
/// - **パート**: 「パート」「アルバイト」(派遣パート含む)
///   → 時給ベース (月給/日給は時給換算)
/// - **派遣・その他**: 契約社員 / 業務委託 / 派遣 / 正社員以外 等
///   → グループ内の salary_type **多数派 (件数同数なら月給優先)** で動的決定。
///     完全に同件数の場合は salary_type 出現比率の多数派 (Hourly が過半数なら時給) を採用。
pub fn aggregate_by_emp_group_native(records: &[SurveyRecord]) -> Vec<EmpGroupNativeAgg> {
    use super::salary_parser::SalaryType;
    use std::collections::HashMap;

    #[derive(Default)]
    struct Bucket {
        emp_types: HashMap<String, usize>,
        monthly_values: Vec<i64>,
        hourly_values: Vec<i64>,
        // 派遣・その他グループの native_unit 動的決定用: 元レコードの salary_type 出現数
        salary_type_counts: HashMap<&'static str, usize>,
    }

    let mut buckets: HashMap<&'static str, Bucket> = HashMap::new();
    for record in records {
        let emp = &record.employment_type;
        let group = classify_emp_group_label(emp);
        let bucket = buckets.entry(group).or_default();
        *bucket.emp_types.entry(emp.clone()).or_insert(0) += 1;
        if let Some(v) = record.salary_parsed.min_value {
            if v > 0 {
                let stype_key = match record.salary_parsed.salary_type {
                    SalaryType::Hourly => "hourly",
                    SalaryType::Monthly => "monthly",
                    SalaryType::Annual => "monthly",
                    SalaryType::Daily => "monthly",
                    SalaryType::Weekly => "monthly",
                };
                *bucket.salary_type_counts.entry(stype_key).or_insert(0) += 1;
                match record.salary_parsed.salary_type {
                    SalaryType::Hourly => {
                        bucket.hourly_values.push(v);
                        bucket.monthly_values.push(v * HOURLY_TO_MONTHLY_HOURS);
                    }
                    SalaryType::Monthly => {
                        bucket.monthly_values.push(v);
                        bucket.hourly_values.push(v / HOURLY_TO_MONTHLY_HOURS);
                    }
                    SalaryType::Annual => {
                        let monthly = v / 12;
                        bucket.monthly_values.push(monthly);
                        bucket.hourly_values.push(monthly / HOURLY_TO_MONTHLY_HOURS);
                    }
                    SalaryType::Daily => {
                        let monthly = v * DAILY_TO_MONTHLY_DAYS;
                        bucket.monthly_values.push(monthly);
                        bucket.hourly_values.push(v / DAILY_HOURS);
                    }
                    SalaryType::Weekly => {
                        // 週給 → 月給 (×4.33 = 52週/12月) と時給 (/40h/週)
                        let monthly = v * WEEKLY_TO_MONTHLY_NUM / WEEKLY_TO_MONTHLY_DEN;
                        bucket.monthly_values.push(monthly);
                        bucket.hourly_values.push(v / WEEKLY_HOURS);
                    }
                }
            }
        }
    }

    let mut result: Vec<EmpGroupNativeAgg> = Vec::new();
    for (group_label, bucket) in buckets {
        // native_unit: グループに応じて自動選択
        // 2026-04-26 Fix-A: 「派遣・その他」の判定を実 salary_type 出現数ベースに修正。
        // 旧実装では monthly_values と hourly_values が常に同件数 (Hourly レコードでも両方 push)
        // だったため `>` 比較が常に false → 常に「月給」選択という silent bug があった。
        let native_unit = match group_label {
            "正社員" => "月給",
            "パート" => "時給",
            _ => {
                // 派遣・その他: 元レコードの salary_type で「時給」が過半数なら時給、
                //               同数 (タイ) は月給優先 (保守的)。
                let h = *bucket.salary_type_counts.get("hourly").unwrap_or(&0);
                let m = *bucket.salary_type_counts.get("monthly").unwrap_or(&0);
                if h > m {
                    "時給"
                } else {
                    "月給"
                }
            }
        };
        let raw_values = if native_unit == "時給" {
            bucket.hourly_values.clone()
        } else {
            bucket.monthly_values.clone()
        };
        if raw_values.is_empty() {
            continue;
        }
        // 2026-04-24: グループ内で IQR 外れ値除外
        let raw_count = raw_values.len();
        let (values, outliers_removed) = super::statistics::filter_outliers_iqr(&raw_values, 1.5);
        if values.is_empty() {
            continue;
        }
        let count = values.len();
        let mean = (values.iter().sum::<i64>() as f64 / count as f64) as i64;
        let min = *values.iter().min().unwrap_or(&0);
        let max = *values.iter().max().unwrap_or(&0);
        let median = median_of(&values);
        // 雇用形態内訳は降順で最大 5 件まで
        let mut emp_list: Vec<(String, usize)> = bucket.emp_types.into_iter().collect();
        emp_list.sort_by(|a, b| b.1.cmp(&a.1));
        let included_emp_types: Vec<String> = emp_list
            .into_iter()
            .take(5)
            .map(|(s, n)| format!("{} ({}件)", s, n))
            .collect();
        result.push(EmpGroupNativeAgg {
            group_label: group_label.to_string(),
            native_unit: native_unit.to_string(),
            count,
            included_emp_types,
            median,
            mean,
            min,
            max,
            values,
            outliers_removed,
            raw_count,
        });
    }
    // 件数降順
    result.sort_by(|a, b| b.count.cmp(&a.count));
    result
}

/// 雇用形態文字列からグループラベルを判定
///
/// 2026-04-26 Fix-A: `crate::handlers::emp_classifier::classify` (EmpGroup) を
/// 唯一の真実源として委譲する。旧実装では「契約」「業務委託」を **正社員** に分類
/// していたが、これは経済的本質 (有期/報酬形態) と整合しない誤分類だった。
/// 修正により契約社員・業務委託は「派遣・その他」グループへ。
///
/// 戻り値ラベル:
/// - `"正社員"` (EmpGroup::Regular)
/// - `"パート"` (EmpGroup::PartTime)
/// - `"派遣・その他"` (EmpGroup::Other) — 表示は感性的に「派遣・その他」とする
fn classify_emp_group_label(emp: &str) -> &'static str {
    use crate::handlers::emp_classifier::{classify, EmpGroup};
    match classify(emp) {
        EmpGroup::Regular => "正社員",
        EmpGroup::PartTime => "パート",
        EmpGroup::Other => "派遣・その他",
    }
}

/// 線形回帰（最小二乗法）
fn linear_regression_points(points: &[ScatterPoint]) -> Option<RegressionResult> {
    let n = points.len();
    if n < 3 {
        return None;
    }
    let n_f = n as f64;
    let sum_x: f64 = points.iter().map(|p| p.x as f64).sum();
    let sum_y: f64 = points.iter().map(|p| p.y as f64).sum();
    let sum_xy: f64 = points.iter().map(|p| p.x as f64 * p.y as f64).sum();
    let sum_x2: f64 = points.iter().map(|p| (p.x as f64).powi(2)).sum();

    let denom = n_f * sum_x2 - sum_x.powi(2);
    if denom.abs() < 1e-10 {
        return None;
    }

    let slope = (n_f * sum_xy - sum_x * sum_y) / denom;
    let intercept = (sum_y - slope * sum_x) / n_f;

    // R²計算
    let mean_y = sum_y / n_f;
    let ss_tot: f64 = points.iter().map(|p| (p.y as f64 - mean_y).powi(2)).sum();
    let ss_res: f64 = points
        .iter()
        .map(|p| {
            let pred = slope * p.x as f64 + intercept;
            (p.y as f64 - pred).powi(2)
        })
        .sum();
    // ss_tot=0（全yが同値、ゼロ分散）の場合、統計的には R² は定義されない。
    // 本実装では 0.0 を返す（ゼロ分散データは「相関なし」として扱う保守的挙動）。
    let r_squared = if ss_tot > 0.0 {
        1.0 - ss_res / ss_tot
    } else {
        0.0
    };

    Some(RegressionResult {
        slope,
        intercept,
        r_squared,
    })
}

#[cfg(test)]
mod desc_salary_tests {
    use super::extract_desc_salary_man;

    #[test]
    fn extracts_monthly_salary_mentions_from_description() {
        // 実データ形式 (カンメタ訴求文): 「それでも月収35万円以上も可能」
        assert_eq!(
            extract_desc_salary_man("✅年間休日125日以上・それでも月収35万円以上も可能"),
            Some(35)
        );
        assert_eq!(extract_desc_salary_man("月給25万円〜"), Some(25));
        // 複数記載は最大値
        assert_eq!(
            extract_desc_salary_man("月収28万、繁忙期は月収40万も"),
            Some(40)
        );
        // 対象外: キーなし / 1桁 (手当等との混同防止) / 数字なし
        assert_eq!(extract_desc_salary_man("賞与3万円支給"), None);
        assert_eq!(extract_desc_salary_man("月収8万円のパート"), None);
        assert_eq!(extract_desc_salary_man("特になし"), None);
    }
}

/// 実データ由来の給与原文 fixture から SurveyRecord を組み立てるテスト専用ヘルパー
/// (2026-09-29 時給/月給混在不具合の逆証明用)。
/// fixture は給与欄の原文のみ (1 行 1 求人)。所在地は全件 `location` を使う。
#[cfg(test)]
pub(crate) mod salary_fixture {
    use super::super::location_parser::parse_location;
    use super::super::salary_parser::{parse_salary, SalaryType};
    use super::super::upload::{CsvSource, SurveyRecord};

    /// f1: 時給 751 件 + 月給 1 件 (Indeed 実データの給与欄)
    pub(crate) const F1_HOURLY_MOSTLY: &str =
        include_str!("../../../tests/fixtures/survey_salary/f1_hourly_mostly.txt");
    /// f2: 月給 567 件 + 年俸/年収 24 件 (Indeed 実データの給与欄)
    pub(crate) const F2_MONTHLY_WITH_ANNUAL: &str =
        include_str!("../../../tests/fixtures/survey_salary/f2_monthly_with_annual.txt");

    pub(crate) fn records(salary_lines: &str, location: &str) -> Vec<SurveyRecord> {
        salary_lines
            .lines()
            .filter(|l| !l.trim().is_empty())
            .enumerate()
            .map(|(i, sal)| SurveyRecord {
                row_index: i,
                source: CsvSource::IndeedSp,
                job_title: "t".to_string(),
                company_name: format!("C{}", i),
                location_raw: location.to_string(),
                salary_raw: sal.to_string(),
                employment_type: String::new(),
                employment_type_inferred: false,
                tags_raw: String::new(),
                url: None,
                is_new: false,
                description: String::new(),
                snippet: String::new(),
                salary_parsed: parse_salary(sal, SalaryType::Monthly),
                location_parsed: parse_location(location, None),
                annual_holidays: None,
                ai_monthly_holidays_min: None,
                ai_monthly_holidays_max: None,
                ai_bonus: None,
                ai_bonus_times_per_year: None,
                ai_paid_leave_rate: None,
                ai_weekly_holiday_type: None,
                ai_overtime_hours_monthly: None,
            })
            .collect()
    }
}

#[cfg(test)]
mod tests {
    use super::super::location_parser::ParsedLocation;
    use super::super::salary_parser::{ParsedSalary, SalaryType};
    use super::super::upload::{CsvSource, SurveyRecord};
    use super::*;

    // ======== テストヘルパー ========

    fn empty_salary() -> ParsedSalary {
        ParsedSalary {
            original_text: String::new(),
            salary_type: SalaryType::Monthly,
            min_value: None,
            max_value: None,
            has_range: false,
            unified_monthly: None,
            unified_annual: None,
            range_category: None,
            confidence: 0.0,
            bonus_months: None,
        }
    }

    fn empty_location() -> ParsedLocation {
        ParsedLocation {
            original_text: String::new(),
            prefecture: None,
            municipality: None,
            region_block: None,
            city_type: None,
            confidence: 0.0,
            method: "empty".to_string(),
        }
    }

    /// テスト用SurveyRecord作成ヘルパー
    fn mock_record(
        company: &str,
        pref: Option<&str>,
        muni: Option<&str>,
        salary_monthly: Option<i64>,
        salary_min: Option<i64>,
        salary_max: Option<i64>,
        salary_type: SalaryType,
        emp_type: &str,
        tags: &str,
    ) -> SurveyRecord {
        let mut sal = empty_salary();
        sal.salary_type = salary_type;
        sal.unified_monthly = salary_monthly;
        sal.min_value = salary_min;
        sal.max_value = salary_max;

        let mut loc = empty_location();
        loc.prefecture = pref.map(|s| s.to_string());
        loc.municipality = muni.map(|s| s.to_string());

        SurveyRecord {
            row_index: 0,
            source: CsvSource::Unknown,
            job_title: String::new(),
            company_name: company.to_string(),
            location_raw: String::new(),
            salary_raw: String::new(),
            employment_type: emp_type.to_string(),
            employment_type_inferred: false,
            tags_raw: tags.to_string(),
            url: None,
            is_new: false,
            description: String::new(),
            snippet: String::new(),
            salary_parsed: sal,
            location_parsed: loc,
            annual_holidays: None,
            ai_monthly_holidays_min: None,
            ai_monthly_holidays_max: None,
            ai_bonus: None,
            ai_bonus_times_per_year: None,
            ai_paid_leave_rate: None,
            ai_weekly_holiday_type: None,
            ai_overtime_hours_monthly: None,
        }
    }

    // ======== 2026-07-21 数値監査対応: タグ集計の母集団統一 ========

    #[test]
    fn tag_salary_excludes_outliers_like_market_mean() {
        // 市場: 25万が20件 + 外れ値500万が1件 (IQR除去対象)。
        // タグ「資格」は 25万×3件 + 外れ値500万×1件に付与。
        // 旧実装: タグ平均=(25万×3+500万)/4≈143.7万 vs 市場平均(除去後)=25万 → +475% と誤表示。
        // 新実装: タグ集計も IQR 域内のみ → タグ平均=25万、diff≈0%。
        let mut records: Vec<SurveyRecord> = (0..20)
            .map(|i| {
                let tag = if i < 3 { "資格" } else { "" };
                // IQR>0 になるよう給与に分散を持たせる (全同値だと IQR=0 で除外自体が
                // スキップされる仕様のため、テストの前提が壊れる)
                let sal = 240_000 + (i as i64) * 1_000;
                mock_record(
                    "会社A",
                    Some("大阪府"),
                    Some("富田林市"),
                    Some(sal),
                    Some(sal),
                    None,
                    SalaryType::Monthly,
                    "正社員",
                    tag,
                )
            })
            .collect();
        records.push(mock_record(
            "会社B",
            Some("大阪府"),
            Some("富田林市"),
            Some(5_000_000),
            Some(5_000_000),
            None,
            SalaryType::Monthly,
            "正社員",
            "資格",
        ));
        let agg = aggregate_records(&records);
        let t = agg
            .by_tag_salary
            .iter()
            .find(|t| t.tag == "資格")
            .expect("タグ集計があるはず");
        assert_eq!(t.count, 3, "外れ値レコードはタグ集計から除外される");
        assert!(
            t.diff_percent.abs() < 5.0,
            "市場平均と同水準のはず (旧実装では数百%に化けた): {}%",
            t.diff_percent
        );
    }

    // ======== A. 線形回帰テスト ========

    #[test]
    fn test_linear_regression_known_points() {
        // y = 2x + 1 の5点
        let points = vec![
            ScatterPoint { x: 1, y: 3 },
            ScatterPoint { x: 2, y: 5 },
            ScatterPoint { x: 3, y: 7 },
            ScatterPoint { x: 4, y: 9 },
            ScatterPoint { x: 5, y: 11 },
        ];
        let result = linear_regression_points(&points).expect("5点あるのでSomeを返すはず");
        assert!((result.slope - 2.0).abs() < 0.01, "slope={}", result.slope);
        assert!(
            (result.intercept - 1.0).abs() < 0.01,
            "intercept={}",
            result.intercept
        );
        assert!(
            (result.r_squared - 1.0).abs() < 0.01,
            "r_squared={}",
            result.r_squared
        );
    }

    #[test]
    fn test_linear_regression_n_less_than_3() {
        let points = vec![ScatterPoint { x: 1, y: 2 }, ScatterPoint { x: 2, y: 4 }];
        assert!(
            linear_regression_points(&points).is_none(),
            "n<3ではNoneを返すべき"
        );
    }

    #[test]
    fn test_linear_regression_all_same_x() {
        // 垂直分布: denom = n*sum(x^2) - sum(x)^2 = 0
        let points = vec![
            ScatterPoint { x: 5, y: 10 },
            ScatterPoint { x: 5, y: 20 },
            ScatterPoint { x: 5, y: 30 },
        ];
        assert!(
            linear_regression_points(&points).is_none(),
            "denom≈0ではNoneを返すべき"
        );
    }

    #[test]
    fn test_linear_regression_r_squared_zero_ss_tot() {
        // 水平分布: 全点のyが同じ → ss_tot=0 → r_squared=0.0（現状動作）
        let points = vec![
            ScatterPoint { x: 1, y: 100 },
            ScatterPoint { x: 2, y: 100 },
            ScatterPoint { x: 3, y: 100 },
        ];
        let result = linear_regression_points(&points).expect("xは分散しているのでSome");
        // ss_tot=0（ゼロ分散）の場合、統計的には R² 未定義だが、
        // 本実装では 0.0 を返す仕様（「相関なし」として扱う保守的挙動、ドキュメント化済）。
        assert!(
            result.slope.abs() < 1e-9,
            "slope should be ~0, got {}",
            result.slope
        );
        assert!((result.intercept - 100.0).abs() < 1e-6);
        assert_eq!(result.r_squared, 0.0, "ss_tot=0時はr_squared=0.0を返す仕様");
    }

    #[test]
    fn test_linear_regression_points_struct_sanity() {
        // 大きな値でも正しくf64変換されて処理される
        let points = vec![
            ScatterPoint {
                x: 100_000,
                y: 200_000,
            },
            ScatterPoint {
                x: 150_000,
                y: 250_000,
            },
            ScatterPoint {
                x: 200_000,
                y: 300_000,
            },
            ScatterPoint {
                x: 250_000,
                y: 350_000,
            },
        ];
        let result = linear_regression_points(&points).expect("4点あればSome");
        // y = x + 100_000 → slope=1.0, intercept=100_000
        assert!((result.slope - 1.0).abs() < 0.01);
        assert!((result.intercept - 100_000.0).abs() < 1.0);
        assert!((result.r_squared - 1.0).abs() < 0.01);
    }

    // ======== B. 集計ロジックテスト ========

    #[test]
    fn test_aggregate_by_company_count_vs_valid() {
        // 企業A: 給与あり + 給与なし / 企業B: 給与あり
        let records = vec![
            mock_record(
                "企業A",
                Some("東京都"),
                Some("千代田区"),
                Some(300_000),
                Some(280_000),
                Some(320_000),
                SalaryType::Monthly,
                "正社員",
                "",
            ),
            mock_record(
                "企業A",
                Some("東京都"),
                Some("千代田区"),
                None,
                None,
                None,
                SalaryType::Monthly,
                "正社員",
                "",
            ),
            mock_record(
                "企業B",
                Some("東京都"),
                Some("新宿区"),
                Some(400_000),
                Some(380_000),
                Some(420_000),
                SalaryType::Monthly,
                "正社員",
                "",
            ),
        ];
        let agg = aggregate_records(&records);

        // count/avg/median の意味論を一致させるため、給与情報のあるレコードのみ集計対象。
        // 企業A: unified_monthly=None のレコードはスキップ → salaries=[300_000]
        //   → count=1, avg=300_000, median=300_000
        let a = agg
            .by_company
            .iter()
            .find(|c| c.name == "企業A")
            .expect("企業A");
        assert_eq!(
            a.count, 1,
            "企業Aは給与情報のある1件のみ（Noneレコードは除外）"
        );
        assert_eq!(a.avg_salary, 300_000);
        assert_eq!(a.median_salary, 300_000);

        let b = agg
            .by_company
            .iter()
            .find(|c| c.name == "企業B")
            .expect("企業B");
        assert_eq!(b.count, 1);
        assert_eq!(b.avg_salary, 400_000);
    }

    #[test]
    fn test_aggregate_by_tag_salary_overall_mean_zero() {
        // 全レコードでunified_monthly=None → tag_salary_mapが populate されない
        let records = vec![
            mock_record(
                "X社",
                Some("東京都"),
                Some("千代田区"),
                None,
                None,
                None,
                SalaryType::Monthly,
                "正社員",
                "タグA,タグB",
            ),
            mock_record(
                "Y社",
                Some("東京都"),
                Some("新宿区"),
                None,
                None,
                None,
                SalaryType::Monthly,
                "正社員",
                "タグA,タグB",
            ),
            mock_record(
                "Z社",
                Some("東京都"),
                Some("渋谷区"),
                None,
                None,
                None,
                SalaryType::Monthly,
                "正社員",
                "タグA,タグB",
            ),
        ];
        let agg = aggregate_records(&records);
        // tag_salary は全給与Noneなので空（3件フィルタ以前に populate されない）
        assert!(
            agg.by_tag_salary.is_empty(),
            "全給与None時は by_tag_salary が空であること（巨大正値の diff_from_avg が出ないこと）"
        );
    }

    #[test]
    fn test_aggregate_is_hourly_detection_majority() {
        // 6 Hourly + 4 Monthly = 10件。hourly_count=6 > 10/2=5 → true
        let mut records = Vec::new();
        for _ in 0..6 {
            records.push(mock_record(
                "H",
                Some("東京都"),
                Some("千代田区"),
                Some(200_000),
                Some(1200),
                Some(1500),
                SalaryType::Hourly,
                "パート",
                "",
            ));
        }
        for _ in 0..4 {
            records.push(mock_record(
                "M",
                Some("東京都"),
                Some("千代田区"),
                Some(250_000),
                Some(200_000),
                Some(300_000),
                SalaryType::Monthly,
                "正社員",
                "",
            ));
        }
        let agg = aggregate_records(&records);
        assert!(agg.is_hourly, "時給6 vs 月給4 → is_hourly=true");
    }

    #[test]
    fn test_aggregate_is_hourly_detection_minority() {
        // 3 Hourly + 7 Monthly = 10件。hourly_count=3, 3>5=false
        let mut records = Vec::new();
        for _ in 0..3 {
            records.push(mock_record(
                "H",
                Some("東京都"),
                Some("千代田区"),
                Some(200_000),
                Some(1200),
                Some(1500),
                SalaryType::Hourly,
                "パート",
                "",
            ));
        }
        for _ in 0..7 {
            records.push(mock_record(
                "M",
                Some("東京都"),
                Some("千代田区"),
                Some(250_000),
                Some(200_000),
                Some(300_000),
                SalaryType::Monthly,
                "正社員",
                "",
            ));
        }
        let agg = aggregate_records(&records);
        assert!(!agg.is_hourly, "時給3 vs 月給7 → is_hourly=false");
    }

    #[test]
    fn test_aggregate_is_hourly_detection_boundary() {
        // 5 Hourly + 5 Monthly = 10件。hourly_count=5, 5>10/2=5 は strict比較で false
        let mut records = Vec::new();
        for _ in 0..5 {
            records.push(mock_record(
                "H",
                Some("東京都"),
                Some("千代田区"),
                Some(200_000),
                Some(1200),
                Some(1500),
                SalaryType::Hourly,
                "パート",
                "",
            ));
        }
        for _ in 0..5 {
            records.push(mock_record(
                "M",
                Some("東京都"),
                Some("千代田区"),
                Some(250_000),
                Some(200_000),
                Some(300_000),
                SalaryType::Monthly,
                "正社員",
                "",
            ));
        }
        let agg = aggregate_records(&records);
        assert!(
            !agg.is_hourly,
            "境界（5-5）: hourly_count > total/2 の strict 比較により false。\
             同数時は Monthly として扱う保守的仕様（ドキュメント化済）"
        );
    }

    #[test]
    fn test_aggregate_by_municipality_salary_median_even_count() {
        // 同一市区町村に4件: [100_000, 200_000, 300_000, 400_000]
        // sorted[4/2] = sorted[2] = 300_000 （現状: 偶数件でも上側要素を取る）
        let records = vec![
            mock_record(
                "A",
                Some("東京都"),
                Some("千代田区"),
                Some(100_000),
                Some(100_000),
                Some(100_000),
                SalaryType::Monthly,
                "正社員",
                "",
            ),
            mock_record(
                "B",
                Some("東京都"),
                Some("千代田区"),
                Some(200_000),
                Some(200_000),
                Some(200_000),
                SalaryType::Monthly,
                "正社員",
                "",
            ),
            mock_record(
                "C",
                Some("東京都"),
                Some("千代田区"),
                Some(300_000),
                Some(300_000),
                Some(300_000),
                SalaryType::Monthly,
                "正社員",
                "",
            ),
            mock_record(
                "D",
                Some("東京都"),
                Some("千代田区"),
                Some(400_000),
                Some(400_000),
                Some(400_000),
                SalaryType::Monthly,
                "正社員",
                "",
            ),
        ];
        let agg = aggregate_records(&records);
        let muni = agg
            .by_municipality_salary
            .iter()
            .find(|m| m.name == "千代田区" && m.prefecture == "東京都")
            .expect("千代田区");
        assert_eq!(muni.count, 4);
        assert_eq!(muni.avg_salary, 250_000);
        // 偶数件の中央値は中央2要素の平均: (sorted[1]+sorted[2])/2 = (200_000+300_000)/2 = 250_000
        // enhanced_salary_statistics と一貫した定義。
        assert_eq!(
            muni.median_salary, 250_000,
            "偶数件の中央値は中央2要素の平均"
        );
    }

    #[test]
    fn test_aggregate_by_prefecture_salary() {
        // 東京都: 2件、大阪府: 2件
        let records = vec![
            mock_record(
                "A",
                Some("東京都"),
                Some("千代田区"),
                Some(300_000),
                Some(280_000),
                Some(320_000),
                SalaryType::Monthly,
                "正社員",
                "",
            ),
            mock_record(
                "B",
                Some("東京都"),
                Some("新宿区"),
                Some(400_000),
                Some(380_000),
                Some(420_000),
                SalaryType::Monthly,
                "正社員",
                "",
            ),
            mock_record(
                "C",
                Some("大阪府"),
                Some("大阪市"),
                Some(250_000),
                Some(200_000),
                Some(300_000),
                SalaryType::Monthly,
                "正社員",
                "",
            ),
            mock_record(
                "D",
                Some("大阪府"),
                Some("堺市"),
                Some(270_000),
                Some(240_000),
                Some(300_000),
                SalaryType::Monthly,
                "正社員",
                "",
            ),
        ];
        let agg = aggregate_records(&records);

        let tokyo = agg
            .by_prefecture_salary
            .iter()
            .find(|p| p.name == "東京都")
            .expect("東京都");
        assert_eq!(tokyo.count, 2);
        assert_eq!(tokyo.avg_salary, 350_000); // (300_000+400_000)/2
        assert_eq!(tokyo.avg_min_salary, 330_000); // (280_000+380_000)/2

        let osaka = agg
            .by_prefecture_salary
            .iter()
            .find(|p| p.name == "大阪府")
            .expect("大阪府");
        assert_eq!(osaka.count, 2);
        assert_eq!(osaka.avg_salary, 260_000); // (250_000+270_000)/2
        assert_eq!(osaka.avg_min_salary, 220_000); // (200_000+240_000)/2
    }

    // =========================================================================
    // 2026-04-26 Fix-A 雇用形態分類統一の逆証明テスト
    // 修正前 (旧 classify_emp_group_label): 「契約」「業務委託」を含む文字列も「正社員」
    //   グループに分類していた。
    // 修正後 (crate::handlers::emp_classifier::classify): 契約社員/業務委託 → 「派遣・その他」
    // 影響: 正社員月給バケットに混入していた契約社員/業務委託の固定報酬が分離され、
    //   正社員グループの中央値・平均が経済的本質に整合した値になる。
    // =========================================================================

    fn rec_emp(emp: &str, salary: i64, salary_type: SalaryType) -> SurveyRecord {
        mock_record(
            "TestCo",
            Some("東京都"),
            Some("新宿区"),
            Some(salary),
            Some(salary),
            Some(salary),
            salary_type,
            emp,
            "",
        )
    }

    #[test]
    fn fixa_emp_group_contract_worker_routes_to_other_not_seishain() {
        // 修正前: 契約社員 → 「正社員」グループに混入 (旧 classify_emp_group_label)
        // 修正後: 契約社員 → 「派遣・その他」グループに分離
        let records = vec![
            rec_emp("正社員", 300_000, SalaryType::Monthly),
            rec_emp("契約社員", 250_000, SalaryType::Monthly),
        ];
        let groups = aggregate_by_emp_group_native(&records);
        let seishain = groups.iter().find(|g| g.group_label == "正社員");
        let other = groups.iter().find(|g| g.group_label == "派遣・その他");
        assert!(seishain.is_some(), "正社員グループ存在");
        assert_eq!(
            seishain.unwrap().count,
            1,
            "正社員グループは1件のみ (契約社員は混入しない)"
        );
        assert!(other.is_some(), "派遣・その他グループ存在");
        assert_eq!(other.unwrap().count, 1, "契約社員1件 → 派遣・その他");
        // 旧仕様逆証明: もし旧分類なら 正社員グループ count=2 / 平均 = (300k+250k)/2 = 275k
        // 新分類: 正社員 count=1 / 平均 = 300k
        assert_eq!(
            seishain.unwrap().mean,
            300_000,
            "契約社員除外で正社員平均 = 300k (旧仕様の 275k ではない)"
        );
    }

    #[test]
    fn fixa_emp_group_gyomu_itaku_routes_to_other_not_seishain() {
        // 修正前: 業務委託 → 「正社員」グループ (誤)
        // 修正後: 業務委託 → 「派遣・その他」 (正)
        let records = vec![
            rec_emp("正社員", 300_000, SalaryType::Monthly),
            rec_emp("業務委託", 800_000, SalaryType::Monthly), // 高額な業務委託報酬
        ];
        let groups = aggregate_by_emp_group_native(&records);
        let seishain = groups.iter().find(|g| g.group_label == "正社員").unwrap();
        let other = groups
            .iter()
            .find(|g| g.group_label == "派遣・その他")
            .unwrap();
        // 旧仕様: 正社員グループに業務委託 80万が混入 → 正社員平均 = (300k+800k)/2 = 550k
        // 新仕様: 正社員 = 300k のみ / 業務委託は派遣・その他に
        assert_eq!(seishain.count, 1);
        assert_eq!(seishain.mean, 300_000);
        assert_eq!(other.count, 1);
        assert_eq!(other.mean, 800_000);
    }

    #[test]
    fn fixa_emp_group_seishain_igai_routes_to_other() {
        // 「正社員以外」 → emp_classifier では Other (修正前は contains("正社員") で Regular 誤分類)
        let records = vec![rec_emp("正社員以外", 200_000, SalaryType::Monthly)];
        let groups = aggregate_by_emp_group_native(&records);
        assert!(groups.iter().any(|g| g.group_label == "派遣・その他"));
        assert!(!groups.iter().any(|g| g.group_label == "正社員"));
    }

    #[test]
    fn fixa_native_unit_other_group_majority_hourly_picks_jikyu() {
        // 派遣・その他 グループで時給レコードが過半数 → native_unit = "時給"
        // 修正前: monthly_values と hourly_values が常に同件数 (全レコードで両方 push) で
        //         `>` 比較が false → 常に「月給」
        // 修正後: salary_type_counts (元レコードベース) で動的決定
        let records = vec![
            rec_emp("派遣社員", 1500, SalaryType::Hourly),
            rec_emp("派遣社員", 1600, SalaryType::Hourly),
            rec_emp("派遣社員", 1700, SalaryType::Hourly),
            rec_emp("派遣社員", 250_000, SalaryType::Monthly),
        ];
        let groups = aggregate_by_emp_group_native(&records);
        let other = groups
            .iter()
            .find(|g| g.group_label == "派遣・その他")
            .unwrap();
        assert_eq!(
            other.native_unit, "時給",
            "時給3件 vs 月給1件 → 時給選択 (旧仕様: 常に月給)"
        );
    }

    #[test]
    fn fixa_native_unit_other_group_majority_monthly_picks_gekkyu() {
        // 派遣・その他 グループで月給レコード過半数 → native_unit = "月給"
        let records = vec![
            rec_emp("派遣社員", 250_000, SalaryType::Monthly),
            rec_emp("派遣社員", 260_000, SalaryType::Monthly),
            rec_emp("派遣社員", 1500, SalaryType::Hourly),
        ];
        let groups = aggregate_by_emp_group_native(&records);
        let other = groups
            .iter()
            .find(|g| g.group_label == "派遣・その他")
            .unwrap();
        assert_eq!(other.native_unit, "月給", "月給2件 vs 時給1件 → 月給選択");
    }

    #[test]
    fn fixa_native_unit_other_group_tie_picks_gekkyu_conservative() {
        // 件数同数 (タイ) → 月給を保守的に選択
        // 修正前: 同数時の挙動が「monthly_values と hourly_values 同件数」で常に false → 月給
        //         (期せずして同じ結果だが、ロジックは破綻していた)
        // 修正後: 明示的に月給優先と仕様化
        let records = vec![
            rec_emp("派遣社員", 250_000, SalaryType::Monthly),
            rec_emp("派遣社員", 1500, SalaryType::Hourly),
        ];
        let groups = aggregate_by_emp_group_native(&records);
        let other = groups
            .iter()
            .find(|g| g.group_label == "派遣・その他")
            .unwrap();
        assert_eq!(other.native_unit, "月給", "タイは月給選択");
    }

    // =========================================================================
    // Finding #19: Pearson 相関係数の既知点列テスト (aggregate_records_core 経由)
    // =========================================================================

    /// SurveyRecord を JobBox ソースで組み立てるヘルパー
    /// annual_holidays, salary_min, salary_type を個別指定する
    fn mk_jobbox_record(
        company: &str,
        salary_min: i64,
        salary_type: SalaryType,
        holidays: i64,
    ) -> SurveyRecord {
        use super::super::upload::CsvSource;
        let mut sal = empty_salary();
        sal.salary_type = salary_type.clone();
        sal.min_value = Some(salary_min);
        sal.max_value = Some(salary_min);
        sal.unified_monthly = if matches!(salary_type, SalaryType::Monthly) {
            Some(salary_min)
        } else {
            None
        };
        let loc = empty_location();
        SurveyRecord {
            row_index: 0,
            source: CsvSource::JobBox,
            job_title: "テスト求人".to_string(),
            company_name: company.to_string(),
            location_raw: "東京都千代田区".to_string(),
            salary_raw: format!("月給{}円", salary_min),
            employment_type: "正社員".to_string(),
            employment_type_inferred: false,
            tags_raw: String::new(),
            url: None,
            is_new: false,
            description: String::new(),
            snippet: String::new(),
            salary_parsed: sal,
            location_parsed: loc,
            annual_holidays: Some(holidays),
            ai_monthly_holidays_min: None,
            ai_monthly_holidays_max: None,
            ai_bonus: None,
            ai_bonus_times_per_year: None,
            ai_paid_leave_rate: None,
            ai_weekly_holiday_type: None,
            ai_overtime_hours_monthly: None,
        }
    }

    #[test]
    fn pearson_perfect_positive() {
        // 既知点列 [(100, 80), (150, 90), (200, 100), (250, 110), (300, 120)]
        // 月給(万円→円): x = salary_min, y = annual_holidays
        // 完全正相関 r = 1.0
        let records: Vec<SurveyRecord> = [
            (100_000, 80),
            (150_000, 90),
            (200_000, 100),
            (250_000, 110),
            (300_000, 120),
        ]
        .iter()
        .enumerate()
        .map(|(i, &(sal, hol))| {
            let mut r = mk_jobbox_record(&format!("社{}", i), sal, SalaryType::Monthly, hol);
            // dedup を回避するため company_name を変える (mk_jobbox_record 内で company 引数を使用)
            r
        })
        .collect();
        let agg = aggregate_records(&records);
        let r = agg
            .jobbox
            .salary_holidays_correlation
            .expect("5 点以上なので Some を返すべき");
        assert!((r - 1.0).abs() < 0.001, "完全正相関のはず: r = {:.6}", r);
    }

    #[test]
    fn pearson_perfect_negative() {
        // 既知点列 [(100, 120), (200, 110), (300, 100)] → 完全負相関 r = -1.0
        let records: Vec<SurveyRecord> = [(100_000, 120), (200_000, 110), (300_000, 100)]
            .iter()
            .enumerate()
            .map(|(i, &(sal, hol))| {
                mk_jobbox_record(&format!("社{}", i), sal, SalaryType::Monthly, hol)
            })
            .collect();
        let agg = aggregate_records(&records);
        let r = agg
            .jobbox
            .salary_holidays_correlation
            .expect("3 点以上なので Some");
        assert!((r + 1.0).abs() < 0.001, "完全負相関のはず: r = {:.6}", r);
    }

    #[test]
    fn pearson_too_few_points() {
        // 2 点以下 → None (n < 3)
        let records: Vec<SurveyRecord> = [(200_000, 120), (300_000, 125)]
            .iter()
            .enumerate()
            .map(|(i, &(sal, hol))| {
                mk_jobbox_record(&format!("社{}", i), sal, SalaryType::Monthly, hol)
            })
            .collect();
        let agg = aggregate_records(&records);
        assert!(
            agg.jobbox.salary_holidays_correlation.is_none(),
            "2 点のみ → None を返すべき"
        );
    }

    #[test]
    fn pearson_zero_variance_x() {
        // 全 x が同値 (var_x = 0) → None
        let records: Vec<SurveyRecord> = [(300_000, 110), (300_000, 120), (300_000, 130)]
            .iter()
            .enumerate()
            .map(|(i, &(sal, hol))| {
                mk_jobbox_record(&format!("社{}", i + 10), sal, SalaryType::Monthly, hol)
            })
            .collect();
        let agg = aggregate_records(&records);
        // var_x = 0 なので denom = 0 → correlation = None
        assert!(
            agg.jobbox.salary_holidays_correlation.is_none(),
            "x の分散がゼロ → None"
        );
    }

    // =========================================================================
    // Finding #21: jobbox_records dedup / 会社名空除外 / 月給制限定 / 給与空除外
    // =========================================================================

    /// jobbox_records テスト用ヘルパー
    fn mk_record_for_jobbox(
        company: &str,
        title: &str,
        location: &str,
        salary_type: SalaryType,
        salary_min: Option<i64>,
        salary_max: Option<i64>,
        holidays: Option<i64>,
        salary_raw: &str,
        emp_type: &str,
    ) -> SurveyRecord {
        use super::super::upload::CsvSource;
        let mut sal = empty_salary();
        sal.salary_type = salary_type;
        sal.min_value = salary_min;
        sal.max_value = salary_max;
        sal.unified_monthly = salary_min;
        let loc = empty_location();
        SurveyRecord {
            row_index: 0,
            source: CsvSource::JobBox,
            job_title: title.to_string(),
            company_name: company.to_string(),
            location_raw: location.to_string(),
            salary_raw: salary_raw.to_string(),
            employment_type: emp_type.to_string(),
            employment_type_inferred: false,
            tags_raw: String::new(),
            url: None,
            is_new: false,
            description: String::new(),
            snippet: String::new(),
            salary_parsed: sal,
            location_parsed: loc,
            annual_holidays: holidays,
            ai_monthly_holidays_min: None,
            ai_monthly_holidays_max: None,
            ai_bonus: None,
            ai_bonus_times_per_year: None,
            ai_paid_leave_rate: None,
            ai_weekly_holiday_type: None,
            ai_overtime_hours_monthly: None,
        }
    }

    #[test]
    fn jobbox_records_dedup_removes_duplicate() {
        // 同 company + title + location + holidays + salary_raw + emp の 2 件 → 1 件に dedup
        let r1 = mk_record_for_jobbox(
            "テスト株式会社",
            "ドライバー",
            "東京都新宿区",
            SalaryType::Monthly,
            Some(250_000),
            Some(300_000),
            Some(120),
            "月給25万〜30万円",
            "正社員",
        );
        let r2 = r1.clone();
        let records = vec![r1, r2];
        let agg = aggregate_records(&records);
        assert_eq!(
            agg.jobbox.jobbox_records.len(),
            1,
            "完全一致の 2 件 → dedup で 1 件になるべき"
        );
    }

    #[test]
    fn jobbox_records_excludes_empty_company() {
        // company_name が空 → jobbox_records に含まれない (Commit 1 の修正検証)
        let r = mk_record_for_jobbox(
            "",
            "営業スタッフ",
            "大阪府大阪市",
            SalaryType::Monthly,
            Some(250_000),
            Some(300_000),
            Some(120),
            "月給25万〜30万円",
            "正社員",
        );
        let agg = aggregate_records(&[r]);
        assert!(
            agg.jobbox.jobbox_records.is_empty(),
            "会社名が空 → jobbox_records に含まれない"
        );
    }

    #[test]
    fn jobbox_records_excludes_non_monthly() {
        // salary_type = Hourly → jobbox_records に含まれない (月給制のみ対象)
        let r = mk_record_for_jobbox(
            "テスト株式会社",
            "パートスタッフ",
            "愛知県名古屋市",
            SalaryType::Hourly,
            Some(1200),
            Some(1500),
            Some(120),
            "時給1200〜1500円",
            "パート・アルバイト",
        );
        let agg = aggregate_records(&[r]);
        assert!(
            agg.jobbox.jobbox_records.is_empty(),
            "Hourly → jobbox_records に含まれない (月給制限定)"
        );
    }

    #[test]
    fn jobbox_records_excludes_no_salary() {
        // salary_min=None && salary_max=None → 含まれない
        let r = mk_record_for_jobbox(
            "テスト株式会社",
            "一般事務",
            "福岡県福岡市",
            SalaryType::Monthly,
            None,
            None,
            Some(120),
            "",
            "正社員",
        );
        let agg = aggregate_records(&[r]);
        assert!(
            agg.jobbox.jobbox_records.is_empty(),
            "給与情報なし (両 None) → jobbox_records に含まれない"
        );
    }

    // =========================================================================
    // Finding #10: popularity 集計ユニットテスト (Commit 1-3 回帰防止)
    // =========================================================================

    /// IndeedSp レコードを組み立てるヘルパー
    fn mk_indeed_sp_record(
        tags: &str,
        salary_type: SalaryType,
        salary_min: Option<i64>,
        holidays: Option<i64>,
        emp_type: &str,
    ) -> SurveyRecord {
        let mut sal = empty_salary();
        sal.salary_type = salary_type.clone();
        sal.min_value = salary_min;
        sal.max_value = salary_min;
        sal.unified_monthly = if matches!(salary_type, SalaryType::Monthly) {
            salary_min
        } else {
            None
        };
        let loc = empty_location();
        SurveyRecord {
            row_index: 0,
            source: CsvSource::IndeedSp,
            job_title: "テスト求人".to_string(),
            company_name: "テスト株式会社".to_string(),
            location_raw: "東京都千代田区".to_string(),
            salary_raw: String::new(),
            employment_type: emp_type.to_string(),
            employment_type_inferred: false,
            tags_raw: tags.to_string(),
            url: None,
            is_new: false,
            description: String::new(),
            snippet: String::new(),
            salary_parsed: sal,
            location_parsed: loc,
            annual_holidays: holidays,
            ai_monthly_holidays_min: None,
            ai_monthly_holidays_max: None,
            ai_bonus: None,
            ai_bonus_times_per_year: None,
            ai_paid_leave_rate: None,
            ai_weekly_holiday_type: None,
            ai_overtime_hours_monthly: None,
        }
    }

    /// 「超人気」優先: 同一レコードに「人気」「超人気」両方含む →
    /// super_popular にのみカウント、popular にはカウントしない (Commit 1 #1 検証)
    #[test]
    fn popularity_counts_super_popular_priority() {
        let r = mk_indeed_sp_record(
            "人気,超人気",
            SalaryType::Monthly,
            Some(250_000),
            None,
            "正社員",
        );
        let agg = aggregate_records(&[r]);
        let pop = &agg.popularity;
        assert_eq!(pop.super_popular_count, 1, "超人気 1 件");
        assert_eq!(pop.popular_count, 0, "超人気優先により popular_count=0");
        assert_eq!(pop.none_count, 0, "none_count=0");
    }

    /// Monthly のみ集計対象: SalaryType::Hourly / Annual は popular_salary_median から除外
    #[test]
    fn popularity_salary_median_monthly_only() {
        let monthly =
            mk_indeed_sp_record("人気", SalaryType::Monthly, Some(300_000), None, "正社員");
        let hourly = mk_indeed_sp_record("人気", SalaryType::Hourly, Some(1500), None, "正社員");
        let annual =
            mk_indeed_sp_record("人気", SalaryType::Annual, Some(5_000_000), None, "正社員");
        let agg = aggregate_records(&[monthly, hourly, annual]);
        let pop = &agg.popularity;
        // popular_count は 3 (IndeedSp 3 件すべてに「人気」タグ)
        assert_eq!(pop.popular_count, 3, "全 3 件が popular_count に計上");
        // popular_salary_median は Monthly 1 件のみ → 300_000
        assert_eq!(
            pop.popular_salary_median,
            Some(300_000),
            "Monthly 1 件のみ中央値算出 → 300_000"
        );
        // popular_n_salary は 1 (Monthly のみ)
        assert_eq!(pop.popular_n_salary, 1, "n_salary=1 (Monthly のみ)");
    }

    /// annual_holidays=None のレコードは popular_holidays_median から除外される
    #[test]
    fn popularity_holidays_median_excludes_no_data() {
        let with_holidays = mk_indeed_sp_record(
            "人気",
            SalaryType::Monthly,
            Some(250_000),
            Some(120),
            "正社員",
        );
        let no_holidays =
            mk_indeed_sp_record("人気", SalaryType::Monthly, Some(250_000), None, "正社員");
        let agg = aggregate_records(&[with_holidays, no_holidays]);
        let pop = &agg.popularity;
        assert_eq!(
            pop.popular_n_holidays, 1,
            "holidays=None のレコードは除外され n=1"
        );
        assert_eq!(
            pop.popular_holidays_median,
            Some(120),
            "holidays 1 件 → 中央値 120"
        );
    }

    /// Commit 1 #1/#3 検証: JobBox/Indeed (PC) で tags_raw="人気" を含むレコード投入 →
    /// popular_count=0 (IndeedSp 以外は除外)
    #[test]
    fn popularity_only_counts_indeedsp_records() {
        let mut r_jobbox =
            mk_indeed_sp_record("人気", SalaryType::Monthly, Some(250_000), None, "正社員");
        r_jobbox.source = CsvSource::JobBox;
        let mut r_indeed_pc =
            mk_indeed_sp_record("人気", SalaryType::Monthly, Some(250_000), None, "正社員");
        r_indeed_pc.source = CsvSource::Indeed;
        let agg = aggregate_records(&[r_jobbox, r_indeed_pc]);
        let pop = &agg.popularity;
        assert_eq!(
            pop.popular_count, 0,
            "IndeedSp 以外は popularity 集計から除外"
        );
        assert_eq!(pop.super_popular_count, 0);
        assert_eq!(pop.indeed_sp_total, 0, "IndeedSp ソース 0 件");
    }

    /// Commit 1 #2 検証: popular_ratio の分母は IndeedSp 件数のみ (他ソース 95 件で薄まらない)
    /// IndeedSp 5 件 (人気1+超人気1+none3) + JobBox 95 件 → popular_ratio = 2/5 = 0.4
    #[test]
    fn popularity_ratio_denominator_is_indeedsp_count() {
        let mut records: Vec<SurveyRecord> = Vec::new();
        // IndeedSp 5 件
        records.push(mk_indeed_sp_record(
            "人気",
            SalaryType::Monthly,
            None,
            None,
            "正社員",
        ));
        records.push(mk_indeed_sp_record(
            "超人気",
            SalaryType::Monthly,
            None,
            None,
            "正社員",
        ));
        for i in 0..3 {
            let mut r = mk_indeed_sp_record("", SalaryType::Monthly, None, None, "正社員");
            // company_name を変えて dedup を回避
            r.company_name = format!("テスト株式会社_{}", i);
            records.push(r);
        }
        // JobBox 95 件 (tags_raw="人気" を含んでも除外されるべき)
        for i in 0..95 {
            let mut r =
                mk_indeed_sp_record("人気", SalaryType::Monthly, Some(250_000), None, "正社員");
            r.source = CsvSource::JobBox;
            r.company_name = format!("JB株式会社_{}", i);
            records.push(r);
        }
        let agg = aggregate_records(&records);
        let pop = &agg.popularity;
        assert_eq!(pop.indeed_sp_total, 5, "IndeedSp は 5 件");
        assert_eq!(pop.popular_count, 1, "人気 1 件");
        assert_eq!(pop.super_popular_count, 1, "超人気 1 件");
        let expected = 2.0 / 5.0;
        assert!(
            (pop.popular_ratio - expected).abs() < 1e-9,
            "popular_ratio = 2/5 = 0.4 (JobBox 95 件で薄まらない): got {}",
            pop.popular_ratio
        );
    }

    /// Commit 2 検証: popular_n_salary=2 (< 5) のとき popular_n_salary が正しく記録される
    /// (表示層での "— (n不足)" は section_07_6_popularity.rs が担当するが、
    ///  aggregator が正しい n 値を出力していることをここで検証)
    #[test]
    fn popularity_n_thresholds_recorded_correctly() {
        // popular: Monthly 2 件 → popular_n_salary=2
        // non-popular: Monthly 4 件 → non_popular_n_salary=4
        let mut records: Vec<SurveyRecord> = Vec::new();
        for i in 0..2 {
            let mut r = mk_indeed_sp_record(
                "人気",
                SalaryType::Monthly,
                Some(250_000 + i * 1000),
                None,
                "正社員",
            );
            r.company_name = format!("人気社_{}", i);
            records.push(r);
        }
        for i in 0..4 {
            let mut r = mk_indeed_sp_record(
                "",
                SalaryType::Monthly,
                Some(230_000 + i * 1000),
                None,
                "正社員",
            );
            r.company_name = format!("none社_{}", i);
            records.push(r);
        }
        let agg = aggregate_records(&records);
        let pop = &agg.popularity;
        assert_eq!(
            pop.popular_n_salary, 2,
            "popular Monthly 2 件 → popular_n_salary=2"
        );
        assert_eq!(
            pop.non_popular_n_salary, 4,
            "non-popular Monthly 4 件 → non_popular_n_salary=4"
        );
    }

    // =========================================================================
    // 2026-07-28 年間休日 3 区分別統計 (HolidayStats) ユニットテスト
    // =========================================================================

    /// (a) 3 区分の年間休日統計が正しく集計される検算。
    /// 区分ごとに既知の値を入れ、中央値・平均・P25・P75 を検証する。
    #[test]
    fn popularity_holiday_stats_three_groups_compute_correctly() {
        let mut records: Vec<SurveyRecord> = Vec::new();
        // 超人気: [100,105,110,115,120] → median=110, mean=110, P25=105, P75=115
        for h in [100, 105, 110, 115, 120] {
            records.push(mk_indeed_sp_record(
                "超人気",
                SalaryType::Monthly,
                Some(250_000),
                Some(h),
                "正社員",
            ));
        }
        // 人気: [120,122,124,126,128] → median=124, mean=124, P25=122, P75=126
        for h in [120, 122, 124, 126, 128] {
            records.push(mk_indeed_sp_record(
                "人気",
                SalaryType::Monthly,
                Some(250_000),
                Some(h),
                "正社員",
            ));
        }
        // タグなし: [90,95,100,105,110] → median=100, mean=100, P25=95, P75=105
        for h in [90, 95, 100, 105, 110] {
            records.push(mk_indeed_sp_record(
                "",
                SalaryType::Monthly,
                Some(250_000),
                Some(h),
                "正社員",
            ));
        }
        let agg = aggregate_records(&records);
        let pop = &agg.popularity;

        let sp = &pop.super_popular_holiday_stats;
        assert_eq!(sp.n, 5, "超人気 n=5");
        assert_eq!(sp.median, Some(110), "超人気 中央値");
        assert_eq!(sp.mean, Some(110), "超人気 平均");
        assert_eq!(sp.p25, Some(105), "超人気 P25");
        assert_eq!(sp.p75, Some(115), "超人気 P75");

        let pp = &pop.popular_holiday_stats;
        assert_eq!(pp.n, 5, "人気 n=5");
        assert_eq!(pp.median, Some(124), "人気 中央値");
        assert_eq!(pp.mean, Some(124), "人気 平均");
        assert_eq!(pp.p25, Some(122), "人気 P25");
        assert_eq!(pp.p75, Some(126), "人気 P75");

        let np = &pop.non_popular_holiday_stats;
        assert_eq!(np.n, 5, "タグなし n=5");
        assert_eq!(np.median, Some(100), "タグなし 中央値");
        assert_eq!(np.mean, Some(100), "タグなし 平均");
        assert_eq!(np.p25, Some(95), "タグなし P25");
        assert_eq!(np.p75, Some(105), "タグなし P75");
    }

    /// (b) annual_holidays=None のレコードは各区分の分母 (n) から除外される。
    /// 超人気優先の判定順 (超人気 > 人気) も同時に検証する。
    #[test]
    fn popularity_holiday_stats_excludes_none_and_counts_denominator() {
        let mut records: Vec<SurveyRecord> = Vec::new();
        // 超人気: 休日あり 2 件 [110,130] + 休日 None 1 件 → n=2
        records.push(mk_indeed_sp_record(
            "超人気",
            SalaryType::Monthly,
            Some(250_000),
            Some(110),
            "正社員",
        ));
        records.push(mk_indeed_sp_record(
            "超人気",
            SalaryType::Monthly,
            Some(250_000),
            Some(130),
            "正社員",
        ));
        records.push(mk_indeed_sp_record(
            "超人気",
            SalaryType::Monthly,
            Some(250_000),
            None,
            "正社員",
        ));
        // 人気: 休日 None のみ 1 件 → n=0
        records.push(mk_indeed_sp_record(
            "人気",
            SalaryType::Monthly,
            Some(250_000),
            None,
            "正社員",
        ));
        // タグなし: 休日あり 1 件 [105] → n=1 (P25=P75=105)
        records.push(mk_indeed_sp_record(
            "",
            SalaryType::Monthly,
            Some(250_000),
            Some(105),
            "正社員",
        ));
        let agg = aggregate_records(&records);
        let pop = &agg.popularity;

        // 超人気: None 除外で n=2、median=(110+130)/2=120
        assert_eq!(pop.super_popular_holiday_stats.n, 2, "超人気 None 除外 n=2");
        assert_eq!(pop.super_popular_holiday_stats.median, Some(120));
        // 人気: 休日データなし → n=0、全 Option None
        assert_eq!(pop.popular_holiday_stats.n, 0, "人気 休日データなし n=0");
        assert_eq!(pop.popular_holiday_stats.median, None);
        assert_eq!(pop.popular_holiday_stats.mean, None);
        // タグなし: n=1、単一値のため median/P25/P75 全て 105
        assert_eq!(pop.non_popular_holiday_stats.n, 1, "タグなし n=1");
        assert_eq!(pop.non_popular_holiday_stats.median, Some(105));
        assert_eq!(pop.non_popular_holiday_stats.p25, Some(105));
        assert_eq!(pop.non_popular_holiday_stats.p75, Some(105));
    }

    /// compute_holiday_stats 単体: 空スライスは n=0 / 全 None
    #[test]
    fn compute_holiday_stats_empty_is_default() {
        let s = compute_holiday_stats(&[]);
        assert_eq!(s, HolidayStats::default());
        assert_eq!(s.n, 0);
        assert_eq!(s.median, None);
    }

    // ============================================================
    // 2026-07-01 SalaryStats (compute_salary_stats) 単体テスト
    // ============================================================

    /// 給与下限 3 件 → 中央値が中央要素を返す
    #[test]
    fn salary_stats_min_median_matches() {
        let pairs = [
            (Some(200_000), Some(300_000)),
            (Some(250_000), Some(350_000)),
            (Some(300_000), Some(400_000)),
        ];
        let stats = compute_salary_stats(&pairs);
        assert_eq!(stats.n, 3);
        assert_eq!(
            stats.min_median,
            Some(250_000),
            "min 3 件 [200k, 250k, 300k] の中央値 = 250k"
        );
    }

    /// 給与上限の平均は算術平均 (整数丸め)
    #[test]
    fn salary_stats_max_mean() {
        let pairs = [
            (Some(200_000), Some(300_000)),
            (Some(250_000), Some(350_000)),
            (Some(300_000), Some(400_000)),
        ];
        let stats = compute_salary_stats(&pairs);
        assert_eq!(
            stats.max_mean,
            Some(350_000),
            "max 3 件 [300k, 350k, 400k] の平均 = 350k"
        );
    }

    /// 5 万円刻みビン最頻値: 200-249 ビンが 2 件、250-299 ビンが 1 件 → 最頻ビン = 200k
    #[test]
    fn salary_stats_mode_50k_bins() {
        // 上限は同じダミー値 (上限側の最頻値は本テストの主眼ではないが Some で埋める)
        let pairs = [
            (Some(200_000), Some(300_000)),
            (Some(210_000), Some(300_000)),
            (Some(250_000), Some(300_000)),
        ];
        let stats = compute_salary_stats(&pairs);
        assert_eq!(
            stats.min_mode,
            Some(200_000),
            "200k と 210k は同じ 200k ビン → 最頻値 = 200k"
        );
    }

    /// 空入力: 全 None、n=0
    #[test]
    fn salary_stats_empty_returns_none() {
        let pairs: [(Option<i64>, Option<i64>); 0] = [];
        let stats = compute_salary_stats(&pairs);
        assert_eq!(stats.n, 0);
        assert_eq!(stats.min_mean, None);
        assert_eq!(stats.min_median, None);
        assert_eq!(stats.min_mode, None);
        assert_eq!(stats.max_mean, None);
        assert_eq!(stats.max_median, None);
        assert_eq!(stats.max_mode, None);
    }

    /// (None, None) のみのペアは n にカウントされない
    #[test]
    fn salary_stats_all_none_pairs_yields_zero_n() {
        let pairs = [(None, None), (None, None)];
        let stats = compute_salary_stats(&pairs);
        assert_eq!(stats.n, 0, "下限・上限どちらも None のペアは n=0");
        assert_eq!(stats.min_mean, None);
        assert_eq!(stats.max_mean, None);
    }

    /// 下限のみ / 上限のみ の片側ペアも n にカウントされる
    #[test]
    fn salary_stats_partial_pairs_count_in_n() {
        let pairs = [
            (Some(200_000), None), // 下限のみ
            (None, Some(400_000)), // 上限のみ
            (Some(250_000), Some(350_000)),
        ];
        let stats = compute_salary_stats(&pairs);
        assert_eq!(stats.n, 3, "3 ペアとも n にカウント");
        // 下限は 2 件 [200k, 250k] → 中央値 = (200+250)/2 = 225k (median_of 偶数件は整数割り算)
        assert_eq!(stats.min_median, Some(225_000));
        // 上限は 2 件 [400k, 350k] → 中央値 = 375k
        assert_eq!(stats.max_median, Some(375_000));
    }

    /// 最頻値タイの場合、小さい方のビンを返す
    #[test]
    fn salary_stats_mode_ties_return_smaller_bin() {
        let pairs = [
            (Some(200_000), None),
            (Some(300_000), None), // 200k ビン 1 件、300k ビン 1 件 → タイ
        ];
        let stats = compute_salary_stats(&pairs);
        assert_eq!(
            stats.min_mode,
            Some(200_000),
            "タイなら小さい方のビン (200k) を返す"
        );
    }

    // =========================================================================
    // 2026-09-29 時給/月給混在の逆証明
    //   疑い2: 時給モードのネイティブ配列 (円/時) に月給 (円/月) が混ざる
    //   疑い1: 都道府県別 下限平均が時給・月給・年俸を単位混在のまま平均する
    // =========================================================================

    fn mixed_3h_2m() -> Vec<SurveyRecord> {
        let mut v = Vec::new();
        for (i, h) in [1100i64, 1200, 1300].iter().enumerate() {
            v.push(mock_record(
                &format!("H{i}"),
                Some("東京都"),
                Some("千代田区"),
                Some(h * 167),
                Some(*h),
                Some(*h),
                SalaryType::Hourly,
                "パート",
                "",
            ));
        }
        for (i, m) in [200_000i64, 300_000].iter().enumerate() {
            v.push(mock_record(
                &format!("M{i}"),
                Some("東京都"),
                Some("千代田区"),
                Some(*m),
                Some(*m),
                Some(*m),
                SalaryType::Monthly,
                "正社員",
                "",
            ));
        }
        v
    }

    /// navy_report::common::compute_distribution_stats と同じ分位点定義
    /// (idx = round((n-1)*p)) で (n, mean, median, p90) を返す
    fn dist(values: &[i64]) -> (usize, i64, i64, i64) {
        let mut v: Vec<i64> = values.iter().copied().filter(|x| *x > 0).collect();
        v.sort_unstable();
        let n = v.len();
        assert!(n > 0, "空配列");
        let pct = |p: f64| v[(((n as f64 - 1.0) * p).round() as usize).min(n - 1)];
        (n, v.iter().sum::<i64>() / n as i64, pct(0.5), pct(0.9))
    }

    #[test]
    fn mix_native_values_in_hourly_mode_are_hourly_only() {
        let agg = aggregate_records(&mixed_3h_2m());
        assert!(agg.is_hourly, "時給3 > 5/2 で時給モード");
        // 修正前: median=1,300 / mean=100,720 (月給 20万/30万が円/時として混入)
        assert_eq!(dist(&agg.salary_min_values_native), (3, 1200, 1200, 1300));
        assert_eq!(agg.salary_min_values_native, vec![1100, 1200, 1300]);
        assert_eq!(agg.salary_max_values_native, vec![1100, 1200, 1300]);
        assert_eq!(
            agg.scatter_min_max_native,
            vec![(1100, 1100), (1200, 1200), (1300, 1300)]
        );
    }

    #[test]
    fn mix_native_values_in_monthly_mode_are_monthly_only() {
        let agg =
            aggregate_records_with_mode(&mixed_3h_2m(), super::super::upload::WageMode::Monthly);
        assert!(!agg.is_hourly);
        assert_eq!(agg.salary_min_values_native, vec![200_000, 300_000]);
        assert_eq!(
            agg.scatter_min_max_native,
            vec![(200_000, 200_000), (300_000, 300_000)]
        );
        // 月給換算配列 (既存) は時給×167 を含む従来どおりの値
        assert_eq!(
            agg.salary_min_values,
            vec![183_700, 200_400, 217_100, 200_000, 300_000]
        );
    }

    #[test]
    fn mix_prefecture_avg_min_salary_is_monthly_equivalent() {
        let agg = aggregate_records(&mixed_3h_2m());
        let tokyo = agg
            .by_prefecture_salary
            .iter()
            .find(|p| p.name == "東京都")
            .unwrap();
        // 修正前: (1100+1200+1300+200000+300000)/5 = 100,720
        // 月給換算: (183,700+200,400+217,100+200,000+300,000)/5 = 220,240
        assert_eq!(tokyo.avg_min_salary, 220_240);
    }

    #[test]
    fn mix_prefecture_avg_min_salary_excludes_annual() {
        let recs = vec![
            mock_record(
                "A",
                Some("大阪府"),
                Some("大阪市"),
                Some(250_000),
                Some(250_000),
                Some(250_000),
                SalaryType::Monthly,
                "正社員",
                "",
            ),
            mock_record(
                "B",
                Some("大阪府"),
                Some("大阪市"),
                Some(4_000_000 / 12),
                Some(4_000_000),
                Some(4_000_000),
                SalaryType::Annual,
                "正社員",
                "",
            ),
        ];
        let agg = aggregate_records(&recs);
        let osaka = agg
            .by_prefecture_salary
            .iter()
            .find(|p| p.name == "大阪府")
            .unwrap();
        // 修正前: (250,000 + 4,000,000)/2 = 2,125,000
        assert_eq!(osaka.avg_min_salary, 250_000);
    }

    /// 月給のみの入力: 既存の出力値が変わらないこと (回帰)
    #[test]
    fn mix_monthly_only_input_keeps_existing_values() {
        let recs: Vec<SurveyRecord> = [
            (200_000i64, 250_000i64),
            (220_000, 300_000),
            (300_000, 300_000),
        ]
        .iter()
        .enumerate()
        .map(|(i, (lo, hi))| {
            mock_record(
                &format!("M{i}"),
                Some("東京都"),
                Some("千代田区"),
                Some((lo + hi) / 2),
                Some(*lo),
                Some(*hi),
                SalaryType::Monthly,
                "正社員",
                "",
            )
        })
        .collect();
        let agg = aggregate_records(&recs);
        assert!(!agg.is_hourly);
        assert_eq!(agg.salary_min_values, vec![200_000, 220_000, 300_000]);
        assert_eq!(agg.salary_max_values, vec![250_000, 300_000, 300_000]);
        assert_eq!(
            agg.salary_min_values_native,
            vec![200_000, 220_000, 300_000]
        );
        assert_eq!(
            agg.salary_max_values_native,
            vec![250_000, 300_000, 300_000]
        );
        let tokyo = &agg.by_prefecture_salary[0];
        assert_eq!(tokyo.avg_min_salary, 240_000); // (200,000+220,000+300,000)/3
        assert_eq!(tokyo.avg_salary, 261_666); // (225,000+260,000+300,000)/3
    }

    /// f1 実データ (時給 751 + 月給 1): 時給モードの統計は時給求人だけで算出される
    #[test]
    fn mix_f1_real_data_hourly_stats_exclude_monthly() {
        let recs = salary_fixture::records(salary_fixture::F1_HOURLY_MOSTLY, "東京都 板橋区");
        let agg = aggregate_records(&recs);
        assert!(agg.is_hourly);
        let (n, mean, median, p90) = dist(&agg.salary_min_values_native);
        eprintln!("f1 native_min: n={n} mean={mean} median={median} p90={p90}");
        assert_eq!((n, mean, median, p90), (751, 1269, 1250, 1400));
        assert!(
            agg.salary_min_values_native.iter().all(|v| *v < 10_000),
            "円/時の配列に月給値が残っている"
        );
    }

    /// f2 実データ (月給 567 + 年俸/年収 24): 大阪の下限平均は年俸を除いた月給換算
    #[test]
    fn mix_f2_real_data_prefecture_avg_min_excludes_annual() {
        let recs = salary_fixture::records(salary_fixture::F2_MONTHLY_WITH_ANNUAL, "大阪府 大阪市");
        let agg = aggregate_records(&recs);
        assert!(!agg.is_hourly);
        let osaka = agg
            .by_prefecture_salary
            .iter()
            .find(|p| p.name == "大阪府")
            .unwrap();
        eprintln!("f2 osaka avg_min_salary={}", osaka.avg_min_salary);
        assert_eq!(agg.salary_min_values.len(), 567);
        assert_eq!(osaka.avg_min_salary, 314_489);
    }
}
