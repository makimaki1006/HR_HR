//! レポート HTML 描画パラメータの統合構造体 (A3 リファクタ 2026-05-22)
//!
//! # 背景
//!
//! 旧コードは `render_survey_report_page` から `_with_enrichment` → `_with_municipalities` →
//! `_with_variant` → `_with_variant_v2` → `_with_variant_v3` → `_with_variant_v3_themed` まで
//! **7 段のラッパ関数**で機能拡張を積み重ねていた。最深層 `_with_variant_v3_themed` は **引数 19 個**に
//! 達し、呼び出し時の引数地獄・新引数追加のたびに段が増える技術負債を抱えていた。
//!
//! # リファクタ方針
//!
//! - 全引数を `RenderConfig<'a>` 1 つに集約 (lifetime `'a` は内包する参照群の共通寿命)
//! - 構築は **builder pattern** (`RenderConfig::builder().agg(..).seeker(..).build()`)
//! - **既存 7 段ラッパは互換維持**: 内部で builder を組み立てて `render_survey_report_page_with_config` に委譲
//! - 新規 caller (将来追加) は builder を直接使う
//!
//! # 引数の意味
//!
//! 各 field の意味は旧 `_with_variant_v3_themed` のシグネチャと同じ。
//! 個別の項目仕様は本ファイルの doc コメント、または `mod.rs` 内 7 段ラッパの doc を参照。

use super::super::super::company::fetch::{NearbyCompany, RegionalCompanySegments};
use super::super::super::insight::fetch::InsightContext;
use super::super::aggregator::{CompanyAgg, EmpTypeSalary, SurveyAggregation};
use super::super::granularity::MunicipalityDemographics;
use super::super::hw_enrichment::HwAreaEnrichment;
use super::super::job_seeker::JobSeekerAnalysis;
use super::super::upload::WageMode;
use super::{ReportTheme, ReportVariant};
use std::collections::HashMap;

/// 求人市場 総合診断レポート HTML 描画パラメータ。
///
/// # 設計意図
///
/// 旧 7 段ラッパ (`render_survey_report_page` ～ `_with_variant_v3_themed`) を 1 つに集約。
/// 全 field は **参照 (`&'a T` / `Option<&'a T>` / `&'a [T]`)** で保持し、所有権は呼出側に残す。
///
/// # 使い方
///
/// ```ignore
/// let cfg = RenderConfig::builder()
///     .agg(&agg)
///     .seeker(&seeker)
///     .by_company(&by_company)
///     .by_emp_type_salary(&by_emp_type_salary)
///     .salary_min_values(&salary_min_values)
///     .salary_max_values(&salary_max_values)
///     .variant(ReportVariant::Full)
///     .theme(ReportTheme::Default)
///     .build();
/// let html = render_survey_report_page_with_config(&cfg);
/// ```
///
/// # 互換性
///
/// 既存の 7 段ラッパは本構造体を使う薄いラッパとして再実装済み (`mod.rs` 参照)。
/// 既存 caller (handler / test) のシグネチャは変更されていない。
pub(crate) struct RenderConfig<'a> {
    /// CSV 集計結果 (求人市場の中核データ)
    pub agg: &'a SurveyAggregation,
    /// 求職者心理分析結果
    pub seeker: &'a JobSeekerAnalysis,
    /// 企業別集計 (給与・件数)
    pub by_company: &'a [CompanyAgg],
    /// 雇用形態別給与
    pub by_emp_type_salary: &'a [EmpTypeSalary],
    /// 下限給与一覧 (ヒストグラム用)
    pub salary_min_values: &'a [i64],
    /// 上限給与一覧 (ヒストグラム用)
    pub salary_max_values: &'a [i64],
    /// HW ローカル/外部統計コンテキスト
    pub hw_context: Option<&'a InsightContext>,
    /// 地域注目企業リスト (内部名は呼出側互換で維持)
    pub salesnow_companies: &'a [NearbyCompany],
    /// 4 セグメント企業 (大手 / 中堅 / 急成長 / 採用活発) - 全業界
    pub salesnow_segments: &'a RegionalCompanySegments,
    /// 4 セグメント企業 (大手 / 中堅 / 急成長 / 採用活発) - 同業界フィルタ後
    pub salesnow_segments_industry: &'a RegionalCompanySegments,
    /// 業界フィルタ (None = 全業界)
    pub industry_filter: Option<&'a str>,
    /// 市区町村別 HW enrichment map (key = `"{prefecture}:{municipality}"`)
    pub hw_enrichment_map: &'a HashMap<String, HwAreaEnrichment>,
    /// 市区町村別デモグラフィック (人口ピラミッド・労働力等)
    pub municipality_demographics: &'a [MunicipalityDemographics],
    /// レポートバリアント (Full / Public / MarketIntelligence)
    pub variant: ReportVariant,
    /// レポートテーマ (Round 24 以降は Default のみ)
    pub theme: ReportTheme,
    /// MarketIntelligence variant 専用 fetch 用 DB 参照 (None で従来動作)
    pub db: Option<&'a crate::db::local_sqlite::LocalDb>,
    /// MarketIntelligence variant 専用 fetch 用 Turso DB 参照 (None で従来動作)
    pub turso: Option<&'a crate::db::turso_http::TursoDb>,
    /// ユーザー選択地域 (都道府県)。空文字列なら未選択 (CSV dominant にフォールバック)
    pub selected_pref: &'a str,
    /// ユーザー選択地域 (市区町村)。空文字列なら未選択 (CSV dominant にフォールバック)
    pub selected_muni: &'a str,
    /// Phase 2-A (2026-05-29): 給与単位モード。
    ///
    /// - `WageMode::Monthly`: 月給ベース描画 (Section 03 万円表示)
    /// - `WageMode::Hourly`: 時給ベース描画 (Section 03 円/時 表示)
    /// - `WageMode::Auto`: agg.is_hourly に従う (旧動作互換)
    ///
    /// 現状の Section 03/05/06 描画は `agg.is_hourly` を直接読むため、本フィールドは
    /// 将来の Section 拡張 (Phase 2-B 以降の時給特有指標 H1/H3/H4) で参照する。
    /// silent fallback 禁止 (Option ではなく enum 必須)。デフォルトは `WageMode::Auto`。
    pub wage_mode: WageMode,
}

