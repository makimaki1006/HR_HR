//! 解説資料 AI パイプライン (flash-lite 作成/レビュー分離、2026-07-17)
//!
//! 目的: 手作業レビューで到達した解説資料の品質 (観測→「だから」+複合考察) を、
//! Gemini flash-lite の多段呼び出しで再現する。役割分担は consult/ai.rs と同じ思想:
//! **数値・事実はコード側で確定し、LLM は解釈文の執筆と自己批判のみを行う**。
//!
//! ## パイプライン (1部あたり Gemini 2〜3回、上限5回設計)
//! ```text
//! Rust: 事実インベントリ構築 (build_fact_inventory)
//!     検証済み数値だけを GuideFact {id, theme, statement, numbers} に列挙
//!   → ①作成コール: 各事実の「だから」+ 複合考察 + 次の一手 を起草
//!   → ②レビューコール: 別プロンプトの逆証明者が起草を批判
//!       (数値の出所・因果断定・分母無視・反対解釈の見落とし)
//!   → ③修正コール: 指摘があるときだけ、指摘を反映して書き直し
//!   → Rust 最終ガード: 禁止表現 / 言い過ぎ / fact_id 実在 / 数値出所 (下記)
//! ```
//!
//! ## 数値出所ガード (ハルシネーション遮断)
//! AI 出力文中の数値トークンは、事実インベントリに列挙した数値の集合に
//! 含まれなければならない (`numbers_ok`)。LLM への指示だけに頼らず、
//! コードで機械検査する (「正確さはモデルの賢さではなくガードで決まる」)。
//!
//! ## graceful degradation
//! API キー未設定・呼び出し失敗・最終ガード不合格のいずれでも panic せず None を
//! 返し、呼び出し側 (handlers.rs) は決定的テンプレ版 (guide.rs) へフォールバックする。

#![allow(dead_code)]

// パス解析 (現在位置: survey::report_html::navy_report::guide_ai):
//   super::super::super::super = handlers
use super::super::super::super::consult::ai::contains_forbidden;
use super::super::super::super::consult::theme::has_overclaim;
use super::super::super::super::helpers::{escape_html, format_number, get_f64};
use super::super::super::super::insight::fetch::InsightContext;
use super::super::super::aggregator::{CardBrief, SurveyAggregation};
use crate::gemini::GeminiClient;
use serde::{Deserialize, Serialize};
use serde_json::{json, Value};
use std::collections::HashSet;

// ============================================================
// 事実インベントリ
// ============================================================

/// LLM に渡す確定事実 1 件。statement は数値込みの完成文 (コードが計算)。
/// numbers は出所ガード (`numbers_ok`) 用に、この事実が正当化する数値トークン。
#[derive(Debug, Clone, Serialize)]
pub(super) struct GuideFact {
    pub id: String,
    pub theme: String,
    pub statement: String,
    pub numbers: Vec<String>,
}

/// §1 貴社の現在地 (表はコードが決定的に描く。AI は解釈のみ)。
#[derive(Debug, Clone, Serialize)]
pub(super) struct CompanyPosition {
    pub name: String,
    pub count: usize,
    pub own_median: i64,
    pub market_median: Option<i64>,
    pub percentile_from_below: Option<f64>,
}

fn median_of(values: &[i64]) -> Option<i64> {
    if values.is_empty() {
        return None;
    }
    let mut v: Vec<i64> = values.to_vec();
    v.sort_unstable();
    Some(v[v.len() / 2])
}

fn man_yen(v: i64) -> String {
    format!("{:.1}万円", v as f64 / 10_000.0)
}

/// 数値トークン正規化: カンマ除去・末尾の小数点除去。
fn norm_num(s: &str) -> String {
    s.replace(',', "").trim_end_matches('.').to_string()
}

/// text 内の数値トークンを抽出する (整数・小数・カンマ区切り)。
fn extract_numbers(text: &str) -> Vec<String> {
    let mut out = Vec::new();
    let mut cur = String::new();
    for c in text.chars() {
        if c.is_ascii_digit() || c == '.' || c == ',' {
            cur.push(c);
        } else if !cur.is_empty() {
            out.push(norm_num(&cur));
            cur.clear();
        }
    }
    if !cur.is_empty() {
        out.push(norm_num(&cur));
    }
    out.into_iter().filter(|t| !t.is_empty()).collect()
}

