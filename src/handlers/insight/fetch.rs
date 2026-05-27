//! 全データソースからの一括取得 → InsightContext構築
//! 既存の analysis/fetch.rs と trend/fetch.rs の関数を再利用

use super::super::analysis::fetch as af;
use super::super::helpers::Row;
use super::super::trend::fetch as tf;

type Db = crate::db::local_sqlite::LocalDb;
type TursoDb = crate::db::turso_http::TursoDb;

/// 市区町村別ピラミッド（P1-5 Section 06 拡張で利用）
///
/// 上位 N 市区町村ごとに当該 muni の年齢階級別 男女別 人口を保持。
/// 用途: navy_report.rs `render_navy_section_06_demographics` の図 6-2b。
pub struct MuniPyramid {
    pub muni_name: String,
    /// `v2_external_population_pyramid` の row そのまま (age_group / male_count / female_count)
    pub bands: Vec<Row>,
}

/// 示唆エンジンへの統一入力
pub struct InsightContext {
    // === ローカルSQLite ===
    pub vacancy: Vec<Row>,
    pub resilience: Vec<Row>,
    pub transparency: Vec<Row>,
    pub temperature: Vec<Row>,
    pub competition: Vec<Row>,
    pub cascade: Vec<Row>,
    pub salary_comp: Vec<Row>,
    pub monopsony: Vec<Row>,
    pub spatial_mismatch: Vec<Row>,
    pub wage_compliance: Vec<Row>,
    pub region_benchmark: Vec<Row>,
    pub text_quality: Vec<Row>,
    // === Turso時系列 ===
    pub ts_counts: Vec<Row>,
    pub ts_vacancy: Vec<Row>,
    pub ts_salary: Vec<Row>,
    pub ts_fulfillment: Vec<Row>,
    pub ts_tracking: Vec<Row>,
    // === Turso外部統計（使用中） ===
    pub ext_job_ratio: Vec<Row>,
    pub ext_labor_stats: Vec<Row>,
    pub ext_min_wage: Vec<Row>,
    pub ext_turnover: Vec<Row>,
    // === Turso外部統計（新規活用） ===
    pub ext_population: Vec<Row>,
    pub ext_pyramid: Vec<Row>,
    /// P1-5 Section 06 拡張: 対象都道府県内で postings 件数上位 3 市区町村のピラミッド。
    /// `pref` が空、または上位 muni のピラミッドデータが取得できない場合は空 Vec。
    pub muni_pyramids: Vec<MuniPyramid>,
    pub ext_migration: Vec<Row>,
    pub ext_daytime_pop: Vec<Row>,
    pub ext_establishments: Vec<Row>,
    pub ext_business_dynamics: Vec<Row>,
    // ext_foreign: 未実装のため省略
    pub ext_care_demand: Vec<Row>,
    pub ext_household_spending: Vec<Row>,
    pub ext_climate: Vec<Row>,
    // === Impl-3 (2026-04-26): 媒体分析タブ ライフスタイル特性 ===
    // P-1: v2_external_social_life (47県 × 4カテゴリ: 趣味/スポーツ/ボランティア/学習)
    pub ext_social_life: Vec<Row>,
    // P-2: v2_external_internet_usage (47県 × internet_usage_rate / smartphone_ownership_rate)
    pub ext_internet_usage: Vec<Row>,
    // === Phase A: SSDSE-A 新規6テーブル ===
    pub ext_households: Vec<Row>,
    pub ext_vital: Vec<Row>,
    pub ext_labor_force: Vec<Row>,
    pub ext_medical_welfare: Vec<Row>,
    pub ext_education_facilities: Vec<Row>,
    pub ext_geography: Vec<Row>,
    // === Impl-2 (2026-04-26): 媒体分析タブ「人材デモグラフィック」section の D-2 で利用 ===
    // v2_external_education (47県 × education_level: 中卒/高卒/短大高専/大卒/大学院)
    // 国勢調査 2020 / 25 歳以上人口の最終学歴別構成
    pub ext_education: Vec<Row>,
    // === CR-9 (2026-04-27): 産業ミスマッチ警戒 section で利用 ===
    // 国勢調査 v2_external_industry_structure (集計コード AS/AR/CR 除外済み、都道府県粒度)
    // 列: industry_code, industry_name, employees_total ほか
    pub ext_industry_employees: Vec<Row>,
    // HW 求人の産業大分類別件数 (12 大分類にマッピング済み、件数降順)
    // 例: [("医療,福祉", 1200), ("製造業", 800), ...]
    pub hw_industry_counts: Vec<(String, i64)>,
    // === P1-6 (2026-05-28): 極端な分類偏り警告 ===
    // HW 求人の職種 (postings.job_type) 別件数 (件数降順、上位 30 件)
    // 例: [("看護師", 1200), ("介護職", 800), ...]
    // navy_report.rs Section 01 Exec Summary の Finding 07 (職種偏り) で利用。
    // populate 場所: survey/handlers.rs の HW context build (industry_counts と同タイミング)
    pub hw_job_type_counts: Vec<(String, i64)>,
    // === Phase A: 県平均（SUM方式、LS/MF/GE等の比較基準） ===
    pub pref_avg_unemployment_rate: Option<f64>,
    pub pref_avg_single_rate: Option<f64>,
    pub pref_avg_physicians_per_10k: Option<f64>,
    pub pref_avg_daycare_per_1k_children: Option<f64>,
    pub pref_avg_habitable_density: Option<f64>,
    // === Phase B: Agoop 人流（v2_flow_* テーブル未投入時は None） ===
    pub flow: Option<super::flow_context::FlowIndicators>,
    // === 通勤圏（距離ベース） ===
    pub commute_zone_count: usize,
    pub commute_zone_pref_count: usize,
    pub commute_zone_total_pop: i64,
    pub commute_zone_working_age: i64,
    pub commute_zone_elderly: i64,
    // === 通勤フロー（実データ: 国勢調査OD） ===
    pub commute_inflow_total: i64,
    pub commute_outflow_total: i64,
    pub commute_self_rate: f64,
    pub commute_inflow_top3: Vec<(String, String, i64)>, // (pref, muni, count)
    // === メタ ===
    pub pref: String,
    pub muni: String,
}