impl<'a> RenderConfig<'a> {
    /// 新規 builder を返す。
    pub fn builder() -> RenderConfigBuilder<'a> {
        RenderConfigBuilder::default()
    }
}

/// `RenderConfig<'a>` 構築用 builder。
///
/// 必須 field (`agg`, `seeker`) が未指定の場合は `build()` が panic する。
/// 任意 field はデフォルト値 (空 slice / `None` / `Full` / `Default`) を使用する。
#[derive(Default)]
pub(crate) struct RenderConfigBuilder<'a> {
    agg: Option<&'a SurveyAggregation>,
    seeker: Option<&'a JobSeekerAnalysis>,
    by_company: Option<&'a [CompanyAgg]>,
    by_emp_type_salary: Option<&'a [EmpTypeSalary]>,
    salary_min_values: Option<&'a [i64]>,
    salary_max_values: Option<&'a [i64]>,
    hw_context: Option<&'a InsightContext>,
    salesnow_companies: Option<&'a [NearbyCompany]>,
    salesnow_segments: Option<&'a RegionalCompanySegments>,
    salesnow_segments_industry: Option<&'a RegionalCompanySegments>,
    industry_filter: Option<&'a str>,
    hw_enrichment_map: Option<&'a HashMap<String, HwAreaEnrichment>>,
    municipality_demographics: Option<&'a [MunicipalityDemographics]>,
    variant: Option<ReportVariant>,
    theme: Option<ReportTheme>,
    db: Option<&'a crate::db::local_sqlite::LocalDb>,
    turso: Option<&'a crate::db::turso_http::TursoDb>,
    selected_pref: Option<&'a str>,
    selected_muni: Option<&'a str>,
    /// Phase 2-A (2026-05-29): wage_mode (None → Auto デフォルト)
    wage_mode: Option<WageMode>,
}

impl<'a> RenderConfigBuilder<'a> {
    pub fn agg(mut self, v: &'a SurveyAggregation) -> Self {
        self.agg = Some(v);
        self
    }