/// 事実インベントリを構築する。数値はすべて agg / ctx 由来 (LLM には計算させない)。
/// 各 statement は「観測」のみで「だから」を含まない (それが LLM の仕事)。
pub(super) fn build_fact_inventory(
    agg: &SurveyAggregation,
    ctx: Option<&InsightContext>,
    company: Option<&str>,
) -> (Vec<GuideFact>, Option<CompanyPosition>) {
    let mut facts: Vec<GuideFact> = Vec::new();
    let mut push = |id: &str, theme: &str, statement: String| {
        let mut numbers = extract_numbers(&statement);
        numbers.sort();
        numbers.dedup();
        facts.push(GuideFact {
            id: id.to_string(),
            theme: theme.to_string(),
            statement,
            numbers,
        });
    };

    // F-SIZE: 土俵の広さと動き
    if agg.total_count > 0 {
        let new_pct = (agg.new_count as f64 / agg.total_count as f64 * 100.0).round();
        let mut munis: Vec<(&str, usize)> = agg
            .by_municipality_salary
            .iter()
            .map(|m| (m.name.as_str(), m.count))
            .collect();
        munis.sort_by(|a, b| b.1.cmp(&a.1));
        let top3 = munis
            .iter()
            .take(3)
            .map(|(n, c)| format!("{} {}件", n, c))
            .collect::<Vec<_>>()
            .join("・");
        push(
            "F-SIZE",
            "市場規模",
            format!(
                "重複整理後の求人は {} 件。掲載から間もない求人の割合 (目安) は {:.0}%。勤務地の上位は {}。検索地の市内に限らず通勤圏に広がる。",
                format_number(agg.total_count as i64),
                new_pct,
                if top3.is_empty() { "不明".to_string() } else { top3 },
            ),
        );
    }

    // F-SAL: 給与
    let lo = median_of(&agg.salary_min_values);
    let hi = median_of(&agg.salary_max_values);
    if let (Some(lo), Some(hi)) = (lo, hi) {
        let spread_man = (hi - lo) as f64 / 10_000.0;
        let range_pct = if !agg.salary_min_values.is_empty() {
            let with_range = agg.scatter_min_max.iter().filter(|p| p.y > p.x).count();
            (with_range as f64 / agg.salary_min_values.len() as f64 * 100.0).min(100.0)
        } else {
            0.0
        };
        push(
            "F-SAL",
            "給与",
            format!(
                "下限給与の中央値 {} / 上限給与の中央値 {} (開き {:.0}万円)。給与欄に幅 (下限〜上限) を示す求人はおよそ {:.0}% (下限給与が取れた求人比)。",
                man_yen(lo),
                man_yen(hi),
                spread_man,
                range_pct,
            ),
        );
    }

    // F-HOL: 年間休日
    let jb = &agg.jobbox;
    if jb.annual_holidays_values.len() >= 20 {
        let med = median_of(&jb.annual_holidays_values).unwrap_or(0);
        let corr = jb
            .salary_holidays_correlation
            .map(|r| format!(" 休日と給与の相関係数は r={:.2} (ほぼ無相関なら休日と給与は独立)。", r))
            .unwrap_or_default();
        push(
            "F-HOL",
            "年間休日",
            format!(
                "年間休日の記載を抽出できた {} 件のうち、120日以上は {:.0}%、125日以上は {:.0}% (中央値 {} 日)。記載がない求人は含まれない。{}",
                format_number(jb.annual_holidays_values.len() as i64),
                jb.holiday_pct_ge_120 * 100.0,
                jb.holiday_pct_ge_125 * 100.0,
                med,
                corr,
            ),
        );
    }

    // F-TAG: 訴求タグ
    let mut tags: Vec<_> = agg
        .by_tag_salary
        .iter()
        .filter(|t| t.count >= 10 && t.diff_percent > 0.0)
        .collect();
    tags.sort_by(|a, b| {
        b.diff_percent
            .partial_cmp(&a.diff_percent)
            .unwrap_or(std::cmp::Ordering::Equal)
    });
    if !tags.is_empty() {
        let list = tags
            .iter()
            .take(3)
            .map(|t| format!("「{}」({}件、市場平均比 +{:.0}%)", t.tag, t.count, t.diff_percent))
            .collect::<Vec<_>>()
            .join("・");
        push(
            "F-TAG",
            "訴求",
            format!("給与が市場平均より高い側に分布するタグ: {}。相関であり因果ではない。", list),
        );
    }

    // F-POP: 人気表示
    let p = &agg.popularity;
    if p.indeed_sp_total > 0 && (p.popular_count + p.super_popular_count) > 0 {
        let mut s = format!(
            "媒体の表示で「人気」「超人気」が付いた求人は {} 件 (対象 {} 件中 {:.0}%)。",
            format_number((p.popular_count + p.super_popular_count) as i64),
            format_number(p.indeed_sp_total as i64),
            p.popular_ratio * 100.0,
        );
        if let (Some(pm), Some(nm)) = (p.popular_salary_median, p.non_popular_salary_median) {
            if p.popular_n_salary >= 5 && p.non_popular_n_salary >= 5 {
                s.push_str(&format!(
                    " 人気表示つきの月給中央値 {} / なし {}。",
                    man_yen(pm),
                    man_yen(nm)
                ));
            }
        }
        s.push_str(" 人気表示の基準は媒体側の非公開ロジック。");
        push("F-POP", "人気表示", s);
    }

    // F-DEM: 需給 (県・産業計)
    if let Some(c) = ctx {
        let ratio = c
            .ext_job_ratio
            .last()
            .map(|r| get_f64(r, "ratio_total"))
            .filter(|v| v.is_finite() && *v > 0.0);
        if let Some(r) = ratio {
            push(
                "F-DEM",
                "需給",
                format!(
                    "直近の有効求人倍率は {:.2} 倍。県単位・産業計の参考値であり、対象職種の実勢とは差がある可能性がある。",
                    r
                ),
            );
        }

        // F-COM: 通勤構造
        if c.commute_inflow_total > 0 || c.commute_outflow_total > 0 {
            let top = c
                .commute_inflow_top3
                .iter()
                .take(3)
                .map(|(_, m, cnt)| format!("{} {}人", m, format_number(*cnt)))
                .collect::<Vec<_>>()
                .join("・");
            // 大小関係 (方向) はコードで確定して明記する (LLM の方向読み違え防止)
            let direction = if c.commute_outflow_total > c.commute_inflow_total {
                "流出が流入を上回る、働き手が市外へ出ていく構造"
            } else {
                "流入が流出を上回る、働き手が市外から入ってくる構造"
            };
            push(
                "F-COM",
                "通勤",
                format!(
                    "市外へ通勤する人 {} 人 / 市外から来る人 {} 人 / 市内で完結する通勤 {:.1}% (国勢調査 OD)。{}。周辺からの流入元の上位は {}。",
                    format_number(c.commute_outflow_total),
                    format_number(c.commute_inflow_total),
                    c.commute_self_rate * 100.0,
                    direction,
                    if top.is_empty() { "不明".to_string() } else { top },
                ),
            );
        }
    }

    // F-CO: 貴社の現在地 (company 指定 + ヒット時)
    let position = company.filter(|s| !s.trim().is_empty()).and_then(|name| {
        let hit = agg
            .by_company
            .iter()
            .filter(|co| co.name.contains(name.trim()) || name.trim().contains(co.name.as_str()))
            .max_by_key(|co| co.count)?;
        let market_median = median_of(&agg.salary_values);
        let pct = if agg.salary_values.is_empty() {
            None
        } else {
            let below = agg
                .salary_values
                .iter()
                .filter(|v| **v <= hit.median_salary)
                .count();
            Some(below as f64 / agg.salary_values.len() as f64 * 100.0)
        };
        Some(CompanyPosition {
            name: hit.name.clone(),
            count: hit.count,
            own_median: hit.median_salary,
            market_median,
            percentile_from_below: pct,
        })
    });
    if let Some(pos) = &position {
        let mut s = format!(
            "依頼企業「{}」の求人 {} 件が収集データ内にある。提示給与の中央値 {} (下限と上限の中間値ベース)。",
            pos.name,
            pos.count,
            man_yen(pos.own_median),
        );
        if let Some(m) = pos.market_median {
            s.push_str(&format!(" 市場全体の中央値は {}。", man_yen(m)));
        }
        if let Some(p) = pos.percentile_from_below {
            s.push_str(&format!(" 分布の下位からおよそ {:.0}% の位置。", p));
        }
        push("F-CO", "依頼企業", s);

        // Phase 2a (2026-07-20): カード単位の観測。依頼企業の求人カードそのものを
        // 市場分布に重ねる (給与欄の形式 / 説明文との乖離 / 休日記載の市場内位置)。
        // 数値・判定はすべてコードで確定し、LLM には解釈だけを書かせる。
        let name = pos.name.as_str();
        let cards: Vec<&CardBrief> = agg
            .card_briefs
            .iter()
            .filter(|cb| cb.company.contains(name) || name.contains(cb.company.as_str()))
            .take(2)
            .collect();
        for (i, cb) in cards.iter().enumerate() {
            if let Some(s) = build_card_statement(cb, agg) {
                push(&format!("F-CO-CARD{}", i + 1), "貴社求人カード", s);
            }
        }
    }

    (facts, position)
}

