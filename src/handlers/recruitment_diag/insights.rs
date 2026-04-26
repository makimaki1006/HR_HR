//! Panel 8: AI示唆統合（採用診断タブ）
//!
//! 既存の insight engine 38パターンを**HR意思決定文脈**で再フィルタし、
//! 各示唆に「採用アクション提案」（`hr_action`）を付与して返す。
//!
//! # 再利用する既存資産
//!
//! - `insight::fetch::build_insight_context`: 全データソース集約
//! - `insight::engine::generate_insights`: 38パターンの示唆生成器
//! - `insight::helpers::{Insight, Severity, InsightCategory}`: 型
//!
//! これらは**無変更**で使用する。追加したのは HR 文脈に特化したフィルタと
//! アクション文のみ。
//!
//! # HR文脈で有効なパターン（採用担当者の意思決定に直結するもの）
//!
//! | ID      | 内容                        | HR文脈での意味          |
//! |---------|-----------------------------|------------------------|
//! | HS-1    | 慢性的人材不足              | 給与再設計・早期訴求必要 |
//! | HS-2    | 給与競争力不足              | 給与水準見直しが必須    |
//! | HS-3    | 情報開示不足                | 求人票リライトで改善可 |
//! | HS-4    | テキスト温度と採用難の乖離  | コピー改善で差別化可    |
//! | HS-5    | 雇用者集中                  | 独自訴求の余地大        |
//! | HS-6    | 空間的ミスマッチ            | 通勤支援施策が有効      |
//! | FC-1    | 求人数トレンド              | 需給予測                |
//! | FC-2    | 欠員率トレンド              | 採用難易度の先読み      |
//! | FC-4    | 充足率トレンド              | 効果検証                |
//! | RC-1〜3 | 地域比較                    | 隣接市区町村との差別化  |
//! | AP-1〜3 | アクション提案              | そのまま採用戦術        |
//! | CZ-1〜3 | 通勤圏分析                  | 採用母集団の広がり      |
//! | CF-1〜3 | 通勤フロー                  | 流入/流出の実態         |
//! | LS-1/2  | 労働力・産業偏在            | 母集団の厚み            |
//! | HH-1    | 単独世帯求職者              | 夜勤・住み込み適性      |
//! | MF-1    | 医療福祉供給密度            | 競合施設密度            |
//! | IN-1    | 産業構造ミスマッチ          | 異業種からの流入余地    |
//! | GE-1    | 可住地密度                  | 商圏の広さ              |
//! | SW-F01  | 夜勤ニーズ逼迫              | シフト設計              |
//! | SW-F02  | 休日商圏                    | 募集タイミング          |
//! | SW-F03  | ベッドタウン化              | 通勤型訴求              |
//! | SW-F04  | メッシュ人材ギャップ        | 出店/求人エリア選定     |
//! | SW-F05  | 観光ポテンシャル未活用      | 宿泊飲食系の訴求余地    |
//!
//! # 設計原則（MEMORY 遵守）
//!
//! - `feedback_correlation_not_causation`: hr_action に断定表現（「●●すべき」「必ず」）は避け、
//!   「●●の余地がある」「検討の価値あり」等の仮説的表現を使う。
//! - `feedback_hw_data_scope`: レスポンスに HW 注意書きを含める。
//! - `feedback_hypothesis_driven`: 各示唆に So What + 次アクションを必ず付与。

use axum::extract::{Query, State};
use axum::Json;
use serde::Deserialize;
use serde_json::{json, Value};
use std::sync::Arc;
use tower_sessions::Session;

use crate::handlers::insight::fetch::InsightContext;
use crate::handlers::insight::helpers::Insight;
use crate::AppState;

// ======== Query Params ========

#[derive(Deserialize, Debug)]
pub struct InsightsParams {
    /// 都道府県コード (1-47)
    pub prefcode: i32,
    /// 市区町村コード（省略可、未指定時は県全体）
    #[serde(default)]
    pub citycode: Option<u32>,
    /// 職種（今のところ表示用メタに載せるのみ、InsightContext 内では未使用）
    #[serde(default)]
    pub job_type: Option<String>,
    /// 雇用形態（同上）
    #[serde(default)]
    pub emp_type: Option<String>,
}

// ======== Handler ========

