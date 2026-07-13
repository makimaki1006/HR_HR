//! 逆証明の道具箱 (AI複合考察への決定的な機械チェック)
//!
//! LLM に都度「反証ロジック」を作らせるのではなく、逆証明の道具をコードとして持ち、
//! コードが実行する (2026-07 設計方針: 決定的・単体テスト可能・追加のAPIコストなし)。
//! 対象は validate_items (ai.rs) を通過した複合考察 items。考察は破棄せず、
//! 指摘を「確認が必要な点」「別の見方」として AiItem に併記する。
//!
//! ## 4つの道具
//! - **T1 標本数チェック**: 引用 evidence の観測数が少ない (企業単位 <3社 / 今回CSV集計の
//!   標本 <30件) とき「少数例からの一般化の可能性」を指摘 → refuted=true
//! - **T2 粒度チェック**: 引用根拠が県・全国粒度のみで対象が市区町村のとき、
//!   粒度の差を指摘 → refuted=true
//! - **T3 反対方向シグナル検索**: 考察の主張軸 (claim_axis) と同軸・逆方向で発火している
//!   シグナル (config::SIGNAL_META) を列挙 → 「別の見方」として併記
//! - **T4 逆因果辞書**: 引用根拠が属する発火シグナルに対応する静的な逆解釈テンプレ
//!   (自己反証プロトタイプで有効だった逆解釈を辞書化) → 「別の見方」として併記
//!
//! ## 裁定 (すべて決定的)
//! - T1/T2 のいずれか発火 → refuted=true (「⚠ 確認が必要 — 面談で検証」ラベル)
//! - T3/T4 のみ → refuted=false のまま alt_interpretation に併記
//! - 何も発火せず → reviewed=true・注記なし
//!
//! テンプレ文はすべて可能性表現で、§19.2 の禁止表現を含まない (テストで担保)。

use super::ai::AiItem;
use super::config;
use super::evidence::{granularity, Evidence};
use super::evidence_pack::ConsultAnalysis;
use super::signals::Signal;

/// 考察の主張が主に関わる軸。生成コールのタグ (文字列) をパースして得る。
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum ClaimAxis {
    Demand,
    Supply,
    Competition,
    Offer,
    Other,
}

impl ClaimAxis {
    /// タグ文字列をパースする。未知の値は Other (明示の縮退。T3 は何もしない)。
    pub fn parse(s: &str) -> ClaimAxis {
        match s.trim() {
            "demand" => ClaimAxis::Demand,
            "supply" => ClaimAxis::Supply,
            "competition" => ClaimAxis::Competition,
            "offer" => ClaimAxis::Offer,
            _ => ClaimAxis::Other,
        }
    }
}

/// 考察の主張の方向。problem=採用への逆風 / opportunity=好機 / neutral=中立。
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum ClaimDirection {
    Problem,
    Opportunity,
    Neutral,
}

impl ClaimDirection {
    /// タグ文字列をパースする。未知の値は Neutral (明示の縮退。T3 は何もしない)。
    pub fn parse(s: &str) -> ClaimDirection {
        match s.trim() {
            "problem" => ClaimDirection::Problem,
            "opportunity" => ClaimDirection::Opportunity,
            _ => ClaimDirection::Neutral,
        }
    }

    /// 逆方向。Neutral に逆方向はない (T3 の対象外)。
    pub fn opposite(self) -> Option<ClaimDirection> {
        match self {
            ClaimDirection::Problem => Some(ClaimDirection::Opportunity),
            ClaimDirection::Opportunity => Some(ClaimDirection::Problem),
            ClaimDirection::Neutral => None,
        }
    }
}

