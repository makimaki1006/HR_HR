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

/// 同順位を按分したパーセンタイル (ミッドランク方式)。
///
/// 2026-07-22 再監査対応: 従来の「target 以下の割合」方式は、同値が大量に集中する
/// 給与分布 (25.0万円へのタイ等) で「中央値と同額なのに下位から60%」という
/// 矛盾した見え方を生んだ。同順位帯の中央に位置づける。
fn midrank_pct(values: &[i64], target: i64) -> Option<f64> {
    if values.is_empty() {
        return None;
    }
    let below = values.iter().filter(|v| **v < target).count() as f64;
    let eq = values.iter().filter(|v| **v == target).count() as f64;
    Some((below + eq / 2.0) / values.len() as f64 * 100.0)
}

/// 数値トークン正規化: カンマ除去・末尾の小数点除去。
fn norm_num(s: &str) -> String {
    s.replace(',', "").trim_end_matches('.').to_string()
}

/// 全角数字・全角記号を半角へ正規化する (ガード監査 HIGH-1 対応)。
/// 全角数字は extract_numbers で「数字でない文字」扱いになり検証を素通りしていた。
fn normalize_digits(text: &str) -> String {
    text.chars()
        .map(|c| match c {
            '０'..='９' => char::from_u32('0' as u32 + (c as u32 - '０' as u32)).unwrap_or(c),
            '，' => ',',
            '．' => '.',
            '％' => '%',
            _ => c,
        })
        .collect()
}