/// カード 1 枚の観測文を組み立てる (Phase 2a)。
///
/// 判定 (単一値か幅か / 説明文給与との乖離 / 休日の市場内帯) はすべてここで確定し、
/// LLM には解釈のみを書かせる。観測できる要素が一つもなければ None。
fn build_card_statement(cb: &CardBrief, agg: &SurveyAggregation) -> Option<String> {
    let mut parts: Vec<String> = Vec::new();

    // 給与欄の形式
    if cb.is_monthly {
        match (cb.salary_min, cb.salary_max) {
            (Some(lo), Some(hi)) if hi > lo => {
                parts.push(format!("給与欄は {}〜{} の幅表示", man_yen(lo), man_yen(hi)));
            }
            (Some(lo), _) => {
                parts.push(format!("給与欄は単一値 {} (幅の表示なし)", man_yen(lo)));
            }
            _ => {}
        }
    }

    // 説明文中の給与記載と給与欄の乖離。給与欄の最大値 (上限、無ければ下限) を
    // 上回るときだけ「反映されていない」と言う (同額なら乖離ではない)。
    if let Some(d) = cb.desc_salary_man {
        let field_best = cb.salary_max.into_iter().chain(cb.salary_min).max();
        if field_best.map_or(false, |h| d * 10_000 > h) {
            parts.push(format!(
                "説明文中には月収{}万円の記載があるが、給与欄の上限には反映されていない",
                d
            ));
        }
    }

    // 年間休日の記載と市場内の帯
    if let Some(h) = cb.annual_holidays {
        let mut s = format!("年間休日の記載 {} 日", h);
        if agg.jobbox.annual_holidays_values.len() >= 20 {
            if h >= 125 {
                s.push_str(&format!(
                    " (市場で125日以上を明示する求人は {:.0}% — この上位帯に入る)",
                    agg.jobbox.holiday_pct_ge_125 * 100.0
                ));
            } else if h >= 120 {
                s.push_str(&format!(
                    " (市場の120日以上 {:.0}% と同じ帯)",
                    agg.jobbox.holiday_pct_ge_120 * 100.0
                ));
            }
        }
        parts.push(s);
    }

    // 新着表示
    if !cb.is_new {
        parts.push("新着表示はない (掲載から時間が経過している可能性)".to_string());
    }

    if parts.is_empty() {
        return None;
    }
    let title = if cb.title.is_empty() {
        "タイトル不明".to_string()
    } else {
        cb.title.clone()
    };
    Some(format!("カード「{}」: {}。", title, parts.join("。")))
}

// ============================================================
// LLM 呼び出し (作成 → レビュー → 修正)
// ============================================================

/// 作成・修正コールの出力。
#[derive(Debug, Clone, Default, Deserialize)]
pub(super) struct GuideDraft {
    pub lead: String,
    pub per_fact: Vec<PerFact>,
    pub composites: Vec<GuideComposite>,
    pub next_steps: Vec<String>,
}

#[derive(Debug, Clone, Default, Deserialize)]
pub(super) struct PerFact {
    pub fact_id: String,
    pub dakara: String,
}

#[derive(Debug, Clone, Default, Deserialize)]
pub(super) struct GuideComposite {
    pub title: String,
    pub thesis: String,
    pub fact_ids: Vec<String>,
    pub so_what: String,
}

/// レビューコールの出力。
#[derive(Debug, Clone, Default, Deserialize)]
struct ReviewResult {
    verdict: String,
    findings: Vec<ReviewFinding>,
}

#[derive(Debug, Clone, Default, Deserialize)]
struct ReviewFinding {
    location: String,
    problem: String,
    fix: String,
}