/// T4 逆因果辞書: シグナルID → 逆・別の因果解釈のテンプレ。
/// 自己反証プロトタイプ (two_stage_result.json) で根拠データに即して有効だった
/// 逆解釈を静的化したもの。すべて可能性表現・禁止表現なし (テストで担保)。
const REVERSE_CAUSAL_DICT: [(&str, &str); 5] = [
    (
        "S-01",
        "長期掲載は常時採用の方針で意図的に続けている場合もあり、充足の難しさを直接示すとは限らない可能性があります。",
    ),
    (
        "S-06",
        "人員減少と募集の並行は、積極的な欠員補充ではなく、採用が進まず欠員が長期化している、または事業縮小に伴う限定的な募集である可能性もあります。",
    ),
    (
        "S-07",
        "周辺地域も同様に働き手が減っていく場合、通勤圏の拡大よりも定着率の向上や業務の省力化が本質的な論点になる可能性もあります。",
    ),
    (
        "S-12",
        "通勤流入が多い地域はすでに広域での求職活動が活発で、配信対象を広げなくても求人情報が届いている可能性もあります。",
    ),
    (
        "S-29",
        "人員が増えている企業は別の業種・職種の拡大による場合もあり、採用の対象が自社と重ならなければ直接の競合ではない可能性もあります。",
    ),
];

/// config::SIGNAL_META からシグナルの軸・方向を引く。
fn signal_meta(id: &str) -> Option<(ClaimAxis, ClaimDirection)> {
    config::SIGNAL_META
        .iter()
        .find(|(sid, _, _)| *sid == id)
        .map(|(_, a, d)| (ClaimAxis::parse(a), ClaimDirection::parse(d)))
}

/// T1 標本数チェック: 引用 evidence の観測数が少ないとき指摘を返す。
///
/// - 企業単位 (granularity=企業) の観測が 1〜2社のみ → 少数例の指摘
/// - 今回CSV集計 (granularity=今回CSV) の標本数 sample_n が 30件未満 → 少数例の指摘
///
/// 企業観測を引用していない (0社) 場合は企業チェックの対象外。
pub(crate) fn t1_sample_size(cited: &[&Evidence]) -> Option<String> {
    let company_n = cited
        .iter()
        .filter(|e| e.granularity == granularity::COMPANY)
        .count();
    if company_n > 0 && company_n < config::TOOLBOX_COMPANY_MIN_OBSERVATIONS {
        return Some(format!(
            "根拠となる企業の観測が{}社分のみで、少数例からの一般化の可能性があります（{}件の観測）。",
            company_n, company_n
        ));
    }
    let min_csv = cited
        .iter()
        .filter(|e| e.granularity == granularity::CSV)
        .filter_map(|e| e.sample_n)
        .min();
    if let Some(n) = min_csv {
        if n < config::TOOLBOX_CSV_SAMPLE_MIN {
            return Some(format!(
                "根拠となる集計のもとの件数が{}件と少なく、少数例からの一般化の可能性があります（{}件の観測）。",
                n, n
            ));
        }
    }
    None
}

/// T2 粒度チェック: 引用根拠がすべて県・全国粒度で、レポート対象が市区町村のとき指摘を返す。
/// 対象市区町村が特定できていない場合は対象外 (粒度の差自体が成立しない)。
pub(crate) fn t2_granularity_gap(cited: &[&Evidence], municipality_known: bool) -> Option<String> {
    if !municipality_known || cited.is_empty() {
        return None;
    }
    let all_broad = cited.iter().all(|e| {
        e.granularity == granularity::PREFECTURE || e.granularity == granularity::NATIONAL
    });
    if all_broad {
        Some(
            "根拠が県・全国の粒度のみで、対象の市区町村の実態とは差がある可能性があります。"
                .to_string(),
        )
    } else {
        None
    }
}

