//! 現象テーマ付与・精度ガード動的生成・網羅ルール・言い過ぎ検出 (2026-07 2層設計)
//!
//! 実証済みの Python プロトタイプ (gemini_v4_repro.py / R2) を Rust に移植したもの。
//! flash-lite で「精度違反0・単品網羅・複合2テーマ」を達成した設計を、
//! **特定地域 (大分市) の手作りではなく pack から動的生成** する形に一般化する。
//!
//! ## 一般化の3本柱
//! 1. **テーマ付与** (`assign_themes`): evidence の metric_name / granularity から決定的に
//!    テーマ (需要/供給/競争/自社給与/到達・通勤/条件・休日/地域経済/個社) を判定。
//!    evidence ID は動的採番なのでハードコードしない。granularity=企業 は必ず「個社」。
//! 2. **精度ガード動的生成** (`build_accuracy_guards`): client_input・発火/未発火シグナルから
//!    「確認済み条件」「言い切り禁止」文をその場の pack 状態に応じて組み立てる。
//! 3. **網羅ルール** (`coverage_plan`): 単品必須 (MUST) = 個社でも弱信号でもない全 evidence、
//!    除外許可 (ALLOW) = 個社・弱信号。これも pack から動的に算出。
//!
//! ## 言い過ぎ検出 (`overclaim_hits`)
//! キーワードガードをすり抜ける文レベルの過剰表現 (希少|不足|欠如|枯渇|皆無 × 休日|人材|求職 等)
//! を近接パターンで検出する。ai.rs の検証層が該当文を落とす/注記するのに使う。

use super::evidence::{granularity, Evidence};
use super::evidence_pack::ConsultAnalysis;
use super::input::ClientInput;
use super::signals::Signal;

/// 現象テーマ (gold standard の8分類)。
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Theme {
    Demand,      // 需要
    Supply,      // 供給
    Competition, // 競争
    OwnSalary,   // 自社給与
    Reach,       // 到達・通勤
    Conditions,  // 条件・休日
    LocalEcon,   // 地域経済
    Company,     // 個社
    Other,       // その他 (分類不能)
}

impl Theme {
    pub fn label_ja(self) -> &'static str {
        match self {
            Theme::Demand => "需要",
            Theme::Supply => "供給",
            Theme::Competition => "競争",
            Theme::OwnSalary => "自社給与",
            Theme::Reach => "到達・通勤",
            Theme::Conditions => "条件・休日",
            Theme::LocalEcon => "地域経済",
            Theme::Company => "個社",
            Theme::Other => "その他",
        }
    }

    /// 個社テーマか (単品必須の除外・複合の主役禁止判定に使う)。
    pub fn is_company(self) -> bool {
        self == Theme::Company
    }
}

/// 1件の evidence をテーマに分類する (決定的)。
///
/// 優先順位:
/// 1. granularity=企業 → 個社 (指標名に依らず確実)
/// 2. metric_name のキーワード照合 (指標種別ベース。値のハードコードなし)
///
/// キーワードは metric_name の実文言 (axes.rs / signals.rs の mint 箇所) に対応する。
/// 未知の指標は Other になり、網羅の MUST から外れる (弱信号扱い) が破棄はしない。
pub fn classify(ev: &Evidence) -> Theme {
    // 企業粒度は例外なく個社
    if ev.granularity == granularity::COMPANY {
        return Theme::Company;
    }
    let m = ev.metric_name.as_str();
    let has = |kw: &str| m.contains(kw);

    // 自社給与: 提示給与の位置・給与Q1/最賃・給与中央値の県平均比 等
    if has("提示給与") || has("給与Q1") || has("給与中央値") || has("時給下限") {
        return Theme::OwnSalary;
    }
    // 到達・通勤
    if has("通勤流入") || has("通勤流出") || has("流入元") {
        return Theme::Reach;
    }
    // 条件・休日
    if has("年間休日") || has("休日") {
        return Theme::Conditions;
    }
    // 競争: 求人件数・掲載企業数・シェア・タグ種類・人気バッジ・非正規比率
    if has("求人件数")
        || has("掲載企業数")
        || has("掲載企業のシェア")
        || has("カードタグ")
        || has("バッジ")
        || has("正社員")
        || has("正職員")
    {
        return Theme::Competition;
    }
    // 需要: 有効求人倍率・新着求人比率
    if has("有効求人倍率") || has("新着求人") {
        return Theme::Demand;
    }
    // 供給: 働き手人口・転職希望率・純移動率・失業率・自然増減
    if has("働き手人口") || has("転職希望") || has("純移動") || has("失業率") || has("自然増減")
    {
        return Theme::Supply;
    }
    // 地域経済: 開業率/廃業率・昼夜間人口比率・家賃
    if has("開業率") || has("廃業率") || has("昼夜間") || has("家賃") {
        return Theme::LocalEcon;
    }
    Theme::Other
}