    pub fn seeker(mut self, v: &'a JobSeekerAnalysis) -> Self {
        self.seeker = Some(v);
        self
    }

    pub fn by_company(mut self, v: &'a [CompanyAgg]) -> Self {
        self.by_company = Some(v);
        self
    }

    pub fn by_emp_type_salary(mut self, v: &'a [EmpTypeSalary]) -> Self {
        self.by_emp_type_salary = Some(v);
        self
    }

    pub fn salary_min_values(mut self, v: &'a [i64]) -> Self {
        self.salary_min_values = Some(v);
        self
    }

    pub fn salary_max_values(mut self, v: &'a [i64]) -> Self {
        self.salary_max_values = Some(v);
        self
    }

    pub fn hw_context(mut self, v: Option<&'a InsightContext>) -> Self {
        self.hw_context = v;
        self
    }

    pub fn salesnow_companies(mut self, v: &'a [NearbyCompany]) -> Self {
        self.salesnow_companies = Some(v);
        self
    }

    pub fn salesnow_segments(mut self, v: &'a RegionalCompanySegments) -> Self {
        self.salesnow_segments = Some(v);
        self
    }

    pub fn salesnow_segments_industry(mut self, v: &'a RegionalCompanySegments) -> Self {
        self.salesnow_segments_industry = Some(v);
        self
    }

    pub fn industry_filter(mut self, v: Option<&'a str>) -> Self {
        self.industry_filter = v;
        self
    }

    pub fn hw_enrichment_map(mut self, v: &'a HashMap<String, HwAreaEnrichment>) -> Self {
        self.hw_enrichment_map = Some(v);
        self
    }

    pub fn municipality_demographics(mut self, v: &'a [MunicipalityDemographics]) -> Self {
        self.municipality_demographics = Some(v);
        self
    }

    pub fn variant(mut self, v: ReportVariant) -> Self {
        self.variant = Some(v);
        self
    }

    pub fn theme(mut self, v: ReportTheme) -> Self {
        self.theme = Some(v);
        self
    }

    pub fn db(mut self, v: Option<&'a crate::db::local_sqlite::LocalDb>) -> Self {
        self.db = v;
        self
    }

    pub fn turso(mut self, v: Option<&'a crate::db::turso_http::TursoDb>) -> Self {
        self.turso = v;
        self
    }

    pub fn selected_pref(mut self, v: &'a str) -> Self {
        self.selected_pref = Some(v);
        self
    }

    pub fn selected_muni(mut self, v: &'a str) -> Self {
        self.selected_muni = Some(v);
        self
    }

    /// Phase 2-A (2026-05-29): 給与単位モード setter。
    pub fn wage_mode(mut self, v: WageMode) -> Self {
        self.wage_mode = Some(v);
        self
    }

    /// `RenderConfig<'a>` を構築する。
    ///
    /// # Panics
    ///
    /// 必須 field (`agg`, `seeker`) が未指定の場合 panic。
    /// その他 field はデフォルト値 (空 slice / `None` / `Full` / `Default`) を使用する。
    ///
    /// # デフォルト値の根拠
    ///
    /// 旧 7 段ラッパは下層を呼ぶ際に空 slice / 空 HashMap / `default()` セグメント /
    /// `None` / `Full` / `Default` を埋めていた。本 builder のデフォルトはそれと同じ。
    pub fn build(self) -> RenderConfig<'a> {
        // 必須 field チェック (panic with clear message)
        let agg = self.agg.expect("RenderConfig.agg is required");
        let seeker = self.seeker.expect("RenderConfig.seeker is required");

        // 任意 field のデフォルト値用 static (std::sync::OnceLock; Rust 1.70+)
        // (HashMap / RegionalCompanySegments は中身を持つため 'static borrow が必要)
        static EMPTY_HW_ENRICHMENT: std::sync::OnceLock<HashMap<String, HwAreaEnrichment>> =
            std::sync::OnceLock::new();
        static EMPTY_SEGMENTS: std::sync::OnceLock<RegionalCompanySegments> =
            std::sync::OnceLock::new();
        let empty_hw = EMPTY_HW_ENRICHMENT.get_or_init(HashMap::new);
        let empty_segments = EMPTY_SEGMENTS.get_or_init(RegionalCompanySegments::default);

