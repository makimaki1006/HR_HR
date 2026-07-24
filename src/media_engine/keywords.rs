//! キーワード生成と Google Ads 突合用の正規化。
//!
//! Python 版 `case_conditions.place_agnostic_keywords` / `_squash_keyword` /
//! `build_volume_map` / `organic_reach.normalize_keyword_key` の移植。

use std::collections::HashMap;

use unicode_normalization::UnicodeNormalization;

/// 地名なしの素キーワード修飾語(地域は location_id 側で効かせる)。
pub const DEFAULT_MODIFIERS: [&str; 5] = ["求人", "正社員", "転職", "パート", "未経験"];

/// 職種＋修飾語の地名なしキーワードを作る(例: 看護師 求人 / 看護師 正社員)。
pub fn place_agnostic_keywords(job: &str) -> Vec<String> {
    let job = job.trim();
    DEFAULT_MODIFIERS
        .iter()
        .map(|m| format!("{job} {m}"))
        .collect()
}

/// 空白・大小・全半角を潰した突合キー。
///
/// Google Ads は返却時にクエリを再トークナイズする(「看護師 求人」→「看護 師 求人」、
/// 「IT」→「it」)。NFKC 正規化 + 小文字化 + 全空白除去でこのズレを吸収する。
pub fn squash_keyword(s: &str) -> String {
    s.nfkc()
        .flat_map(|c| c.to_lowercase())
        .filter(|c| !c.is_whitespace())
        .collect()
}

/// 語順に依存しない正規化キー(空白区切りトークンを小文字化してソート結合)。
pub fn normalize_keyword_key(s: &str) -> String {
    let mut tokens: Vec<String> = s
        .split_whitespace()
        .map(|t| t.chars().flat_map(|c| c.to_lowercase()).collect::<String>())
        .filter(|t| !t.is_empty())
        .collect();
    tokens.sort();
    tokens.join(" ")
}

/// Google Ads の返却行を要求キーワードへ突合する。
///
/// ①完全一致 → ②空白/大小/全半角吸収([`squash_keyword`]) → ③語順非依存
/// ([`normalize_keyword_key`]) の順にフォールバックし、再トークナイズ・語順違いに
/// よる取りこぼしを防ぐ。返さなかった語は `None`(0 でない)。
///
/// `rows` は (返却テキスト, 値) の並び。完全一致は後勝ち(Python の代入と一致)、
/// 正規化キーは先勝ち(setdefault と一致)。
pub fn build_volume_map<V: Clone>(
    requested: &[String],
    rows: &[(String, V)],
) -> Vec<(String, Option<V>)> {
    let mut by_text: HashMap<&str, &V> = HashMap::new();
    let mut by_squash: HashMap<String, &V> = HashMap::new();
    let mut by_sorted: HashMap<String, &V> = HashMap::new();
    for (text, value) in rows {
        by_text.insert(text.as_str(), value); // 後勝ち
        by_squash.entry(squash_keyword(text)).or_insert(value); // 先勝ち
        by_sorted.entry(normalize_keyword_key(text)).or_insert(value); // 先勝ち
    }
    requested
        .iter()
        .map(|kw| {
            let hit = by_text
                .get(kw.as_str())
                .or_else(|| by_squash.get(&squash_keyword(kw)))
                .or_else(|| by_sorted.get(&normalize_keyword_key(kw)));
            (kw.clone(), hit.map(|v| (*v).clone()))
        })
        .collect()
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn place_agnostic_has_no_place() {
        let kws = place_agnostic_keywords("看護師");
        assert_eq!(kws[0], "看護師 求人");
        assert_eq!(kws.len(), 5);
        assert!(kws.iter().all(|k| k.starts_with("看護師 ")));
    }

    #[test]
    fn squash_absorbs_retokenization() {
        assert_eq!(
            squash_keyword("看護師 求人 東京"),
            squash_keyword("看護 師 求人 東京")
        );
        assert_eq!(squash_keyword("ITエンジニア 求人"), "itエンジニア求人");
        assert_eq!(squash_keyword("看護師　求人"), "看護師求人");
    }

    #[test]
    fn normalize_key_is_word_order_invariant() {
        assert_eq!(
            normalize_keyword_key("介護 求人 東京"),
            normalize_keyword_key("東京 介護 求人")
        );
        assert_eq!(normalize_keyword_key("IT 求人"), normalize_keyword_key("it 求人"));
    }

    #[test]
    fn volume_map_absorbs_retokenization_and_word_order() {
        let rows = vec![
            ("看護 師 求人 東京".to_string(), 1000_i64),
            ("it エンジニア 求人 東京".to_string(), 10),
            ("東京 営業 求人".to_string(), 320),
        ];
        let requested = vec![
            "看護師 求人 東京".to_string(),
            "ITエンジニア 求人 東京".to_string(),
            "営業 求人 東京".to_string(),
            "存在しない語 大阪".to_string(),
        ];
        let got = build_volume_map(&requested, &rows);
        assert_eq!(got[0].1, Some(1000)); // 再分割を吸収
        assert_eq!(got[1].1, Some(10)); // 小文字化+分割
        assert_eq!(got[2].1, Some(320)); // 語順違い
        assert_eq!(got[3].1, None); // 誤マッチしない
    }
}
