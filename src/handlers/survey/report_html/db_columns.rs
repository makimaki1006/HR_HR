//! 外部統計 (ext_*) 列名の Single Source of Truth (SSoT) + 列コントラクトテスト。
//!
//! # 防止対象の事故クラス: 「列名 × SQL エイリアス不一致 → silent 0」(3 回再発)
//!
//! `navy_report` の各 section は `InsightContext` に格納された行
//! (`Vec<serde_json::Map>`) から `get_f64(r, "キー")` / `get_i64` /
//! `get_str_ref` で値を読む。ここで指定する「キー」は、その行を生成した
//! SQL の SELECT エイリアス (`... as employees_total` 等) と **完全一致** して
//! いなければならない。
//!
//! [`helpers::get_f64`](super::super::super::helpers::get_f64) は **missing key /
//! NULL / 型変換不能をすべて `0.0` に落とす silent fallback** である
//! (helpers.rs:42-48 の警告参照)。そのため render 側のキー文字列と SQL 側の
//! エイリアスが 1 文字でもずれると、例外も警告も出ずに「常に 0」「(0 年)」
//! 「均等基準へフォールバック」といった沈黙のバグになる。過去の実例:
//!   - `"year"` (旧) vs `fiscal_year` (実カラム) → 表 7-E が「(0 年)」
//!     (section_07_lifestyle.rs:86-88 のコメント)
//!   - `"employees"` (旧) vs `employees_total` (実エイリアス) → §9-B が
//!     均等基準 (1/N) へ誤フォールバック (section_09_market_intelligence.rs:356-358)
//!
//! # この仕組みの二本柱
//! 1. **render 側 SSoT**: navy_report の事故ゾーン (section_04 / 07 / 09) が読む
//!    キー文字列リテラルを、下記 const 参照に置換する。これで render 側で
//!    キーを打ち間違える経路を構造的に塞ぐ。
//! 2. **コントラクトテスト** (本ファイル `#[cfg(test)]`): 各 const が、その行を
//!    生成する fetch 関数の SQL の **SELECT 句に実在する** ことを検証する。
//!    SQL のエイリアスがリネームされた瞬間、テストが赤くなる。
//!
//! # なぜ `get_f64_required` を追加しないのか (スコープ判断)
//! silent 0 の別解として「キー不在なら `None`/`Err` を返す helper」を足す案も
//! あるが、helpers.rs には既に `get_f64_opt` / `get_i64_opt` が存在し、新規
//! helper 追加は呼び出し側の全面書き換えを誘発してスコープが肥大する。
//! 本ファイルは「render 側 SSoT + SQL 実在性のコントラクトテスト」でキー不一致
//! を根本から防ぐ方針を採り、helper の追加は行わない。
//!
//! # コントラクトテストの実装方式について (`include_str!` を採用した理由)
//! 理想は fetch 側の SQL も本 const を参照する (両側 SSoT) ことだが、SQL は
//! `trend/fetch.rs` 等の **別担当ファイル** に inline `let sql = "..."` で埋め込
//! まれており、それらを `pub(crate) const` 化する編集は並列作業の競合を招く
//! (本タスクは「担当ファイルのみ編集」)。そこで、SQL 側は編集せず、コンパイル
//! 時に fetch ソースを `include_str!` で取り込み、SELECT 句に const 値が実在
//! するかを検証する方式を採る。live なソースを直接読むため二重管理による drift
//! は発生せず、エイリアスのリネームを確実に検出できる。

// ============================================================
// 識別子 / 年度系 (複数テーブル共通)
// ============================================================