const GUIDE_SYSTEM: &str = "\
あなたは採用市場レポートの解説資料を書く執筆者です。読み手はレポートを受け取る企業の担当者です。以下を厳守してください。\n\
1. 事実インベントリ (facts) に書かれた数値・事実以外を一切書かない。新しい数値を計算しない (割り算・掛け算・差の計算も禁止。facts に書いてある数値だけを使う)。\n\
2. 各事実の dakara は「その数字が読み手の求人にとって何を意味するか」の着地文。数字の言い換えではなく、読み手が明日やることが変わる含意を書く。\n\
   - 悪い例 (空虚。禁止): 「〜を検討する余地があるかもしれません」「〜を注視する必要があります」「〜を把握することが重要です」\n\
   - 良い例の型: 「(観測の核心。市場の過半と同じ/上回る/下回る等の位置づけ)ため、(具体的な打ち手の方向)が検討候補になります」\n\
   - 良い例 (数値は伏せ字。実際は facts の数値を使う): 「下限給与は市場の中央値と同水準のため、下限額で見劣りしているわけではない可能性があります。差が出ているのは上限側なので、昇給後の到達額を給与欄の上限として見せることが検討候補になります」\n\
3. 数字の大小関係を正しく読む。例えば流出が流入を上回るなら「働き手が市外へ出ていく構造」であり、その逆ではない。方向を取り違えない。\n\
4. すべて可能性表現 (「〜の可能性があります」「〜とみられます」)。断定・因果の断定 (「〜だから応募が増える」等) は禁止。\n\
5. composites は複数テーマの facts を編んだ考察を2〜4本。単一テーマの言い換えや「関連性がある可能性があります」のような中身のない文は不可。2つの数字を重ねたときに初めて言えることを書く。\n\
6. 誇張しない。禁止語: 必ず・確実に・完璧・絶対・劇的・問題ない。\n\
7. 平易な言葉で書く。専門用語・略語・社内用語は使わない。\n\
出力は日本語。";

const REVIEW_SYSTEM: &str = "\
あなたは解説資料の逆証明レビュアーです。起草 (draft) を事実インベントリ (facts) と突き合わせ、以下の観点で問題を全て挙げてください。\n\
1. 数値の出所: draft 中の数値が facts に存在するか。facts に無い数値・計算された数値は即指摘。\n\
2. 因果の断定: 相関しか示せないデータで因果を断定していないか。\n\
3. 分母と粒度: 比率・統計の分母や粒度 (県単位・記載ありのみ等) を無視した言い回しがないか。\n\
4. 大小関係の方向: 数字の大小 (流出と流入、比率の過半かどうか等) から言える方向を取り違えていないか。例えば流出が流入を上回るのに「流入がある」側の解釈だけを書くのは方向の誤り。\n\
5. 反対解釈: 同じ数字から逆の解釈が成り立つのに一方だけを書いていないか。\n\
6. 着地の空虚さ: dakara や so_what が「検討する余地がある」「注視する必要がある」「把握することが重要」のような、読み手の行動が何も変わらない文になっていないか。空虚な着地は必ず指摘し、その数字の位置づけ (過半と同じ/上回る/下回る) から言える具体的な打ち手の方向を修正案として書く。\n\
7. 誇張・断定表現。\n\
問題が一つも無ければ verdict を pass、あれば needs_fix とし、findings に場所 (fact_id やタイトル)・問題・修正案を書く。指摘は厳しく、見逃しなく。出力は日本語。";

fn draft_schema() -> Value {
    json!({
        "type": "object",
        "properties": {
            "lead": { "type": "string" },
            "per_fact": {
                "type": "array",
                "items": {
                    "type": "object",
                    "properties": {
                        "fact_id": { "type": "string" },
                        "dakara": { "type": "string" }
                    },
                    "required": ["fact_id", "dakara"]
                }
            },
            "composites": {
                "type": "array",
                "items": {
                    "type": "object",
                    "properties": {
                        "title": { "type": "string" },
                        "thesis": { "type": "string" },
                        "fact_ids": { "type": "array", "items": { "type": "string" } },
                        "so_what": { "type": "string" }
                    },
                    "required": ["title", "thesis", "fact_ids", "so_what"]
                }
            },
            "next_steps": { "type": "array", "items": { "type": "string" } }
        },
        "required": ["lead", "per_fact", "composites", "next_steps"]
    })
}

fn review_schema() -> Value {
    json!({
        "type": "object",
        "properties": {
            "verdict": { "type": "string", "enum": ["pass", "needs_fix"] },
            "findings": {
                "type": "array",
                "items": {
                    "type": "object",
                    "properties": {
                        "location": { "type": "string" },
                        "problem": { "type": "string" },
                        "fix": { "type": "string" }
                    },
                    "required": ["location", "problem", "fix"]
                }
            }
        },
        "required": ["verdict", "findings"]
    })
}

fn facts_json(facts: &[GuideFact]) -> String {
    serde_json::to_string(&json!({ "facts": facts })).unwrap_or_else(|_| "{}".to_string())
}

fn parse_draft(v: &Value) -> Option<GuideDraft> {
    serde_json::from_value(v.clone()).ok()
}

// ============================================================
// Rust 最終ガード
// ============================================================

/// 文中の数値トークンが許可集合に含まれるか。
/// 1桁の数値 (「3つ」等の序数) は許可。2桁以上は facts 由来でなければ不合格。
pub(super) fn numbers_ok(text: &str, allowed: &HashSet<String>) -> bool {
    extract_numbers(text)
        .iter()
        .all(|t| t.chars().filter(|c| c.is_ascii_digit()).count() <= 1 || allowed.contains(t))
}