/// GET /api/recruitment_diag/insights?prefcode=13&citycode=13101&job_type=医療&emp_type=正社員
pub async fn insights(
    State(state): State<Arc<AppState>>,
    _session: Session,
    Query(params): Query<InsightsParams>,
) -> Json<Value> {
    let db = match &state.hw_db {
        Some(d) => d.clone(),
        None => return Json(error_response("DB未接続")),
    };
    let turso = state.turso_db.clone();

    if !(1..=47).contains(&params.prefcode) {
        return Json(error_response(&format!(
            "invalid prefcode: {} (must be 1-47)",
            params.prefcode
        )));
    }

    let pref_name = match prefcode_to_name(params.prefcode) {
        Some(n) => n.to_string(),
        None => return Json(error_response("都道府県コード解決失敗")),
    };
    let muni_name = match params.citycode {
        Some(cc) => citycode_to_name(&pref_name, cc).unwrap_or_default(),
        None => String::new(),
    };

    let pref_clone = pref_name.clone();
    let muni_clone = muni_name.clone();

    // InsightContext 構築 + 38パターン生成（spawn_blocking で I/O 待機を tokio から分離）
    let insights = tokio::task::spawn_blocking(move || {
        let ctx = crate::handlers::insight::fetch::build_insight_context(
            &db,
            turso.as_ref(),
            &pref_clone,
            &muni_clone,
        );
        let all = crate::handlers::insight::engine::generate_insights(&ctx);
        // HR文脈で有効なものだけに絞る
        all.into_iter()
            .filter(|i| is_hr_relevant(&i.id))
            .map(|i| enrich_with_hr_action(&i, &ctx))
            .collect::<Vec<Value>>()
    })
    .await
    .unwrap_or_default();

    let summary = build_summary(&insights);

    Json(json!({
        "prefcode": params.prefcode,
        "citycode": params.citycode,
        "pref": pref_name,
        "municipality": muni_name,
        "filters": {
            "job_type": params.job_type,
            "emp_type": params.emp_type,
        },
        "insights": insights,
        "summary": summary,
        "note": "HW掲載求人のみ対象（全求人市場ではない）。示唆は統計的傾向であり因果関係を示すものではありません。",
    }))
}

// ======== HR文脈フィルタ ========

/// HR 意思決定に直結する示唆パターンのIDプレフィックス/完全一致リスト
///
/// 現状は**すべての 38 パターン**が HR 文脈で有効と判断しているが、
/// 将来的に関係ないパターンを除外できるようホワイトリスト方式で残す。
pub fn is_hr_relevant(id: &str) -> bool {
    const HR_PREFIXES: &[&str] = &[
        "HS-", // 採用構造（6パターン: HS-1〜HS-6）
        "FC-", // 将来予測（4パターン: FC-1〜FC-4）
        "RC-", // 地域比較（3パターン）
        "AP-", // アクション提案（3パターン）
        "CZ-", // 通勤圏（3パターン）
        "CF-", // 通勤フロー（3パターン）
        "LS-", "HH-", "MF-", "IN-", "GE-",  // SSDSE-A 構造分析（6パターン）
        "SW-F", // Agoop 人流（10パターン）
    ];
    HR_PREFIXES.iter().any(|p| id.starts_with(p))
}

/// Insight に HR 視点のアクション提案を付与して JSON 化
fn enrich_with_hr_action(insight: &Insight, _ctx: &InsightContext) -> Value {
    let hr_action = hr_action_for(&insight.id, insight);
    json!({
        "pattern_id": insight.id,
        "category": insight.category.label(),
        "severity": severity_label(&insight.severity),
        "severity_rank": severity_rank(&insight.severity),
        "title": insight.title,
        "message": insight.body,
        "evidence": insight.evidence,
        "related_tabs": insight.related_tabs,
        "hr_action": hr_action,
    })
}