        RenderConfig {
            agg,
            seeker,
            by_company: self.by_company.unwrap_or(&[]),
            by_emp_type_salary: self.by_emp_type_salary.unwrap_or(&[]),
            salary_min_values: self.salary_min_values.unwrap_or(&[]),
            salary_max_values: self.salary_max_values.unwrap_or(&[]),
            hw_context: self.hw_context,
            salesnow_companies: self.salesnow_companies.unwrap_or(&[]),
            salesnow_segments: self.salesnow_segments.unwrap_or(empty_segments),
            salesnow_segments_industry: self.salesnow_segments_industry.unwrap_or(empty_segments),
            industry_filter: self.industry_filter,
            hw_enrichment_map: self.hw_enrichment_map.unwrap_or(empty_hw),
            municipality_demographics: self.municipality_demographics.unwrap_or(&[]),
            variant: self.variant.unwrap_or(ReportVariant::Full),
            theme: self.theme.unwrap_or(ReportTheme::Default),
            db: self.db,
            turso: self.turso,
            selected_pref: self.selected_pref.unwrap_or(""),
            selected_muni: self.selected_muni.unwrap_or(""),
            // Phase 2-A (2026-05-29): wage_mode デフォルトは Auto
            // (silent fallback ではなく明示的に Auto enum 値で表現)
            wage_mode: self.wage_mode.unwrap_or(WageMode::Auto),
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn builder_with_minimum_required_fields() {
        let agg = SurveyAggregation::default();
        let seeker = JobSeekerAnalysis::default();
        let cfg = RenderConfig::builder().agg(&agg).seeker(&seeker).build();

        // デフォルト値の検証
        assert!(cfg.by_company.is_empty(), "by_company default = empty");
        assert!(
            cfg.by_emp_type_salary.is_empty(),
            "by_emp_type_salary default = empty"
        );
        assert!(
            cfg.salary_min_values.is_empty(),
            "salary_min_values default = empty"
        );
        assert!(
            cfg.salary_max_values.is_empty(),
            "salary_max_values default = empty"
        );
        assert!(cfg.hw_context.is_none(), "hw_context default = None");
        assert!(
            cfg.salesnow_companies.is_empty(),
            "salesnow_companies default = empty"
        );
        assert!(
            cfg.industry_filter.is_none(),
            "industry_filter default = None"
        );
        assert!(
            cfg.hw_enrichment_map.is_empty(),
            "hw_enrichment_map default = empty"
        );
        assert!(
            cfg.municipality_demographics.is_empty(),
            "municipality_demographics default = empty"
        );
        assert_eq!(cfg.variant, ReportVariant::Full, "variant default = Full");
        assert_eq!(cfg.theme, ReportTheme::Default, "theme default = Default");
        assert!(cfg.db.is_none(), "db default = None");
        assert!(cfg.turso.is_none(), "turso default = None");
        assert_eq!(cfg.selected_pref, "", "selected_pref default = empty");
        assert_eq!(cfg.selected_muni, "", "selected_muni default = empty");
    }

    #[test]
    fn builder_with_variant_and_theme_overrides() {
        let agg = SurveyAggregation::default();
        let seeker = JobSeekerAnalysis::default();
        let cfg = RenderConfig::builder()
            .agg(&agg)
            .seeker(&seeker)
            .variant(ReportVariant::Public)
            .theme(ReportTheme::Default)
            .selected_pref("東京都")
            .selected_muni("千代田区")
            .build();

        assert_eq!(cfg.variant, ReportVariant::Public);
        assert_eq!(cfg.selected_pref, "東京都");
        assert_eq!(cfg.selected_muni, "千代田区");
    }

    #[test]
    #[should_panic(expected = "RenderConfig.agg is required")]
    fn builder_missing_agg_panics() {
        let seeker = JobSeekerAnalysis::default();
        let _ = RenderConfig::builder().seeker(&seeker).build();
    }

    #[test]
    #[should_panic(expected = "RenderConfig.seeker is required")]
    fn builder_missing_seeker_panics() {
        let agg = SurveyAggregation::default();
        let _ = RenderConfig::builder().agg(&agg).build();
    }
}
