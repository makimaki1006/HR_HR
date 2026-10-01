//! 採用診断タブ/API ハンドラ (Panel 1-3)

use axum::extract::{Query, State};
use axum::response::Html;
use axum::Json;
use serde::{Deserialize, Serialize};
use std::sync::Arc;
use tower_sessions::Session;

use crate::geo::{city_code, pref_name_to_code};
use crate::handlers::competitive::{build_option, build_option_with_data, escape_html};
use crate::handlers::helpers::Row;
use crate::handlers::jobmap::fromto as fft;
use crate::handlers::overview::get_session_filters;
use crate::AppState;

use super::fetch;
use super::render;
use super::types::RdErrorResponse;
use super::{expand_employment_type, CAUSATION_NOTE, HW_SCOPE_NOTE};
use ts_rs::TS;

const DEFAULT_AGOOP_YEAR: i32 = 2021;
const INFLOW_DATA_WARNING: &str =
    "v2_flow_fromto_city は Turso 書き込み制限により約83%のみ投入済み。残り17%は完全投入後に反映予定。";

/// Panel 1 観光地・繁華街判定閾値（F1 #3 修正、2026-04-26）
/// 平日昼滞在 / 平日夜滞在 (≒居住人口) の比が本値を超える地域は
/// 「観光地・繁華街型」と判定し、採用難度スコアの分母を居住人口側に補正する。
///
/// 値の根拠: build_talent_pool_so_what の流入超過型閾値 1.2 より厳しめに設定。
/// 1.5 は "通勤流入超過 50% 以上" に相当する強い昼夜不均衡。
/// 銀座・京都四条河原町など昼間の外来流入が著しいエリアを対象とする傾向。
pub(crate) const TOURIST_AREA_DAYNIGHT_RATIO: f64 = 1.5;

// ========== タブ骨格 ==========