/// T3 反対方向シグナル検索: 考察と同じ軸で逆方向に発火しているシグナルを列挙する。
///
/// - claim が neutral / other、または軸が Other のときは対象外
/// - 考察がすでに引用している evidence を持つシグナルは除外 (生成時に考慮済みのため)
/// - 最大 config::TOOLBOX_OPPOSITE_SIGNAL_MAX 件
pub(crate) fn t3_opposite_signals(
    axis: ClaimAxis,
    direction: ClaimDirection,
    signals: &[Signal],
    cited_ids: &[String],
) -> Option<String> {
    if axis == ClaimAxis::Other {
        return None;
    }
    let opposite = direction.opposite()?;
    let hits: Vec<&Signal> = signals
        .iter()
        .filter(|s| s.fired)
        .filter(|s| signal_meta(&s.id).is_some_and(|(a, d)| a == axis && d == opposite))
        .filter(|s| !s.evidence_ids.iter().any(|id| cited_ids.contains(id)))
        .take(config::TOOLBOX_OPPOSITE_SIGNAL_MAX)
        .collect();
    if hits.is_empty() {
        return None;
    }
    let names = hits
        .iter()
        .map(|s| format!("{}（{}）", s.name, s.id))
        .collect::<Vec<_>>()
        .join("、");
    Some(format!(
        "反対方向の観測もあります: {}。一方向には読み切れない可能性があります。",
        names
    ))
}

/// T4 逆因果辞書: 引用根拠が属する発火シグナルに辞書エントリがあれば逆解釈を返す。
pub(crate) fn t4_reverse_causal(signals: &[Signal], cited_ids: &[String]) -> Vec<String> {
    let mut out = Vec::new();
    for (sig_id, text) in REVERSE_CAUSAL_DICT.iter() {
        let triggered = signals.iter().any(|s| {
            s.id == *sig_id && s.fired && s.evidence_ids.iter().any(|id| cited_ids.contains(id))
        });
        if triggered {
            out.push((*text).to_string());
        }
    }
    out
}