/// 全データを一括取得してInsightContextを構築
///
/// # 並列化 (2026-05-24, audit_I P0-1)
/// 40+ の Turso/SQLite fetch を `std::thread::scope` で 8 グループに分割し並列実行する。
/// Render (US) → Turso (日本) の RTT が 2-5s/req のため、シリアル実行では 100-200s を要し
/// Render 60s navigation timeout を超過していた。並列化により最遅グループで律速 (~10s 目標)。
///
/// 設計選択 (`company/fetch.rs:285` の前例踏襲):
/// - `std::thread::scope` を採用。`LocalDb` / `TursoDb` は内部 `Arc` で `Send + Sync` 安全。
/// - グループ境界は所要時間バランスを考慮 (Turso ext stat 系を 4-5 グループに分散)。
/// - `unwrap_or` の silent fallback は警告ログ付きで回避 (MEMORY: feedback_silent_fallback_audit)。
pub(crate) fn build_insight_context(
    db: &Db,
    turso: Option<&TursoDb>,
    pref: &str,
    muni: &str,
) -> InsightContext {
    // === 並列フェッチ (8 グループ) ===
    // 各 thread は db (LocalDb) / turso (TursoDb) を `&` borrow で共有 (Arc 内部のため安全)。
    // 戻り値型を tuple で明示しないと型推論が複雑になるので、グループ毎に明示する。
    type TsBundle = (Vec<Row>, Vec<Row>, Vec<Row>, Vec<Row>, Vec<Row>);
    type ExtTsBundle = (Vec<Row>, Vec<Row>, Vec<Row>, Vec<Row>);
    type LocalBundle = (
        Vec<Row>, Vec<Row>, Vec<Row>, Vec<Row>, Vec<Row>, Vec<Row>,
        Vec<Row>, Vec<Row>, Vec<Row>, Vec<Row>, Vec<Row>, Vec<Row>,
    );
    // PopBundle 末尾要素は P1-5 Section 06 拡張で追加した「上位 3 市区町村のピラミッド」
    type PopBundle = (Vec<Row>, Vec<Row>, Vec<Row>, Vec<Row>, Vec<MuniPyramid>);
    type EstBundle = (Vec<Row>, Vec<Row>, Vec<Row>, Vec<Row>, Vec<Row>);
    type LifeBundle = (Vec<Row>, Vec<Row>, Vec<Row>);
    type PhaseABundle = (Vec<Row>, Vec<Row>, Vec<Row>, Vec<Row>, Vec<Row>, Vec<Row>);
    type MeanFlowBundle = (Option<f64>, Option<f64>, Option<super::flow_context::FlowIndicators>);

    let (
        ts_bundle,
        ext_ts_bundle,
        local_bundle,
        pop_bundle,
        est_bundle,
        life_bundle,
        phase_a_bundle,
        mean_flow_bundle,
    ) = std::thread::scope(|s| {
        // G1: Turso 時系列 (5 fetches)
        let h_ts = s.spawn(|| -> TsBundle {
            if let Some(tdb) = turso {
                (
                    tf::fetch_ts_counts(tdb, pref),
                    tf::fetch_ts_vacancy(tdb, pref),
                    tf::fetch_ts_salary(tdb, pref),
                    tf::fetch_ts_fulfillment(tdb, pref),
                    tf::fetch_ts_tracking(tdb, pref),
                )
            } else {
                (vec![], vec![], vec![], vec![], vec![])
            }
        });

        // G2: Turso 外部統計 時系列 (4 fetches)
        let h_ext_ts = s.spawn(|| -> ExtTsBundle {
            if let Some(tdb) = turso {
                (
                    tf::fetch_ext_job_openings_ratio(tdb, pref),
                    tf::fetch_ext_labor_stats(tdb, pref),
                    tf::fetch_ext_minimum_wage_history(tdb, pref),
                    tf::fetch_ext_turnover(tdb, pref),
                )
            } else {
                (vec![], vec![], vec![], vec![])
            }
        });

        // G3: ローカル SQLite (12 fetches、高速だが並列化で他グループ完了待ち時間を活用)
        let h_local = s.spawn(|| -> LocalBundle {
            (
                af::fetch_vacancy_data(db, pref, muni),
                af::fetch_resilience_data(db, pref, muni),
                af::fetch_transparency_data(db, pref, muni),
                af::fetch_temperature_data(db, pref, muni),
                af::fetch_competition_data(db, pref),
                af::fetch_cascade_data(db, pref, muni),
                af::fetch_salary_competitiveness(db, pref, muni),
                af::fetch_monopsony_data(db, pref, muni),
                af::fetch_spatial_mismatch(db, pref, muni),
                af::fetch_wage_compliance(db, pref, muni),
                af::fetch_region_benchmark(db, pref, muni),
                af::fetch_text_quality(db, pref, muni),
            )
        });

        // G4: Turso 人口・移動 (4 fetches + P1-5 muni 別ピラミッド)
        // 上位 3 muni 別ピラミッドはこの G4 で同期取得する。muni 別 pyramid は 3 県跨ぎなど
        // 重い処理になりうるが、他グループ (G3 ローカル / G6 ライフ等) と並列なので律速にはなりにくい。
        let h_pop = s.spawn(|| -> PopBundle {
            let pop = af::fetch_population_data(db, turso, pref, muni);
            let pyramid = af::fetch_population_pyramid(db, turso, pref, muni);
            let migration = af::fetch_migration_data(db, turso, pref, muni);
            let daytime = af::fetch_daytime_population(db, turso, pref, muni);

            // P1-5: 対象都道府県内で postings 件数上位 3 muni のピラミッド取得
            // pref が空のときは silent fallback を避けるため明示的に空 Vec を返す。
            let muni_pyramids: Vec<MuniPyramid> = if pref.is_empty() {
                Vec::new()
            } else {
                let top_munis = af::fetch_top_muni_names(db, pref, 3);
                top_munis
                    .into_iter()
                    .filter_map(|m| {
                        let bands = af::fetch_population_pyramid(db, turso, pref, &m);
                        if bands.is_empty() {
                            None
                        } else {
                            Some(MuniPyramid {
                                muni_name: m,
                                bands,
                            })
                        }
                    })
                    .collect()
            };

            (pop, pyramid, migration, daytime, muni_pyramids)
        });

        // G5: Turso 事業所・消費・気候 (5 fetches)
        let h_est = s.spawn(|| -> EstBundle {
            (
                af::fetch_establishments(db, turso, pref),
                af::fetch_business_dynamics(db, turso, pref),
                af::fetch_care_demand(db, turso, pref),
                af::fetch_household_spending(db, turso, pref),
                af::fetch_climate(db, turso, pref),
            )
        });

        // G6: Turso ライフスタイル (3 fetches)
        let h_life = s.spawn(|| -> LifeBundle {
            (
                af::fetch_social_life(db, turso, pref),
                af::fetch_internet_usage(db, turso, pref),
                af::fetch_education(db, turso, pref),
            )
        });

        // G7: Turso Phase A SSDSE-A (6 fetches)
        let h_phase_a = s.spawn(|| -> PhaseABundle {
            (
                af::fetch_households(db, turso, pref, muni),
                af::fetch_vital_statistics(db, turso, pref, muni),
                af::fetch_labor_force(db, turso, pref, muni),
                af::fetch_medical_welfare(db, turso, pref, muni),
                af::fetch_education_facilities(db, turso, pref, muni),
                af::fetch_geography(db, turso, pref, muni),
            )
        });

        // G8: Turso 県平均 + Agoop 人流 (3 fetches)
        let h_mean_flow = s.spawn(|| -> MeanFlowBundle {
            (
                af::fetch_prefecture_mean(
                    db,
                    turso,
                    pref,
                    "SUM(unemployed)",
                    "SUM(employed) + SUM(unemployed)",
                    "v2_external_labor_force",
                ),
                af::fetch_prefecture_mean(
                    db,
                    turso,
                    pref,
                    "SUM(single_households)",
                    "SUM(total_households)",
                    "v2_external_households",
                ),
                super::flow_context::build_flow_context(db, turso, pref, muni, 2019),
            )
        });

        // join all. silent fallback 監査 (MEMORY: feedback_silent_fallback_audit) に従い、
        // panic が起きたら警告ログを出してから空値を返す。
        let ts = h_ts.join().unwrap_or_else(|e| {
            tracing::warn!(?e, "build_insight_context G1 (ts) thread panicked, using empty defaults");
            (vec![], vec![], vec![], vec![], vec![])
        });
        let ext_ts = h_ext_ts.join().unwrap_or_else(|e| {
            tracing::warn!(?e, "build_insight_context G2 (ext_ts) thread panicked, using empty defaults");
            (vec![], vec![], vec![], vec![])
        });
        let local = h_local.join().unwrap_or_else(|e| {
            tracing::warn!(?e, "build_insight_context G3 (local) thread panicked, using empty defaults");
            (
                vec![], vec![], vec![], vec![], vec![], vec![],
                vec![], vec![], vec![], vec![], vec![], vec![],
            )
        });
        let pop = h_pop.join().unwrap_or_else(|e| {
            tracing::warn!(?e, "build_insight_context G4 (pop) thread panicked, using empty defaults");
            (vec![], vec![], vec![], vec![], Vec::<MuniPyramid>::new())
        });
        let est = h_est.join().unwrap_or_else(|e| {
            tracing::warn!(?e, "build_insight_context G5 (est) thread panicked, using empty defaults");
            (vec![], vec![], vec![], vec![], vec![])
        });
        let life = h_life.join().unwrap_or_else(|e| {
            tracing::warn!(?e, "build_insight_context G6 (life) thread panicked, using empty defaults");
            (vec![], vec![], vec![])
        });
        let phase_a = h_phase_a.join().unwrap_or_else(|e| {
            tracing::warn!(?e, "build_insight_context G7 (phase_a) thread panicked, using empty defaults");
            (vec![], vec![], vec![], vec![], vec![], vec![])
        });
        let mean_flow = h_mean_flow.join().unwrap_or_else(|e| {
            tracing::warn!(?e, "build_insight_context G8 (mean_flow) thread panicked, using empty defaults");
            (None, None, None)
        });

        (ts, ext_ts, local, pop, est, life, phase_a, mean_flow)
    });

    // unpack
    let (ts_counts, ts_vacancy, ts_salary, ts_fulfillment, ts_tracking) = ts_bundle;
    let (ext_job_ratio, ext_labor_stats, ext_min_wage_ts, ext_turnover_ts) = ext_ts_bundle;
    let (
        vacancy,
        resilience,
        transparency,
        temperature,
        competition,
        cascade,
        salary_comp,
        monopsony,
        spatial_mismatch,
        wage_compliance,
        region_benchmark,
        text_quality,
    ) = local_bundle;
    let (ext_population, ext_pyramid, ext_migration, ext_daytime_pop, muni_pyramids) = pop_bundle;
    let (ext_establishments, ext_business_dynamics, ext_care_demand, ext_household_spending, ext_climate) =
        est_bundle;
    let (ext_social_life, ext_internet_usage, ext_education) = life_bundle;
    let (
        ext_households,
        ext_vital,
        ext_labor_force,
        ext_medical_welfare,
        ext_education_facilities,
        ext_geography,
    ) = phase_a_bundle;
    let (pref_avg_unemployment_rate, pref_avg_single_rate, flow) = mean_flow_bundle;

    let mut ctx = InsightContext {
        // ローカルSQLite（analysis/fetch.rsの関数を再利用）
        vacancy,
        resilience,
        transparency,
        temperature,
        competition,
        cascade,
        salary_comp,
        monopsony,
        spatial_mismatch,
        wage_compliance,
        region_benchmark,
        text_quality,
        // Turso時系列
        ts_counts,
        ts_vacancy,
        ts_salary,
        ts_fulfillment,
        ts_tracking,
        // Turso外部統計（使用中）
        ext_job_ratio,
        ext_labor_stats,
        ext_min_wage: ext_min_wage_ts,
        ext_turnover: ext_turnover_ts,
        // Turso外部統計（新規活用 - analysis/fetch.rsの関数を再利用）
        ext_population,
        ext_pyramid,
        // P1-5 Section 06 拡張: 上位 3 市区町村のピラミッド
        muni_pyramids,
        ext_migration,
        ext_daytime_pop,
        ext_establishments,
        ext_business_dynamics,
        ext_care_demand,
        ext_household_spending,
        ext_climate,
        // Impl-3 (2026-04-26): ライフスタイル特性
        ext_social_life,
        ext_internet_usage,
        // Phase A: SSDSE-A 新規6テーブル
        ext_households,
        ext_vital,
        ext_labor_force,
        ext_medical_welfare,
        ext_education_facilities,
        ext_geography,
        // Impl-2 (2026-04-26): 学歴分布 (subtab5_phase4_7::fetch_education を再利用)
        ext_education,
        // CR-9 (2026-04-27 / 2026-04-28 修正): 産業ミスマッチ警戒
        // 注: integrate エンドポイントが本コンテキストを使用するため、
        //     /report/survey 専用の遅いフェッチをここに含めると integrate がタイムアウトする。
        //     CR-9 用データは survey_report_html ハンドラ側で別途フェッチして上書きする。
        ext_industry_employees: vec![],
        hw_industry_counts: vec![],
        // P1-6 (2026-05-28): build_insight_context では空初期化のみ。
        // 実データの populate は survey/handlers.rs の HW context build 時に行う
        // (CR-9 hw_industry_counts と同じ理由: integrate エンドポイントのタイムアウト回避)
        hw_job_type_counts: vec![],
        // Phase A: 県平均（SUM方式、market-level benchmark）
        pref_avg_unemployment_rate,
        pref_avg_single_rate,
        pref_avg_physicians_per_10k: None, // ctx作成後に人口で計算（相互依存回避）
        pref_avg_daycare_per_1k_children: None, // 同上
        pref_avg_habitable_density: None,  // 同上
        // Phase B: Agoop 人流（デフォルトyear=2019、コロナバイアス最小）
        flow,
        // 通勤圏（距離ベース）
        commute_zone_count: 0,
        commute_zone_pref_count: 0,
        commute_zone_total_pop: 0,
        commute_zone_working_age: 0,
        commute_zone_elderly: 0,
        // 通勤フロー（実データ）
        commute_inflow_total: 0,
        commute_outflow_total: 0,
        commute_self_rate: 0.0,
        commute_inflow_top3: vec![],
        // メタ
        pref: pref.to_string(),
        muni: muni.to_string(),
    };

    // 通勤圏データ計算（市区町村選択時のみ）
    if !muni.is_empty() {
        let zone = af::fetch_commute_zone(db, pref, muni, 30.0);
        if !zone.is_empty() {
            let mut pref_set = std::collections::HashSet::new();
            for m in &zone {
                pref_set.insert(m.prefecture.clone());
            }
            ctx.commute_zone_count = zone.len();
            ctx.commute_zone_pref_count = pref_set.len();

            let pyramid = af::fetch_commute_zone_pyramid(db, turso, &zone);
            for row in &pyramid {
                let male = super::super::helpers::get_i64(row, "male_count");
                let female = super::super::helpers::get_i64(row, "female_count");
                let total = male + female;
                ctx.commute_zone_total_pop += total;
                let age = super::super::helpers::get_str_ref(row, "age_group");
                match age {
                    "15-19" | "20-24" | "25-29" | "30-34" | "35-39" | "40-44" | "45-49"
                    | "50-54" | "55-59" | "60-64" | "10-19" | "20-29" | "30-39" | "40-49"
                    | "50-59" | "60-69" => ctx.commute_zone_working_age += total,
                    _ => {}
                }
                match age {
                    "65-69" | "70-74" | "75-79" | "80-84" | "85+" | "70-79" | "80+" => {
                        ctx.commute_zone_elderly += total
                    }
                    _ => {}
                }
            }
        }
    }

    // 通勤フロー（実データ）
    // 2026-05-14: Turso fallback 対応で turso 引数を伝搬。v2_external_commute_od が
    //   ローカル DB に投入されていなくても Turso 側にあれば取得できる。
    if !muni.is_empty() {
        let inflow = af::fetch_commute_inflow(db, turso, pref, muni);
        ctx.commute_inflow_total = inflow.iter().map(|f| f.total_commuters).sum();
        ctx.commute_inflow_top3 = inflow
            .iter()
            .take(3)
            .map(|f| {
                (
                    f.partner_pref.clone(),
                    f.partner_muni.clone(),
                    f.total_commuters,
                )
            })
            .collect();

        let outflow = af::fetch_commute_outflow(db, turso, pref, muni);
        ctx.commute_outflow_total = outflow.iter().map(|f| f.total_commuters).sum();
        ctx.commute_self_rate = af::fetch_self_commute_rate(db, turso, pref, muni);
    }

    ctx
}