/// `GET /tab/recruitment_diag` : 初期ページ
pub async fn tab_recruitment_diag(
    State(state): State<Arc<AppState>>,
    session: Session,
) -> Html<String> {
    let filters = get_session_filters(&session).await;

    // 都道府県 options
    let pref_code_map = pref_name_to_code();
    let mut pref_names: Vec<&&'static str> = pref_code_map.keys().collect();
    pref_names.sort_by_key(|name| {
        pref_code_map
            .get(*name)
            .map(|c| c.parse::<i32>().unwrap_or(99))
            .unwrap_or(99)
    });

    let pref_options: String = std::iter::once(build_option("", "-- 都道府県 --"))
        .chain(pref_names.iter().map(|p| {
            let prefcode = pref_code_map
                .get(**p)
                .map(|c| c.trim_start_matches('0').to_string())
                .unwrap_or_default();
            let selected = **p == filters.prefecture.as_str();
            let data_attrs: Vec<(&str, String)> = if !prefcode.is_empty() {
                vec![("prefcode", prefcode)]
            } else {
                Vec::new()
            };
            if selected {
                let attrs: String = data_attrs
                    .iter()
                    .map(|(k, v)| format!(r#" data-{}="{}""#, k, escape_html(v)))
                    .collect();
                format!(
                    r#"<option value="{}"{} selected>{}</option>"#,
                    escape_html(p),
                    attrs,
                    escape_html(p)
                )
            } else {
                build_option_with_data(p, p, &data_attrs)
            }
        }))
        .collect::<Vec<_>>()
        .join("\n");

    // hw_db がなければエラーメッセージ
    if state.hw_db.is_none() {
        return Html(
            r#"<div class="p-8 text-center text-gray-400">
                <h2 class="text-2xl mb-4">採用診断</h2>
                <p>hellowork.db が読み込まれていません。</p>
            </div>"#
                .to_string(),
        );
    }

    // Agent D テンプレへは都道府県 options のみ注入（業種/雇用形態はテンプレ側ハードコード）。
    // セッションフィルタは上記の pref_options 生成時の selected 判定で使用済み。
    let html = render::render_diag_page(&pref_options);
    Html(html)
}

// ========== Panel 1: 採用難度スコア ==========

#[derive(Deserialize)]
pub struct DifficultyParams {
    #[serde(default)]
    pub job_type: String,
    #[serde(default)]
    pub emp_type: String,
    #[serde(default)]
    pub prefecture: String,
    #[serde(default)]
    pub municipality: String,
    #[serde(default)]
    pub prefcode: Option<String>,
    #[serde(default)]
    pub citycode: Option<i64>,
}

/// Panel 1 の応答で、解決後の入力条件をそのまま返す部分。
#[derive(Debug, Clone, Serialize, TS)]
pub struct RdDifficultyInputs {
    /// 業種 (クエリの `job_type` そのまま。空文字は全業種)。
    pub job_type: String,
    /// 雇用形態 (クエリの `emp_type` そのまま。展開前の UI 値)。
    pub emp_type: String,
    /// 都道府県名 (クエリ優先、空ならセッションの値)。
    pub prefecture: String,
    /// 市区町村名 (クエリ優先、空ならセッションの値)。
    pub municipality: String,
    /// 市区町村コード (数値。解決できなければ null)。
    pub citycode: Option<i64>,
}

/// Panel 1 の数値部分。
#[derive(Debug, Clone, Serialize, TS)]
pub struct RdDifficultyMetrics {
    /// 該当エリアの HW 掲載求人件数 (件)。
    pub hw_count: i64,
    /// スコアの分母に使った人口 (人、月平均)。観光地判定時は夜間、それ以外は昼間。
    pub population: f64,
    /// Agoop 平日昼滞在人口 (人、月平均)。citycode 未解決なら 0。
    pub day_population: f64,
    /// Agoop 平日深夜滞在人口 (人、月平均。居住人口の代理)。
    pub night_population: f64,
    /// 昼夜比 = 昼 ÷ 夜 (夜が 0 なら 0)。
    pub day_night_ratio: f64,
    /// 昼夜比が `notes.tourist_threshold` を超えたか (観光地・繁華街補正の有無)。
    pub is_tourist_area: bool,
    /// 採用難度スコア (人口 1 万人あたり HW 求人件数)。人口 0 なら 0。
    pub score_per_10k: f64,
    /// 同条件の全国 HW 掲載求人件数 (件)。
    pub national_hw_count: i64,
    /// 該当エリア件数 ÷ 全国件数 (0〜1 の比率。全国 0 件なら 0)。
    pub area_share_of_national: f64,
}

/// Panel 1 の `notes`。
#[derive(Debug, Clone, Serialize, TS)]
pub struct RdDifficultyNotes {
    /// HW 掲載求人のみが対象である旨。
    pub hw_scope: String,
    /// 相関であって因果ではない旨。
    pub causation: String,
    /// スコアの計算式 (観光地補正の有無で文言が変わる)。
    pub calculation: String,
    /// Agoop 人流データの年 (西暦)。
    pub population_year: i32,
    /// 観光地判定の昼夜比閾値 (この値を超えると補正)。
    pub tourist_threshold: f64,
}

/// `GET /api/recruitment_diag/difficulty` の成功時の本体。
#[derive(Debug, Clone, Serialize, TS)]
pub struct RdDifficultyResponse {
    /// 常に `"difficulty_score"`。
    pub panel: String,
    pub inputs: RdDifficultyInputs,
    pub metrics: RdDifficultyMetrics,
    /// 難度ランク 0〜5 (0 はデータ不足、1 穴場 〜 5 超激戦)。
    pub rank: i32,
    /// ランクの表示名 (例: "激戦")。
    pub rank_label: String,
    /// 示唆文 (傾向の提示)。
    pub so_what: String,
    /// 観光地補正を適用したときの注記。非適用なら null。
    pub tourist_correction_note: Option<String>,
    pub notes: RdDifficultyNotes,
}

/// `GET /api/recruitment_diag/difficulty` の応答 (TS: `RdDifficultyResponse | RdErrorResponse`)。
#[derive(Debug, Clone, Serialize, TS)]
#[serde(untagged)]
pub enum RdDifficultyResult {
    Ok(RdDifficultyResponse),
    Err(RdErrorResponse),
}

/// `GET /api/recruitment_diag/difficulty`
///
/// 採用難度スコア = HW 該当求人件数 ÷ 昼間人口 × 10,000
/// （単位: 「昼間人口 1 万人あたり求人件数」。数値が高いほど激戦、低いほど穴場）
///
/// 全国同業種平均と比較し、rank を付与。
pub async fn api_difficulty_score(
    State(state): State<Arc<AppState>>,
    session: Session,
    Query(params): Query<DifficultyParams>,
) -> Json<RdDifficultyResult> {
    let filters = get_session_filters(&session).await;
    let pref = if params.prefecture.is_empty() {
        filters.prefecture.clone()
    } else {
        params.prefecture.clone()
    };
    let muni = if params.municipality.is_empty() {
        filters.municipality.clone()
    } else {
        params.municipality.clone()
    };

    let db = match &state.hw_db {
        Some(d) => d.clone(),
        None => {
            return Json(RdDifficultyResult::Err(RdErrorResponse::new(
                "hellowork.db 未接続",
            )));
        }
    };
    let turso = state.turso_db.clone();

    // citycode 解決: 明示パラメータ優先、なければ pref+muni から解決
    let citycode = params.citycode.or_else(|| {
        if !pref.is_empty() && !muni.is_empty() {
            city_code::city_name_to_code(&pref, &muni).map(|c| c as i64)
        } else {
            None
        }
    });

    let job_type = params.job_type.clone();
    let emp_types: Vec<&'static str> = expand_employment_type(&params.emp_type);

    let job_type_c = job_type.clone();
    let emp_types_c = emp_types.clone();
    let pref_c = pref.clone();
    let muni_c = muni.clone();

    let result = tokio::task::spawn_blocking(move || {
        // 1. HW件数（該当エリア）
        let hw_count = fetch::count_hw_postings(&db, &job_type_c, &emp_types_c, &pref_c, &muni_c);

        // 2. Agoop mesh1km 昼夜人口（F1 #3: 観光地補正のため両方取得）
        //    - day  : 平日昼滞在 (dayflag=1, timezone=0)
        //    - night: 平日深夜滞在 (dayflag=1, timezone=1) ≒ 居住人口の代理
        let (day_population, night_population) = if let Some(code) = citycode {
            let day =
                fetch::sum_mesh_population(&db, turso.as_ref(), code, DEFAULT_AGOOP_YEAR, 1, 0);
            let night =
                fetch::sum_mesh_population(&db, turso.as_ref(), code, DEFAULT_AGOOP_YEAR, 1, 1);
            (day, night)
        } else {
            (0.0, 0.0)
        };

        // 3. 全国同業種平均スコア
        let national_hw = fetch::count_hw_postings_national(&db, &job_type_c, &emp_types_c);

        (hw_count, day_population, night_population, national_hw)
    })
    .await
    .unwrap_or((0, 0.0, 0.0, 0));

    let (hw_count, day_population, night_population, national_hw) = result;

    Json(RdDifficultyResult::Ok(build_difficulty_response(
        RdDifficultyInputs {
            job_type,
            emp_type: params.emp_type,
            prefecture: pref,
            municipality: muni,
            citycode,
        },
        hw_count,
        day_population,
        night_population,
        national_hw,
    )))
}

/// Panel 1 の成功応答を組み立てる (DB 取得後の計算部分。純粋関数)。
fn build_difficulty_response(
    inputs: RdDifficultyInputs,
    hw_count: i64,
    day_population: f64,
    night_population: f64,
    national_hw: i64,
) -> RdDifficultyResponse {
    // F1 #3: 観光地・繁華街補正（compute_difficulty_score_with_tourist_correction を経由）
    //
    // 銀座・京都四条河原町などの繁華街は平日昼の外来滞在が膨張するため、
    // day 単独を分母にすると「穴場」と誤判定されやすい傾向がうかがえる。
    // 昼夜比が TOURIST_AREA_DAYNIGHT_RATIO (1.5) を超える場合は
    // 居住人口の代理である night を分母に採用し、実態的な採用難度を算出。
    //
    // 注意: 相関≠因果。本補正は「居住人口ベースで見ても激戦傾向」という
    //       傾向把握用であり、外来求職者を完全に排除する保証はない。
    let (score, population, day_night_ratio, is_tourist_area) =
        compute_difficulty_score_with_tourist_correction(
            hw_count,
            day_population,
            night_population,
        );

    // 全国平均スコア（近似: 全国総人口1.25億とする簡易値。より厳密には全国mesh合計が必要）
    // ただし国勢調査ベースではなく Agoop 滞在値のため、安易な国全体合計は double count 要注意。
    // 現段階では「全国 HW 件数」との相対比較のみ提示し、スコアの正規化値を `relative_vs_national` で返す。
    let relative_vs_national = if national_hw > 0 {
        (hw_count as f64) / (national_hw as f64)
    } else {
        0.0
    };

    // rank 判定（5段階、人口1万人あたり件数ベース）
    let (rank, rank_label, so_what) = classify_difficulty(score, hw_count, population);

    let calculation_note = if is_tourist_area {
        "score = (HW該当求人件数) ÷ (Agoop平日深夜滞在人口=居住人口代理) × 10,000 [F1 #3: 観光地補正適用、平日昼滞在は外来流入で膨張するため不採用]"
    } else {
        "score = (HW該当求人件数) ÷ (Agoop平日昼滞在人口) × 10,000"
    };
    let tourist_note = if is_tourist_area {
        Some(format!(
            "※観光地・繁華街判定（昼夜比 {:.2} > {:.1}）。昼間滞在膨張による『穴場』誤判定を避けるため、居住人口側で再算出した値です。",
            day_night_ratio, TOURIST_AREA_DAYNIGHT_RATIO
        ))
    } else {
        None
    };

    RdDifficultyResponse {
        panel: "difficulty_score".to_string(),
        inputs,
        metrics: RdDifficultyMetrics {
            hw_count,
            population,
            day_population,
            night_population,
            day_night_ratio,
            is_tourist_area,
            score_per_10k: score,
            national_hw_count: national_hw,
            area_share_of_national: relative_vs_national,
        },
        rank,
        rank_label: rank_label.to_string(),
        so_what,
        tourist_correction_note: tourist_note,
        notes: RdDifficultyNotes {
            hw_scope: HW_SCOPE_NOTE.to_string(),
            causation: CAUSATION_NOTE.to_string(),
            calculation: calculation_note.to_string(),
            population_year: DEFAULT_AGOOP_YEAR,
            tourist_threshold: TOURIST_AREA_DAYNIGHT_RATIO,
        },
    }
}

/// F1 #3: 観光地補正後の採用難度スコア計算（純粋関数、ユニットテスト用）
///
/// **戻り値**: `(score, population_used, day_night_ratio, is_tourist_area)`
///
/// **ロジック**:
/// 1. day_night_ratio = day / night (night=0 ならば 0.0)
/// 2. is_tourist_area = (night > 0 AND ratio > TOURIST_AREA_DAYNIGHT_RATIO)
/// 3. population = if is_tourist_area { night } else { day }
/// 4. score = if population > 0 { hw_count / population * 10_000 } else { 0 }
///
/// **注意**: 相関≠因果。本補正は「居住人口ベースで見ても激戦傾向」を示すための
/// 統計的傾向把握用であり、外来求職者の応募行動を完全に排除する保証はない。
pub(crate) fn compute_difficulty_score_with_tourist_correction(
    hw_count: i64,
    day_population: f64,
    night_population: f64,
) -> (f64, f64, f64, bool) {
    let day_night_ratio = if night_population > 0.0 {
        day_population / night_population
    } else {
        0.0
    };
    let is_tourist_area = night_population > 0.0 && day_night_ratio > TOURIST_AREA_DAYNIGHT_RATIO;
    let population = if is_tourist_area {
        night_population
    } else {
        day_population
    };
    let score = if population > 0.0 {
        (hw_count as f64) / population * 10_000.0
    } else {
        0.0
    };
    (score, population, day_night_ratio, is_tourist_area)
}

/// 採用難度の rank 分類 + So What 生成。
///
/// score = 人口1万人あたり求人件数。5段階で分類し、アクション提案まで返す。
fn classify_difficulty(score: f64, hw_count: i64, population: f64) -> (i32, &'static str, String) {
    if hw_count == 0 {
        return (
            0,
            "データ不足",
            "該当条件の HW 掲載求人がないため採用難度は算出不可。業種/雇用形態の条件を緩めて再検索を推奨。".to_string(),
        );
    }
    if population <= 0.0 {
        return (
            0,
            "人口データ不足",
            "Agoop 人流データが未投入の市区町村。HW 件数のみで参照してください。".to_string(),
        );
    }

    // score: 1万人あたりの求人件数
    // 経験則的な閾値（あくまで傾向。因果ではない）
    if score < 1.0 {
        (
            1,
            "穴場（競合ほぼなし）",
            format!(
                "同エリア内に類似求人が少なく（{:.1}件/万人）、応募獲得の競争圧は低い傾向。\
                 一方で人材供給も限定的な可能性があるため、広域圏からの通勤可否の検証を推奨。",
                score
            ),
        )
    } else if score < 3.0 {
        (
            2,
            "穏やか",
            format!(
                "1万人あたり {:.1} 件。競合は存在するが採用競争は過熱していない傾向。\
                 差別化条件（賞与・年休）を明確にすれば応募は獲得しやすい可能性。",
                score
            ),
        )
    } else if score < 7.0 {
        (
            3,
            "平均的",
            format!(
                "1万人あたり {:.1} 件と全国平均的な水準。標準的な給与水準＋広告接触回数で採用可能な傾向。",
                score
            ),
        )
    } else if score < 15.0 {
        (
            4,
            "激戦",
            format!(
                "1万人あたり {:.1} 件と競合密度が高い傾向。給与の上乗せまたは差別化要素（働き方／福利厚生）での\
                 訴求が必要。Panel 5 の条件ギャップ確認を推奨。",
                score
            ),
        )
    } else {
        (
            5,
            "超激戦",
            format!(
                "1万人あたり {:.1} 件と極めて高密度。採用単価・期間の大幅増加が見込まれる可能性。\
                 Panel 7 の穴場マップで隣接エリアへの射程拡大を強く推奨。",
                score
            ),
        )
    }
}

// ========== Panel 2: 人材プール診断 ==========

#[derive(Deserialize)]
pub struct TalentPoolParams {
    #[serde(default)]
    pub prefecture: String,
    #[serde(default)]
    pub municipality: String,
    #[serde(default)]
    pub citycode: Option<i64>,
    #[serde(default)]
    pub year: Option<i32>,
}

/// Panel 2 の応答で、解決後の入力条件を返す部分。
#[derive(Debug, Clone, Serialize, TS)]
pub struct RdTalentPoolInputs {
    /// 都道府県名 (クエリ優先、空ならセッションの値)。
    pub prefecture: String,
    /// 市区町村名 (クエリ優先、空ならセッションの値)。
    pub municipality: String,
    /// 市区町村コード (数値。成功時は必ずある)。
    pub citycode: i64,
    /// Agoop 人流データの年 (西暦。既定 2021)。
    pub year: i32,
}

/// Panel 2 の数値部分。
#[derive(Debug, Clone, Serialize, TS)]
pub struct RdTalentPoolMetrics {
    /// Agoop 平日昼滞在人口 (人、月平均)。
    pub day_population: f64,
    /// Agoop 平日深夜滞在人口 (人、月平均)。
    pub night_population: f64,
    /// 通勤流入 = 昼 − 夜 (人。負ならベッドタウン型の流出超過)。
    pub commuter_inflow: f64,
    /// 昼夜比 = 昼 ÷ 夜 (夜が 0 なら 0)。
    pub day_night_ratio: f64,
}

/// Panel 2 の `notes`。
#[derive(Debug, Clone, Serialize, TS)]
pub struct RdTalentPoolNotes {
    /// HW 掲載求人のみが対象である旨。
    pub hw_scope: String,
    /// 相関であって因果ではない旨。
    pub causation: String,
    /// データの出典。
    pub data_source: String,
    /// 集計方法。
    pub method: String,
}

/// `GET /api/recruitment_diag/talent_pool` の成功時の本体。
#[derive(Debug, Clone, Serialize, TS)]
pub struct RdTalentPoolResponse {
    /// 常に `"talent_pool"`。
    pub panel: String,
    pub inputs: RdTalentPoolInputs,
    pub metrics: RdTalentPoolMetrics,
    /// 示唆文 (流入超過型 / ベッドタウン型 / 均衡型 / データ未投入)。
    pub so_what: String,
    pub notes: RdTalentPoolNotes,
}

/// `GET /api/recruitment_diag/talent_pool` の応答 (TS: `RdTalentPoolResponse | RdErrorResponse`)。
#[derive(Debug, Clone, Serialize, TS)]
#[serde(untagged)]
pub enum RdTalentPoolResult {
    Ok(RdTalentPoolResponse),
    Err(RdErrorResponse),
}

/// `GET /api/recruitment_diag/talent_pool`
///
/// Agoop mesh1km で該当 citycode の昼夜人口を集計。
/// - 昼人口: dayflag=1, timezone=0 (平日昼)
/// - 夜人口: dayflag=1, timezone=1 (平日深夜)
/// - 通勤流入: 昼 - 夜
pub async fn api_talent_pool(
    State(state): State<Arc<AppState>>,
    session: Session,
    Query(params): Query<TalentPoolParams>,
) -> Json<RdTalentPoolResult> {
    let filters = get_session_filters(&session).await;
    let pref = if params.prefecture.is_empty() {
        filters.prefecture.clone()
    } else {
        params.prefecture.clone()
    };
    let muni = if params.municipality.is_empty() {
        filters.municipality.clone()
    } else {
        params.municipality.clone()
    };

    let citycode = params.citycode.or_else(|| {
        if !pref.is_empty() && !muni.is_empty() {
            city_code::city_name_to_code(&pref, &muni).map(|c| c as i64)
        } else {
            None
        }
    });

    let code = match citycode {
        Some(c) => c,
        None => {
            return Json(RdTalentPoolResult::Err(RdErrorResponse::new(
                "市区町村が指定されていません (citycode or prefecture+municipality が必要)",
            )));
        }
    };

    let year = params.year.unwrap_or(DEFAULT_AGOOP_YEAR);

    let db = match &state.hw_db {
        Some(d) => d.clone(),
        None => {
            return Json(RdTalentPoolResult::Err(RdErrorResponse::new(
                "hellowork.db 未接続",
            )))
        }
    };
    let turso = state.turso_db.clone();

    let (day, night) = tokio::task::spawn_blocking(move || {
        let day = fetch::sum_mesh_population(&db, turso.as_ref(), code, year, 1, 0);
        let night = fetch::sum_mesh_population(&db, turso.as_ref(), code, year, 1, 1);
        (day, night)
    })
    .await
    .unwrap_or((0.0, 0.0));

    Json(RdTalentPoolResult::Ok(build_talent_pool_response(
        RdTalentPoolInputs {
            prefecture: pref,
            municipality: muni,
            citycode: code,
            year,
        },
        day,
        night,
    )))
}

/// Panel 2 の成功応答を組み立てる (DB 取得後の計算部分。純粋関数)。
fn build_talent_pool_response(
    inputs: RdTalentPoolInputs,
    day: f64,
    night: f64,
) -> RdTalentPoolResponse {
    let commuter_inflow = day - night;
    let day_night_ratio = if night > 0.0 { day / night } else { 0.0 };

    let so_what = build_talent_pool_so_what(day, night, commuter_inflow, day_night_ratio);

    RdTalentPoolResponse {
        panel: "talent_pool".to_string(),
        inputs,
        metrics: RdTalentPoolMetrics {
            day_population: day,
            night_population: night,
            commuter_inflow,
            day_night_ratio,
        },
        so_what,
        notes: RdTalentPoolNotes {
            hw_scope: HW_SCOPE_NOTE.to_string(),
            causation: CAUSATION_NOTE.to_string(),
            data_source: "国土交通省 全国の人流オープンデータ（Agoop社提供）mesh1km".to_string(),
            method: "平日 昼(timezone=0)・深夜(timezone=1) を SUM / 月数で月平均化。集計値(2) は double count 防止のため不使用。".to_string(),
        },
    }
}

fn build_talent_pool_so_what(day: f64, night: f64, inflow: f64, ratio: f64) -> String {
    if day <= 0.0 && night <= 0.0 {
        return "Agoop データが未投入の市区町村。別エリアで再試行してください。".to_string();
    }
    if night <= 0.0 {
        return "夜間人口データが取得不可。昼人口のみで採用圏を検討してください。".to_string();
    }
    if ratio > 1.2 {
        format!(
            "昼夜比 {:.2} の流入超過型（通勤流入 {:.0} 人）。他市区町村から働きに来る構造のため、\
             求人訴求は『通勤利便性』『勤務時間』を前面に出すと反応が高まる傾向。",
            ratio, inflow
        )
    } else if ratio < 0.85 {
        format!(
            "昼夜比 {:.2} のベッドタウン型（流出超過 {:.0} 人）。居住者の多くは外へ通勤しているため、\
             『自宅近接』『短時間シフト』訴求で潜在層の発掘余地がある可能性。",
            ratio, inflow
        )
    } else {
        format!(
            "昼夜比 {:.2} の均衡型。居住者＝就業者が重なるエリアで、\
             地域密着の求人訴求（『地元で働く』）が機能しやすい傾向。",
            ratio
        )
    }
}

// ========== Panel 3: 流入元分析 ==========

#[derive(Deserialize)]
pub struct InflowParams {
    #[serde(default)]
    pub prefecture: String,
    #[serde(default)]
    pub municipality: String,
    #[serde(default)]
    pub citycode: Option<i64>,
    #[serde(default)]
    pub year: Option<i32>,
}

/// Panel 3 の応答で、解決後の入力条件を返す部分。
#[derive(Debug, Clone, Serialize, TS)]
pub struct RdInflowInputs {
    /// 都道府県名 (クエリ優先、空ならセッションの値)。
    pub prefecture: String,
    /// 市区町村名 (クエリ優先、空ならセッションの値)。
    pub municipality: String,
    /// 市区町村コード (数値。成功時は必ずある)。
    pub citycode: i64,
    /// Agoop 人流データの年 (西暦。既定 2021)。
    pub year: i32,
}

/// Panel 3 の流入元 1 区分。
#[derive(Debug, Clone, Serialize, TS)]
pub struct RdInflowBreakdownItem {
    /// 流入元区分 (0 同一市区町村 / 1 同県別市 / 2 同地方別県 / 3 異地方)。
    pub from_area: i64,
    /// 区分の正式名 (例: "同一市区町村")。
    pub area_name: String,
    /// 区分の短縮名 (例: "同市区町村")。
    pub short_name: String,
    /// 流入人口 (人、平日昼の年合計)。
    pub population: f64,
    /// 全流入に占める割合 (0〜1 の比率。合計 0 なら 0)。
    pub share: f64,
}

/// Panel 3 の `notes`。
#[derive(Debug, Clone, Serialize, TS)]
pub struct RdInflowNotes {
    /// HW 掲載求人のみが対象である旨。
    pub hw_scope: String,
    /// 相関であって因果ではない旨。
    pub causation: String,
    /// データの出典。
    pub data_source: String,
    /// 集計方法。
    pub method: String,
}

/// `GET /api/recruitment_diag/inflow` の成功時の本体。
#[derive(Debug, Clone, Serialize, TS)]
pub struct RdInflowResponse {
    /// 常に `"inflow_analysis"`。
    pub panel: String,
    pub inputs: RdInflowInputs,
    /// 流入元区分ごとの内訳 (取得できた区分のみ。0〜4 件)。
    pub breakdown: Vec<RdInflowBreakdownItem>,
    /// 流入人口の合計 (人)。内訳 0 件のときは -0.0 になる (f64 の空和)。
    pub total_population: f64,
    /// 示唆文 (傾向の提示)。
    pub so_what: String,
    /// `v2_flow_fromto_city` が約 83% のみ投入済みである旨の警告。
    pub data_warning: String,
    pub notes: RdInflowNotes,
}

/// `GET /api/recruitment_diag/inflow` の応答 (TS: `RdInflowResponse | RdErrorResponse`)。
#[derive(Debug, Clone, Serialize, TS)]
#[serde(untagged)]
pub enum RdInflowResult {
    Ok(RdInflowResponse),
    Err(RdErrorResponse),
}

/// `GET /api/recruitment_diag/inflow`
///
/// `v2_flow_fromto_city` から from_area 4区分 (同市/同県別市/同地方別県/異地方) の流入量を取得。
/// 83% 部分データ警告必須（MEMORY 遵守）。
pub async fn api_inflow_analysis(
    State(state): State<Arc<AppState>>,
    session: Session,
    Query(params): Query<InflowParams>,
) -> Json<RdInflowResult> {
    let filters = get_session_filters(&session).await;
    let pref = if params.prefecture.is_empty() {
        filters.prefecture.clone()
    } else {
        params.prefecture.clone()
    };
    let muni = if params.municipality.is_empty() {
        filters.municipality.clone()
    } else {
        params.municipality.clone()
    };

    let citycode = params.citycode.or_else(|| {
        if !pref.is_empty() && !muni.is_empty() {
            city_code::city_name_to_code(&pref, &muni).map(|c| c as i64)
        } else {
            None
        }
    });

    let code = match citycode {
        Some(c) => c,
        None => {
            return Json(RdInflowResult::Err(RdErrorResponse::new(
                "市区町村が指定されていません (citycode or prefecture+municipality が必要)",
            )));
        }
    };

    let year = params.year.unwrap_or(DEFAULT_AGOOP_YEAR);

    let db = match &state.hw_db {
        Some(d) => d.clone(),
        None => {
            return Json(RdInflowResult::Err(RdErrorResponse::new(
                "hellowork.db 未接続",
            )))
        }
    };
    let turso = state.turso_db.clone();

    let rows = tokio::task::spawn_blocking(move || {
        fetch::fetch_inflow_breakdown_rows(&db, turso.as_ref(), code, year)
    })
    .await
    .unwrap_or_default();

    Json(RdInflowResult::Ok(build_inflow_response(
        RdInflowInputs {
            prefecture: pref,
            municipality: muni,
            citycode: code,
            year,
        },
        &rows,
    )))
}

/// Panel 3 の成功応答を組み立てる (DB 取得後の計算部分。純粋関数)。
fn build_inflow_response(inputs: RdInflowInputs, rows: &[Row]) -> RdInflowResponse {
    let total: f64 = rows
        .iter()
        .map(|r| crate::handlers::helpers::get_f64(r, "total_population"))
        .sum();

    let mut breakdown: Vec<RdInflowBreakdownItem> = Vec::with_capacity(4);
    for r in rows {
        let from_area = crate::handlers::helpers::get_i64(r, "from_area");
        let pop = crate::handlers::helpers::get_f64(r, "total_population");
        let share = if total > 0.0 { pop / total } else { 0.0 };
        breakdown.push(RdInflowBreakdownItem {
            from_area,
            area_name: fft::from_area_label(from_area).to_string(),
            short_name: fft::from_area_short_label(from_area).to_string(),
            population: pop,
            share,
        });
    }

    let so_what = build_inflow_so_what(&breakdown, total);

    RdInflowResponse {
        panel: "inflow_analysis".to_string(),
        inputs,
        breakdown,
        total_population: total,
        so_what,
        data_warning: INFLOW_DATA_WARNING.to_string(),
        notes: RdInflowNotes {
            hw_scope: HW_SCOPE_NOTE.to_string(),
            causation: CAUSATION_NOTE.to_string(),
            data_source: "国土交通省 全国の人流オープンデータ（Agoop社提供）v2_flow_fromto_city"
                .to_string(),
            method: "平日昼(dayflag=1, timezone=0) の年合計。from_area は 4 区分（Agoop 地方ブロック粒度）。".to_string(),
        },
    }
}

/// 指定区分 (`from_area`) の最初の要素の share。無ければ 0。
///
/// 旧実装は `json!` 化した値から `as_f64()` で読んでいたため、非有限値 (NaN/inf) は
/// JSON の null になり 0 扱いだった。その挙動を保つため非有限値も 0 にする。
fn inflow_share_of(breakdown: &[RdInflowBreakdownItem], from_area: i64) -> f64 {
    breakdown
        .iter()
        .find(|b| b.from_area == from_area)
        .map(|b| b.share)
        .filter(|s| s.is_finite())
        .unwrap_or(0.0)
}

fn build_inflow_so_what(breakdown: &[RdInflowBreakdownItem], total: f64) -> String {
    if total <= 0.0 {
        return "該当市区町村の流入データが未投入（83% 投入済のうち残り 17% に該当する可能性）。\
                別エリアで再試行してください。"
            .to_string();
    }

    // 自市区町村率
    let same_city = inflow_share_of(breakdown, 0);

    // 異地方率
    let far_area = inflow_share_of(breakdown, 3);

    if same_city > 0.8 {
        format!(
            "流入の {:.0}% が同市区町村内。採用は地域限定求人で完結する可能性が高い傾向。\
             広域求人媒体への出稿は費用対効果が低下する見込み。",
            same_city * 100.0
        )
    } else if far_area > 0.15 {
        format!(
            "異地方からの流入が {:.0}%。広域採用ポテンシャルがあるエリアのため、\
             全国媒体・引越し支援の訴求で応募母集団を広げられる可能性。",
            far_area * 100.0
        )
    } else {
        format!(
            "同市区町村 {:.0}% + 同県別市 {:.0}% が中心。通勤圏内採用が主軸となる傾向。\
             県内他市区町村からの通勤訴求を推奨。",
            same_city * 100.0,
            inflow_share_of(breakdown, 1) * 100.0
        )
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use serde_json::{json, Value};

    /// テスト用の inflow 内訳要素 (area_name 等は so_what に使わないので空)。
    fn item(from_area: i64, share: f64, population: f64) -> RdInflowBreakdownItem {
        RdInflowBreakdownItem {
            from_area,
            area_name: String::new(),
            short_name: String::new(),
            population,
            share,
        }
    }

    #[test]
    fn classify_difficulty_no_data() {
        let (rank, label, _) = classify_difficulty(0.0, 0, 0.0);
        assert_eq!(rank, 0);
        assert_eq!(label, "データ不足");
    }

    #[test]
    fn classify_difficulty_no_population() {
        let (rank, label, _) = classify_difficulty(0.0, 10, 0.0);
        assert_eq!(rank, 0);
        assert_eq!(label, "人口データ不足");
    }

    #[test]
    fn classify_difficulty_anaba() {
        // score=0.5 (< 1.0), hw=5, pop=100k → 穴場 rank 1
        let (rank, label, msg) = classify_difficulty(0.5, 5, 100_000.0);
        assert_eq!(rank, 1);
        assert_eq!(label, "穴場（競合ほぼなし）");
        assert!(msg.contains("競争圧は低い"));
    }

    #[test]
    fn classify_difficulty_mild() {
        let (rank, _, _) = classify_difficulty(2.0, 20, 100_000.0);
        assert_eq!(rank, 2);
    }

    #[test]
    fn classify_difficulty_average() {
        let (rank, _, _) = classify_difficulty(5.0, 50, 100_000.0);
        assert_eq!(rank, 3);
    }

    #[test]
    fn classify_difficulty_heavy() {
        let (rank, label, _) = classify_difficulty(10.0, 100, 100_000.0);
        assert_eq!(rank, 4);
        assert_eq!(label, "激戦");
    }

    #[test]
    fn classify_difficulty_super_heavy() {
        let (rank, label, msg) = classify_difficulty(20.0, 200, 100_000.0);
        assert_eq!(rank, 5);
        assert_eq!(label, "超激戦");
        // アクション提案を含む
        assert!(msg.contains("穴場マップ"));
    }

    #[test]
    fn talent_pool_so_what_inflow_type() {
        // 昼 12000 / 夜 10000 = 比 1.2（境界） → 1.2 は 1.2 "> 1.2" で false なので均衡型
        let s = build_talent_pool_so_what(12000.0, 10000.0, 2000.0, 1.2);
        assert!(s.contains("均衡型"));
        // 1.3 は流入超過型
        let s = build_talent_pool_so_what(13000.0, 10000.0, 3000.0, 1.3);
        assert!(s.contains("流入超過型"));
    }

    #[test]
    fn talent_pool_so_what_bedtown() {
        let s = build_talent_pool_so_what(8000.0, 10000.0, -2000.0, 0.8);
        assert!(s.contains("ベッドタウン型"));
    }

    #[test]
    fn talent_pool_so_what_no_data() {
        let s = build_talent_pool_so_what(0.0, 0.0, 0.0, 0.0);
        assert!(s.contains("未投入"));
    }

    #[test]
    fn inflow_so_what_no_data() {
        let s = build_inflow_so_what(&[], 0.0);
        assert!(s.contains("未投入"));
    }

    #[test]
    fn inflow_so_what_local_dominant() {
        // from_area=0 が 90% を占める（同市区町村内完結型）
        let breakdown = vec![
            item(0, 0.9, 9000.0),
            item(1, 0.05, 500.0),
            item(2, 0.03, 300.0),
            item(3, 0.02, 200.0),
        ];
        let s = build_inflow_so_what(&breakdown, 10000.0);
        assert!(s.contains("地域限定"));
        assert!(s.contains("90%"));
    }

    #[test]
    fn inflow_so_what_wide_area() {
        // from_area=3 が 20% → 異地方流入の広域型
        let breakdown = vec![
            item(0, 0.4, 4000.0),
            item(1, 0.2, 2000.0),
            item(2, 0.2, 2000.0),
            item(3, 0.2, 2000.0),
        ];
        let s = build_inflow_so_what(&breakdown, 10000.0);
        assert!(s.contains("広域採用ポテンシャル"));
    }

    // ========================================================================
    // Phase 1A-1: struct 置き換え前の json!() との等価テスト
    // 旧 json!() 式はそのまま `legacy_*` に残し、同じ入力の出力と文字列一致を見る。
    // ========================================================================

    /// 置き換え前の `error_body` (式はそのまま残す)
    fn legacy_error_body(msg: &str) -> Value {
        json!({
            "error": msg,
            "notes": {
                "hw_scope": HW_SCOPE_NOTE,
                "causation": CAUSATION_NOTE,
            },
        })
    }

    /// 置き換え前の Panel 1 成功時の組み立て (json!() 式はそのまま)
    #[allow(clippy::too_many_arguments)]
    fn legacy_difficulty_json(
        job_type: String,
        emp_type: String,
        pref: String,
        muni: String,
        citycode: Option<i64>,
        hw_count: i64,
        day_population: f64,
        night_population: f64,
        national_hw: i64,
    ) -> Value {
        let (score, population, day_night_ratio, is_tourist_area) =
            compute_difficulty_score_with_tourist_correction(
                hw_count,
                day_population,
                night_population,
            );
        let relative_vs_national = if national_hw > 0 {
            (hw_count as f64) / (national_hw as f64)
        } else {
            0.0
        };
        let (rank, rank_label, so_what) = classify_difficulty(score, hw_count, population);
        let calculation_note = if is_tourist_area {
            "score = (HW該当求人件数) ÷ (Agoop平日深夜滞在人口=居住人口代理) × 10,000 [F1 #3: 観光地補正適用、平日昼滞在は外来流入で膨張するため不採用]"
        } else {
            "score = (HW該当求人件数) ÷ (Agoop平日昼滞在人口) × 10,000"
        };
        let tourist_note = if is_tourist_area {
            Some(format!(
                "※観光地・繁華街判定（昼夜比 {:.2} > {:.1}）。昼間滞在膨張による『穴場』誤判定を避けるため、居住人口側で再算出した値です。",
                day_night_ratio, TOURIST_AREA_DAYNIGHT_RATIO
            ))
        } else {
            None
        };
        json!({
            "panel": "difficulty_score",
            "inputs": {
                "job_type": job_type,
                "emp_type": emp_type,
                "prefecture": pref,
                "municipality": muni,
                "citycode": citycode,
            },
            "metrics": {
                "hw_count": hw_count,
                "population": population,
                "day_population": day_population,
                "night_population": night_population,
                "day_night_ratio": day_night_ratio,
                "is_tourist_area": is_tourist_area,
                "score_per_10k": score,
                "national_hw_count": national_hw,
                "area_share_of_national": relative_vs_national,
            },
            "rank": rank,
            "rank_label": rank_label,
            "so_what": so_what,
            "tourist_correction_note": tourist_note,
            "notes": {
                "hw_scope": HW_SCOPE_NOTE,
                "causation": CAUSATION_NOTE,
                "calculation": calculation_note,
                "population_year": DEFAULT_AGOOP_YEAR,
                "tourist_threshold": TOURIST_AREA_DAYNIGHT_RATIO,
            },
        })
    }

    /// 置き換え前の Panel 2 成功時の組み立て
    fn legacy_talent_pool_json(
        pref: String,
        muni: String,
        code: i64,
        year: i32,
        day: f64,
        night: f64,
    ) -> Value {
        let commuter_inflow = day - night;
        let day_night_ratio = if night > 0.0 { day / night } else { 0.0 };
        let so_what = build_talent_pool_so_what(day, night, commuter_inflow, day_night_ratio);
        json!({
            "panel": "talent_pool",
            "inputs": {
                "prefecture": pref,
                "municipality": muni,
                "citycode": code,
                "year": year,
            },
            "metrics": {
                "day_population": day,
                "night_population": night,
                "commuter_inflow": commuter_inflow,
                "day_night_ratio": day_night_ratio,
            },
            "so_what": so_what,
            "notes": {
                "hw_scope": HW_SCOPE_NOTE,
                "causation": CAUSATION_NOTE,
                "data_source": "国土交通省 全国の人流オープンデータ（Agoop社提供）mesh1km",
                "method": "平日 昼(timezone=0)・深夜(timezone=1) を SUM / 月数で月平均化。集計値(2) は double count 防止のため不使用。",
            },
        })
    }

    /// 置き換え前の Panel 3 の so_what (Value 走査版。式はそのまま)
    fn legacy_inflow_so_what(breakdown: &[Value], total: f64) -> String {
        if total <= 0.0 {
            return "該当市区町村の流入データが未投入（83% 投入済のうち残り 17% に該当する可能性）。\
                    別エリアで再試行してください。"
                .to_string();
        }
        let same_city = breakdown
            .iter()
            .find(|v| v["from_area"].as_i64() == Some(0))
            .and_then(|v| v["share"].as_f64())
            .unwrap_or(0.0);
        let far_area = breakdown
            .iter()
            .find(|v| v["from_area"].as_i64() == Some(3))
            .and_then(|v| v["share"].as_f64())
            .unwrap_or(0.0);
        if same_city > 0.8 {
            format!(
                "流入の {:.0}% が同市区町村内。採用は地域限定求人で完結する可能性が高い傾向。\
                 広域求人媒体への出稿は費用対効果が低下する見込み。",
                same_city * 100.0
            )
        } else if far_area > 0.15 {
            format!(
                "異地方からの流入が {:.0}%。広域採用ポテンシャルがあるエリアのため、\
                 全国媒体・引越し支援の訴求で応募母集団を広げられる可能性。",
                far_area * 100.0
            )
        } else {
            format!(
                "同市区町村 {:.0}% + 同県別市 {:.0}% が中心。通勤圏内採用が主軸となる傾向。\
                 県内他市区町村からの通勤訴求を推奨。",
                same_city * 100.0,
                breakdown
                    .iter()
                    .find(|v| v["from_area"].as_i64() == Some(1))
                    .and_then(|v| v["share"].as_f64())
                    .unwrap_or(0.0)
                    * 100.0
            )
        }
    }

    /// 置き換え前の Panel 3 成功時の組み立て
    fn legacy_inflow_json(pref: String, muni: String, code: i64, year: i32, rows: &[Row]) -> Value {
        let total: f64 = rows
            .iter()
            .map(|r| crate::handlers::helpers::get_f64(r, "total_population"))
            .sum();
        let mut breakdown: Vec<Value> = Vec::with_capacity(4);
        for r in rows {
            let from_area = crate::handlers::helpers::get_i64(r, "from_area");
            let pop = crate::handlers::helpers::get_f64(r, "total_population");
            let share = if total > 0.0 { pop / total } else { 0.0 };
            breakdown.push(json!({
                "from_area": from_area,
                "area_name": fft::from_area_label(from_area),
                "short_name": fft::from_area_short_label(from_area),
                "population": pop,
                "share": share,
            }));
        }
        let so_what = legacy_inflow_so_what(&breakdown, total);
        json!({
            "panel": "inflow_analysis",
            "inputs": {
                "prefecture": pref,
                "municipality": muni,
                "citycode": code,
                "year": year,
            },
            "breakdown": breakdown,
            "total_population": total,
            "so_what": so_what,
            "data_warning": INFLOW_DATA_WARNING,
            "notes": {
                "hw_scope": HW_SCOPE_NOTE,
                "causation": CAUSATION_NOTE,
                "data_source": "国土交通省 全国の人流オープンデータ（Agoop社提供）v2_flow_fromto_city",
                "method": "平日昼(dayflag=1, timezone=0) の年合計。from_area は 4 区分（Agoop 地方ブロック粒度）。",
            },
        })
    }

    fn s<T: Serialize>(v: &T) -> String {
        serde_json::to_string(v).unwrap()
    }

    #[test]
    fn error_response_matches_legacy_json() {
        for msg in ["hellowork.db 未接続", "", "市区町村が指定されていません"] {
            assert_eq!(
                s(&RdDifficultyResult::Err(RdErrorResponse::new(msg))),
                s(&legacy_error_body(msg))
            );
        }
    }

    #[test]
    fn difficulty_response_matches_legacy_json() {
        // (hw, day, night, national, citycode): 通常 / 観光地補正 / 人口 0 / 件数 0 / 小数 / 夜のみ
        let cases: Vec<(i64, f64, f64, i64, Option<i64>)> = vec![
            (50, 100_000.0, 90_000.0, 5000, Some(3201)),
            (20, 30_000.0, 10_000.0, 100, Some(13101)),
            (10, 0.0, 0.0, 0, None),
            (0, 12_345.5, 6_000.25, 10, Some(1100)),
            (7, 0.0, 8_000.0, 3, Some(2201)),
            (1, 250_000.0, 0.0, 999_999, None),
            (3, 1234.567, 1234.5, 7, Some(4202)),
        ];
        for (i, (hw, day, night, nat, code)) in cases.into_iter().enumerate() {
            let (pref, muni) = if i % 2 == 0 {
                ("岩手県", "盛岡市")
            } else {
                ("", "")
            };
            let new = RdDifficultyResult::Ok(build_difficulty_response(
                RdDifficultyInputs {
                    job_type: "飲食業".to_string(),
                    emp_type: String::new(),
                    prefecture: pref.to_string(),
                    municipality: muni.to_string(),
                    citycode: code,
                },
                hw,
                day,
                night,
                nat,
            ));
            let legacy = legacy_difficulty_json(
                "飲食業".to_string(),
                String::new(),
                pref.to_string(),
                muni.to_string(),
                code,
                hw,
                day,
                night,
                nat,
            );
            assert_eq!(s(&new), s(&legacy), "case {i}");
        }
    }

    #[test]
    fn talent_pool_response_matches_legacy_json() {
        let cases: Vec<(f64, f64)> = vec![
            (13_000.0, 10_000.0),
            (8_000.0, 10_000.0),
            (10_000.0, 10_000.0),
            (0.0, 0.0),
            (5_000.5, 0.0),
            (0.0, 4_000.25),
        ];
        for (i, (day, night)) in cases.into_iter().enumerate() {
            let new = RdTalentPoolResult::Ok(build_talent_pool_response(
                RdTalentPoolInputs {
                    prefecture: "東京都".to_string(),
                    municipality: String::new(),
                    citycode: 13101,
                    year: 2021,
                },
                day,
                night,
            ));
            let legacy = legacy_talent_pool_json(
                "東京都".to_string(),
                String::new(),
                13101,
                2021,
                day,
                night,
            );
            assert_eq!(s(&new), s(&legacy), "case {i}");
        }
    }

    fn inflow_row(from_area: i64, pop: f64) -> Row {
        let mut r = Row::new();
        r.insert("from_area".to_string(), json!(from_area));
        r.insert("total_population".to_string(), json!(pop));
        r
    }

    #[test]
    fn inflow_response_matches_legacy_json() {
        let cases: Vec<Vec<Row>> = vec![
            vec![],
            vec![inflow_row(0, 0.0)],
            vec![
                inflow_row(0, 9000.0),
                inflow_row(1, 500.0),
                inflow_row(2, 300.0),
                inflow_row(3, 200.0),
            ],
            vec![
                inflow_row(0, 4000.0),
                inflow_row(1, 2000.0),
                inflow_row(2, 2000.0),
                inflow_row(3, 2000.0),
            ],
            vec![inflow_row(1, 333.3), inflow_row(2, 100.1)],
            vec![inflow_row(0, 1.0), inflow_row(3, 2.0), inflow_row(7, 3.0)],
        ];
        for (i, rows) in cases.iter().enumerate() {
            let new = RdInflowResult::Ok(build_inflow_response(
                RdInflowInputs {
                    prefecture: "岩手県".to_string(),
                    municipality: "盛岡市".to_string(),
                    citycode: 3201,
                    year: 2021,
                },
                rows,
            ));
            let legacy =
                legacy_inflow_json("岩手県".to_string(), "盛岡市".to_string(), 3201, 2021, rows);
            assert_eq!(s(&new), s(&legacy), "case {i}");
        }
    }

    #[test]
    fn inflow_so_what_matches_legacy_on_value_breakdown() {
        let shares: Vec<Vec<(i64, f64)>> = vec![
            vec![(0, 0.9), (1, 0.05), (2, 0.03), (3, 0.02)],
            vec![(0, 0.4), (1, 0.2), (2, 0.2), (3, 0.2)],
            vec![(1, 0.5), (2, 0.5)],
            vec![(0, 0.8), (3, 0.15)],
            vec![],
        ];
        for sh in shares {
            let items: Vec<RdInflowBreakdownItem> =
                sh.iter().map(|(a, sh)| item(*a, *sh, 100.0)).collect();
            let values: Vec<Value> = sh
                .iter()
                .map(|(a, sh)| json!({"from_area": a, "share": sh, "population": 100.0}))
                .collect();
            for total in [0.0, 100.0] {
                assert_eq!(
                    build_inflow_so_what(&items, total),
                    legacy_inflow_so_what(&values, total)
                );
            }
        }
    }

    #[test]
    fn ts_decls_have_main_fields() {
        let cfg = crate::handlers::recruitment_diag::types::ts_config();
        let d = RdDifficultyResult::decl(&cfg);
        assert!(d.contains("RdDifficultyResponse | RdErrorResponse"), "{d}");
        let d = RdDifficultyResponse::decl(&cfg);
        assert!(d.contains("rank: number"), "{d}");
        assert!(d.contains("tourist_correction_note: string | null"), "{d}");
        let d = RdDifficultyInputs::decl(&cfg);
        assert!(d.contains("citycode: number | null"), "{d}");
        let d = RdDifficultyMetrics::decl(&cfg);
        assert!(d.contains("hw_count: number"), "{d}");
        assert!(d.contains("is_tourist_area: boolean"), "{d}");
        let d = RdTalentPoolResult::decl(&cfg);
        assert!(d.contains("RdTalentPoolResponse | RdErrorResponse"), "{d}");
        let d = RdTalentPoolMetrics::decl(&cfg);
        assert!(d.contains("commuter_inflow: number"), "{d}");
        let d = RdInflowResult::decl(&cfg);
        assert!(d.contains("RdInflowResponse | RdErrorResponse"), "{d}");
        let d = RdInflowResponse::decl(&cfg);
        assert!(d.contains("breakdown: Array<RdInflowBreakdownItem>"), "{d}");
        assert!(d.contains("total_population: number"), "{d}");
        let d = RdInflowBreakdownItem::decl(&cfg);
        assert!(d.contains("from_area: number"), "{d}");
        assert!(d.contains("share: number"), "{d}");
    }

    #[test]
    fn error_body_contains_notes() {
        let b = serde_json::to_value(RdErrorResponse::new("test")).unwrap();
        assert_eq!(b["error"], "test");
        assert!(b["notes"]["hw_scope"].as_str().unwrap().contains("HW"));
    }

    // ========================================================================
    // F1 #3: Panel 1 観光地補正 逆証明テスト群（2026-04-26）
    // memory `feedback_reverse_proof_tests.md` 準拠で修正前/修正後の具体値を assert する。
    // ========================================================================

    /// **F1 #3-1**: TOURIST_AREA_DAYNIGHT_RATIO 定数の値が 1.5 であること
    #[test]
    fn f1_panel1_tourist_threshold_constant_is_1_5() {
        assert!(
            (TOURIST_AREA_DAYNIGHT_RATIO - 1.5).abs() < 1e-9,
            "F1 #3: 観光地閾値は 1.5 (build_talent_pool 流入超過 1.2 より厳しい)"
        );
    }

    /// **F1 #3-2**: 銀座的な観光地（昼夜比 3.0）でスコアが補正される
    ///
    /// シナリオ: hw_count=20, day=30,000人, night=10,000人 (昼夜比 3.0)
    /// - 修正前: score = 20 / 30000 * 10000 = 6.67 → rank 3「平均的」(穴場誤判定)
    /// - 修正後: score = 20 / 10000 * 10000 = 20.00 → rank 5「超激戦」(実態反映)
    #[test]
    fn f1_panel1_tourist_correction_ginza_like_increases_score() {
        let hw_count: i64 = 20;
        let day = 30_000.0_f64;
        let night = 10_000.0_f64;

        let (score, population, ratio, is_tourist) =
            compute_difficulty_score_with_tourist_correction(hw_count, day, night);

        // 昼夜比 3.0 > 1.5 → 観光地判定
        assert!(is_tourist, "昼夜比 3.0 は観光地判定されること");
        assert!((ratio - 3.0).abs() < 1e-9, "ratio は 3.0");
        // 補正後 population は night
        assert!(
            (population - night).abs() < 1e-9,
            "補正後分母は night={}",
            night
        );
        // 補正後 score = 20 / 10000 * 10000 = 20.00
        assert!(
            (score - 20.0).abs() < 1e-6,
            "F1 #3 補正後 score = 20.00 (修正前 6.67 から大幅上昇)"
        );

        // 補正後の rank は 5「超激戦」
        let (rank, label, _) = classify_difficulty(score, hw_count, population);
        assert_eq!(rank, 5);
        assert_eq!(label, "超激戦");

        // 修正前 (補正なし、day を分母にした場合) の参考値
        let pre_correction_score = (hw_count as f64) / day * 10_000.0;
        assert!(
            (pre_correction_score - 6.667).abs() < 0.01,
            "修正前 score ≈ 6.67 ('平均的' 誤判定)"
        );
    }

    /// **F1 #3-3**: 京都四条河原町的な観光地（昼夜比 2.0）でも補正発動
    ///
    /// シナリオ: hw_count=10, day=20,000人, night=10,000人 (昼夜比 2.0)
    /// - 修正前: score = 10 / 20000 * 10000 = 5.00 → rank 3 「平均的」
    /// - 修正後: score = 10 / 10000 * 10000 = 10.00 → rank 4 「激戦」
    #[test]
    fn f1_panel1_tourist_correction_kyoto_like_changes_rank() {
        let (score, _pop, ratio, is_tourist) =
            compute_difficulty_score_with_tourist_correction(10, 20_000.0, 10_000.0);
        assert!(is_tourist);
        assert!((ratio - 2.0).abs() < 1e-9);
        assert!((score - 10.0).abs() < 1e-6);
        let (rank, label, _) = classify_difficulty(score, 10, 10_000.0);
        assert_eq!(rank, 4);
        assert_eq!(label, "激戦");
    }

    /// **F1 #3-4**: 通常エリア（昼夜比 1.2）では補正発動しない
    ///
    /// シナリオ: hw_count=5, day=12,000人, night=10,000人 (昼夜比 1.2)
    /// 1.2 <= 1.5 → 観光地ではない → day を分母にする
    #[test]
    fn f1_panel1_no_tourist_correction_for_normal_area() {
        let (score, population, ratio, is_tourist) =
            compute_difficulty_score_with_tourist_correction(5, 12_000.0, 10_000.0);
        assert!(!is_tourist, "昼夜比 1.2 は観光地判定されない");
        assert!((ratio - 1.2).abs() < 1e-9);
        // population は day (補正なし)
        assert!((population - 12_000.0).abs() < 1e-9);
        // score = 5 / 12000 * 10000 = 4.166...
        assert!((score - 4.1667).abs() < 0.01);
    }

    /// **F1 #3-5**: 境界値検証（昼夜比 = 1.5 ちょうど）
    ///
    /// 1.5 は **>** 比較なので発動しない（境界では補正なし）
    #[test]
    fn f1_panel1_tourist_correction_boundary_at_1_5() {
        let (_score, _pop, ratio, is_tourist) =
            compute_difficulty_score_with_tourist_correction(5, 15_000.0, 10_000.0);
        assert!((ratio - 1.5).abs() < 1e-9);
        assert!(
            !is_tourist,
            "境界値 1.5 では > 1.5 を満たさず観光地判定されない"
        );
    }

    /// **F1 #3-6**: night_population が 0 の場合は補正不能、day を使う
    #[test]
    fn f1_panel1_no_tourist_correction_when_night_zero() {
        let (score, population, ratio, is_tourist) =
            compute_difficulty_score_with_tourist_correction(5, 10_000.0, 0.0);
        assert_eq!(ratio, 0.0);
        assert!(!is_tourist);
        assert!((population - 10_000.0).abs() < 1e-9);
        assert!((score - 5.0).abs() < 1e-6);
    }

    /// **F1 #3-7**: 補正前後の score 差は分母を切り替えた値そのもの
    /// （傾向把握用、因果ではない: feedback_correlation_not_causation 遵守）
    #[test]
    fn f1_panel1_correction_score_delta_matches_population_swap() {
        let hw: i64 = 50;
        let day = 25_000.0_f64;
        let night = 10_000.0_f64;
        let (corrected_score, _pop, _ratio, is_tourist) =
            compute_difficulty_score_with_tourist_correction(hw, day, night);
        assert!(is_tourist);
        let pre = (hw as f64) / day * 10_000.0;
        let post = (hw as f64) / night * 10_000.0;
        assert!((corrected_score - post).abs() < 1e-6);
        // 修正前後の差は night/day の比率の逆数
        let ratio_change = post / pre;
        let expected = day / night;
        assert!(
            (ratio_change - expected).abs() < 1e-6,
            "score 比 = day/night の比 = {}",
            expected
        );
    }
}