/// analysis.evidence 全件にテーマを決定的に付与する (エンジン側の一括処理)。
/// 冪等 (同一入力なら同一結果)。
pub fn assign_themes(evidence: &mut [Evidence]) {
    for ev in evidence.iter_mut() {
        ev.theme = classify(ev).label_ja().to_string();
    }
}

/// evidence_id → テーマ (label_ja) を引く。付与済み theme を尊重し、空なら再分類する。
pub fn theme_of(analysis: &ConsultAnalysis, evidence_id: &str) -> Option<String> {
    analysis
        .evidence
        .iter()
        .find(|e| e.id == evidence_id)
        .map(|e| {
            if e.theme.is_empty() {
                classify(e).label_ja().to_string()
            } else {
                e.theme.clone()
            }
        })
}

// =============================================================================
// 弱信号の判定 (網羅ルールの ALLOW 側)
// =============================================================================

/// 弱信号 evidence か。網羅 MUST から外し、除外を許可する。
///
/// 「弱信号」= 判定の閾値近傍で方向が定まらない/データ品質注記付きの観測。
/// pack から動的に判定するため、対応する未発火シグナルの根拠であるものは弱信号扱いにしない
/// (それは「言い切り禁止」ガードの対象)。ここでは metric_name ベースの静的な弱信号
/// (純移動率=均衡域, 新着=中程度域, サンプル不足系) と、Other テーマを弱信号とみなす。
pub fn is_weak(ev: &Evidence) -> bool {
    let t = classify(ev);
    if t == Theme::Other {
        return true;
    }
    // 純移動率・新着求人比率は方向が弱く出やすいため弱信号候補。
    // ただし値は解釈済みなので metric 種別で軽く扱う。
    let m = ev.metric_name.as_str();
    m.contains("純移動") || m.contains("新着求人")
}

// =============================================================================
// 網羅ルール (coverage_plan)
// =============================================================================

/// 網羅計画。単品層の MUST / 除外許可 ALLOW を pack から動的に算出したもの。
#[derive(Debug, Clone, Default)]
pub struct CoveragePlan {
    /// 単品層で必ず取り上げる evidence_id (個社でも弱信号でもない全件)。
    pub must_single: Vec<String>,
    /// 除外を許可する evidence_id (個社・弱信号)。
    pub allow_excluded: Vec<String>,
}

impl CoveragePlan {
    pub fn is_must(&self, id: &str) -> bool {
        self.must_single.iter().any(|m| m == id)
    }
    pub fn is_allowed_excluded(&self, id: &str) -> bool {
        self.allow_excluded.iter().any(|a| a == id)
    }
}

/// 網羅計画を pack から算出する (決定的)。
pub fn coverage_plan(analysis: &ConsultAnalysis) -> CoveragePlan {
    let mut must = Vec::new();
    let mut allow = Vec::new();
    for ev in &analysis.evidence {
        let t = classify(ev);
        if t.is_company() || is_weak(ev) {
            allow.push(ev.id.clone());
        } else {
            must.push(ev.id.clone());
        }
    }
    CoveragePlan {
        must_single: must,
        allow_excluded: allow,
    }
}

// =============================================================================
// 精度ガードの動的生成 (build_accuracy_guards)
// =============================================================================