/// 年度 (最低賃金履歴 / 求人倍率 / 労働統計 / 離職率 等)。
/// 旧キー `"year"` は常に 0 を返す事故のもと。必ず本 const を使う。
pub(crate) const FISCAL_YEAR: &str = "fiscal_year";
/// 参照年 (家計支出 / 地理指標)。
pub(crate) const REFERENCE_YEAR: &str = "reference_year";
/// 産業大分類名 (産業構造 / 企業一覧)。旧キー `"employees"` 系事故と同居。
pub(crate) const INDUSTRY_NAME: &str = "industry_name";
/// 家計支出のカテゴリ名 (`"消費支出"` 等)。
pub(crate) const CATEGORY: &str = "category";

// ============================================================
// 最低賃金 (min_wage): trend/fetch.rs::fetch_ext_minimum_wage_history
//   SELECT fiscal_year, hourly_min_wage FROM v2_external_minimum_wage_history
// ============================================================

/// 最低賃金 (時給・円)。
pub(crate) const HOURLY_MIN_WAGE: &str = "hourly_min_wage";

// ============================================================
// 有効求人倍率 (job_ratio): trend/fetch.rs::fetch_ext_job_openings_ratio
//   SELECT fiscal_year, ratio_total, ratio_excl_part FROM v2_external_job_openings_ratio
// ============================================================

/// 有効求人倍率 (全体)。
pub(crate) const RATIO_TOTAL: &str = "ratio_total";

// ============================================================
// 労働統計 (labor_stats)
//   離職率/入職率: trend/fetch.rs::fetch_ext_turnover(_with_industry)
//     SELECT fiscal_year, entry_rate, separation_rate, net_rate FROM ...
//   失業率/労働力率: analysis/fetch/subtab7_phase_a.rs::fetch_labor_force
//     SELECT ... unemployment_rate, labor_force_participation_rate, reference_date ...
// ============================================================

/// 離職率。
pub(crate) const SEPARATION_RATE: &str = "separation_rate";
/// 入職率。
pub(crate) const ENTRY_RATE: &str = "entry_rate";
/// 完全失業率 (%)。
pub(crate) const UNEMPLOYMENT_RATE: &str = "unemployment_rate";
/// 労働力率 (%)。
pub(crate) const LABOR_FORCE_PARTICIPATION_RATE: &str = "labor_force_participation_rate";

// ============================================================
// 家計支出 (household_spending): analysis/fetch/subtab5_phase4.rs::fetch_household_spending
//   SELECT prefecture, category, monthly_amount, reference_year FROM ...
// ============================================================

/// 月額支出 (円)。ORDER BY monthly_amount DESC のため `.last()` は最小サブ
/// カテゴリを拾う点に注意 (section_09 の `find(category == "消費支出")` 参照)。
pub(crate) const MONTHLY_AMOUNT: &str = "monthly_amount";

// ============================================================
// 産業構造 (industry_structure): analysis/fetch/subtab5_phase4_7.rs::fetch_industry_structure
//   SELECT industry_code, industry_name, SUM(...) as establishments,
//          SUM(employees_total) as employees_total, ... FROM v2_external_industry_structure
// ============================================================

/// 従業者総数。旧キー `"employees"` は常に 0 → §9-B 均等基準フォールバック事故。
pub(crate) const EMPLOYEES_TOTAL: &str = "employees_total";

// ============================================================
// 地理指標 (geography): analysis/fetch/subtab7_phase_a.rs::fetch_geography
//   SELECT ... total_area_km2, habitable_area_km2,
//          population_density_per_km2, habitable_density_per_km2, reference_year ...
// ============================================================

/// 総面積 (km^2)。
pub(crate) const TOTAL_AREA_KM2: &str = "total_area_km2";
/// 可住地面積 (km^2)。表 2-B タイトルに明記される列。
pub(crate) const HABITABLE_AREA_KM2: &str = "habitable_area_km2";
/// 人口密度 (人/km^2)。
pub(crate) const POPULATION_DENSITY_PER_KM2: &str = "population_density_per_km2";

// ============================================================
// 詳細版 (Extended / Section 10) 専用クロス集計テーブル (2026-07-09)
//   すべて trend/fetch.rs::fetch_cross_* が生成 (fetch 側 SQL も本 const を参照する両側 SSoT)。
//   介護・ハローワークデータは一切含まない。公的統計 × 今回の求人データのみ。
// ============================================================

