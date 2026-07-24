//! 候補キーワードの分類整理(Gemini)。
//!
//! `/api/suggest` で数十〜数百件出る関連キーワードを、**軸で分類して見やすくする**だけの
//! モジュール。LLM に判断・推奨・優劣づけは一切させない(ツールは事実提示のみ、断言は人がする)。
//!
//! # 捏造防止の設計
//! - LLM に渡すのは**キーワード文字列とカテゴリ名だけ**。検索量などの数値は
//!   プロンプトにもスキーマにも入れない(→ LLM が数値を出力する余地がゼロ)。
//!   数値は [`merge`] が元データ(`source`)から引き直す。
//! - responseSchema は `{items:[{keyword, category}]}` のみ。理由・推奨・スコアの欄を持たない。
//! - [`merge`] が LLM 出力を元データと突合し、
//!   - 元データに無いキーワード(= LLM の創作)は**破棄**して `hallucinated_count` に計上、
//!   - LLM が返さなかった元キーワードは**「その他」へ回収**して `unassigned_count` に計上。
//!   これにより「出力キーワード集合 ⊆ 入力キーワード集合」かつ「欠落ゼロ」が構造的に保証される。

use std::collections::{HashMap, HashSet};

use serde_json::{json, Value};

/// 既定の分類カテゴリ(この配列以外のカテゴリは LLM に提示しない)。
pub const DEFAULT_CATEGORIES: [&str; 7] = [
    "職種・仕事内容",
    "雇用形態",
    "経験・資格",
    "年齢・属性",
    "勤務条件(時間/勤務地/給与)",
    "媒体・サービス名",
    "その他",
];

/// 未分類・破棄後の回収先カテゴリ名。
pub const FALLBACK_CATEGORY: &str = "その他";

/// 既定カテゴリを `Vec<String>` で返す(呼び出し側の利便用)。
pub fn default_categories() -> Vec<String> {
    DEFAULT_CATEGORIES.iter().map(|s| s.to_string()).collect()
}

/// 分類プロンプトを組み立てる(純粋)。
///
/// 与えたキーワードを、与えたカテゴリのいずれか 1 つに割り当てさせるだけの指示。
/// 数値は一切渡さない。理由・推奨・優劣づけは明示的に禁止する。
pub fn build_prompt(keywords: &[String], categories: &[String]) -> String {
    let cat_lines = categories
        .iter()
        .map(|c| format!("- {c}"))
        .collect::<Vec<_>>()
        .join("\n");
    let kw_lines = keywords
        .iter()
        .map(|k| format!("- {k}"))
        .collect::<Vec<_>>()
        .join("\n");

    format!(
        "あなたは求人キーワードの分類作業者です。以下のキーワードを、以下のカテゴリのいずれか1つに振り分けてください。\n\
\n\
# カテゴリ(この一覧にあるものだけを使う)\n\
{cat_lines}\n\
\n\
# キーワード\n\
{kw_lines}\n\
\n\
# 守るべき規律\n\
1. 入力キーワードは一字一句そのまま返す(表記ゆれの修正・要約・言い換えを行わない)。\n\
2. 新しい語を作らない(入力に無いキーワードを出力してはならない)。\n\
3. すべてのキーワードを必ずどれか1つのカテゴリに入れる(欠落・重複を出さない)。\n\
4. カテゴリは与えたものだけを使う(新しいカテゴリ名を作らない)。判断に迷うものは「{FALLBACK_CATEGORY}」に入れる。\n\
5. 理由・推奨・優劣・順位・評価コメントは書かない。検索量などの数値も書かない。\n\
6. 出力は指定された JSON スキーマ(items の keyword と category のみ)に厳密に従う。\n"
    )
}

/// 分類結果の responseSchema(`{items:[{keyword, category}]}`)。
///
/// 数値フィールドを持たないことが捏造防止の要。
pub fn response_schema() -> Value {
    json!({
        "type": "object",
        "properties": {
            "items": {
                "type": "array",
                "items": {
                    "type": "object",
                    "properties": {
                        "keyword": {"type": "string"},
                        "category": {"type": "string"}
                    },
                    "required": ["keyword", "category"]
                }
            }
        },
        "required": ["items"]
    })
}