/// 未発火シグナルID → そのシグナルが主張しかねない内容の「言い切り禁止」ガード文。
///
/// gemini_v4_repro.py の GUARD を一般化: 「そのシグナルが発火していないなら、その主張を
/// 断定しない」。pack で当該シグナルが発火していれば逆にガードを出さない (=言ってよい)。
/// 静的マップ (signal_id → ガード文) を持ち、pack の fired 状態で出す/出さないを切り替える。
const NOT_FIRED_GUARDS: [(&str, &str); 11] = [
    (
        "S-02",
        "提示給与が市場下位という判定は出ていません。「給与が低い」と書かないでください。",
    ),
    (
        "S-04",
        "給与が最低賃金に近いという判定は出ていません。「最賃近接」と書かないでください。",
    ),
    (
        "S-08",
        "転職を考える層が「薄い」という判定は出ていません。「薄い」と言い切らないでください（全国並みの可能性）。",
    ),
    (
        "S-09",
        "転職を考える層が「厚い」という判定は出ていません。「厚い」と言い切らないでください（全国並みの可能性）。",
    ),
    (
        "S-22",
        "家賃が全国比で「高い」という判定は出ていません。「家賃が高い」と書かないでください（安い可能性）。",
    ),
    (
        "S-26",
        "訴求タグが「少ない/横並び」という判定は出ていません。「タグが少ない」「横並び」と書かないでください。",
    ),
    (
        "S-27",
        "人気表示が「集中」という判定は出ていません。「人気が集中」と書かないでください。",
    ),
    (
        "S-30",
        "正社員以外が「多い」という判定は出ていません。「正社員以外が多い」と書かないでください（正社員中心の可能性）。",
    ),
    (
        "S-05",
        "市場給与が県平均を下回るという判定は出ていません。給与が県平均未満と言い切らないでください。",
    ),
    (
        "S-13",
        "新着比率が高いという判定は出ていません。市場の動きが速いと言い切らないでください。",
    ),
    (
        "S-15",
        "特定企業への求人集中という判定は出ていません。「集中」と書かないでください。",
    ),
];

/// 精度ガード文を pack から動的に組み立てる (LLM プロンプトに埋め込む)。
///
/// 生成物:
/// - **確認済み条件**: client が給与を入力していれば「確認済みは給与のみ」→「条件で勝てている」禁止
/// - **言い切り禁止**: 未発火シグナルに対応するガード文 (発火していれば出さない)
/// - **観測限界**: 該当 evidence があるとき (休日はカード観測分のみ 等)
/// - **個社の扱い**: 個社テーマ evidence は呼び水どまり → excluded へ
///
/// pack の状態で内容が変わる (大分市の値をハードコードしない)。
pub fn build_accuracy_guards(analysis: &ConsultAnalysis, client: &ClientInput) -> String {
    let mut lines: Vec<String> = Vec::new();
    lines.push("## 精度ガード（厳守。これに反する主張は禁止）".to_string());

    // --- 確認済み条件 ---
    let salary_input = client.target_salary_min.is_some() || client.target_salary_max.is_some();
    if salary_input {
        lines.push(
            "- 顧客から確認できている条件は【給与のみ】です。休日・手当・勤務時間・勤務地などは未確認。\
             「条件で勝てている」と書かず、「給与では勝てている（休日など他条件は未確認）」と書いてください。"
                .to_string(),
        );
    } else {
        lines.push(
            "- 顧客から提示給与の入力がありません。自社の条件が市場より強い/弱いと断定しないでください。"
                .to_string(),
        );
    }

    // --- 言い切り禁止 (未発火シグナル) ---
    let fired = |id: &str| analysis.signals.iter().any(|s| s.id == id && s.fired);
    let mut nf_guards: Vec<&str> = Vec::new();
    for (sid, guard) in NOT_FIRED_GUARDS.iter() {
        // シグナルが「評価対象として存在するが発火していない」ときのみガードを出す。
        let exists = analysis.signals.iter().any(|s| &s.id == sid);
        if exists && !fired(sid) {
            nf_guards.push(guard);
        }
    }
    if !nf_guards.is_empty() {
        lines.push("- 次はデータが支持していないので言い切らないでください（未発火）:".to_string());
        for g in nf_guards {
            lines.push(format!("  ・{}", g));
        }
    }

    // --- 観測限界 (該当 evidence があるときのみ) ---
    let has_ev = |kw: &str| analysis.evidence.iter().any(|e| e.metric_name.contains(kw));
    if has_ev("年間休日") {
        lines.push(
            "- 年間休日は求人カードで見えた分のみの観測です。「市場に休日が無い」ではなく\
             「カード上で高い休日を見せる求人が少ない」と書いてください。"
                .to_string(),
        );
    }
    if has_ev("家賃") {
        lines.push(
            "- 家賃は1畳あたりの県代表値です。勤務地周辺の実勢とは差がある可能性があります。"
                .to_string(),
        );
    }

    // --- 個社の扱い ---
    let has_company = analysis.evidence.iter().any(|e| classify(e).is_company());
    if has_company {
        lines.push(
            "- 個社（特定企業）の観測は各1社分の呼び水にとどめ、市場全体の断定に使わないでください。\
             単品・複合の主役にせず、excluded に入れてください。"
                .to_string(),
        );
    }

    // --- 弱信号 ---
    lines.push(
        "- 方向が定まらない弱い観測（均衡域・中程度域）は使うなら軽く。無理なら excluded に。"
            .to_string(),
    );

    lines.join("\n")
}