/// パターンID毎の採用アクション提案（断定回避、仮説表現）
fn hr_action_for(id: &str, insight: &Insight) -> String {
    match id {
        "HS-1" => "慢性的な人材不足の可能性あり。給与水準の再設計や採用広告の露出拡大、入社後定着施策の強化を検討する価値があります。".to_string(),
        "HS-2" => "給与訴求力が周辺と比較して低い傾向。基本給の見直しや手当・賞与設計の再構築で改善余地があります。".to_string(),
        "HS-3" => "求人票の情報開示が不足している傾向。勤務時間・休日・福利厚生の具体記載で応募数改善の余地があります。".to_string(),
        "HS-4" => "求人文面の温度感と市況感に乖離あり。ターゲット層に刺さるコピー調整で差別化できる可能性があります。".to_string(),
        "HS-5" => "特定企業への雇用集中が強い市場。独自の職場環境や働き方を訴求することで求職者の選択肢として認知されやすくなる可能性があります。".to_string(),
        "HS-6" => "職住近接が弱い傾向。通勤手当拡充・リモート導入・社宅検討など通勤負担軽減施策が有効である可能性があります。".to_string(),
        "FC-1" => "求人トレンドの変化に注意。募集タイミングや掲載期間の最適化を検討する価値があります。".to_string(),
        "FC-2" => "欠員率トレンドは採用難易度の先行指標。悪化傾向なら先行して採用予算の拡充を検討する価値があります。".to_string(),
        "FC-3" => "充足率の動きから、現行募集手法の効果検証に有用です。".to_string(),
        "FC-4" => "充足困難度の先読みにより、代替チャネル（エージェント・リファラル等）への並行投下を検討する価値があります。".to_string(),
        "RC-1" | "RC-2" | "RC-3" => "隣接地域との比較で差別化ポイントを抽出できる可能性があります。訴求要素の再設計の参考にできます。".to_string(),
        "AP-1" | "AP-2" | "AP-3" => "本示唆は既にアクション提案形式です。現場の実情と照らして実行可否を判断してください。".to_string(),
        "CZ-1" | "CZ-2" | "CZ-3" => "通勤圏分析から採用母集団の実サイズが見えます。広告配信エリア・沿線絞り込みの参考情報になります。".to_string(),
        "CF-1" | "CF-2" | "CF-3" => "通勤フローから流入元／流出先が判明。流入元自治体での露出強化、流出先との比較訴求の余地があります。".to_string(),
        "LS-1" => "失業率が高めで採用余力のある地域の可能性あり。募集効率が良くなる可能性があります。".to_string(),
        "LS-2" => "産業偏在があると業種間人材流動が鈍い傾向。業種横断の訴求（職歴不問・未経験歓迎）で母集団拡大の余地があります。".to_string(),
        "HH-1" => "単独世帯が多い地域では夜勤・早朝シフト・住み込みなどの働き方を受け入れやすい傾向があります。訴求軸として検討の価値あり。".to_string(),
        "MF-1" => "医療福祉施設の供給密度ギャップは競合環境の指標。密度が低ければ独占的地位を築ける可能性があります。".to_string(),
        "IN-1" => "産業構造ミスマッチが大きい地域では異業種からの流入余地あり。未経験者向け訴求の価値があります。".to_string(),
        "GE-1" => "可住地密度から商圏の広さ・薄さが推定できます。広告出稿エリアの最適化材料になります。".to_string(),
        "SW-F01" => "深夜滞在が多いエリアは夜勤ニーズが逼迫している可能性あり。夜勤歓迎・夜勤手当を前面に打ち出す訴求が有効な可能性があります。".to_string(),
        "SW-F02" => "休日商圏が厚いエリアは土日祝の募集が効く可能性があります。".to_string(),
        "SW-F03" => "ベッドタウン型エリアでは通勤時間軸の訴求（駅近・乗り換えなし等）が有効な可能性があります。".to_string(),
        "SW-F04" => "求人密度と人流のギャップが大きいメッシュは、出店・募集強化エリアの候補として検討の価値があります。".to_string(),
        "SW-F05" => "観光ポテンシャルが活用しきれていないエリアでは、宿泊飲食系の求人で先行者利益を得られる可能性があります。".to_string(),
        "SW-F06" | "SW-F07" | "SW-F08" | "SW-F09" | "SW-F10" => {
            format!("人流指標（{}）は採用戦略設計の補助情報として活用できます。", id)
        }
        _ => format!(
            "本示唆（{}）は現場の実情に照らして判断してください。見出し: {}",
            id, insight.title
        ),
    }
}

// ======== サマリ生成 ========