/// `keyword:volume` をカンマ/改行区切りで受け、キーワード→検索量の対応表にする(純粋)。
///
/// 値が数値でない要素は無視する。キーワード側に `:` を含む場合に備え、最後の `:` で分割する。
pub fn parse_volumes(raw: &str) -> HashMap<String, i64> {
    let mut map = HashMap::new();
    for part in raw.split(['\n', ',']) {
        let part = part.trim();
        if part.is_empty() {
            continue;
        }
        // 全角コロンも許容する。
        let normalized = part.replace('：', ":");
        if let Some((k, v)) = normalized.rsplit_once(':') {
            let k = k.trim();
            if k.is_empty() {
                continue;
            }
            if let Ok(n) = v.trim().parse::<i64>() {
                map.insert(k.to_string(), n);
            }
        }
    }
    map
}

/// LLM の分類結果を元データと突合して返却 JSON を組み立てる(純粋)。
///
/// - `assignments`: `{items:[{keyword, category}]}` 形の LLM 出力。
/// - `source`: 元データ(キーワード, avg_monthly)。**数値の唯一の出所**。
///
/// 返却: `{categories:[{name, keywords:[{keyword, avg_monthly}], total_volume, count}],
/// unassigned_count, hallucinated_count}`。
/// カテゴリ内は avg_monthly 降順(None は最後、同値は元データ順)、カテゴリは total_volume 降順。
pub fn merge(assignments: &Value, source: &[(String, Option<i64>)]) -> Value {
    // 元データ: キーワード → (元順序, avg_monthly)
    let mut order: HashMap<&str, usize> = HashMap::new();
    let mut vol: HashMap<&str, Option<i64>> = HashMap::new();
    for (i, (kw, v)) in source.iter().enumerate() {
        // 元データ内の重複は先勝ち(順序保持)。
        order.entry(kw.as_str()).or_insert(i);
        vol.entry(kw.as_str()).or_insert(*v);
    }

    let mut assigned: HashMap<&str, String> = HashMap::new(); // keyword -> category
    let mut seen: HashSet<&str> = HashSet::new();
    let mut hallucinated_count: i64 = 0;

    if let Some(items) = assignments.get("items").and_then(Value::as_array) {
        for it in items {
            let kw = it.get("keyword").and_then(Value::as_str).unwrap_or("").trim();
            let cat = it
                .get("category")
                .and_then(Value::as_str)
                .unwrap_or("")
                .trim();
            if kw.is_empty() {
                continue;
            }
            // 元データに無い語 = LLM の創作。破棄して計上。
            let Some((&src_kw, _)) = order.get_key_value(kw) else {
                hallucinated_count += 1;
                continue;
            };
            // 同じ語が複数回返ってきた場合は先勝ち(重複増殖を防ぐ)。
            if !seen.insert(src_kw) {
                continue;
            }
            let cat = if cat.is_empty() {
                FALLBACK_CATEGORY.to_string()
            } else {
                cat.to_string()
            };
            assigned.insert(src_kw, cat);
        }
    }

    // LLM が返さなかった元キーワードは「その他」へ回収(欠落させない)。
    let mut unassigned_count: i64 = 0;
    for (kw, _) in source.iter() {
        let k = kw.as_str();
        if !assigned.contains_key(k) {
            // 元データ重複の 2 件目以降は既に assigned 済みなのでここには来ない。
            assigned.insert(k, FALLBACK_CATEGORY.to_string());
            unassigned_count += 1;
        }
    }

    // カテゴリごとに集約。
    let mut buckets: HashMap<String, Vec<&str>> = HashMap::new();
    for (kw, cat) in assigned.iter() {
        buckets.entry(cat.clone()).or_default().push(kw);
    }

    let mut cats: Vec<Value> = Vec::new();
    let mut cat_sort_keys: Vec<(i64, usize, String)> = Vec::new();
    for (name, mut kws) in buckets.into_iter() {
        // カテゴリ内: avg_monthly 降順(None は最後)、同値は元データ順。
        kws.sort_by(|a, b| {
            let va = vol.get(a).copied().flatten();
            let vb = vol.get(b).copied().flatten();
            match (vb, va) {
                (Some(x), Some(y)) => x.cmp(&y),
                // b が値持ち・a が None → a は後ろ(None は最後)。
                (Some(_), None) => std::cmp::Ordering::Greater,
                (None, Some(_)) => std::cmp::Ordering::Less,
                (None, None) => std::cmp::Ordering::Equal,
            }
            .then_with(|| {
                order
                    .get(a)
                    .copied()
                    .unwrap_or(usize::MAX)
                    .cmp(&order.get(b).copied().unwrap_or(usize::MAX))
            })
        });
        let total_volume: i64 = kws
            .iter()
            .map(|k| vol.get(k).copied().flatten().unwrap_or(0))
            .sum();
        let count = kws.len();
        let rows: Vec<Value> = kws
            .iter()
            .map(|k| {
                json!({
                    "keyword": k,
                    "avg_monthly": vol.get(k).copied().flatten(),
                })
            })
            .collect();
        cat_sort_keys.push((total_volume, count, name.clone()));
        cats.push(json!({
            "name": name,
            "keywords": rows,
            "total_volume": total_volume,
            "count": count,
        }));
    }

    // カテゴリ: total_volume 降順 → count 降順 → 名前昇順(決定論)。
    let mut idx: Vec<usize> = (0..cats.len()).collect();
    idx.sort_by(|&a, &b| {
        let (va, ca, na) = &cat_sort_keys[a];
        let (vb, cb, nb) = &cat_sort_keys[b];
        vb.cmp(va).then_with(|| cb.cmp(ca)).then_with(|| na.cmp(nb))
    });
    let ordered: Vec<Value> = idx.into_iter().map(|i| cats[i].clone()).collect();

    json!({
        "categories": ordered,
        "unassigned_count": unassigned_count,
        "hallucinated_count": hallucinated_count,
    })
}

