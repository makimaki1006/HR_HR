//! 応募者の住所を「都道府県 + 市区町村」までに丸める (求人文面管理 /api/job-copy/*)。
//!
//! HubSpot の応募者の都道府県欄・市区町村欄には、番地・建物名・部屋番号まで入っていることがある。
//! 元の文字列は API の JSON に載せない (ブラウザの開発者ツール・HAR・プロキシのログに残るため)。
//! 市区町村は `master_city.csv` にある名前だけを採用し、読み取れないときは推測せず「市区町村不明」にする。
//! フロントエンドの `frontend/src/screens/job-copy/applicantArea.ts` と同じ規則 (画面側でも丸め直す)。

use std::collections::HashMap;
use std::sync::OnceLock;
use unicode_normalization::UnicodeNormalization;

const MASTER_CITY_CSV: &str = include_str!("master_city.csv");
pub const AREA_UNKNOWN: &str = "不明";
const AREA_OTHER: &str = "その他";

struct Prefecture {
    name: &'static str,
    short: &'static str,
    /// (正規化した書き方, 表示する市区町村名)。長い書き方から順に並べる。
    cities: Vec<(String, String)>,
}

fn normalize(value: &str) -> String {
    value
        .nfkc()
        .filter(|c| !c.is_whitespace())
        .map(|c| match c {
            'ヶ' => 'ケ',
            'ヵ' => 'カ',
            c => c,
        })
        .collect()
}

fn prefectures() -> &'static [Prefecture] {
    static PREFECTURES: OnceLock<Vec<Prefecture>> = OnceLock::new();
    PREFECTURES.get_or_init(|| {
        let codes = super::pref_name_to_code();
        let mut by_code: Vec<(&'static str, &'static str)> =
            codes.iter().map(|(name, code)| (*code, *name)).collect();
        by_code.sort();
        let mut names: HashMap<u32, Vec<String>> = HashMap::new();
        for line in MASTER_CITY_CSV.lines().skip(1) {
            let parts: Vec<&str> = line.split(',').collect();
            let (Some(code), Some(city)) = (parts.get(1), parts.get(2)) else {
                continue;
            };
            let (Ok(code), city) = (code.trim().parse::<u32>(), city.trim()) else {
                continue;
            };
            if !city.is_empty() {
                names.entry(code).or_default().push(city.to_owned());
            }
        }
        by_code
            .into_iter()
            .map(|(code, name)| {
                let mut aliases: Vec<(String, String)> = Vec::new();
                let mut add = |key: String, value: &str| {
                    if !aliases.iter().any(|(existing, _)| *existing == key) {
                        aliases.push((key, value.to_owned()));
                    }
                };
                for city in names
                    .get(&code.parse::<u32>().unwrap_or(0))
                    .map(Vec::as_slice)
                    .unwrap_or(&[])
                {
                    add(normalize(city), city);
                    // 郡名を省いた書き方 (石狩郡当別町 → 当別町)
                    if let Some(index) = city.find('郡') {
                        let rest = &city[index + '郡'.len_utf8()..];
                        if !rest.is_empty() && (rest.ends_with('町') || rest.ends_with('村')) {
                            add(normalize(rest), city);
                        }
                    }
                    // 政令指定都市の区を書かない住所は市までにする (浜松市中央区 → 浜松市)
                    if city.ends_with('区') {
                        if let Some(index) = city.find('市') {
                            let shi = &city[..index + '市'.len_utf8()];
                            if shi.len() < city.len() {
                                add(normalize(shi), shi);
                            }
                        }
                    }
                }
                aliases.sort_by_key(|(key, _)| std::cmp::Reverse(key.chars().count()));
                let short = if name == "北海道" {
                    name
                } else {
                    &name[..name.len() - name.chars().last().map_or(0, char::len_utf8)]
                };
                Prefecture {
                    name,
                    short,
                    cities: aliases,
                }
            })
            .collect()
    })
}