/// 道具箱の実行とコード裁定 (考察は破棄しない)。
///
/// 各考察について:
/// - T1/T2 のいずれか発火 → `refuted=true` + 指摘文を `refute_reason` に
/// - T3/T4 の結果 → `alt_interpretation` に併記 (無ければ None)
/// - 道具箱は決定的なため、全項目 `reviewed=true`
///
/// 戻り値は (裁定後の items, refuted_count, reviewed_count)。
pub fn apply_toolbox(
    mut items: Vec<AiItem>,
    analysis: &ConsultAnalysis,
) -> (Vec<AiItem>, usize, usize) {
    let mut refuted_count = 0usize;
    let mut reviewed_count = 0usize;
    let municipality_known = !analysis.report_meta.municipality.trim().is_empty();
    for item in items.iter_mut() {
        let cited: Vec<&Evidence> = analysis
            .evidence
            .iter()
            .filter(|e| item.evidence_ids.contains(&e.id))
            .collect();

        // T1/T2: 妥当性の指摘 → refuted
        let mut reasons = Vec::new();
        if let Some(r) = t1_sample_size(&cited) {
            reasons.push(r);
        }
        if let Some(r) = t2_granularity_gap(&cited, municipality_known) {
            reasons.push(r);
        }

        // T3/T4: 逆・別の解釈 → 併記
        let axis = ClaimAxis::parse(&item.claim_axis);
        let direction = ClaimDirection::parse(&item.claim_direction);
        let mut alts = Vec::new();
        if let Some(a) = t3_opposite_signals(axis, direction, &analysis.signals, &item.evidence_ids)
        {
            alts.push(a);
        }
        alts.extend(t4_reverse_causal(&analysis.signals, &item.evidence_ids));

        item.reviewed = true;
        reviewed_count += 1;
        if reasons.is_empty() {
            item.refuted = false;
            item.refute_reason = None;
        } else {
            item.refuted = true;
            item.refute_reason = Some(reasons.join(""));
            refuted_count += 1;
        }
        item.alt_interpretation = if alts.is_empty() {
            None
        } else {
            Some(alts.join(""))
        };
    }
    (items, refuted_count, reviewed_count)
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::handlers::consult::ai::contains_forbidden;
    use crate::handlers::consult::evidence::EvidenceKind;
    use crate::handlers::consult::evidence_pack::{analyze, tests::rich_input};

    fn ev(id: &str, gran: &str, sample_n: Option<usize>) -> Evidence {
        Evidence {
            id: id.to_string(),
            kind: EvidenceKind::Observed,
            metric_name: format!("指標{}", id),
            value_text: "1".to_string(),
            unit: "".to_string(),
            source_name: "テスト".to_string(),
            granularity: gran.to_string(),
            sample_n,
            as_of: None,
            note: String::new(),
        }
    }

    fn sig(id: &str, name: &str, fired: bool, ev_ids: &[&str]) -> Signal {
        Signal {
            id: id.to_string(),
            name: name.to_string(),
            fired,
            evidence_ids: ev_ids.iter().map(|s| s.to_string()).collect(),
            interpretation: String::new(),
            alternative_explanations: vec![],
            data_note: String::new(),
        }
    }

    // ---- T1 標本数チェック ----

    #[test]
    fn t1_fires_on_few_company_observations() {
        let e1 = ev("E-001", granularity::COMPANY, None);
        let e2 = ev("E-002", granularity::COMPANY, None);
        let out = t1_sample_size(&[&e1, &e2]).expect("2社の企業観測は少数例として発火");
        assert!(out.contains("2社分"), "社数を明示する: {}", out);
        assert!(out.contains("少数例からの一般化の可能性"));
        assert!(out.contains("可能性"), "可能性表現である");
    }

    #[test]
    fn t1_fires_on_small_csv_sample() {
        let e1 = ev("E-001", granularity::CSV, Some(12));
        let out = t1_sample_size(&[&e1]).expect("標本12件のCSV集計は発火");
        assert!(out.contains("12件"), "件数を明示する: {}", out);
        assert!(out.contains("少数例からの一般化の可能性"));
    }

    #[test]
    fn t1_quiet_when_sample_sufficient() {
        // 3社以上の企業観測 + 標本の大きいCSV集計 → 発火しない
        let e1 = ev("E-001", granularity::COMPANY, None);
        let e2 = ev("E-002", granularity::COMPANY, None);
        let e3 = ev("E-003", granularity::COMPANY, None);
        let e4 = ev("E-004", granularity::CSV, Some(120));
        assert!(t1_sample_size(&[&e1, &e2, &e3, &e4]).is_none());
        // 企業観測を引用していない (0社) 場合は企業チェックの対象外
        let e5 = ev("E-005", granularity::MUNICIPALITY, None);
        assert!(t1_sample_size(&[&e5]).is_none());
        // sample_n の無いCSV集計は判定不能なので発火しない (勝手に少数扱いしない)
        let e6 = ev("E-006", granularity::CSV, None);
        assert!(t1_sample_size(&[&e6]).is_none());
    }

    // ---- T2 粒度チェック ----

    #[test]
    fn t2_fires_when_only_broad_granularity() {
        let e1 = ev("E-001", granularity::PREFECTURE, None);
        let e2 = ev("E-002", granularity::NATIONAL, None);
        let out = t2_granularity_gap(&[&e1, &e2], true).expect("県・全国のみは粒度差を指摘");
        assert!(out.contains("県・全国"));
        assert!(out.contains("市区町村"));
        assert!(out.contains("可能性"));
    }

    #[test]
    fn t2_quiet_when_municipal_evidence_or_unknown_target() {
        let pref = ev("E-001", granularity::PREFECTURE, None);
        let muni = ev("E-002", granularity::MUNICIPALITY, None);
        // 市区町村粒度の根拠を含む → 発火しない
        assert!(t2_granularity_gap(&[&pref, &muni], true).is_none());
        // 対象市区町村が不明 → 対象外
        assert!(t2_granularity_gap(&[&pref], false).is_none());
        // 引用が空 → 対象外 (validate_items 通過後は起きないが防御)
        assert!(t2_granularity_gap(&[], true).is_none());
    }

    // ---- T3 反対方向シグナル検索 ----

    #[test]
    fn t3_lists_opposite_fired_signals_up_to_max() {
        // claim: supply/problem → 逆方向は supply/opportunity (S-09, S-12, S-21)
        let signals = vec![
            sig("S-09", "転職希望層が全国比で厚い", true, &["E-101"]),
            sig("S-12", "周辺地域からの通勤流入が多い", true, &["E-102"]),
            sig("S-21", "失業率が全国比で高い (余剰寄り)", true, &["E-103"]),
            // 同軸・同方向 (problem) は対象外
            sig("S-07", "働き手人口が減少する見込みの地域", true, &["E-104"]),
            // 逆方向でも未発火は対象外
            sig("S-11", "有効求人倍率が低い", false, &["E-105"]),
        ];
        let out = t3_opposite_signals(
            ClaimAxis::Supply,
            ClaimDirection::Problem,
            &signals,
            &["E-000".to_string()],
        )
        .expect("反対方向の発火シグナルがあれば併記");
        // 最大2件 (TOOLBOX_OPPOSITE_SIGNAL_MAX) なので S-09, S-12 のみ
        assert!(out.contains("S-09") && out.contains("S-12"));
        assert!(!out.contains("S-21"), "上限2件を超えない: {}", out);
        assert!(!out.contains("S-07"), "同方向は含めない");
        assert!(!out.contains("S-11"), "未発火は含めない");
        assert!(out.contains("反対方向の観測"));
        assert!(out.contains("可能性"));
    }

    #[test]
    fn t3_excludes_signals_already_cited_by_item() {
        let signals = vec![sig(
            "S-12",
            "周辺地域からの通勤流入が多い",
            true,
            &["E-102"],
        )];
        // 考察が S-12 の根拠 E-102 を引用している → 生成時に考慮済みとして除外
        let out = t3_opposite_signals(
            ClaimAxis::Supply,
            ClaimDirection::Problem,
            &signals,
            &["E-102".to_string()],
        );
        assert!(out.is_none());
    }

    #[test]
    fn t3_quiet_for_neutral_or_other_claims() {
        let signals = vec![sig(
            "S-12",
            "周辺地域からの通勤流入が多い",
            true,
            &["E-102"],
        )];
        let none_ids: Vec<String> = vec![];
        // neutral は逆方向が定義できない
        assert!(t3_opposite_signals(
            ClaimAxis::Supply,
            ClaimDirection::Neutral,
            &signals,
            &none_ids
        )
        .is_none());
        // 軸 Other は検索対象外
        assert!(t3_opposite_signals(
            ClaimAxis::Other,
            ClaimDirection::Problem,
            &signals,
            &none_ids
        )
        .is_none());
    }

    // ---- T4 逆因果辞書 ----

    #[test]
    fn t4_triggers_only_for_cited_fired_dict_signals() {
        let signals = vec![
            sig(
                "S-06",
                "人員減少中でも募集を継続する企業の存在",
                true,
                &["E-011", "E-012"],
            ),
            sig("S-12", "周辺地域からの通勤流入が多い", true, &["E-018"]),
            sig(
                "S-29",
                "人員を増やしながら募集する企業の存在",
                false,
                &["E-036"],
            ),
        ];
        // S-06 の根拠を引用 → S-06 の逆因果が出る
        let out = t4_reverse_causal(&signals, &["E-011".to_string()]);
        assert_eq!(out.len(), 1);
        assert!(out[0].contains("事業縮小") || out[0].contains("欠員が長期化"));
        // S-29 は未発火なので、根拠を引用しても出ない
        let out2 = t4_reverse_causal(&signals, &["E-036".to_string()]);
        assert!(out2.is_empty());
        // 辞書シグナルの根拠を引用していない → 出ない
        let out3 = t4_reverse_causal(&signals, &["E-999".to_string()]);
        assert!(out3.is_empty());
    }

    #[test]
    fn t4_dict_texts_are_possibility_phrased_and_clean() {
        for (id, text) in REVERSE_CAUSAL_DICT.iter() {
            assert!(text.contains("可能性"), "{} は可能性表現である", id);
            assert!(!contains_forbidden(text), "{} に禁止表現がない", id);
            // 辞書のキーは実在シグナル (SIGNAL_META に載っている)
            assert!(
                signal_meta(id).is_some(),
                "{} は SIGNAL_META に存在する",
                id
            );
        }
    }

    // ---- シグナルメタデータの網羅性 ----

    #[test]
    fn signal_meta_covers_exactly_s01_to_s30_with_valid_values() {
        assert_eq!(config::SIGNAL_META.len(), 30);
        let expected: Vec<String> = (1..=30).map(|i| format!("S-{:02}", i)).collect();
        let actual: Vec<&str> = config::SIGNAL_META.iter().map(|(id, _, _)| *id).collect();
        assert_eq!(
            actual,
            expected.iter().map(|s| s.as_str()).collect::<Vec<_>>()
        );
        for (id, axis, dir) in config::SIGNAL_META.iter() {
            assert!(
                ["demand", "supply", "competition", "offer", "other"].contains(axis),
                "{} の axis が不正: {}",
                id,
                axis
            );
            assert!(
                ["problem", "opportunity", "neutral"].contains(dir),
                "{} の direction が不正: {}",
                id,
                dir
            );
        }
    }

    #[test]
    fn claim_parse_falls_back_explicitly() {
        assert_eq!(ClaimAxis::parse("supply"), ClaimAxis::Supply);
        assert_eq!(ClaimAxis::parse("  demand "), ClaimAxis::Demand);
        assert_eq!(ClaimAxis::parse("unknown"), ClaimAxis::Other);
        assert_eq!(ClaimAxis::parse(""), ClaimAxis::Other);
        assert_eq!(ClaimDirection::parse("problem"), ClaimDirection::Problem);
        assert_eq!(ClaimDirection::parse("junk"), ClaimDirection::Neutral);
        assert_eq!(
            ClaimDirection::Problem.opposite(),
            Some(ClaimDirection::Opportunity)
        );
        assert_eq!(
            ClaimDirection::Opportunity.opposite(),
            Some(ClaimDirection::Problem)
        );
        assert_eq!(ClaimDirection::Neutral.opposite(), None);
    }

    // ---- 裁定 (apply_toolbox) の3状態 ----

    /// rich_input の分析結果から、指定粒度の evidence id を探すヘルパ
    fn find_evidence_ids(analysis: &ConsultAnalysis, gran: &str, n: usize) -> Vec<String> {
        analysis
            .evidence
            .iter()
            .filter(|e| e.granularity == gran)
            .take(n)
            .map(|e| e.id.clone())
            .collect()
    }

    #[test]
    fn adjudicate_three_states_deterministically() {
        let analysis = analyze(&rich_input());

        // 状態1: 企業観測2社を引用 → T1 発火 → refuted
        let company_ids = find_evidence_ids(&analysis, granularity::COMPANY, 2);
        assert!(!company_ids.is_empty(), "テスト前提: 企業粒度の証拠がある");
        // 状態2: 県粒度のみ引用 + supply/problem タグ → T2 発火 (市区町村対象) or T3 併記
        let pref_ids = find_evidence_ids(&analysis, granularity::PREFECTURE, 2);
        assert!(!pref_ids.is_empty(), "テスト前提: 県粒度の証拠がある");
        // 状態3: 市区町村粒度 (S-23 自然減など辞書外) + other/neutral → 注記なしを狙う
        let muni_ids = find_evidence_ids(&analysis, granularity::MUNICIPALITY, 1);
        assert!(!muni_ids.is_empty(), "テスト前提: 市区町村粒度の証拠がある");

        let items = vec![
            AiItem {
                title: "企業観測ベースの考察".to_string(),
                body: "欠員補充型の採用が発生している可能性があります".to_string(),
                evidence_ids: company_ids,
                caveat: "".to_string(),
                claim_axis: "competition".to_string(),
                claim_direction: "problem".to_string(),
                ..Default::default()
            },
            AiItem {
                title: "県粒度のみの考察".to_string(),
                body: "供給が細っている可能性があります".to_string(),
                evidence_ids: pref_ids,
                caveat: "".to_string(),
                claim_axis: "supply".to_string(),
                claim_direction: "problem".to_string(),
                ..Default::default()
            },
            AiItem {
                title: "中立の考察".to_string(),
                body: "地域の背景として押さえておく論点の可能性があります".to_string(),
                evidence_ids: muni_ids,
                caveat: "".to_string(),
                claim_axis: "other".to_string(),
                claim_direction: "neutral".to_string(),
                ..Default::default()
            },
        ];

        let (out, refuted_count, reviewed_count) = apply_toolbox(items, &analysis);

        // 道具箱は決定的: 全項目 reviewed=true
        assert_eq!(reviewed_count, 3);
        assert!(out.iter().all(|i| i.reviewed));

        // 状態1: T1 (企業2社) で refuted
        assert!(out[0].refuted, "企業観測2社の考察は「確認が必要」になる");
        let reason = out[0].refute_reason.as_deref().unwrap();
        assert!(reason.contains("少数例からの一般化の可能性"), "{}", reason);

        // 状態2: 県粒度のみ → T2 で refuted (rich_input は市区町村が特定されている)
        assert!(out[1].refuted, "県粒度のみの考察は粒度差の指摘を受ける");
        assert!(out[1]
            .refute_reason
            .as_deref()
            .unwrap()
            .contains("県・全国の粒度のみ"));

        // 状態3: T1/T2 とも発火せず refuted=false。other/neutral なので T3 もなし
        assert!(!out[2].refuted, "注記対象外の考察は refuted にならない");
        assert!(out[2].refute_reason.is_none());

        assert_eq!(refuted_count, 2);

        // 全出力テキストが禁止表現を含まない
        for item in &out {
            if let Some(r) = &item.refute_reason {
                assert!(!contains_forbidden(r));
            }
            if let Some(a) = &item.alt_interpretation {
                assert!(!contains_forbidden(a));
            }
        }
    }

    #[test]
    fn adjudicate_alt_only_state_via_t3_or_t4() {
        // rich_input では S-12 (通勤流入・supply/opportunity) が発火する。
        // supply/opportunity と逆の supply/problem シグナル (S-07 等) も発火するため、
        // 「supply/opportunity タグ + 市区町村粒度根拠」の考察には T3 の別の見方が付く。
        let analysis = analyze(&rich_input());
        assert!(
            analysis.signals.iter().any(|s| s.id == "S-07" && s.fired),
            "テスト前提: S-07 (働き手減少) が発火している"
        );
        let muni_ids = find_evidence_ids(&analysis, granularity::MUNICIPALITY, 1);
        let items = vec![AiItem {
            title: "通勤圏の広がりを活かす考察".to_string(),
            body: "配信地域を通勤圏まで広げる余地がある可能性があります".to_string(),
            evidence_ids: muni_ids,
            caveat: "".to_string(),
            claim_axis: "supply".to_string(),
            claim_direction: "opportunity".to_string(),
            ..Default::default()
        }];
        let (out, refuted_count, _) = apply_toolbox(items, &analysis);
        assert_eq!(refuted_count, 0, "T3/T4 のみでは refuted にしない");
        assert!(!out[0].refuted);
        let alt = out[0]
            .alt_interpretation
            .as_deref()
            .expect("反対方向の発火シグナルがあるので別の見方が付く");
        assert!(alt.contains("反対方向の観測"), "{}", alt);
    }
}
