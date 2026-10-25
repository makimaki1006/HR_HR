//! 市区町村名 → citycode の静的マッピング
//!
//! `data/agoop/turso_csv/master_city.csv` を `include_str!` で埋め込み、
//! 起動時に (prefcode, city_name) → citycode のハッシュマップを構築する。

use super::pref_name_to_code;
use std::collections::HashMap;
use std::sync::OnceLock;

const MASTER_CITY_CSV: &str = include_str!("master_city.csv");

fn map() -> &'static HashMap<(String, String), u32> {
    static MAP: OnceLock<HashMap<(String, String), u32>> = OnceLock::new();
    MAP.get_or_init(|| {
        let mut m = HashMap::new();
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
            let prefcode = parts[1].trim();
            let city_name = parts[2].trim().to_string();
            if prefcode.is_empty() || city_name.is_empty() {
                continue;
            }
            // prefcode を 2桁 0 埋めで正規化（"1" → "01"）
            let prefcode_padded = format!("{:0>2}", prefcode);
            m.insert((prefcode_padded, city_name), citycode);
        }
        m
    })
}

/// 都道府県名 + 市区町村名 から citycode を引く
pub fn city_name_to_code(pref_name: &str, city_name: &str) -> Option<u32> {
    let pref_map = pref_name_to_code();
    let prefcode = pref_map.get(pref_name)?;
    map()
        .get(&(prefcode.to_string(), city_name.to_string()))
        .copied()
}

/// 都道府県名から、その都道府県の市区町村名と citycode を citycode 順に返す (ハローワーク DB が無いときの選択肢)
pub fn cities_of(pref_name: &str) -> Vec<(String, u32)> {
    let Some(prefcode) = pref_name_to_code().get(pref_name).map(|c| c.to_string()) else {
        return Vec::new();
    };
    let mut v: Vec<(String, u32)> = map()
        .iter()
        .filter(|((p, _), _)| *p == prefcode)
        .map(|((_, name), code)| (name.clone(), *code))
        .collect();
    v.sort_by_key(|(_, code)| *code);
    v
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn known_city_codes() {
        // 札幌市中央区 = 1101 (pref=01, city=札幌市中央区)
        assert_eq!(city_name_to_code("北海道", "札幌市中央区"), Some(1101));
        // 東京都千代田区 = 13101
        assert_eq!(city_name_to_code("東京都", "千代田区"), Some(13101));
        // 大阪府大阪市北区 = 27127
        assert_eq!(city_name_to_code("大阪府", "大阪市北区"), Some(27127));
    }

    #[test]
    fn cities_of_lists_the_prefecture_in_code_order() {
        let tokyo = cities_of("東京都");
        assert_eq!(tokyo.len(), 62);
        assert_eq!(tokyo[0], ("千代田区".to_string(), 13101));
        assert!(tokyo.windows(2).all(|w| w[0].1 < w[1].1));
        assert!(cities_of("架空県").is_empty());
    }

    #[test]
    fn unknown_returns_none() {
        assert_eq!(city_name_to_code("東京都", "存在しない市"), None);
        assert_eq!(city_name_to_code("架空県", "千代田区"), None);
    }
}