// ------------------------------------------------------------
// cross_future_workforce (図1 働き手の将来マップ):
//   trend/fetch.rs::fetch_cross_future_workforce
//   SELECT prefecture, muni_code, municipality,
//          wa_2020, working_age_ratio_2020, wa_decline_rate
//   FROM cross_future_workforce WHERE prefecture = ?1 ORDER BY wa_decline_rate
//   出典: 国の将来人口推計 (国立社会保障・人口問題研究所)。
//   ※ 列名は upload_cross_tables.py の DDL と 1 文字一致 (wa_2020 / wa_decline_rate)。
//     旧 const 値 (working_age_2020 / working_age_decline_2040) は Turso に存在せず
//     §10 が丸ごと silent skip する事故の原因だった (2026-07-09 修正)。
// ------------------------------------------------------------

/// 市区町村名 (cross_future_workforce)。
pub(crate) const MUNICIPALITY: &str = "municipality";
/// 市区町村コード。
pub(crate) const MUNI_CODE: &str = "muni_code";
/// 働き手 (15〜64歳) 人口 (2020年、人)。点の大きさに使用。DDL: `wa_2020`。
pub(crate) const WA_2020: &str = "wa_2020";
/// 人口に占める働き手の割合 (2020年、%)。散布図 Y 軸。DDL: `working_age_ratio_2020`。
pub(crate) const WORKING_AGE_RATIO_2020: &str = "working_age_ratio_2020";
/// 働き手の 2020年→2040年 増減率 (%、負値=減少)。散布図 X 軸 / 減少ランキング。
/// DDL: `wa_decline_rate` (CSV 実データは -6.56 等の負値=減少率)。
pub(crate) const WA_DECLINE_RATE: &str = "wa_decline_rate";

// ------------------------------------------------------------
// cross_wage_public (図2 給与の相場比較):
//   trend/fetch.rs::fetch_cross_wage_public
//   SELECT prefecture, year_month, scheduled_earnings,
//          min_wage_monthly_160h, min_wage_hourly
//   FROM cross_wage_public WHERE prefecture = ?1 ORDER BY year_month
//   出典: 毎月勤労統計 地方調査 (所定内給与) / 最低賃金 × 月160時間 (固定)。
// ------------------------------------------------------------

/// 対象年月 ("2025-01" 形式)。折れ線 X 軸。
pub(crate) const YEAR_MONTH: &str = "year_month";
/// 県の平均給与 (所定内給与、円)。折れ線 線1。
pub(crate) const SCHEDULED_EARNINGS: &str = "scheduled_earnings";
/// 最低賃金で月160時間働いた場合の月額 (円)。折れ線 線2 (階段状)。DDL: `min_wage_monthly_160h`。
pub(crate) const MIN_WAGE_MONTHLY_160H: &str = "min_wage_monthly_160h";
/// 最低賃金 (時給・円、cross_wage_public 版)。DDL: `min_wage_hourly`。
/// 注意: 外部統計 v2_external_minimum_wage_history 側の [`HOURLY_MIN_WAGE`] (`hourly_min_wage`)
/// とは **別テーブル・別列名**。両者を混同しない。
pub(crate) const MIN_WAGE_HOURLY: &str = "min_wage_hourly";

// ------------------------------------------------------------
// cross_switcher_supply (図3 転職を考えている人 / 図4 採用ネック診断):
//   trend/fetch.rs::fetch_cross_switcher_supply
//   SELECT region_code, region_name,
//          job_change_desire_rate, side_job_holders, additional_job_seekers,
//          job_change_seekers, pref_job_openings_ratio
//   FROM cross_switcher_supply
//   WHERE region_code = '00000' OR substr(region_code,1,2) = (対象県コードの先頭2桁)
//   出典: 就業構造基本調査 / 一般職業紹介状況 有効求人倍率。
//   ※ このテーブルには prefecture / municipality 列は存在しない。地域は region_code
//     ("00000"=全国 / "44000"=県 / "44201"=市区町村) と region_name (全国/大分県/大分市)。
// ------------------------------------------------------------