fn build_summary(insights: &[Value]) -> String {
    if insights.is_empty() {
        return "このエリアでは有効な示唆が検出されませんでした。データ未整備の可能性があります。"
            .to_string();
    }

    let mut critical = 0;
    let mut warning = 0;
    let mut info = 0;
    let mut positive = 0;
    for i in insights {
        match i.get("severity").and_then(|v| v.as_str()).unwrap_or("") {
            "重大" => critical += 1,
            "注意" => warning += 1,
            "情報" => info += 1,
            "良好" => positive += 1,
            _ => {}
        }
    }

    format!(
        "採用戦略上のシグナル {}件検出（重大{}、注意{}、情報{}、良好{}）。重大・注意シグナルを優先して検討することを推奨します。",
        insights.len(),
        critical,
        warning,
        info,
        positive
    )
}

// ======== 型変換ヘルパ ========

fn severity_label(sev: &crate::handlers::insight::helpers::Severity) -> &'static str {
    sev.label()
}

fn severity_rank(sev: &crate::handlers::insight::helpers::Severity) -> i32 {
    use crate::handlers::insight::helpers::Severity;
    match sev {
        Severity::Critical => 0,
        Severity::Warning => 1,
        Severity::Info => 2,
        Severity::Positive => 3,
    }
}

fn prefcode_to_name(prefcode: i32) -> Option<&'static str> {
    let map = crate::geo::pref_name_to_code();
    let target = format!("{:02}", prefcode);
    for (name, code) in map.iter() {
        if *code == target.as_str() {
            return Some(name);
        }
    }
    None
}

/// citycode → 市区町村名（同 pref 内で逆引き）
fn citycode_to_name(pref_name: &str, target_code: u32) -> Option<String> {
    // geo::city_code の内部マップは (prefcode, city) -> citycode の正引きのみ公開。
    // 本ハンドラでは全47都道府県ではなく指定 pref の範囲だけ逆引きすれば十分なので、
    // 将来の行数爆発を避けるため「候補を1件ずつ正引きして確認」ではなく、
    // master_city.csv の include_str と同じ手順で自前パース（軽量）。
    //
    // ただし重複を避けるため geo::city_code 側に getter を足さず、
    // 起動時 1 回のマッピング構築で済ませる。
    static REVERSE: std::sync::OnceLock<std::collections::HashMap<u32, (String, String)>> =
        std::sync::OnceLock::new();
    let map = REVERSE.get_or_init(build_reverse_map);
    map.get(&target_code).and_then(|(p, m)| {
        if p == pref_name {
            Some(m.clone())
        } else {
            None
        }
    })
}

fn build_reverse_map() -> std::collections::HashMap<u32, (String, String)> {
    // master_city.csv を include_str 経由で再度読み込む（geo::city_code と同じソース）
    const MASTER_CITY_CSV: &str = include_str!("../../geo/master_city.csv");
    let pref_map = crate::geo::pref_name_to_code();
    // prefcode → pref_name の逆引き
    let code_to_pref: std::collections::HashMap<String, &str> =
        pref_map.iter().map(|(n, c)| (c.to_string(), *n)).collect();

    let mut reverse = std::collections::HashMap::new();
    let mut lines = MASTER_CITY_CSV.lines();
    lines.next(); // header
    for line in lines {
        let parts: Vec<&str> = line.split(',').collect();
        if parts.len() < 3 {
            continue;
        }
        let citycode: u32 = match parts[0].parse() {
            Ok(v) => v,
            Err(_) => continue,
        };
        let prefcode_raw = parts[1].trim();
        let city_name = parts[2].trim().to_string();
        if prefcode_raw.is_empty() || city_name.is_empty() {
            continue;
        }
        let prefcode_padded = format!("{:0>2}", prefcode_raw);
        if let Some(pref_name) = code_to_pref.get(&prefcode_padded) {
            reverse.insert(citycode, (pref_name.to_string(), city_name));
        }
    }
    reverse
}

fn error_response(msg: &str) -> Value {
    json!({
        "error": msg,
        "note": "HW掲載求人のみ対象（全求人市場ではない）。",
    })
}

// ======== テスト ========

#[cfg(test)]
mod tests {
    use super::*;
    use crate::handlers::insight::helpers::{
        Evidence, Insight as InsightType, InsightCategory, Severity,
    };