/// text 内の数値トークンを抽出する (整数・小数・カンマ区切り)。全角は正規化してから走査。
fn extract_numbers(text: &str) -> Vec<String> {
    let text = normalize_digits(text);
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

/// 漢数字による数値表記 (2文字以上 + 数量単位) の検出 (ガード監査 HIGH-1 対応)。
/// 「百三十日」「三十五万円」等はトークン化されず検証を素通りしていた。
/// 1文字 (「一人ひとり」「一部」等の慣用) は誤検出を避けるため対象外 (1桁許可と整合)。
fn has_kanji_numeral_quantity(text: &str) -> bool {
    const KD: [char; 13] = [
        '〇', '一', '二', '三', '四', '五', '六', '七', '八', '九', '十', '百', '千',
    ];
    // 2026-07-22 再監査 [HIGH] 対応: 単位を2群に分ける。
    // - 金額・比率単位 (万/千/円/割/倍/%) は漢数字1文字でも数量表記
    //   (「五万円」「八割」「三倍」が run>=2 条件で素通りしていた)
    // - 日/人/件 は「一人ひとり」「一件ずつ」等の慣用があるため2文字以上のみ
    const STRICT_UNITS: [char; 6] = ['万', '円', '割', '倍', '%', '千'];
    const LOOSE_UNITS: [char; 3] = ['日', '人', '件'];
    let chars: Vec<char> = text.chars().collect();
    let mut run = 0usize;
    for c in &chars {
        if KD.contains(c) {
            run += 1;
        } else if c.is_whitespace() {
            // 「百三十 日」のように空白を挟む表記も検出するため run を維持
        } else {
            if run >= 1 && STRICT_UNITS.contains(c) {
                return true;
            }
            if run >= 2 && LOOSE_UNITS.contains(c) {
                return true;
            }
            run = 0;
        }
    }
    false
}

/// 数量単位 (万/千/割/倍/%) が直後に続く数値トークンを抽出する。
/// ガード監査 HIGH-2/HIGH-3 対応: 「3万5千」の桁分解や「8割」「3倍」は
/// 1桁トークンとして無条件許可されていた。単位付きは桁数によらず出所照合する。
fn extract_unit_numbers(text: &str) -> Vec<String> {
    let text = normalize_digits(text);
    let chars: Vec<char> = text.chars().collect();
    let mut out = Vec::new();
    let mut cur = String::new();
    for (i, c) in chars.iter().enumerate() {
        if c.is_ascii_digit() || *c == '.' || *c == ',' {
            cur.push(*c);
        } else {
            if !cur.is_empty() && matches!(c, '万' | '千' | '割' | '倍' | '%' | '円') {
                // 「◯時」「◯分」等は対象外。数量単位が直後のときのみ照合対象。
                let _ = i;
                out.push(norm_num(&cur));
            }
            cur.clear();
        }
    }
    out.into_iter().filter(|t| !t.is_empty()).collect()
}

/// 事実インベントリを構築する。数値はすべて agg / ctx 由来 (LLM には計算させない)。
/// 各 statement は「観測」のみで「だから」を含まない (それが LLM の仕事)。
pub(super) fn build_fact_inventory(
    agg: &SurveyAggregation,
    ctx: Option<&InsightContext>,
    // 2026-07-21 監査対応: 検索地 (muni) を受け取り、市場の実体が広域である事実を明示する
    search_muni: Option<&str>,
    company: Option<&str>,
) -> (Vec<GuideFact>, Option<CompanyPosition>) {
    let mut facts: Vec<GuideFact> = Vec::new();
    let mut push = |id: &str, theme: &str, statement: String| {
        let mut numbers = extract_numbers(&statement);
        // 「22.0万円」を LLM が「22万円」と自然に言い換えたときに落とさないよう、
        // 末尾 .0 を除いた形も許可集合に加える (ガード監査の補足指摘対応)。
        let plain: Vec<String> = numbers
            .iter()
            .filter(|t| t.ends_with(".0"))
            .map(|t| t.trim_end_matches(".0").to_string())
            .collect();
        numbers.extend(plain);
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
        // 2026-07-21 監査対応: 検索地そのものの件数を明示 (市場の実体が広域である事実)。
        // 2026-07-22 再監査対応: 検索地単体の件数に加え給与中央値も併記 (二段構え)。
        let locale_note = search_muni
            .and_then(|m| {
                agg.by_municipality_salary
                    .iter()
                    .find(|x| x.name == m)
                    .map(|x| {
                        format!(
                            "うち勤務地が検索地の{}なのは {} 件 (この{}件だけの給与中央値は {}) で、市場の実体は周辺市を含む広域。以降の市場全体の数値は広域ベースであり、{}単独の実勢ではない点に注意。",
                            m,
                            format_number(x.count as i64),
                            format_number(x.count as i64),
                            man_yen(x.median_salary),
                            m,
                        )
                    })
            })
            .unwrap_or_else(|| "検索地の市内に限らず通勤圏に広がる。".to_string());
        push(
            "F-SIZE",
            "市場規模",
            format!(
                "重複整理後の求人は {} 件。掲載から間もない求人の割合 (目安) は {:.0}% (裏返すと約{:.0}%は掲載から時間が経った求人で、これが市場の常態)。勤務地の上位は {}。{}",
                format_number(agg.total_count as i64),
                new_pct,
                100.0 - new_pct,
                if top3.is_empty() { "不明".to_string() } else { top3 },
                locale_note,
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
        // 2026-07-21 監査対応: 「開き」は別々に集計した中央値の差であり個々の求人の幅では
        // ない点を明記。79% の分母表現も一義に。
        push(
            "F-SAL",
            "給与",
            format!(
                "下限給与の中央値 {} (n={}) / 上限給与の中央値 {} (それぞれ別々に集計した中央値で、その差は {:.0}万円。個々の求人の給与幅そのものではない)。下限給与を確認できた求人のうち、上限も併記して幅を示すものはおよそ {:.0}%。",
                man_yen(lo),
                format_number(agg.salary_min_values.len() as i64),
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
        // 2026-07-21: 相関の解釈 (符号含む) はコードで確定して渡す。値+条件付きヒント
        // だけだと、逆相関市場 (r<=-0.3: 休日が多いほど給与が低い、いわゆる補償賃金の
        // 通説型) で LLM が「独立」と書いてしまう余地があった (方向読み違えの通勤問題と同型)。
        let corr = jb
            .salary_holidays_correlation
            .map(|r| {
                // 「独立」は言い過ぎ (線形相関が無いだけで非線形・交絡は否定できない) — 監査指摘。
                let reading = if r <= -0.3 {
                    "休日が多い求人ほど給与が低い傾向 (逆相関) がある。休日訴求は給与面の印象とトレードオフになる可能性に注意"
                } else if r >= 0.3 {
                    "休日が多い求人ほど給与も高い傾向 (正の相関) がある"
                } else {
                    "線形の関連は見られない (独立の証明ではない)。少なくとも「休日が多い求人は給与が低い」という単純な関係はこのデータでは確認できない"
                };
                format!(
                    " 休日と給与の相関係数は r={:.2} (休日記載のある求人のみの集計)。{}。",
                    r, reading
                )
            })
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
            format!(
                "給与が市場平均より高い側に分布するタグ: {} (比較は平均ベース。他セクションの中央値とは基準が異なる)。相関であり因果ではなく、応募への効果を示すデータは無い。市場全体の傾向の説明であり、依頼企業の職種・雇用形態に合わないタグをそのまま流用しない (例: アルバイト向けの語彙を正社員募集に使わない)。",
                list
            ),
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

    // F-DEM (有効求人倍率) は 2026-07-22 再監査で削除。
    // 顧客レビュー「使えないと自認する数値を載せる意味がない」+ 論理監査「だからが本文と
    // 無関係に破綻」— 県・全産業計の値は職種の実勢と乖離し、この資料の提案に接続しない。
    if let Some(c) = ctx {
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
            // 2026-07-21 監査対応: 含意の読み方までコードで確定 (流出型なのに「流入者を
            // 取り込め」と逆向きの戦略を書いた実出力への対策)。流入元自治体が同時に
            // 求人の競合地でもある点も明記する。
            let strategic_note = if c.commute_outflow_total > c.commute_inflow_total {
                "自然な読み方: 市外へ通っている地元居住者に「地元で同水準の条件で働ける (通勤時間の短縮)」を訴求する流れ。流入元上位の自治体は配信対象の候補だが、同時に求人が集まる競合地でもある点に注意。"
            } else {
                "自然な読み方: 市外から通ってくる働き手が既に多いため、周辺自治体への露出拡大が母集団確保につながりやすい。"
            };
            push(
                "F-COM",
                "通勤",
                format!(
                    "市外へ通勤する人 {} 人 / 市外から来る人 {} 人 / 通勤者に占める市内完結の割合 {:.1}% (国勢調査 OD。全住民・全産業の通勤であり特定職種の求職者の動きそのものではない)。{}。周辺からの流入元の上位は {}。{}",
                    format_number(c.commute_outflow_total),
                    format_number(c.commute_inflow_total),
                    c.commute_self_rate * 100.0,
                    direction,
                    if top.is_empty() { "不明".to_string() } else { top },
                    strategic_note,
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
        // 同順位按分 (ミッドランク)。タイ集中時に「中央値と同額なのに下位60%」と
        // ならないようにする (2026-07-22 再監査対応)。
        let pct = midrank_pct(&agg.salary_values, hit.median_salary);
        Some(CompanyPosition {
            name: hit.name.clone(),
            count: hit.count,
            own_median: hit.median_salary,
            market_median,
            percentile_from_below: pct,
        })
    });
    if let Some(pos) = &position {
        // カードは F-CO の物差し補正にも使うため先に取得。
        let name = pos.name.as_str();
        let cards: Vec<&CardBrief> = agg
            .card_briefs
            .iter()
            .filter(|cb| cb.company.contains(name) || name.contains(cb.company.as_str()))
            .take(2)
            .collect();

        // 2026-07-21 監査対応 (最重要): 物差しを揃えた多面比較。
        // 旧実装は「貴社=単一値 (実質下限) vs 市場=中間値」の比較だけを見せ、
        // 実際は市場中央並みの給与を「下位22%」と誤導していた。
        // - 主比較: 下限どうし (単一値カードに公平)
        // - 補足: 中間値どうし (単一値表記は低めに出る旨を明記)
        // - 説明文に上限記載がある場合: それを反映した仮の中間値での位置も併記
        // 2026-07-22 再監査対応: 結論先出し。「で、うちは高いの安いの?」に最初の1文で
        // 答え、内訳 (下限比較・中間値比較の注意) は補足に回す。位置はすべて同順位按分。
        let own_min = cards.iter().find_map(|cb| cb.salary_min.filter(|_| cb.is_monthly));
        let market_lo_median = median_of(&agg.salary_min_values);
        // 説明文の上限記載を反映した仮の中間値 (単一値カードのみ意味を持つ)
        let hypo: Option<(i64, i64)> = match (own_min, pos.market_median, cards.first()) {
            (Some(own_lo), Some(m), Some(card)) => {
                let is_single = card.salary_max.map_or(true, |h| h <= own_lo);
                match (is_single, card.desc_salary_man) {
                    (true, Some(d)) => Some(((own_lo + d * 10_000) / 2, m)),
                    _ => None,
                }
            }
            _ => None,
        };

        let mut s = format!(
            "依頼企業「{}」の求人 {} 件が収集データ内にある。",
            pos.name, pos.count,
        );
        // 結論の1文 (仮の中間値が市場中央値以上のときは「表記の課題」と言い切れる)
        if let Some((hypo_mid, m)) = hypo {
            if hypo_mid >= m {
                let d = cards.first().and_then(|c| c.desc_salary_man).unwrap_or(0);
                s.push_str(&format!(
                    " 結論: 課題は金額ではなく給与欄の表記の可能性が高い。説明文にある月収{}万円を給与欄の上限として反映すると中間値 {} となり、市場の中間値の中央値 {} を{}。",
                    d,
                    man_yen(hypo_mid),
                    man_yen(m),
                    if hypo_mid > m { "上回る" } else { "同水準になる" },
                ));
            }
        }
        // 補足1: 下限どうしの比較 (単一値カードに公平な物差し)
        if let (Some(own_lo), Some(mlo)) = (own_min, market_lo_median) {
            let rel = if own_lo > mlo {
                "上回る"
            } else if own_lo == mlo {
                "同額"
            } else {
                "下回る"
            };
            s.push_str(&format!(
                " 補足: 給与欄の下限 {} は市場の下限中央値 {} と{}",
                man_yen(own_lo),
                man_yen(mlo),
                rel,
            ));
            if let Some(p) = midrank_pct(&agg.salary_min_values, own_lo) {
                s.push_str(&format!(
                    " (同順位を按分した位置で下位から約 {:.0}%、n={})",
                    p,
                    format_number(agg.salary_min_values.len() as i64)
                ));
            }
            s.push_str("。");
        }
        // 補足2: 中間値比較は単一値表記だと低めに出る旨
        if let (Some(m), Some(p)) = (pos.market_median, pos.percentile_from_below) {
            s.push_str(&format!(
                " 下限と上限の中間値どうしの比較では下位から約 {:.0}% (市場の中間値の中央値 {}) だが、幅を書かない単一値表記の求人は中間値=下限になるため、実態より低めに見える。",
                p,
                man_yen(m),
            ));
        }
        push("F-CO", "依頼企業", s);

        // Phase 2a (2026-07-20): カード単位の観測。依頼企業の求人カードそのものを
        // 市場分布に重ねる (給与欄の形式 / 説明文との乖離 / 休日記載の市場内位置)。
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

    // 新着表示。2026-07-21 監査対応: 市場の約8割は新着でないのが常態のため、
    // 単体でネガティブ扱いしない (市場文脈を併記)。
    if !cb.is_new && agg.total_count > 0 {
        let new_pct = agg.new_count as f64 / agg.total_count as f64 * 100.0;
        parts.push(format!(
            "新着表示はない (市場でも新着表示は約{:.0}%のみで、これ自体は珍しくない)",
            new_pct
        ));
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
2. 数値は半角の算用数字のみで書く。漢数字 (三十五万)・全角数字 (３５)・桁分解 (3万5千) は禁止。「◯割」「◯倍」のような facts に無い割合・倍率を作らない。\n\
3. 各事実の dakara は「その数字が読み手の求人にとって何を意味するか」の着地文。数字の言い換えではなく、読み手が明日やることが変わる含意を書く。\n\
   - 悪い例 (空虚。禁止): 「〜を検討する余地があるかもしれません」「〜を注視する必要があります」「〜を把握することが重要です」\n\
   - 良い例の型: 「(観測の核心。市場の過半と同じ/上回る/下回る等の位置づけ)ため、(具体的な打ち手の方向)が検討候補になります」\n\
4. 数字の大小関係と、facts に書かれた「読み方」の向きを正しく使う。方向を取り違えない。\n\
5. lead を含む全文で可能性表現・提案形 (「〜の可能性があります」「〜が検討候補になります」)。断定・因果の断定・命令形や「〜する。」「〜が求められます。」で終わる指示文は禁止。提案には現実的な留意点 (例: 給与欄の上限を上げれば面談時の期待値調整が必要) を可能な範囲で添える。\n\
6. composites と next_steps は per_fact の言い換え・繰り返しを禁止。composites は複数テーマの facts の数値を実際に引用し、重ねたときに初めて言えることだけを2〜4本。next_steps は読み手が求人票・配信設定で今日直せる操作に限定し (3〜4項目)、分析表の作成など読み手への宿題は出さない。提案は next_steps のみに書き、composites や dakara で同じ提案を繰り返さない。\n\
7. facts が「〜の根拠には使えない」「〜に限る」「〜しない」と用途を限定している数値・観測は、その限定に従う。特に F-TAG は市場の傾向説明に留め、依頼企業のカード (職種・雇用形態) に明らかに適合する場合以外、提案に使わない。\n\
8. 依頼企業の給与の位置づけは F-CO の結論に全セクションで従う。別の場所で矛盾する前提 (「給与が低いので補う」等) を書かない。\n\
9. 応募数・応募意欲への言及は禁止 (応募データは資料に存在しない)。\n\
10. 誇張しない。禁止語: 必ず・確実・完璧・絶対・劇的・問題ない・強力。\n\
11. 平易な言葉で書く。専門用語・略語・社内用語は使わない。\n\
出力は日本語。";

const REVIEW_SYSTEM: &str = "\
あなたは解説資料の逆証明レビュアーです。起草 (draft) を事実インベントリ (facts) と突き合わせ、以下の観点で問題を全て挙げてください。\n\
1. 数値の出所: draft 中の数値が facts に存在するか。facts に無い数値・計算された数値は即指摘。\n\
2. 因果の断定: 相関しか示せないデータで因果を断定していないか。\n\
3. 分母と粒度: 比率・統計の分母や粒度 (県単位・記載ありのみ等) を無視した言い回しがないか。\n\
4. 大小関係の方向: 数字の大小 (流出と流入、比率の過半かどうか等) から言える方向を取り違えていないか。例えば流出が流入を上回るのに「流入がある」側の解釈だけを書くのは方向の誤り。\n\
5. 反対解釈: 同じ数字から逆の解釈が成り立つのに一方だけを書いていないか。\n\
6. 着地の空虚さ: dakara や so_what が「検討する余地がある」「注視する必要がある」「把握することが重要」のような、読み手の行動が何も変わらない文になっていないか。空虚な着地は必ず指摘し、その数字の位置づけ (過半と同じ/上回る/下回る) から言える具体的な打ち手の方向を修正案として書く。\n\
7. 誇張・断定表現・命令形終止。\n\
8. 提案の文脈適合: 募集する雇用形態・職種に合わない提案 (例: アルバイト向け語彙のタグを正社員技術職に推奨)、facts が用途を限定した数値の流用、per_fact と composites と next_steps の内容の重複・水増し。\n\
9. 提案の副作用の欠落: 打ち手に現実的なリスク (例: 給与欄の上限表示を上げると面談時の期待値調整が必要) があるのに触れていない場合は指摘する。\n\
10. 結論の一貫性: 依頼企業の給与の位置づけ (F-CO の結論) と矛盾する前提が他セクションに無いか。dakara がその fact の観測から論理的に導けない話 (観測していない応募数・効果への飛躍、無関係な提案の接続) になっていないか。\n\
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
/// - 2桁以上は facts 由来でなければ不合格
/// - 1桁は序数 (「3つ」) を許容するため原則許可。ただし数量単位 (万/千/割/倍/%) が
///   直後に続く場合は桁数によらず照合する (「8割」「3万5千」型の捏造対策、監査 HIGH-2/3)
/// - 漢数字2文字以上+数量単位は表記自体を不合格 (「三十五万円」型のすり抜け対策、監査 HIGH-1)
pub(super) fn numbers_ok(text: &str, allowed: &HashSet<String>) -> bool {
    if has_kanji_numeral_quantity(text) {
        return false;
    }
    let plain_ok = extract_numbers(text)
        .iter()
        .all(|t| t.chars().filter(|c| c.is_ascii_digit()).count() <= 1 || allowed.contains(t));
    let unit_ok = extract_unit_numbers(text).iter().all(|t| allowed.contains(t));
    plain_ok && unit_ok
}

/// ガード検査結果 (機械検査の指摘リスト。修正コールへのフィードバックにも使う)。
///
/// 数値の許可集合は監査 MED-4 (fact 跨ぎの数値ロンダリング) 対応でスコープする:
/// - per_fact の dakara → その fact の数値のみ
/// - composite → 参照 fact_ids の数値の和集合
/// - lead / next_steps → 全 facts の和集合 (全体要約のため)
pub(super) fn guard_violations(draft: &GuideDraft, facts: &[GuideFact]) -> Vec<String> {
    let allowed: HashSet<String> = facts.iter().flat_map(|f| f.numbers.iter().cloned()).collect();
    let allowed_of = |ids: &[&str]| -> HashSet<String> {
        facts
            .iter()
            .filter(|f| ids.contains(&f.id.as_str()))
            .flat_map(|f| f.numbers.iter().cloned())
            .collect()
    };
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
        // 2026-07-21 監査 MED-5: 単独の断定副詞も禁止表現扱い (「必ず母集団が増えます」等の
        // 言い換え回避対策)。「必ずしも〜ない」はヘッジ表現なので除外する。
        const ASSERTIVE_WORDS: [&str; 3] = ["絶対", "完璧", "劇的"];
        if ASSERTIVE_WORDS.iter().any(|p| text.contains(p))
            || text
                .match_indices("必ず")
                .any(|(pos, m)| !text[pos + m.len()..].starts_with("しも"))
            || text.contains("確実")
        {
            out.push(format!("{}: 禁止表現を含む (断定副詞)", label));
        }
        // 2026-07-22 再監査対応: 応募数への言及は資料内に応募データが存在しないため
        // すべて根拠なき因果 (「タグ付与で応募数が増加」等の実出力を検出した対策)。
        const UNFOUNDED_OUTCOMES: [&str; 4] = ["応募が増", "応募数が増", "応募増加", "応募意欲"];
        if UNFOUNDED_OUTCOMES.iter().any(|p| text.contains(p)) {
            out.push(format!(
                "{}: 禁止表現を含む (応募数・応募意欲への言及。応募データは資料に存在しない)",
                label
            ));
        }
        if has_overclaim(text) {
            out.push(format!("{}: 言い過ぎ表現 (希少・皆無等の断定) を含む", label));
        }
        if !numbers_ok(text, allowed) {
            out.push(format!(
                "{}: 事実インベントリに無い数値を含む (漢数字・全角数字・桁分解表記も不可。半角算用数字で facts の値のみ使用)",
                label
            ));
        }
        if VAGUE_PHRASES.iter().any(|p| text.contains(p)) {
            out.push(format!(
                "{}: 空虚な着地 (「検討する余地」等)。数字の位置づけから言える具体的な打ち手の方向に書き直す",
                label
            ));
        }
        // 2026-07-21: 誇張形容 (実出力で「強力な訴求ポイントです」を検出した対策)。
        const OVERSTATEMENTS: [&str; 4] = ["強力な", "圧倒的", "最強", "抜群"];
        if OVERSTATEMENTS.iter().any(|p| text.contains(p)) {
            out.push(format!(
                "{}: 誇張形容 (「強力な」等)。中立的な形容と可能性表現に書き直す",
                label
            ));
        }
        out
    }

    let mut v: Vec<String> = Vec::new();
    v.extend(text_issues("lead:", &draft.lead, &allowed));
    for pf in &draft.per_fact {
        if !fact_ids.contains(pf.fact_id.as_str()) {
            v.push(format!("per_fact {}: 実在しない fact_id", pf.fact_id));
            continue;
        }
        // MED-4: 許可集合は当該 fact の数値に限定 (fact 跨ぎの数値流用を遮断)
        v.extend(text_issues(
            &format!("per_fact {}:", pf.fact_id),
            &pf.dakara,
            &allowed_of(&[pf.fact_id.as_str()]),
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
        let ref_ids: Vec<&str> = c.fact_ids.iter().map(|s| s.as_str()).collect();
        let scoped = allowed_of(&ref_ids);
        // 2026-07-22 再監査対応: 列挙した観測のうち2つ以上から実際に数値を引用することを要求。
        // (a) fact_ids の全列挙で許可集合だけ最大化する抜け道 (MED) と、
        // (b) 「〜を考慮した条件設定を行う」のような数字のない言い換え水増しの両方を検出する。
        {
            let combined = format!("{} {}", c.thesis, c.so_what);
            let used_plain = extract_numbers(&combined);
            let used_unit = extract_unit_numbers(&combined);
            let facts_with_number_used = c
                .fact_ids
                .iter()
                .filter(|id| {
                    facts.iter().find(|f| f.id == **id).map_or(false, |f| {
                        f.numbers
                            .iter()
                            .any(|n| used_plain.contains(n) || used_unit.contains(n))
                    })
                })
                .count();
            if facts_with_number_used < 2 {
                v.push(format!(
                    "composite「{}」: 列挙した観測のうち数値を引用しているのが2件未満 (見出しの言い換えでなく、複数の数字を重ねて初めて言えることを書く)",
                    c.title
                ));
            }
        }
        v.extend(text_issues(
            &format!("composite「{}」thesis:", c.title),
            &c.thesis,
            &scoped,
        ));
        v.extend(text_issues(
            &format!("composite「{}」so_what:", c.title),
            &c.so_what,
            &scoped,
        ));
    }
    for (i, s) in draft.next_steps.iter().enumerate() {
        v.extend(text_issues(&format!("next_step {}:", i + 1), s, &allowed));
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

    // draft の JSON 表現 (レビュー・修正コールの入力用)
    let draft_json = |d: &GuideDraft| -> String {
        serde_json::to_string(&json!({
            "lead": d.lead,
            "per_fact": d.per_fact.iter().map(|p| json!({"fact_id": p.fact_id, "dakara": p.dakara})).collect::<Vec<_>>(),
            "composites": d.composites.iter().map(|c| json!({"title": c.title, "thesis": c.thesis, "fact_ids": c.fact_ids, "so_what": c.so_what})).collect::<Vec<_>>(),
            "next_steps": d.next_steps,
        }))
        .unwrap_or_default()
    };

    // ② レビュー (逆証明)。監査 MED-7 対応:
    // - レビューコール失敗は「意味検証 (因果・分母・反対解釈) 未実施」を意味するため、
    //   pass 扱いにせず None (決定的テンプレへフォールバック) とする。
    // - verdict=needs_fix なのに findings が空の場合は合成指摘で修正を起動する。
    let run_review = |user: String| async move {
        client
            .generate_json(REVIEW_SYSTEM, &user, review_schema())
            .await
            .and_then(|v| serde_json::from_value::<ReviewResult>(v).ok())
    };
    let review_findings_of = |r: &ReviewResult| -> Vec<String> {
        let mut fs: Vec<String> = r
            .findings
            .iter()
            .map(|f| format!("[{}] {} → 修正案: {}", f.location, f.problem, f.fix))
            .collect();
        if fs.is_empty() && r.verdict == "needs_fix" {
            fs.push(
                "[全体] レビュアーが問題ありと判定 (詳細なし)。因果断定・分母・反対解釈の観点で全体を点検して書き直す"
                    .to_string(),
            );
        }
        fs
    };

    calls += 1;
    let review1 = run_review(format!("facts:\n{}\n\ndraft:\n{}", facts_str, draft_json(&draft))).await?;
    let mut total_findings = 0usize;
    let mut findings = review_findings_of(&review1);
    findings.extend(guard_violations(&draft, facts));
    total_findings += findings.len();

    // ③ 修正 → ④ 再レビュー → ⑤ 再修正 (指摘が続く限り、上限5コール)。
    // 監査 MED-7: 修正後の出力が未レビューのまま出荷される穴を塞ぐ。
    for round in 0..2 {
        if findings.is_empty() {
            break;
        }
        let user_fix = format!(
            "対象地域: {}\nfacts:\n{}\n\n前回の起草に対して以下の指摘があった。全て反映して書き直してください。\
             指摘のない箇所は維持してよい。\n指摘:\n- {}\n\n前回の起草:\n{}",
            region,
            facts_str,
            findings.join("\n- "),
            draft_json(&draft)
        );
        calls += 1;
        if let Some(resp) = client.generate_json(GUIDE_SYSTEM, &user_fix, draft_schema()).await {
            if let Some(d) = parse_draft(&resp) {
                draft = d;
            }
        }
        if round == 0 {
            // 修正後にもう一度だけ意味レビューをかける (計4コール目)。
            calls += 1;
            let review2 =
                run_review(format!("facts:\n{}\n\ndraft:\n{}", facts_str, draft_json(&draft)))
                    .await?;
            findings = review_findings_of(&review2);
            findings.extend(guard_violations(&draft, facts));
            total_findings += findings.len();
        } else {
            findings.clear();
        }
    }

    // 最終ガード (機械検査)。監査 MED-6/MED-8 対応:
    // - 致命 = 禁止表現 / 出所不明の数値 / 実在しない fact_id / 言い過ぎ / 誇張形容。
    //   修正2回を経ても残ったこれらは項目単位で落とす (顧客向け文書に残す実害の方が大きい)。
    // - ラベルはコロンまで含めた前方一致で判定 (F-CO と F-CO-CARD1 の prefix 衝突による
    //   巻き添えドロップを防ぐ)。
    let violations = guard_violations(&draft, facts);
    let fatal = |label_with_colon: &str| {
        violations.iter().any(|v| {
            v.starts_with(label_with_colon)
                && (v.contains("禁止表現")
                    || v.contains("無い数値")
                    || v.contains("実在しない")
                    || v.contains("言い過ぎ")
                    || v.contains("誇張形容"))
        })
    };
    if fatal("lead:") {
        draft.lead = String::new();
    }
    draft
        .per_fact
        .retain(|pf| !fatal(&format!("per_fact {}:", pf.fact_id)));
    draft
        .composites
        .retain(|c| !fatal(&format!("composite「{}」", c.title)));
    let steps: Vec<String> = draft
        .next_steps
        .iter()
        .enumerate()
        .filter(|(i, _)| !fatal(&format!("next_step {}:", i + 1)))
        .map(|(_, s)| s.clone())
        .collect();
    draft.next_steps = steps;

    if draft.per_fact.is_empty() && draft.composites.is_empty() {
        tracing::warn!(?violations, "guide AI: 最終ガードで全滅。決定的テンプレへフォールバック");
        return None;
    }

    tracing::info!(
        calls,
        total_findings,
        per_fact = draft.per_fact.len(),
        composites = draft.composites.len(),
        remaining_violations = violations.len(),
        "guide AI pipeline finished"
    );

    Some(GuideAiOutcome {
        draft,
        review_findings: total_findings,
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
         集計から算出しています。解釈の文章は、数値の出所確認を含む複数段階の検証を経て掲載しています。\
         記載はデータから言える範囲の傾向・可能性であり、断定ではありません。</div></header>\n",
        escape_html(region)
    ));

    if !d.lead.is_empty() {
        html.push_str(&format!(
            "<div class=\"keybox\">{}</div>\n",
            escape_html(&d.lead)
        ));
    }

    // §1 貴社の現在地
    //
    // 2026-07-21 監査対応: 旧実装は中間値ベースの「下位◯%」だけを表にしており、
    // F-CO 事実文で直した物差し補正 (下限どうしの比較 / 単一値表記の注意 / 仮の中間値)
    // が画面に出ていなかった。観測は F-CO 事実文 (コード確定の完全版) をそのまま表示する。
    if position.is_some() {
        html.push_str("<h2>貴社の現在地</h2>\n");
        if let Some(fco) = facts.iter().find(|f| f.id == "F-CO") {
            html.push_str(&format!("<p>{}</p>\n", escape_html(&fco.statement)));
        }
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
         本資料の数値はレポート本体と同じ集計から生成しています。解釈の文章は、数値の出所確認・\
         表現の点検を含む複数段階の検証を経ています。応募数・採用可否を保証するものでは\
         ありません。</footer>\n",
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
    let (facts, position) = build_fact_inventory(agg, ctx, Some(muni).filter(|m| !m.is_empty()), company);
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
        let (facts, pos) = build_fact_inventory(&rich_agg(), None, None, Some("テスト工業"));
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
        let (facts, _) = build_fact_inventory(&rich_agg(), None, None, None);
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
    fn kanji_single_digit_quantities_are_blocked() {
        // 2026-07-22 再監査 [HIGH]: 単漢字+金額/比率単位のすり抜け回帰テスト
        let allowed: HashSet<String> = ["25.0", "25"].iter().map(|s| s.to_string()).collect();
        assert!(!numbers_ok("上限は五万円まで可能です", &allowed));
        assert!(!numbers_ok("求職者の八割が対象です", &allowed));
        assert!(!numbers_ok("応募が三倍になる可能性", &allowed));
        assert!(!numbers_ok("月給十万円からのスタート", &allowed));
        // 慣用表現 (単漢字+日/人/件) は従来どおり許容
        assert!(numbers_ok("一人ひとりに合わせた対応", &allowed));
        assert!(numbers_ok("一件ずつ確認します", &allowed));
    }

    #[test]
    fn unfounded_application_outcome_is_flagged() {
        // 2026-07-22 再監査: 応募数への言及 (資料に応募データが無い) は禁止表現扱い
        let (facts, _) = build_fact_inventory(&rich_agg(), None, None, None);
        let draft = GuideDraft {
            lead: String::new(),
            per_fact: facts
                .iter()
                .map(|f| PerFact {
                    fact_id: f.id.clone(),
                    dakara: "タグの付与により応募数が増加する可能性があります。".to_string(),
                })
                .collect(),
            composites: vec![],
            next_steps: vec![],
        };
        let v = guard_violations(&draft, &facts);
        assert!(
            v.iter().any(|s| s.contains("応募数・応募意欲への言及")),
            "応募数への飛躍が検出されるはず: {:?}",
            v
        );
    }

    #[test]
    fn composite_must_cite_numbers_from_two_facts() {
        // 2026-07-22 再監査: 数値を引用しない複合 (見出しの言い換え) を検出
        let (facts, _) = build_fact_inventory(&rich_agg(), None, None, None);
        let draft = GuideDraft {
            lead: String::new(),
            per_fact: facts
                .iter()
                .map(|f| PerFact {
                    fact_id: f.id.clone(),
                    dakara: "判断材料になる可能性があります。".to_string(),
                })
                .collect(),
            composites: vec![GuideComposite {
                title: "条件設定の最適化".to_string(),
                thesis: "市場の給与中央値および休日水準を考慮した条件設定の余地があります。".to_string(),
                fact_ids: vec!["F-SAL".to_string(), "F-HOL".to_string()],
                so_what: "条件の見せ方の見直しが検討候補になります。".to_string(),
            }],
            next_steps: vec![],
        };
        let v = guard_violations(&draft, &facts);
        assert!(
            v.iter().any(|s| s.contains("数値を引用しているのが2件未満")),
            "数値なし複合が検出されるはず: {:?}",
            v
        );
    }

    #[test]
    fn midrank_pct_handles_ties() {
        // 中央値と同額のタイ集中で「下位60%」と出ない (同順位按分)
        let vals: Vec<i64> = vec![200, 250, 250, 250, 250, 250, 300, 350, 400, 450];
        let p = midrank_pct(&vals, 250).unwrap();
        assert!(
            (p - 35.0).abs() < 1.0,
            "below=1, eq=5 → (1+2.5)/10 = 35%: {}",
            p
        );
    }

    #[test]
    fn numbers_guard_blocks_fullwidth_kanji_and_unit_decomposition() {
        // ガード監査 HIGH-1/2/3 の回帰テスト
        let allowed: HashSet<String> = ["25.0", "25", "21"].iter().map(|s| s.to_string()).collect();
        // 全角数字の捏造は不合格 (正規化後に照合される)
        assert!(!numbers_ok("上限給与は ４０万円 に達する可能性があります", &allowed));
        // 漢数字2文字以上+数量単位は表記自体を不合格
        assert!(!numbers_ok("年間休日は 百三十 日以上が主流です", &allowed));
        assert!(!numbers_ok("三十五万円 まで可能です", &allowed));
        // 桁分解 (数字+万/千) は単位付き照合で不合格
        assert!(!numbers_ok("月給 3万5千円 も可能です", &allowed));
        // 1桁+割/倍の割合捏造は不合格
        assert!(!numbers_ok("求職者の 8割 が対象です", &allowed));
        assert!(!numbers_ok("応募が 3倍 になる可能性があります", &allowed));
        // 正当な表現は通る: 許可済み数値+単位 / 序数 / 「一人ひとり」(漢数字1文字)
        assert!(numbers_ok("市場の 21% が新着で、25万円 が下限です", &allowed));
        assert!(numbers_ok("次の3つが優先です。一人ひとりに合わせます", &allowed));
        // 「25.0万円」→「25万円」の言い換えは fact 側の .0 なし変種で許可される
        assert!(numbers_ok("下限は 25万円 です", &allowed));
    }

    #[test]
    fn numbers_guard_scopes_per_fact_allowed_set() {
        // ガード監査 MED-4: 別 fact の数値 (休日53%) を給与の話に流用したら検出
        let (facts, _) = build_fact_inventory(&rich_agg(), None, None, None);
        let draft = GuideDraft {
            lead: String::new(),
            per_fact: vec![PerFact {
                fact_id: "F-SAL".to_string(),
                // 53 は F-HOL 由来で F-SAL の numbers には無い
                dakara: "給与は市場平均を 53% 上回る可能性があります。".to_string(),
            }],
            composites: vec![],
            next_steps: vec![],
        };
        let v = guard_violations(&draft, &facts);
        assert!(
            v.iter().any(|s| s.starts_with("per_fact F-SAL:") && s.contains("無い数値")),
            "fact 跨ぎの数値流用が検出されるはず: {:?}",
            v
        );
    }

    #[test]
    fn fatal_label_no_prefix_collision() {
        // ガード監査 MED-8: F-CO-CARD1 の違反ラベルが F-CO を巻き添えにしない
        // (ラベルはコロン込み前方一致で判定される)
        let label_co = "per_fact F-CO:";
        let violation_card = "per_fact F-CO-CARD1: 事実インベントリに無い数値を含む";
        assert!(!violation_card.starts_with(label_co));
    }

    #[test]
    fn guard_detects_fake_fact_id_and_missing_coverage() {
        let (facts, _) = build_fact_inventory(&rich_agg(), None, None, None);
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
        let (facts, _) = build_fact_inventory(&agg, None, None, Some("テスト工業"));
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
        let (facts, _) = build_fact_inventory(&agg, None, None, Some("テスト工業"));
        let card = facts.iter().find(|f| f.id == "F-CO-CARD1").unwrap();
        assert!(
            !card.statement.contains("反映されていない"),
            "同額は乖離ではない: {}",
            card.statement
        );
    }

    #[test]
    fn holiday_correlation_sign_is_code_determined() {
        // 逆相関市場 (r=-0.5): 事実文にトレードオフ注意が入る
        let mut agg = rich_agg();
        agg.jobbox.salary_holidays_correlation = Some(-0.5);
        let (facts, _) = build_fact_inventory(&agg, None, None, None);
        let hol = facts.iter().find(|f| f.id == "F-HOL").unwrap();
        assert!(hol.statement.contains("逆相関"), "{}", hol.statement);
        assert!(hol.statement.contains("トレードオフ"), "{}", hol.statement);

        // 無相関市場 (r=0.08): 独立の明記
        let mut agg2 = rich_agg();
        agg2.jobbox.salary_holidays_correlation = Some(0.08);
        let (facts2, _) = build_fact_inventory(&agg2, None, None, None);
        let hol2 = facts2.iter().find(|f| f.id == "F-HOL").unwrap();
        assert!(hol2.statement.contains("独立"), "{}", hol2.statement);
        assert!(!hol2.statement.contains("逆相関"), "{}", hol2.statement);
    }

    #[test]
    fn guard_detects_overstatement() {
        // 実出力で検出した「強力な訴求ポイントです」型の誇張形容
        let (facts, _) = build_fact_inventory(&rich_agg(), None, None, None);
        let draft = GuideDraft {
            lead: "貴社の休日は強力な訴求ポイントです。".to_string(),
            per_fact: facts
                .iter()
                .map(|f| PerFact {
                    fact_id: f.id.clone(),
                    dakara: "判断材料になる可能性があります。".to_string(),
                })
                .collect(),
            composites: vec![],
            next_steps: vec![],
        };
        let v = guard_violations(&draft, &facts);
        assert!(
            v.iter().any(|s| s.contains("誇張形容")),
            "誇張形容が検出されるはず: {:?}",
            v
        );
    }

    #[test]
    fn guard_detects_vague_landing() {
        let (facts, _) = build_fact_inventory(&rich_agg(), None, None, None);
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
        let (facts, _) = build_fact_inventory(&rich_agg(), Some(&ctx), None, None);
        let com = facts.iter().find(|f| f.id == "F-COM").unwrap();
        assert!(
            com.statement.contains("市外へ出ていく構造"),
            "流出超過の方向がコードで明記されるはず: {}",
            com.statement
        );
    }

    #[test]
    fn guard_detects_single_theme_composite_and_forbidden() {
        let (facts, _) = build_fact_inventory(&rich_agg(), None, None, None);
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