#[cfg(test)]
mod tests {
    use super::*;

    fn src(v: &[(&str, Option<i64>)]) -> Vec<(String, Option<i64>)> {
        v.iter().map(|(k, n)| (k.to_string(), *n)).collect()
    }

    #[test]
    fn prompt_contains_discipline_lines() {
        let p = build_prompt(
            &["ドライバー 求人".to_string()],
            &default_categories(),
        );
        // 規律文言(捏造防止の核)が全て入っていること。
        assert!(p.contains("一字一句そのまま"), "{p}");
        assert!(p.contains("新しい語を作らない"), "{p}");
        assert!(p.contains("すべてのキーワードを必ずどれか1つのカテゴリに入れる"), "{p}");
        assert!(p.contains("カテゴリは与えたものだけを使う"), "{p}");
        assert!(p.contains("理由・推奨・優劣・順位・評価コメントは書かない"), "{p}");
        assert!(p.contains("数値も書かない"), "{p}");
        // キーワードとカテゴリが列挙されている。
        assert!(p.contains("- ドライバー 求人"));
        assert!(p.contains("- 雇用形態"));
    }

    #[test]
    fn prompt_contains_no_numbers_from_source() {
        // build_prompt は数値を受け取らない = プロンプトに検索量が混入しない。
        let p = build_prompt(&["a".to_string(), "b".to_string()], &default_categories());
        assert!(!p.contains("avg_monthly"));
        assert!(!p.contains("検索量:"));
    }

    #[test]
    fn schema_has_only_keyword_and_category() {
        let s = response_schema();
        let props = &s["properties"]["items"]["items"]["properties"];
        assert!(props.get("keyword").is_some());
        assert!(props.get("category").is_some());
        // 数値欄を持たない(LLM に数値を書かせない)。
        assert_eq!(props.as_object().unwrap().len(), 2);
        assert!(props.get("avg_monthly").is_none());
    }

    #[test]
    fn merge_discards_hallucinated_keywords() {
        let source = src(&[("大型ドライバー 求人", Some(100))]);
        let a = json!({"items": [
            {"keyword": "大型ドライバー 求人", "category": "職種・仕事内容"},
            {"keyword": "存在しないキーワード", "category": "職種・仕事内容"},
        ]});
        let out = merge(&a, &source);
        assert_eq!(out["hallucinated_count"], 1);
        assert_eq!(out["unassigned_count"], 0);
        let cats = out["categories"].as_array().unwrap();
        assert_eq!(cats.len(), 1);
        assert_eq!(cats[0]["count"], 1);
        assert_eq!(cats[0]["keywords"][0]["keyword"], "大型ドライバー 求人");
    }

    #[test]
    fn merge_recovers_missing_keywords_into_other() {
        let source = src(&[("a", Some(10)), ("b", Some(20)), ("c", None)]);
        // LLM は a しか返さなかった。
        let a = json!({"items": [{"keyword": "a", "category": "雇用形態"}]});
        let out = merge(&a, &source);
        assert_eq!(out["unassigned_count"], 2);
        assert_eq!(out["hallucinated_count"], 0);
        // 全キーワードがどこかに存在する(欠落ゼロ)。
        let mut found: Vec<String> = vec![];
        for c in out["categories"].as_array().unwrap() {
            for k in c["keywords"].as_array().unwrap() {
                found.push(k["keyword"].as_str().unwrap().to_string());
            }
        }
        found.sort();
        assert_eq!(found, vec!["a".to_string(), "b".to_string(), "c".to_string()]);
        // b, c は「その他」に回収されている。
        let other = out["categories"]
            .as_array()
            .unwrap()
            .iter()
            .find(|c| c["name"] == FALLBACK_CATEGORY)
            .unwrap();
        assert_eq!(other["count"], 2);
    }