/// ガード検査結果 (機械検査の指摘リスト。修正コールへのフィードバックにも使う)。
pub(super) fn guard_violations(draft: &GuideDraft, facts: &[GuideFact]) -> Vec<String> {
    let allowed: HashSet<String> = facts.iter().flat_map(|f| f.numbers.iter().cloned()).collect();
    let fact_ids: HashSet<&str> = facts.iter().map(|f| f.id.as_str()).collect();
    let themes_of = |ids: &[String]| -> usize {
        let mut ts: HashSet<&str> = HashSet::new();
        for id in ids {
            if let Some(f) = facts.iter().find(|f| f.id == *id) {
                ts.insert(f.theme.as_str());
            }
        }
        ts.len()
    };
    /// 空虚な着地の定型句 (読み手の行動が変わらない文)。致命扱いはしないが
    /// 指摘として修正コールを強制起動する (2026-07-17 run1/run2 実測で 4 箇所検出)。
    const VAGUE_PHRASES: [&str; 6] = [
        "検討する余地",
        "注視する必要",
        "把握することが重要",
        "把握し",
        "意識することが重要",
        "工夫が必要かもしれません",
    ];

    fn text_issues(label: &str, text: &str, allowed: &HashSet<String>) -> Vec<String> {
        let mut out = Vec::new();
        if contains_forbidden(text) {
            out.push(format!("{}: 禁止表現を含む", label));
        }
        if has_overclaim(text) {
            out.push(format!("{}: 言い過ぎ表現 (希少・皆無等の断定) を含む", label));
        }
        if !numbers_ok(text, allowed) {
            out.push(format!("{}: 事実インベントリに無い数値を含む", label));
        }
        if VAGUE_PHRASES.iter().any(|p| text.contains(p)) {
            out.push(format!(
                "{}: 空虚な着地 (「検討する余地」等)。数字の位置づけから言える具体的な打ち手の方向に書き直す",
                label
            ));
        }
        out
    }

    let mut v: Vec<String> = Vec::new();
    v.extend(text_issues("lead", &draft.lead, &allowed));
    for pf in &draft.per_fact {
        if !fact_ids.contains(pf.fact_id.as_str()) {
            v.push(format!("per_fact {}: 実在しない fact_id", pf.fact_id));
            continue;
        }
        // 数値ガードは全 facts の数値を許可 (statement 引用を許容)
        v.extend(text_issues(
            &format!("per_fact {}", pf.fact_id),
            &pf.dakara,
            &allowed,
        ));
    }
    for c in &draft.composites {
        if c.fact_ids.is_empty() || c.fact_ids.iter().any(|id| !fact_ids.contains(id.as_str())) {
            v.push(format!("composite「{}」: fact_ids が空か実在しない id を含む", c.title));
            continue;
        }
        if themes_of(&c.fact_ids) < 2 {
            v.push(format!("composite「{}」: 2テーマ未満 (複合になっていない)", c.title));
        }
        v.extend(text_issues(
            &format!("composite「{}」thesis", c.title),
            &c.thesis,
            &allowed,
        ));
        v.extend(text_issues(
            &format!("composite「{}」so_what", c.title),
            &c.so_what,
            &allowed,
        ));
    }
    for (i, s) in draft.next_steps.iter().enumerate() {
        v.extend(text_issues(&format!("next_step {}", i + 1), s, &allowed));
    }
    // カバレッジ: 全 facts に per_fact の着地があること
    for f in facts {
        if !draft.per_fact.iter().any(|pf| pf.fact_id == f.id) {
            v.push(format!("カバレッジ: {} の dakara が無い", f.id));
        }
    }
    v
}

// ============================================================
// オーケストレータ
// ============================================================

/// パイプライン実行結果 (レビュー往復の監査情報つき)。
#[derive(Debug, Clone, Default)]
pub(super) struct GuideAiOutcome {
    pub draft: GuideDraft,
    /// レビューコールの指摘件数 (0 = 一発 pass)
    pub review_findings: usize,
    /// 使用した Gemini 呼び出し回数
    pub calls_used: usize,
}

/// flash-lite 作成→レビュー→修正のパイプライン。最大4コール。
/// 最終ガード不合格・API失敗は None (呼び出し側で決定的テンプレへフォールバック)。
pub(super) async fn generate_guide_ai(
    client: &GeminiClient,
    facts: &[GuideFact],
    region: &str,
) -> Option<GuideAiOutcome> {
    if facts.is_empty() {
        return None;
    }
    let facts_str = facts_json(facts);
    let mut calls = 0usize;

    // ① 作成
    let user1 = format!(
        "対象地域: {}\n以下の事実インベントリだけを使い、解説資料の本文を起草してください。\n\
         - lead: 資料全体の要約 (2〜3文)\n\
         - per_fact: 全ての fact について dakara (着地文) を書く\n\
         - composites: 複数テーマを編んだ考察 2〜4本\n\
         - next_steps: 読み手が次にやるべきこと 3〜4項目 (優先順)\n{}",
        region, facts_str
    );
    calls += 1;
    let resp = client.generate_json(GUIDE_SYSTEM, &user1, draft_schema()).await?;
    let mut draft = parse_draft(&resp)?;

    // ② レビュー (逆証明)
    let user2 = format!(
        "facts:\n{}\n\ndraft:\n{}",
        facts_str,
        serde_json::to_string(&json!({
            "lead": draft.lead,
            "per_fact": draft.per_fact.iter().map(|p| json!({"fact_id": p.fact_id, "dakara": p.dakara})).collect::<Vec<_>>(),
            "composites": draft.composites.iter().map(|c| json!({"title": c.title, "thesis": c.thesis, "fact_ids": c.fact_ids, "so_what": c.so_what})).collect::<Vec<_>>(),
            "next_steps": draft.next_steps,
        }))
        .unwrap_or_default()
    );
    calls += 1;
    let review: ReviewResult = client
        .generate_json(REVIEW_SYSTEM, &user2, review_schema())
        .await
        .and_then(|v| serde_json::from_value(v).ok())
        .unwrap_or_default();

    // Rust ガードの機械指摘も合流
    let mut all_findings: Vec<String> = review
        .findings
        .iter()
        .map(|f| format!("[{}] {} → 修正案: {}", f.location, f.problem, f.fix))
        .collect();
    all_findings.extend(guard_violations(&draft, facts));

    // ③ 修正 (指摘があるときだけ)
    if !all_findings.is_empty() {
        let user3 = format!(
            "対象地域: {}\nfacts:\n{}\n\n前回の起草に対して以下の指摘があった。全て反映して書き直してください。\
             指摘のない箇所は維持してよい。\n指摘:\n- {}\n\n前回の起草:\n{}",
            region,
            facts_str,
            all_findings.join("\n- "),
            serde_json::to_string(&json!({
                "lead": draft.lead,
                "per_fact": draft.per_fact.iter().map(|p| json!({"fact_id": p.fact_id, "dakara": p.dakara})).collect::<Vec<_>>(),
                "composites": draft.composites.iter().map(|c| json!({"title": c.title, "thesis": c.thesis, "fact_ids": c.fact_ids, "so_what": c.so_what})).collect::<Vec<_>>(),
                "next_steps": draft.next_steps,
            }))
            .unwrap_or_default()
        );
        calls += 1;
        if let Some(resp3) = client.generate_json(GUIDE_SYSTEM, &user3, draft_schema()).await {
            if let Some(d3) = parse_draft(&resp3) {
                draft = d3;
            }
        }
    }

    // 最終ガード (機械検査)。カバレッジ欠けは該当 fact を素通しせず「観測のみ表示」に
    // なるだけなので致命ではない — 致命 (禁止語・数値捏造・偽 fact_id) が残っていたら
    // 項目単位で落とし、全滅なら None。
    let violations = guard_violations(&draft, facts);
    let fatal = |label_prefix: &str| {
        violations.iter().any(|v| {
            v.starts_with(label_prefix)
                && (v.contains("禁止表現") || v.contains("無い数値") || v.contains("実在しない"))
        })
    };
    if fatal("lead") {
        draft.lead = String::new();
    }
    draft.per_fact.retain(|pf| !fatal(&format!("per_fact {}", pf.fact_id)));
    draft
        .composites
        .retain(|c| !fatal(&format!("composite「{}」", c.title)));
    let steps: Vec<String> = draft
        .next_steps
        .iter()
        .enumerate()
        .filter(|(i, _)| !fatal(&format!("next_step {}", i + 1)))
        .map(|(_, s)| s.clone())
        .collect();
    draft.next_steps = steps;

    if draft.per_fact.is_empty() && draft.composites.is_empty() {
        tracing::warn!(?violations, "guide AI: 最終ガードで全滅。決定的テンプレへフォールバック");
        return None;
    }

    tracing::info!(
        calls,
        review_findings = all_findings.len(),
        per_fact = draft.per_fact.len(),
        composites = draft.composites.len(),
        remaining_violations = violations.len(),
        "guide AI pipeline finished"
    );

    Some(GuideAiOutcome {
        draft,
        review_findings: all_findings.len(),
        calls_used: calls,
    })
}