// =============================================================================
// 言い過ぎ検出 (overclaim_hits) — キーワードガードをすり抜ける文レベルの過剰表現
// =============================================================================

/// 過剰表現の程度語 (「〜が無い/枯れている」系)。
const OVERCLAIM_SEVERITY: [&str; 6] = ["希少", "不足", "欠如", "枯渇", "皆無", "払底"];
/// 過剰表現の対象語 (これらに severity 語が近接すると言い過ぎと判定)。
const OVERCLAIM_TARGET: [&str; 5] = ["休日", "人材", "求職", "働き手", "応募"];

/// 近接文字数 (severity と target がこの文字数以内に共起したら言い過ぎとみなす)。
const OVERCLAIM_PROXIMITY_CHARS: usize = 12;

/// テキスト中の言い過ぎ表現を検出し、ヒットした (severity, target) の組を返す。
///
/// 例: 「休日が希少」「人材が枯渇」等。キーワード完全一致では拾えない文レベルの誇張を、
/// severity 語と target 語の近接 (OVERCLAIM_PROXIMITY_CHARS 文字以内) で検出する。
pub fn overclaim_hits(text: &str) -> Vec<(String, String)> {
    let chars: Vec<char> = text.chars().collect();
    let mut hits = Vec::new();
    // severity 語の各出現位置を求め、その近傍に target 語があるか調べる (双方向)。
    for sev in OVERCLAIM_SEVERITY.iter() {
        let sev_chars: Vec<char> = sev.chars().collect();
        for start in 0..chars.len() {
            if window_matches(&chars, start, &sev_chars) {
                let s_end = start + sev_chars.len();
                // 否定表現 (「不足とは限らない」「希少ではない」等) は慎重な言い回しであり
                // 言い過ぎではない。severity 語の直後に否定マーカーが来る場合はスキップする。
                if severity_is_negated(&chars, s_end) {
                    continue;
                }
                for tgt in OVERCLAIM_TARGET.iter() {
                    let tgt_chars: Vec<char> = tgt.chars().collect();
                    // 前方 (target が severity より前) と後方の両方を近接窓で調べる。
                    let lo = start.saturating_sub(OVERCLAIM_PROXIMITY_CHARS);
                    let hi = (s_end + OVERCLAIM_PROXIMITY_CHARS).min(chars.len());
                    for w in lo..hi {
                        if window_matches(&chars, w, &tgt_chars) {
                            hits.push((sev.to_string(), tgt.to_string()));
                        }
                    }
                }
            }
        }
    }
    hits.sort();
    hits.dedup();
    hits
}