fn split_prefecture(text: &str) -> Option<(&'static Prefecture, String)> {
    let all = prefectures();
    if let Some(prefecture) = all.iter().find(|p| text.starts_with(p.name)) {
        return Some((prefecture, text[prefecture.name.len()..].to_owned()));
    }
    all.iter()
        .find(|p| p.short == text)
        .map(|p| (p, String::new()))
}

fn match_city(prefecture: &Prefecture, text: &str) -> Option<String> {
    prefecture
        .cities
        .iter()
        .find(|(key, _)| text.starts_with(key.as_str()))
        .map(|(_, name)| name.clone())
}

/// 丸めた地域。どちらもマスタにある名前だけ。
#[derive(Clone, Debug, Default, PartialEq, Eq)]
pub struct RoundedArea {
    pub prefecture: Option<String>,
    pub municipality: Option<String>,
}

/// 都道府県欄と市区町村欄 (どちらも住所全体が入っていることがある) から、都道府県と市区町村だけを取り出す。
pub fn round_area(prefecture: Option<&str>, municipality: Option<&str>) -> RoundedArea {
    let prefecture_value = prefecture.map(normalize).unwrap_or_default();
    let city_value = municipality.map(normalize).unwrap_or_default();
    let from_prefecture = (!prefecture_value.is_empty())
        .then(|| split_prefecture(&prefecture_value))
        .flatten();
    let from_city = (!city_value.is_empty())
        .then(|| split_prefecture(&city_value))
        .flatten();
    let found = from_prefecture
        .as_ref()
        .map(|(p, _)| *p)
        .or_else(|| from_city.as_ref().map(|(p, _)| *p));
    // 市区町村欄が都道府県から始まるときは、それを外してから市区町村を探す
    let mut rest = match (&from_city, found) {
        (Some((p, rest)), Some(f)) if std::ptr::eq(*p, f) => rest.clone(),
        _ => city_value.clone(),
    };
    if rest.is_empty() {
        rest = from_prefecture
            .as_ref()
            .map(|(_, r)| r.clone())
            .unwrap_or_default();
    }
    if let Some(found) = found {
        return RoundedArea {
            prefecture: Some(found.name.to_owned()),
            municipality: match_city(found, &rest),
        };
    }
    // 都道府県が分からないときは、市区町村名が一つの都道府県にしか無い場合だけ採用する
    let mut best: Vec<(&'static str, String, usize)> = Vec::new();
    for prefecture in prefectures() {
        let Some((key, name)) = prefecture
            .cities
            .iter()
            .find(|(key, _)| rest.starts_with(key.as_str()))
        else {
            continue;
        };
        let length = key.chars().count();
        match best.first() {
            Some((_, _, best_length)) if length < *best_length => {}
            Some((_, _, best_length)) if length == *best_length => {
                best.push((prefecture.name, name.clone(), length))
            }
            _ => best = vec![(prefecture.name, name.clone(), length)],
        }
    }
    match best.as_slice() {
        [(prefecture, city, _)] => RoundedArea {
            prefecture: Some((*prefecture).to_owned()),
            municipality: Some(city.clone()),
        },
        _ => RoundedArea::default(),
    }
}

/// 都道府県の集計ラベル ("大分県" / "不明")。
pub fn prefecture_label(area: &RoundedArea) -> String {
    area.prefecture
        .clone()
        .unwrap_or_else(|| AREA_UNKNOWN.to_owned())
}

/// 市区町村の集計ラベル ("大分県大分市" / "大分県（市区町村不明）" / "不明")。
pub fn municipality_label(area: &RoundedArea) -> String {
    match (&area.prefecture, &area.municipality) {
        (None, _) => AREA_UNKNOWN.to_owned(),
        (Some(prefecture), Some(city)) => format!("{prefecture}{city}"),
        (Some(prefecture), None) => format!("{prefecture}（市区町村不明）"),
    }
}

/// 集計済みのラベルを丸める。元のラベルは「都道府県 / 市区町村」(都道府県が無いときは「都道府県不明 / …」)
/// や住所そのもの。丸め済みのラベルを渡しても同じ結果になる。
pub fn round_area_label(
    municipality: bool,
    label: &str,
    fallback_prefecture: Option<&str>,
) -> String {
    let trimmed = label.trim();
    if trimmed.is_empty() {
        return AREA_UNKNOWN.to_owned();
    }
    if trimmed == AREA_UNKNOWN || trimmed == AREA_OTHER {
        return trimmed.to_owned();
    }
    if !municipality {
        return prefecture_label(&round_area(Some(trimmed), None));
    }
    if let Some((prefecture, city)) = trimmed.split_once(" / ") {
        let prefecture = if prefecture == "都道府県不明" {
            fallback_prefecture
        } else {
            Some(prefecture)
        };
        return municipality_label(&round_area(prefecture, Some(city)));
    }
    if let Some(prefecture) = trimmed.strip_suffix("（市区町村不明）") {
        return municipality_label(&RoundedArea {
            prefecture: round_area(Some(prefecture), None).prefecture,
            municipality: None,
        });
    }
    municipality_label(&round_area(fallback_prefecture, Some(trimmed)))
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn drops_street_number_and_building_after_the_city() {
        let area = round_area(
            Some("大分県"),
            Some("大分市府内町3丁目10-1 府内ビル201号室"),
        );
        assert_eq!(
            area,
            RoundedArea {
                prefecture: Some("大分県".into()),
                municipality: Some("大分市".into())
            }
        );
        assert_eq!(municipality_label(&area), "大分県大分市");
        // 都道府県欄に住所全体が入っている
        let area = round_area(Some("東京都千代田区丸の内1-1-1 ○○マンション305"), None);
        assert_eq!(municipality_label(&area), "東京都千代田区");
        assert_eq!(prefecture_label(&area), "東京都");
    }

    #[test]
    fn reads_full_width_county_omitted_and_ward_less_addresses() {
        assert_eq!(
            municipality_label(&round_area(Some("北海道"), Some("当別町　太美町１２３"))),
            "北海道石狩郡当別町"
        );
        assert_eq!(
            municipality_label(&round_area(Some("静岡県"), Some("浜松市中央区元城町103-2"))),
            "静岡県浜松市"
        );
        assert_eq!(
            municipality_label(&round_area(Some("大阪府"), Some("大阪市北区梅田1丁目"))),
            "大阪府大阪市北区"
        );
    }

    #[test]
    fn never_guesses_an_unknown_city_or_prefecture() {
        let area = round_area(Some("大分県"), Some("架空町1-2-3"));
        assert_eq!(municipality_label(&area), "大分県（市区町村不明）");
        assert_eq!(
            round_area(None, Some("番地だけ 1-2-3")),
            RoundedArea::default()
        );
        // 府中市は東京都と広島県にあるので、都道府県が無ければ決めない
        assert_eq!(
            round_area(None, Some("府中市宮町1-1")),
            RoundedArea::default()
        );
        // 一つの都道府県にしか無い市なら都道府県も決まる
        assert_eq!(
            municipality_label(&round_area(None, Some("別府市北浜2-9-1"))),
            "大分県別府市"
        );
    }

    #[test]
    fn rounds_server_labels_idempotently() {
        assert_eq!(
            round_area_label(true, "大分県 / 大分市府内町3-10-1 201号室", None),
            "大分県大分市"
        );
        assert_eq!(round_area_label(true, "大分県大分市", None), "大分県大分市");
        assert_eq!(
            round_area_label(true, "大分県（市区町村不明）", None),
            "大分県（市区町村不明）"
        );
        assert_eq!(
            round_area_label(true, "大分県 / 市区町村不明", None),
            "大分県（市区町村不明）"
        );
        assert_eq!(
            round_area_label(true, "都道府県不明 / 別府市北浜2-9-1", None),
            "大分県別府市"
        );
        assert_eq!(
            round_area_label(false, "大分県大分市府内町3-10-1", None),
            "大分県"
        );
        assert_eq!(round_area_label(false, "不明", None), "不明");
        assert_eq!(round_area_label(true, "その他", None), "その他");
        assert_eq!(round_area_label(true, "  ", None), "不明");
    }
}