/// 地域コード ("00000"=全国 / "44000"=県 / "44201"=市区町村)。
pub(crate) const REGION_CODE: &str = "region_code";
/// 地域名 (全国 / 大分県 / 大分市 …)。DDL: `region_name` (旧コードが誤って読んでいた
/// `municipality` 列はこのテーブルには存在しない)。
pub(crate) const REGION_NAME: &str = "region_name";
/// 転職を考えている割合 (%)。
pub(crate) const JOB_CHANGE_DESIRE_RATE: &str = "job_change_desire_rate";
/// 副業をしている人の数 (人)。
pub(crate) const SIDE_JOB_HOLDERS: &str = "side_job_holders";
/// もっと働きたいと答えた人の数 (追加就業希望者、人)。
pub(crate) const ADDITIONAL_JOB_SEEKERS: &str = "additional_job_seekers";
/// 転職を考えている人の数 (転職希望者、人)。
pub(crate) const JOB_CHANGE_SEEKERS: &str = "job_change_seekers";
/// 有効求人倍率 (倍、参考値)。DDL: `pref_job_openings_ratio`。
pub(crate) const PREF_JOB_OPENINGS_RATIO: &str = "pref_job_openings_ratio";

// ============================================================
// コントラクトテスト
// ============================================================
#[cfg(test)]
mod contract_tests {
    use super::*;

    // 各 fetch モジュールのソースをコンパイル時に取り込む (live source を直接検証)。
    // パスは本ファイル (src/handlers/survey/report_html/db_columns.rs) からの相対。
    const TREND_FETCH_SRC: &str = include_str!("../../trend/fetch.rs");
    const HOUSEHOLD_SRC: &str = include_str!("../../analysis/fetch/subtab5_phase4.rs");
    const INDUSTRY_SRC: &str = include_str!("../../analysis/fetch/subtab5_phase4_7.rs");
    const LABOR_GEO_SRC: &str = include_str!("../../analysis/fetch/subtab7_phase_a.rs");

    /// `src` 内の各 `SELECT ... FROM` 区間 (SELECT 句) をバイト境界安全に列挙する。
    /// 区間内に日本語リテラル (`'全体' as municipality` 等) を含んでもよい:
    /// 端点は ASCII の "SELECT"/"FROM" 位置なので slice は常に char 境界。
    fn select_clauses(src: &str) -> Vec<&str> {
        let mut out = Vec::new();
        let mut cursor = 0usize;
        while let Some(rel) = src[cursor..].find("SELECT ") {
            let sel = cursor + rel;
            match src[sel..].find("FROM ") {
                Some(frel) => {
                    let end = sel + frel;
                    out.push(&src[sel..end]);
                    cursor = end + "FROM ".len();
                }
                // 対応する FROM が無ければ以降に SELECT 句は無いとみなす
                None => break,
            }
        }
        out
    }

    /// `col` がソース `src` の **いずれかの SELECT 句** に現れることを保証する。
    /// SQL エイリアスがリネームされる (= 事故) と false になり、テストが赤くなる。
    /// コメントや WHERE/ORDER BY だけに現れる誤検出を避けるため SELECT 句限定。
    fn assert_selected(src: &str, src_label: &str, col: &str) {
        let found = select_clauses(src)
            .iter()
            .any(|clause| clause.contains(col));
        assert!(
            found,
            "列コントラクト違反: 列名 SSoT の `{col}` が {src_label} の SELECT 句に存在しません。\n\
             SQL のエイリアスがリネームされたか、SSoT const がタイポしています。\n\
             navy_report は get_f64/get_i64 でこのキーを読むため、不一致は silent 0 事故になります。\n\
             SQL 側 (SELECT ... as {col}) と本 const の双方を確認してください。"
        );
    }