// ============================================================
// レンダリング (AI 版解説資料)
// ============================================================

/// AI パイプラインの結果を解説資料 HTML にする。
/// 観測 (statement) はコード確定値、だから/複合/次の一手は AI 起草+検証済み。
pub(super) fn render_guide_ai_html(
    facts: &[GuideFact],
    position: Option<&CompanyPosition>,
    outcome: &GuideAiOutcome,
    region: &str,
) -> String {
    let d = &outcome.draft;
    let mut html = String::with_capacity(32 * 1024);
    html.push_str("<!DOCTYPE html>\n<html lang=\"ja\">\n<head>\n<meta charset=\"utf-8\">\n");
    html.push_str(&format!(
        "<title>求人市場レポート 解説資料【{}】</title>\n",
        escape_html(region)
    ));
    html.push_str(super::guide::GUIDE_CSS);
    html.push_str("</head>\n<body>\n<div class=\"page\">\n");

    html.push_str(&format!(
        "<header class=\"doc\">\
         <div class=\"eyebrow\">READER'S GUIDE</div>\
         <h1>求人市場 総合診断レポート 解説資料</h1>\
         <div class=\"lede\">対象: 求人市場 総合診断レポート【{}】。数値はすべてレポート本体と同じ\
         集計から確定させ、解釈文は AI の起草を逆証明レビューと機械検証にかけたものです。\
         記載はデータから言える範囲の傾向・可能性であり、断定ではありません。</div></header>\n",
        escape_html(region)
    ));

    if !d.lead.is_empty() {
        html.push_str(&format!(
            "<div class=\"keybox\">{}</div>\n",
            escape_html(&d.lead)
        ));
    }

    // §1 貴社の現在地 (表はコード確定値)
    if let Some(pos) = position {
        html.push_str("<h2>貴社の現在地</h2>\n");
        html.push_str("<table><tr><th>観測 (収集データ内の貴社求人)</th><th>市場との重ね合わせ</th></tr>\n");
        html.push_str(&format!(
            "<tr><td>掲載 {} 件 (企業名: {})</td><td>同じ検索画面で比較される求人の中での掲載数です</td></tr>\n",
            pos.count,
            escape_html(&pos.name),
        ));
        if pos.own_median > 0 {
            html.push_str(&format!(
                "<tr><td>提示給与の中央値 {}</td><td>市場の中央値は {}{}</td></tr>\n",
                man_yen(pos.own_median),
                pos.market_median.map(man_yen).unwrap_or_else(|| "—".to_string()),
                pos.percentile_from_below
                    .map(|p| format!("。分布の下位からおよそ {:.0}% の位置 (下限と上限の中間値ベースの参考値)", p))
                    .unwrap_or_default(),
            ));
        }
        html.push_str("</table>\n");
        if let Some(pf) = d.per_fact.iter().find(|pf| pf.fact_id == "F-CO") {
            html.push_str(&format!(
                "<div class=\"dakara\">→ <strong>だから:</strong> {}</div>\n",
                escape_html(&pf.dakara)
            ));
        }
    }

    // §2 市場の実像 (観測=コード確定、だから=AI検証済み)
    html.push_str("<h2>市場の実像 — レポートの数字から言えること</h2>\n");
    for f in facts {
        if f.id == "F-CO" {
            continue; // §1 で表示済み
        }
        html.push_str(&format!("<h3>{}</h3>\n", escape_html(&f.theme)));
        html.push_str(&format!("<p>{}</p>\n", escape_html(&f.statement)));
        if let Some(pf) = d.per_fact.iter().find(|pf| pf.fact_id == f.id) {
            html.push_str(&format!(
                "<div class=\"dakara\">→ <strong>だから:</strong> {}</div>\n",
                escape_html(&pf.dakara)
            ));
        }
    }

    // §3 複合考察 (AI 起草・レビュー済み)
    if !d.composites.is_empty() {
        html.push_str("<h2>複合考察 — 数字を重ねると見えること</h2>\n");
        for c in &d.composites {
            html.push_str(&format!("<h3>{}</h3>\n", escape_html(&c.title)));
            html.push_str(&format!("<p>{}</p>\n", escape_html(&c.thesis)));
            html.push_str(&format!(
                "<div class=\"dakara\">→ <strong>だから:</strong> {}</div>\n",
                escape_html(&c.so_what)
            ));
            let refs = c
                .fact_ids
                .iter()
                .filter_map(|id| facts.iter().find(|f| f.id == *id))
                .map(|f| f.theme.clone())
                .collect::<Vec<_>>()
                .join("×");
            if !refs.is_empty() {
                html.push_str(&format!(
                    "<p class=\"note\">根拠: {} の観測の組み合わせ</p>\n",
                    escape_html(&refs)
                ));
            }
        }
    }

    // §4 次の一手
    if !d.next_steps.is_empty() {
        html.push_str("<h2>次の一手 (優先順)</h2>\n<ol>\n");
        for s in &d.next_steps {
            html.push_str(&format!("<li>{}</li>\n", escape_html(s)));
        }
        html.push_str("</ol>\n");
    }
    html.push_str(
        "<p class=\"note\">いずれも応募数・採用を保証するものではなく、市場データから見た\
         判断材料の提示です。仕事内容固有の条件は市場データでは測れないため、応募実績と\
         面談で検証する領域です。</p>\n",
    );

    html.push_str(
        "<footer class=\"doc\">出典: 今回アップロードされた求人検索データ (重複整理後) / \
         国勢調査 (通勤 OD 含む) / e-Stat 各種統計 (詳細はレポート本体の「注記・出典・免責」)。<br>\
         本資料の数値はレポート本体と同じ集計から生成し、解釈文は AI の起草に逆証明レビューと\
         機械検証 (数値出所・禁止表現・過剰主張) を適用しています。応募数・採用可否を保証する\
         ものではありません。</footer>\n",
    );
    html.push_str("</div>\n</body>\n</html>\n");
    html
}