    #[test]
    fn merge_sorts_within_and_across_categories() {
        let source = src(&[
            ("低", Some(5)),
            ("高", Some(500)),
            ("中", Some(50)),
            ("null語", None),
            ("別枠", Some(1000)),
        ]);
        let a = json!({"items": [
            {"keyword": "低", "category": "雇用形態"},
            {"keyword": "高", "category": "雇用形態"},
            {"keyword": "中", "category": "雇用形態"},
            {"keyword": "null語", "category": "雇用形態"},
            {"keyword": "別枠", "category": "経験・資格"},
        ]});
        let out = merge(&a, &source);
        let cats = out["categories"].as_array().unwrap();
        // カテゴリは total_volume 降順: 経験・資格(1000) > 雇用形態(555)
        assert_eq!(cats[0]["name"], "経験・資格");
        assert_eq!(cats[0]["total_volume"], 1000);
        assert_eq!(cats[1]["name"], "雇用形態");
        assert_eq!(cats[1]["total_volume"], 555);
        // カテゴリ内は avg_monthly 降順、None は最後。
        let kws: Vec<&str> = cats[1]["keywords"]
            .as_array()
            .unwrap()
            .iter()
            .map(|k| k["keyword"].as_str().unwrap())
            .collect();
        assert_eq!(kws, vec!["高", "中", "低", "null語"]);
        assert!(cats[1]["keywords"][3]["avg_monthly"].is_null());
    }

    #[test]
    fn merge_uses_source_numbers_not_llm_numbers() {
        // LLM が勝手に数値を付けても無視され、元データの値だけが出る。
        let source = src(&[("a", Some(42))]);
        let a = json!({"items": [{"keyword": "a", "category": "雇用形態", "avg_monthly": 999999}]});
        let out = merge(&a, &source);
        assert_eq!(out["categories"][0]["keywords"][0]["avg_monthly"], 42);
        assert_eq!(out["categories"][0]["total_volume"], 42);
    }

    #[test]
    fn merge_dedups_repeated_assignments() {
        let source = src(&[("a", Some(10))]);
        let a = json!({"items": [
            {"keyword": "a", "category": "雇用形態"},
            {"keyword": "a", "category": "経験・資格"},
        ]});
        let out = merge(&a, &source);
        let cats = out["categories"].as_array().unwrap();
        assert_eq!(cats.len(), 1);
        assert_eq!(cats[0]["name"], "雇用形態"); // 先勝ち
        assert_eq!(out["hallucinated_count"], 0);
        assert_eq!(out["unassigned_count"], 0);
    }

    #[test]
    fn merge_empty_or_broken_llm_output_falls_back_to_other() {
        let source = src(&[("a", Some(1)), ("b", None)]);
        let out = merge(&json!({}), &source);
        assert_eq!(out["unassigned_count"], 2);
        assert_eq!(out["hallucinated_count"], 0);
        assert_eq!(out["categories"][0]["name"], FALLBACK_CATEGORY);
        assert_eq!(out["categories"][0]["count"], 2);
    }

    #[test]
    fn merge_treats_unknown_category_name_as_is_but_keeps_keyword() {
        // カテゴリ名を勝手に作られても、キーワードは失われない(可視化はされる)。
        let source = src(&[("a", Some(1))]);
        let a = json!({"items": [{"keyword": "a", "category": "勝手なカテゴリ"}]});
        let out = merge(&a, &source);
        assert_eq!(out["categories"][0]["keywords"][0]["keyword"], "a");
    }

    #[test]
    fn parse_volumes_reads_pairs() {
        let m = parse_volumes("大型 求人:1200, 中型:300\n壊れ行, 空:, x：50");
        assert_eq!(m.get("大型 求人"), Some(&1200));
        assert_eq!(m.get("中型"), Some(&300));
        assert_eq!(m.get("x"), Some(&50)); // 全角コロン
        assert!(m.get("壊れ行").is_none());
        assert!(m.get("空").is_none());
    }
}