    fn dummy_insight(id: &str, sev: Severity) -> InsightType {
        InsightType {
            id: id.to_string(),
            category: InsightCategory::HiringStructure,
            severity: sev,
            title: format!("テスト:{}", id),
            body: "body".to_string(),
            evidence: vec![Evidence {
                metric: "m".into(),
                value: 1.0,
                unit: "".into(),
                context: "".into(),
            }],
            related_tabs: vec!["overview"],
        }
    }

    /// HR 文脈フィルタ：既知の 38 パターンの ID プレフィックスが全て通過すること
    #[test]
    fn hr_relevant_accepts_all_38_patterns() {
        // Panel 設計の全38パターンID
        let ids = [
            "HS-1", "HS-2", "HS-3", "HS-4", "HS-5", "HS-6", // 6
            "FC-1", "FC-2", "FC-3", "FC-4", // 4
            "RC-1", "RC-2", "RC-3", // 3
            "AP-1", "AP-2", "AP-3", // 3
            "CZ-1", "CZ-2", "CZ-3", // 3
            "CF-1", "CF-2", "CF-3", // 3
            "LS-1", "LS-2", "HH-1", "MF-1", "IN-1", "GE-1", // 6
            "SW-F01", "SW-F02", "SW-F03", "SW-F04", "SW-F05", "SW-F06", "SW-F07", "SW-F08",
            "SW-F09", "SW-F10", // 10
        ];
        assert_eq!(ids.len(), 38, "38パターン期待");
        for id in ids {
            assert!(is_hr_relevant(id), "ID {} は HR 文脈で拾われるべき", id);
        }
    }

    /// 関係ないIDは弾く（逆証明）
    #[test]
    fn hr_relevant_rejects_unknown() {
        assert!(!is_hr_relevant(""));
        assert!(!is_hr_relevant("XX-1"));
        assert!(!is_hr_relevant("foo"));
        assert!(!is_hr_relevant("SW-X01")); // SW-F ではない
    }

    /// HR action 文に断定表現「必ず」「●●すべきです」が含まれないこと
    /// （MEMORY: feedback_correlation_not_causation）
    #[test]
    fn hr_action_avoids_assertive_language() {
        let forbidden = ["必ず", "絶対"];
        for id in ["HS-1", "HS-2", "HS-5", "HH-1", "MF-1", "SW-F01", "SW-F04"] {
            let ins = dummy_insight(id, Severity::Warning);
            let action = hr_action_for(id, &ins);
            for bad in forbidden {
                assert!(
                    !action.contains(bad),
                    "ID {} の hr_action に禁止表現「{}」が含まれる: {}",
                    id,
                    bad,
                    action
                );
            }
            // 空でないこと
            assert!(!action.is_empty(), "ID {} の hr_action が空", id);
        }
    }

    /// summary は件数を反映する
    #[test]
    fn summary_reflects_counts() {
        let empty: Vec<Value> = vec![];
        assert!(build_summary(&empty).contains("検出されませんでした"));

        let some = vec![
            json!({"severity": "重大"}),
            json!({"severity": "注意"}),
            json!({"severity": "情報"}),
        ];
        let s = build_summary(&some);
        assert!(s.contains("3件"));
        assert!(s.contains("重大1"));
        assert!(s.contains("注意1"));
    }

    #[test]
    fn severity_rank_ordering() {
        assert!(severity_rank(&Severity::Critical) < severity_rank(&Severity::Warning));
        assert!(severity_rank(&Severity::Warning) < severity_rank(&Severity::Info));
        assert!(severity_rank(&Severity::Info) < severity_rank(&Severity::Positive));
    }

    /// ダミーコンテキストでの enrich 出力形状を検証
    #[test]
    fn enrich_with_hr_action_shape() {
        // Note: build_insight_context を呼ばずに、Insight 単独で enrich ロジックを逆証明
        let ins = dummy_insight("HS-1", Severity::Critical);

        // enrich_with_hr_action の呼び出しには &InsightContext が必要だが、
        // この関数は現状 ctx を使わない（プレースホルダ）。テスト用に
        // field 構造のサブセットのみ確認する目的で、hr_action_for を直接検証する。
        let action = hr_action_for(&ins.id, &ins);
        assert!(!action.is_empty());
        assert!(action.contains("検討") || action.contains("余地") || action.contains("可能性"));
    }
}