// ============================================================
// handlers.rs 用の一括入口
// ============================================================

/// 事実インベントリ構築 → AI パイプライン → HTML の一括関数。
/// API キー未設定・呼び出し失敗・最終ガード全滅のいずれも None を返し、
/// 呼び出し側は決定的テンプレ (guide.rs) へフォールバックする。
pub(crate) async fn render_survey_guide_page_ai(
    agg: &SurveyAggregation,
    ctx: Option<&InsightContext>,
    pref: &str,
    muni: &str,
    company: Option<&str>,
) -> Option<String> {
    let client = GeminiClient::from_env()?;
    let (facts, position) = build_fact_inventory(agg, ctx, company);
    let region = if muni.is_empty() {
        pref.to_string()
    } else {
        format!("{} {}", pref, muni)
    };
    let outcome = generate_guide_ai(&client, &facts, &region).await?;
    Some(render_guide_ai_html(
        &facts,
        position.as_ref(),
        &outcome,
        &region,
    ))
}

// ============================================================
// テスト (LLM 呼び出しなし — 事実インベントリとガードの検証)
// ============================================================

#[cfg(test)]
mod tests {
    use super::*;
    use super::super::super::super::aggregator::{CompanyAgg, TagSalaryAgg};

    fn rich_agg() -> SurveyAggregation {
        let mut agg = SurveyAggregation {
            total_count: 600,
            new_count: 120,
            salary_values: (0..100).map(|i| 250_000 + i * 1_000).collect(),
            salary_min_values: (0..100).map(|i| 220_000 + i * 800).collect(),
            salary_max_values: (0..100).map(|i| 300_000 + i * 1_200).collect(),
            ..Default::default()
        };
        agg.by_company.push(CompanyAgg {
            name: "テスト工業株式会社".to_string(),
            count: 2,
            avg_salary: 260_000,
            median_salary: 255_000,
        });
        agg.by_tag_salary.push(TagSalaryAgg {
            tag: "経験者歓迎".to_string(),
            count: 50,
            avg_salary: 330_000,
            diff_from_avg: 30_000,
            diff_percent: 10.5,
        });
        agg.jobbox.annual_holidays_values = (0..40).map(|i| 105 + (i % 30)).collect();
        agg.jobbox.holiday_pct_ge_120 = 0.53;
        agg.jobbox.holiday_pct_ge_125 = 0.18;
        agg
    }

    #[test]
    fn fact_inventory_builds_expected_ids() {
        let (facts, pos) = build_fact_inventory(&rich_agg(), None, Some("テスト工業"));
        let ids: Vec<&str> = facts.iter().map(|f| f.id.as_str()).collect();
        assert!(ids.contains(&"F-SIZE"));
        assert!(ids.contains(&"F-SAL"));
        assert!(ids.contains(&"F-HOL"));
        assert!(ids.contains(&"F-TAG"));
        assert!(ids.contains(&"F-CO"));
        assert!(pos.is_some());
        // ctx なし → 需給/通勤の facts は無い (嘘をつかない)
        assert!(!ids.contains(&"F-DEM"));
        assert!(!ids.contains(&"F-COM"));
    }

    #[test]
    fn fact_numbers_are_extracted_for_provenance() {
        let (facts, _) = build_fact_inventory(&rich_agg(), None, None);
        let size = facts.iter().find(|f| f.id == "F-SIZE").unwrap();
        assert!(size.numbers.contains(&"600".to_string()), "{:?}", size.numbers);
        let hol = facts.iter().find(|f| f.id == "F-HOL").unwrap();
        assert!(hol.numbers.contains(&"120".to_string()));
        assert!(hol.numbers.contains(&"53".to_string()));
    }