    /// 自己テスト: select_clauses が SELECT..FROM を正しく切り出し、
    /// FROM 以降 (ORDER BY 等) の語を SELECT 句に含めないこと。
    #[test]
    fn select_clause_extractor_excludes_from_onwards() {
        let sql = "let x = \"SELECT a, b as employees_total FROM t ORDER BY zzz_after_from\";";
        let clauses = select_clauses(sql);
        assert_eq!(clauses.len(), 1);
        assert!(clauses[0].contains("employees_total"));
        assert!(!clauses[0].contains("zzz_after_from"));
    }

    // ---- 事故当事者 6 種を網羅するコントラクト ----

    #[test]
    fn contract_min_wage() {
        // trend/fetch.rs: SELECT fiscal_year, hourly_min_wage FROM v2_external_minimum_wage_history
        assert_selected(TREND_FETCH_SRC, "trend/fetch.rs", HOURLY_MIN_WAGE);
        assert_selected(TREND_FETCH_SRC, "trend/fetch.rs", FISCAL_YEAR);
    }

    #[test]
    fn contract_job_ratio() {
        // trend/fetch.rs: SELECT fiscal_year, ratio_total, ratio_excl_part FROM v2_external_job_openings_ratio
        assert_selected(TREND_FETCH_SRC, "trend/fetch.rs", RATIO_TOTAL);
    }

    #[test]
    fn contract_labor_stats() {
        // 離職率/入職率: trend/fetch.rs fetch_ext_turnover(_with_industry)
        assert_selected(TREND_FETCH_SRC, "trend/fetch.rs", SEPARATION_RATE);
        assert_selected(TREND_FETCH_SRC, "trend/fetch.rs", ENTRY_RATE);
        // 失業率/労働力率: analysis/fetch/subtab7_phase_a.rs fetch_labor_force
        assert_selected(LABOR_GEO_SRC, "subtab7_phase_a.rs", UNEMPLOYMENT_RATE);
        assert_selected(
            LABOR_GEO_SRC,
            "subtab7_phase_a.rs",
            LABOR_FORCE_PARTICIPATION_RATE,
        );
    }

    #[test]
    fn contract_household_spending() {
        // subtab5_phase4.rs: SELECT prefecture, category, monthly_amount, reference_year FROM ...
        assert_selected(HOUSEHOLD_SRC, "subtab5_phase4.rs", MONTHLY_AMOUNT);
        assert_selected(HOUSEHOLD_SRC, "subtab5_phase4.rs", CATEGORY);
        assert_selected(HOUSEHOLD_SRC, "subtab5_phase4.rs", REFERENCE_YEAR);
    }

    #[test]
    fn contract_industry_structure() {
        // subtab5_phase4_7.rs: ... SUM(employees_total) as employees_total, ... industry_name ...
        assert_selected(INDUSTRY_SRC, "subtab5_phase4_7.rs", EMPLOYEES_TOTAL);
        assert_selected(INDUSTRY_SRC, "subtab5_phase4_7.rs", INDUSTRY_NAME);
    }

    #[test]
    fn contract_geography() {
        // subtab7_phase_a.rs fetch_geography: total_area_km2, habitable_area_km2,
        //   population_density_per_km2, reference_year
        assert_selected(LABOR_GEO_SRC, "subtab7_phase_a.rs", TOTAL_AREA_KM2);
        assert_selected(LABOR_GEO_SRC, "subtab7_phase_a.rs", HABITABLE_AREA_KM2);
        assert_selected(
            LABOR_GEO_SRC,
            "subtab7_phase_a.rs",
            POPULATION_DENSITY_PER_KM2,
        );
        assert_selected(LABOR_GEO_SRC, "subtab7_phase_a.rs", REFERENCE_YEAR);
    }

