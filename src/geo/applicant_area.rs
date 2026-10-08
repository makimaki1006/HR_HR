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
pub const AREA_OTHER: &str = "その他";
/// 応募がこの件数未満の地域は「その他」にまとめる (画面側の MINIMUM_AREA_COUNT と同じ値)。
pub const MINIMUM_AREA_COUNT: u64 = 3;

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

/// マスタにある市区町村名か (郡名を省いた書き方・区を省いた市の書き方を含む)。応募理由の文から、
/// 市区町村名の後ろに続く町名を見つけるのに使う。
pub fn is_municipality_name(text: &str) -> bool {
    let key = normalize(text);
    prefectures().iter().any(|prefecture| {
        prefecture
            .cities
            .iter()
            .any(|(existing, _)| *existing == key)
    })
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

/// 「その他」「不明」以外の、名前のある地域ラベルか。
pub fn is_named_area(label: &str) -> bool {
    label != AREA_UNKNOWN && label != AREA_OTHER
}

/// 地域ごとの件数で、MINIMUM_AREA_COUNT 未満の地域を「その他」にまとめる。合計は変えない。
/// 並びは元の順のまま (「その他」は最初にまとめた地域の位置)。
pub fn merge_small_areas(counts: Vec<(String, u64)>) -> Vec<(String, u64)> {
    let mut merged: Vec<(String, u64)> = Vec::new();
    for (label, count) in counts {
        let target = if is_named_area(&label) && count < MINIMUM_AREA_COUNT {
            AREA_OTHER.to_owned()
        } else {
            label
        };
        match merged.iter_mut().find(|(existing, _)| *existing == target) {
            Some((_, total)) => *total += count,
            None => merged.push((target, count)),
        }
    }
    merged
}

/// 性別 × 年代 × 地域の組み合わせの 1 セル。
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct JointCell {
    pub gender: String,
    pub age: String,
    pub prefecture: String,
    pub municipality: String,
    pub count: u64,
}

fn merge_cells(cells: Vec<JointCell>) -> Vec<JointCell> {
    let mut index: HashMap<(String, String, String, String), usize> = HashMap::new();
    let mut merged: Vec<JointCell> = Vec::new();
    for cell in cells {
        let key = (
            cell.gender.clone(),
            cell.age.clone(),
            cell.prefecture.clone(),
            cell.municipality.clone(),
        );
        match index.get(&key) {
            Some(&at) => merged[at].count += cell.count,
            None => {
                index.insert(key, merged.len());
                merged.push(cell);
            }
        }
    }
    merged
}

/// 地域を伏せる判定に使う、1 件 (または同じ属性の数件) の応募。
///
/// `group` は、この属性と一緒にブラウザへ送る区分 (掲載期間など)。組み合わせの件数は group ごとに数える:
/// 「掲載期間 X の男性・30代」が 1 件で、その人の市区町村まで分かると、その 1 件を特定できるため。
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct ApplicantKey {
    pub group: String,
    pub gender: String,
    pub age: String,
    pub prefecture: String,
    pub municipality: String,
    pub count: u64,
}

/// 各応募の地域を、送ってよい細かさまで伏せる。戻り値は入力と同じ順の (都道府県, 市区町村)。合計は変えない。
///
/// 1. 地域を都道府県 + 市区町村に丸める。市区町村が分かれば、都道府県はその市区町村から決め直す
///    (都道府県欄が空でも、市区町村欄から都道府県が分かることがあるため)。
/// 2. 求人内の応募が 3 件未満の都道府県・市区町村は「その他」にする。
/// 3. group × 性別 × 年代 × 地域が 3 件未満なら市区町村を「その他」にし、まだ 3 件未満なら都道府県も
///    「その他」にする。「女性・60代・由布市 = 1件」のように、組み合わせで 1 件の応募を特定できる地域は残さない。
///
/// 地域ごとの合計は、必ずこの戻り値から数え直して送る。元の地域で数えた合計を一緒に送ると、
/// 名前の出ている組み合わせを引き算して、伏せた応募の地域が分かってしまう。
pub fn protect_applicant_keys(keys: &[ApplicantKey]) -> Vec<(String, String)> {
    let rounded: Vec<(String, String)> = keys
        .iter()
        .map(|key| {
            let prefecture = round_area_label(false, &key.prefecture, None);
            let fallback = is_named_area(&prefecture).then_some(prefecture.as_str());
            let municipality = round_area_label(true, &key.municipality, fallback);
            let prefecture = if is_named_area(&municipality) {
                round_area_label(false, &municipality, None)
            } else {
                prefecture
            };
            (prefecture, municipality)
        })
        .collect();
    let mut prefecture_totals: HashMap<&str, u64> = HashMap::new();
    let mut municipality_totals: HashMap<&str, u64> = HashMap::new();
    for ((prefecture, municipality), key) in rounded.iter().zip(keys) {
        *prefecture_totals.entry(prefecture).or_default() += key.count;
        *municipality_totals.entry(municipality).or_default() += key.count;
    }
    let keep = |totals: &HashMap<&str, u64>, label: &str| {
        if is_named_area(label) && totals.get(label).copied().unwrap_or(0) < MINIMUM_AREA_COUNT {
            AREA_OTHER.to_owned()
        } else {
            label.to_owned()
        }
    };
    let mut areas: Vec<(String, String)> = rounded
        .iter()
        .map(|(prefecture, municipality)| {
            (
                keep(&prefecture_totals, prefecture),
                keep(&municipality_totals, municipality),
            )
        })
        .collect();
    for city_level in [true, false] {
        let mut sizes: HashMap<(&str, &str, &str, &str, &str), u64> = HashMap::new();
        for ((prefecture, municipality), key) in areas.iter().zip(keys) {
            *sizes
                .entry((&key.group, &key.gender, &key.age, prefecture, municipality))
                .or_default() += key.count;
        }
        let small: Vec<bool> = areas
            .iter()
            .zip(keys)
            .map(|((prefecture, municipality), key)| {
                let label = if city_level { municipality } else { prefecture };
                is_named_area(label)
                    && sizes[&(
                        key.group.as_str(),
                        key.gender.as_str(),
                        key.age.as_str(),
                        prefecture.as_str(),
                        municipality.as_str(),
                    )] < MINIMUM_AREA_COUNT
            })
            .collect();
        for ((prefecture, municipality), small) in areas.iter_mut().zip(small) {
            if small {
                if city_level {
                    *municipality = AREA_OTHER.to_owned();
                } else {
                    *prefecture = AREA_OTHER.to_owned();
                }
            }
        }
    }
    areas
}

/// 組み合わせの集計を、ブラウザに送る前に丸めてまとめる。合計は変えない。
/// 規則は protect_applicant_keys (区分 group は無し)。
pub fn protect_joint_cells(cells: Vec<JointCell>) -> Vec<JointCell> {
    let keys: Vec<ApplicantKey> = cells
        .iter()
        .map(|cell| ApplicantKey {
            group: String::new(),
            gender: cell.gender.clone(),
            age: cell.age.clone(),
            prefecture: cell.prefecture.clone(),
            municipality: cell.municipality.clone(),
            count: cell.count,
        })
        .collect();
    let areas = protect_applicant_keys(&keys);
    merge_cells(
        cells
            .into_iter()
            .zip(areas)
            .map(|(cell, (prefecture, municipality))| JointCell {
                prefecture,
                municipality,
                ..cell
            })
            .collect(),
    )
}

/// 伏せた後の組み合わせから、都道府県・市区町村ごとの合計を数え直す (送る合計はこれだけにする)。
pub fn area_totals(cells: &[JointCell], municipality: bool) -> Vec<(String, u64)> {
    let mut totals: Vec<(String, u64)> = Vec::new();
    for cell in cells {
        let label = if municipality {
            &cell.municipality
        } else {
            &cell.prefecture
        };
        match totals.iter_mut().find(|(existing, _)| existing == label) {
            Some((_, total)) => *total += cell.count,
            None => totals.push((label.clone(), cell.count)),
        }
    }
    totals
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

    fn cell(
        gender: &str,
        age: &str,
        prefecture: &str,
        municipality: &str,
        count: u64,
    ) -> JointCell {
        JointCell {
            gender: gender.into(),
            age: age.into(),
            prefecture: prefecture.into(),
            municipality: municipality.into(),
            count,
        }
    }

    #[test]
    fn small_areas_are_merged_into_other() {
        let merged = merge_small_areas(vec![
            ("大分県大分市".into(), 3),
            ("大分県由布市".into(), 1),
            ("大分県別府市".into(), 2),
            ("不明".into(), 1),
        ]);
        assert_eq!(
            merged,
            vec![
                ("大分県大分市".to_owned(), 3),
                ("その他".to_owned(), 3),
                ("不明".to_owned(), 1)
            ]
        );
    }

    #[test]
    fn a_single_applicant_is_not_identified_by_gender_age_and_city() {
        // 由布市は合計 3 人 (地域としては残る) だが、「女性・60代・由布市」は 1 人。
        let cells = protect_joint_cells(vec![
            cell("女性", "60代", "大分県", "大分県 / 由布市湯布院町1234-5", 1),
            cell("男性", "30代", "大分県", "大分県 / 由布市挾間町", 2),
            cell("男性", "20代", "大分県", "大分県 / 大分市府内町", 3),
        ]);
        assert_eq!(
            cells,
            vec![
                cell("女性", "60代", "その他", "その他", 1),
                cell("男性", "30代", "その他", "その他", 2),
                cell("男性", "20代", "大分県", "大分県大分市", 3),
            ]
        );
        assert!(cells
            .iter()
            .all(|c| c.count >= MINIMUM_AREA_COUNT || !is_named_area(&c.municipality)));
        assert_eq!(cells.iter().map(|c| c.count).sum::<u64>(), 6);
    }

    #[test]
    fn small_cells_keep_the_prefecture_when_it_has_enough_people() {
        let cells = protect_joint_cells(vec![
            cell("女性", "20代", "大分県", "大分県大分市", 1),
            cell("女性", "20代", "大分県", "大分県別府市", 2),
            cell("男性", "40代", "大分県", "大分県大分市", 3),
        ]);
        // 女性・20代は大分県で 3 人になるので、都道府県は残す。市区町村は「その他」。
        assert_eq!(
            cells,
            vec![
                cell("女性", "20代", "大分県", "その他", 3),
                cell("男性", "40代", "大分県", "大分県大分市", 3),
            ]
        );
    }

    #[test]
    fn the_group_is_part_of_the_combination() {
        let key = |group: &str| ApplicantKey {
            group: group.into(),
            gender: "男性".into(),
            age: "20代".into(),
            prefecture: "大分県".into(),
            municipality: "大分県大分市".into(),
            count: 1,
        };
        // 3 in the job, but 2 + 1 per period: the city is hidden, and the prefecture too.
        let areas = protect_applicant_keys(&[key("A"), key("A"), key("B")]);
        assert_eq!(areas, vec![("その他".to_owned(), "その他".to_owned()); 3]);
        let areas = protect_applicant_keys(&[key("A"), key("A"), key("A")]);
        assert_eq!(
            areas,
            vec![("大分県".to_owned(), "大分県大分市".to_owned()); 3]
        );
    }

    #[test]
    fn area_totals_come_from_the_protected_cells() {
        let cells = protect_joint_cells(vec![
            cell("女性", "60代", "大分県", "大分県由布市", 1),
            cell("男性", "30代", "大分県", "大分県由布市", 2),
            cell("男性", "20代", "大分県", "大分県大分市", 3),
        ]);
        assert_eq!(
            area_totals(&cells, true),
            vec![("その他".to_owned(), 3), ("大分県大分市".to_owned(), 3)]
        );
    }

    #[test]
    fn the_prefecture_is_taken_from_the_rounded_city() {
        let cells = protect_joint_cells(vec![
            cell("女性", "20代", "不明", "都道府県不明 / 別府市北浜2-9-1", 3),
            cell("女性", "20代", "大分県", "大分県 / 別府市", 1),
        ]);
        assert_eq!(
            cells,
            vec![cell("女性", "20代", "大分県", "大分県別府市", 4)]
        );
    }
}