    #[test]
    fn numbers_guard_rejects_fabricated_and_allows_listed() {
        let allowed: HashSet<String> =
            ["600", "21", "29.2"].iter().map(|s| s.to_string()).collect();
        assert!(numbers_ok("市場は 600 件で新着 21% です", &allowed));
        assert!(numbers_ok("中央値 29.2万円 とみられます", &allowed));
        // 捏造数値 (12%) は不合格 — 大分12%移植事故の再発防止と同型
        assert!(!numbers_ok("120日以上は 12% にとどまります", &allowed));
        // 1桁の序数は許可
        assert!(numbers_ok("次の3つが優先です", &allowed));
    }

    #[test]
    fn guard_detects_fake_fact_id_and_missing_coverage() {
        let (facts, _) = build_fact_inventory(&rich_agg(), None, None);
        let draft = GuideDraft {
            lead: "全体として市場は動いているとみられます。".to_string(),
            per_fact: vec![PerFact {
                fact_id: "F-XXX".to_string(),
                dakara: "何かが言える可能性があります。".to_string(),
            }],
            composites: vec![],
            next_steps: vec![],
        };
        let v = guard_violations(&draft, &facts);
        assert!(v.iter().any(|s| s.contains("実在しない fact_id")));
        assert!(v.iter().any(|s| s.contains("カバレッジ")));
    }

    #[test]
    fn card_facts_detect_single_value_desc_gap_and_holiday_band() {
        // Phase 2a: 給与欄単一値 + 説明文35万 + 休日126日 (上位帯) + 非新着 のカード
        let mut agg = rich_agg();
        agg.card_briefs.push(CardBrief {
            company: "テスト工業株式会社".to_string(),
            title: "設備の金属コーティング技術者".to_string(),
            is_monthly: true,
            salary_min: Some(250_000),
            salary_max: Some(250_000),
            annual_holidays: Some(126),
            is_new: false,
            desc_salary_man: Some(35),
        });
        let (facts, _) = build_fact_inventory(&agg, None, Some("テスト工業"));
        let card = facts
            .iter()
            .find(|f| f.id == "F-CO-CARD1")
            .expect("カード事実が生成されるはず");
        assert!(card.statement.contains("単一値"), "{}", card.statement);
        assert!(card.statement.contains("月収35万円"), "{}", card.statement);
        assert!(card.statement.contains("125日以上"), "{}", card.statement);
        assert!(card.statement.contains("新着表示はない"), "{}", card.statement);
        // 数値出所ガードの許可集合に説明文給与が入る
        assert!(card.numbers.contains(&"35".to_string()));
    }

    #[test]
    fn card_desc_salary_equal_to_field_is_not_a_gap() {
        // 説明文の 25万 = 給与欄 25万 → 「反映されていない」とは言わない
        let mut agg = rich_agg();
        agg.card_briefs.push(CardBrief {
            company: "テスト工業株式会社".to_string(),
            title: "t".to_string(),
            is_monthly: true,
            salary_min: Some(250_000),
            salary_max: None,
            annual_holidays: None,
            is_new: true,
            desc_salary_man: Some(25),
        });
        let (facts, _) = build_fact_inventory(&agg, None, Some("テスト工業"));
        let card = facts.iter().find(|f| f.id == "F-CO-CARD1").unwrap();
        assert!(
            !card.statement.contains("反映されていない"),
            "同額は乖離ではない: {}",
            card.statement
        );
    }

    #[test]
    fn guard_detects_vague_landing() {
        let (facts, _) = build_fact_inventory(&rich_agg(), None, None);
        let draft = GuideDraft {
            lead: String::new(),
            per_fact: facts
                .iter()
                .map(|f| PerFact {
                    fact_id: f.id.clone(),
                    dakara: "自社の立ち位置を把握し、調整を検討する余地があるかもしれません。".to_string(),
                })
                .collect(),
            composites: vec![],
            next_steps: vec![],
        };
        let v = guard_violations(&draft, &facts);
        assert!(
            v.iter().any(|s| s.contains("空虚な着地")),
            "空虚語が検出されるはず: {:?}",
            v
        );
    }

    #[test]
    fn commute_fact_states_direction() {
        let mut ctx = InsightContext::default();
        ctx.commute_inflow_total = 19_545;
        ctx.commute_outflow_total = 36_956;
        ctx.commute_self_rate = 0.315;
        let (facts, _) = build_fact_inventory(&rich_agg(), Some(&ctx), None);
        let com = facts.iter().find(|f| f.id == "F-COM").unwrap();
        assert!(
            com.statement.contains("市外へ出ていく構造"),
            "流出超過の方向がコードで明記されるはず: {}",
            com.statement
        );
    }

    #[test]
    fn guard_detects_single_theme_composite_and_forbidden() {
        let (facts, _) = build_fact_inventory(&rich_agg(), None, None);
        let draft = GuideDraft {
            lead: String::new(),
            per_fact: facts
                .iter()
                .map(|f| PerFact {
                    fact_id: f.id.clone(),
                    dakara: "判断材料になる可能性があります。".to_string(),
                })
                .collect(),
            composites: vec![
                GuideComposite {
                    title: "単一テーマの言い換え".to_string(),
                    thesis: "給与の話だけ。".to_string(),
                    fact_ids: vec!["F-SAL".to_string()],
                    so_what: "上限を見せる余地があります。".to_string(),
                },
                GuideComposite {
                    title: "禁止語入り".to_string(),
                    thesis: "こうすれば必ず採用できると考えられます。".to_string(),
                    fact_ids: vec!["F-SAL".to_string(), "F-HOL".to_string()],
                    so_what: "確実に成果が出ます。".to_string(),
                },
            ],
            next_steps: vec![],
        };
        let v = guard_violations(&draft, &facts);
        assert!(v.iter().any(|s| s.contains("2テーマ未満")));
        assert!(v.iter().any(|s| s.contains("禁止表現")));
    }
}