    // ---- 詳細版 (Section 10) cross_* テーブル (2026-07-09) ----
    // 二重の突合で silent 0 / silent skip を根絶する:
    //   (A) fetch 側 SQL の SELECT 句に各 const が実在する (Rust↔Rust)。
    //   (B) Turso 投入スクリプト upload_cross_tables.py の DDL に各 const 値が
    //       実在する (Rust↔Python の言語またぎ)。
    // 今回の事故 (2026-07-09) は (A) は満たすが (B) を満たさない自己整合的な誤名
    //   (working_age_2020 等: fetch SQL と const は一致するが Turso には無い列)
    // ですり抜けた。(B) を追加してこの穴を塞ぐ。
    const TREND_FETCH_SRC_CROSS: &str = include_str!("../../trend/fetch.rs");

    // Turso 投入スクリプトの DDL (CREATE TABLE 文) をコンパイル時に取り込む。
    // パスは本ファイル (src/handlers/survey/report_html/db_columns.rs) から
    // リポジトリルート直下の scripts/ まで 4 階層上がる。
    const UPLOAD_CROSS_TABLES_PY: &str = include_str!("../../../../scripts/upload_cross_tables.py");

    /// `col` が Turso 投入 DDL (upload_cross_tables.py) に列定義として実在することを保証する。
    /// DDL は `col_name TYPE` 形式なので「col の直後に半角スペース」を必須にして、
    /// 別列名の部分文字列 (例: wa_2020 ⊂ ... ) への誤マッチを避ける。
    fn assert_in_ddl(col: &str) {
        let needle = format!("{} ", col);
        assert!(
            UPLOAD_CROSS_TABLES_PY.contains(&needle),
            "列コントラクト違反 (言語またぎ): 列名 SSoT の `{col}` が \
             scripts/upload_cross_tables.py の DDL に存在しません。\n\
             Turso に実在しない列名を SELECT すると、クエリが失敗し §10 (詳細版4図) が \
             丸ごと silent skip します (2026-07-09 事故)。\n\
             upload_cross_tables.py の CREATE TABLE 文と本 const の双方を確認してください。"
        );
    }

    /// (A) fetch SQL の SELECT 句と (B) Turso DDL の両方に col が実在することを一括検証。
    fn assert_cross_col(label: &str, col: &str) {
        assert_selected(TREND_FETCH_SRC_CROSS, label, col);
        assert_in_ddl(col);
    }

    #[test]
    fn contract_cross_future_workforce() {
        let label = "trend/fetch.rs (cross_future_workforce)";
        assert_cross_col(label, MUNICIPALITY);
        assert_cross_col(label, MUNI_CODE);
        assert_cross_col(label, WA_2020);
        assert_cross_col(label, WORKING_AGE_RATIO_2020);
        assert_cross_col(label, WA_DECLINE_RATE);
    }

    #[test]
    fn contract_cross_wage_public() {
        let label = "trend/fetch.rs (cross_wage_public)";
        assert_cross_col(label, YEAR_MONTH);
        assert_cross_col(label, SCHEDULED_EARNINGS);
        assert_cross_col(label, MIN_WAGE_MONTHLY_160H);
        assert_cross_col(label, MIN_WAGE_HOURLY);
    }

    #[test]
    fn contract_cross_switcher_supply() {
        let label = "trend/fetch.rs (cross_switcher_supply)";
        assert_cross_col(label, REGION_CODE);
        assert_cross_col(label, REGION_NAME);
        assert_cross_col(label, JOB_CHANGE_DESIRE_RATE);
        assert_cross_col(label, SIDE_JOB_HOLDERS);
        assert_cross_col(label, ADDITIONAL_JOB_SEEKERS);
        assert_cross_col(label, JOB_CHANGE_SEEKERS);
        assert_cross_col(label, PREF_JOB_OPENINGS_RATIO);
    }
}