/// text 中に言い過ぎ表現が1つでもあるか。
pub fn has_overclaim(text: &str) -> bool {
    !overclaim_hits(text).is_empty()
}

/// severity 語 (chars[..s_end] の末尾) の直後に否定マーカーがあるか。
/// 「不足とは限らない」「希少ではない」「枯渇していない」等の慎重表現を言い過ぎから除外する。
fn severity_is_negated(chars: &[char], s_end: usize) -> bool {
    const NEG_MARKERS: [&str; 7] = [
        "とは限ら",
        "とは言え",
        "ではな",
        "ではあり",
        "わけでは",
        "していな",
        "しておら",
    ];
    // severity 語の直後〜数文字以内に否定マーカーが始まるかを見る (助詞を挟むことがあるため小窓)。
    let hi = (s_end + 4).min(chars.len());
    for w in s_end..hi {
        for m in NEG_MARKERS.iter() {
            let mc: Vec<char> = m.chars().collect();
            if window_matches(chars, w, &mc) {
                return true;
            }
        }
    }
    false
}

/// chars[start..] が pat と一致するか (境界チェック込み)。
fn window_matches(chars: &[char], start: usize, pat: &[char]) -> bool {
    if start + pat.len() > chars.len() {
        return false;
    }
    chars[start..start + pat.len()] == *pat
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::handlers::consult::evidence::EvidenceKind;
    use crate::handlers::consult::evidence_pack::{analyze, tests::rich_input};
    use crate::handlers::consult::input::ConsultInput;

    fn ev(metric: &str, gran: &str) -> Evidence {
        Evidence {
            id: "E-000".to_string(),
            kind: EvidenceKind::Observed,
            metric_name: metric.to_string(),
            value_text: "x".to_string(),
            unit: "".to_string(),
            source_name: "t".to_string(),
            granularity: gran.to_string(),
            sample_n: None,
            as_of: None,
            note: String::new(),
            theme: String::new(),
        }
    }

    #[test]
    fn company_granularity_is_always_company_theme() {
        // 企業粒度は指標名に依らず個社
        let e = ev(
            "サンプル運輸 の1年人員増減率と掲載件数",
            granularity::COMPANY,
        );
        assert_eq!(classify(&e), Theme::Company);
    }

    #[test]
    fn classify_maps_metric_names_to_themes() {
        assert_eq!(
            classify(&ev("有効求人倍率", granularity::PREFECTURE)),
            Theme::Demand
        );
        assert_eq!(
            classify(&ev("新着求人比率", granularity::CSV)),
            Theme::Demand
        );
        assert_eq!(
            classify(&ev("働き手人口の将来増減率", granularity::MUNICIPALITY)),
            Theme::Supply
        );
        assert_eq!(
            classify(&ev("転職希望率 (県/全国)", granularity::PREFECTURE)),
            Theme::Supply
        );
        assert_eq!(
            classify(&ev("自然増減 (出生-死亡)", granularity::MUNICIPALITY)),
            Theme::Supply
        );
        assert_eq!(
            classify(&ev("今回CSVの求人件数", granularity::CSV)),
            Theme::Competition
        );
        assert_eq!(
            classify(&ev("最多掲載企業のシェア", granularity::CSV)),
            Theme::Competition
        );
        assert_eq!(
            classify(&ev("観測できた求人カードタグの種類数", granularity::CSV)),
            Theme::Competition
        );
        assert_eq!(
            classify(&ev("正社員/正職員以外の求人比率", granularity::CSV)),
            Theme::Competition
        );
        assert_eq!(
            classify(&ev("提示給与の市場内パーセンタイル", granularity::CSV)),
            Theme::OwnSalary
        );
        assert_eq!(
            classify(&ev("給与中央値の県平均比", granularity::CSV)),
            Theme::OwnSalary
        );
        assert_eq!(
            classify(&ev("通勤流入合計", granularity::MUNICIPALITY)),
            Theme::Reach
        );
        assert_eq!(
            classify(&ev("年間休日120日以上の求人比率", granularity::CSV)),
            Theme::Conditions
        );
        assert_eq!(
            classify(&ev(
                "開業率 / 廃業率 (経済センサス調査間 累計)",
                granularity::PREFECTURE
            )),
            Theme::LocalEcon
        );
        assert_eq!(
            classify(&ev("1畳あたり家賃", granularity::PREFECTURE)),
            Theme::LocalEcon
        );
        assert_eq!(
            classify(&ev("昼夜間人口比率", granularity::MUNICIPALITY)),
            Theme::LocalEcon
        );
        // 未知の指標は Other
        assert_eq!(
            classify(&ev("謎の指標", granularity::NATIONAL)),
            Theme::Other
        );
    }

    #[test]
    fn assign_themes_covers_all_evidence_and_is_deterministic() {
        let mut a1 = analyze(&rich_input());
        let mut a2 = analyze(&rich_input());
        assign_themes(&mut a1.evidence);
        assign_themes(&mut a2.evidence);
        // 全 evidence に非空テーマが付く
        for e in &a1.evidence {
            assert!(!e.theme.is_empty(), "{} にテーマが付いていない", e.id);
        }
        // 決定的
        let t1: Vec<_> = a1.evidence.iter().map(|e| (&e.id, &e.theme)).collect();
        let t2: Vec<_> = a2.evidence.iter().map(|e| (&e.id, &e.theme)).collect();
        assert_eq!(t1, t2);
    }

    #[test]
    fn analyze_assigns_themes_automatically() {
        // evidence_pack::analyze がテーマ付与まで済ませていること (mint 箇所での付与)
        let a = analyze(&rich_input());
        assert!(
            a.evidence.iter().all(|e| !e.theme.is_empty()),
            "analyze の出力は全 evidence にテーマが付いている"
        );
        // 企業観測は個社テーマ
        let has_company_theme = a.evidence.iter().any(|e| e.theme == "個社");
        assert!(
            has_company_theme,
            "rich_input には企業観測があり個社テーマが付く"
        );
    }

    #[test]
    fn coverage_plan_excludes_company_and_weak_only() {
        let a = analyze(&rich_input());
        let plan = coverage_plan(&a);
        // MUST と ALLOW で全 evidence を漏れなくカバー (取りこぼしゼロ)
        let total = a.evidence.len();
        assert_eq!(plan.must_single.len() + plan.allow_excluded.len(), total);
        // 個社は必ず ALLOW 側
        for e in &a.evidence {
            if classify(e).is_company() {
                assert!(plan.is_allowed_excluded(&e.id), "{} 個社は除外許可", e.id);
                assert!(!plan.is_must(&e.id));
            }
        }
        // MUST は空でない (主要指標がある)
        assert!(!plan.must_single.is_empty());
    }

    #[test]
    fn build_guards_changes_with_client_salary() {
        let a = analyze(&rich_input());
        // 給与入力あり → 「給与では勝てている」文
        let with_salary = build_accuracy_guards(
            &a,
            &ClientInput {
                target_salary_max: Some(300_000),
                ..Default::default()
            },
        );
        assert!(
            with_salary.contains("給与では勝てている"),
            "{}",
            with_salary
        );
        assert!(with_salary.contains("条件で勝てている") && with_salary.contains("書かず"));
        // 給与入力なし → 別の文 (強い/弱いと断定しない)
        let no_salary = build_accuracy_guards(&a, &ClientInput::default());
        assert!(
            no_salary.contains("提示給与の入力がありません"),
            "{}",
            no_salary
        );
        assert!(!no_salary.contains("給与では勝てている"));
    }

    #[test]
    fn build_guards_omits_guard_when_signal_fired() {
        // 未発火シグナルのガードは出す、発火していれば出さない (pack 状態で変わる)。
        // rich_input を2通り作り、S-22 (家賃高) の発火有無でガードの有無が変わることを確認。
        // rich_input は家賃 県2200/全国1200 ≈1.83倍 で S-22 発火 → 家賃「高い」ガードは出ない。
        let a_fired = analyze(&rich_input());
        assert!(
            a_fired.signals.iter().any(|s| s.id == "S-22" && s.fired),
            "テスト前提: rich_input で S-22 が発火"
        );
        let g_fired = build_accuracy_guards(&a_fired, &ClientInput::default());
        assert!(
            !g_fired.contains("「家賃が高い」と書かないでください"),
            "S-22 発火時は家賃高いガードを出さない: {}",
            g_fired
        );

        // 家賃が全国比で低い入力 → S-22 未発火 → 家賃「高い」ガードを出す
        let mut low_rent = rich_input();
        low_rent.rent_per_tatami = Some(800);
        low_rent.rent_per_tatami_national = Some(1_200);
        let a_notfired = analyze(&low_rent);
        assert!(
            !a_notfired.signals.iter().any(|s| s.id == "S-22" && s.fired),
            "テスト前提: 低家賃入力で S-22 未発火"
        );
        let g_notfired = build_accuracy_guards(&a_notfired, &ClientInput::default());
        assert!(
            g_notfired.contains("「家賃が高い」と書かないでください"),
            "S-22 未発火時は家賃高いガードを出す: {}",
            g_notfired
        );
    }

    #[test]
    fn build_guards_works_on_non_oita_synthetic() {
        // 大分以外の合成 (rich_input=群馬県高崎市) でも破綻せずガードが生成される
        let a = analyze(&rich_input());
        assert_eq!(a.report_meta.prefecture, "群馬県");
        let g = build_accuracy_guards(&a, &ClientInput::default());
        assert!(g.contains("精度ガード"));
        assert!(!g.is_empty());
    }

    #[test]
    fn overclaim_detects_proximity_patterns() {
        // キーワード完全一致では拾えない文レベルの過剰表現
        assert!(has_overclaim("この地域は休日が希少です"));
        assert!(has_overclaim("人材が枯渇している可能性"));
        assert!(has_overclaim("求職者が皆無に近い"));
        let hits = overclaim_hits("休日が不足し、人材も欠如している");
        assert!(hits.iter().any(|(s, t)| s == "不足" && t == "休日"));
        assert!(hits.iter().any(|(s, t)| s == "欠如" && t == "人材"));
        // 遠すぎる共起は拾わない
        assert!(!has_overclaim(
            "休日については別途整理する。ずっと後の段落で人材の話をするが不足とは限らない話"
        ));
        // 無害な文
        assert!(!has_overclaim("給与では市場で勝てている可能性があります"));
    }

    #[test]
    fn is_weak_flags_other_and_soft_metrics() {
        assert!(is_weak(&ev("純移動率", granularity::MUNICIPALITY)));
        assert!(is_weak(&ev("新着求人比率", granularity::CSV)));
        assert!(is_weak(&ev("謎の指標", granularity::NATIONAL)));
        assert!(!is_weak(&ev("有効求人倍率", granularity::PREFECTURE)));
    }

    // rich_input 以外の合成入力でも動くことの確認 (一般化の担保)
    fn minimal_input() -> ConsultInput {
        ConsultInput {
            pref: "沖縄県".to_string(),
            muni: "那覇市".to_string(),
            as_of: "2026-07-15".to_string(),
            data_sources: vec!["今回の求人CSV集計".to_string()],
            total_postings: 200,
            company_count: 40,
            salary_values: (0..80).map(|i| 200_000 + i * 1_000).collect(),
            salary_median: Some(240_000),
            salary_q1: Some(220_000),
            salary_n: 80,
            job_openings_ratio: Some(1.1),
            ..Default::default()
        }
    }

    #[test]
    fn generalizes_to_other_regions() {
        let a = analyze(&minimal_input());
        assert!(a.evidence.iter().all(|e| !e.theme.is_empty()));
        let plan = coverage_plan(&a);
        assert_eq!(
            plan.must_single.len() + plan.allow_excluded.len(),
            a.evidence.len()
        );
        let g = build_accuracy_guards(&a, &ConsultInput::default().client);
        assert!(g.contains("精度ガード"));
    }
}
