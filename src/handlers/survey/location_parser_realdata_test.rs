//! Real Indeed CSV data regression test for parse_location.
//! Spawned from 2026-04-23 user bug report: 媒体分析タブで東京都データが京都府として表示される.
#![cfg(test)]

use super::location_parser::parse_location;

fn parse_and_assert(text: &str, expected_pref: &str) {
    let r = parse_location(text, None);
    assert_eq!(
        r.prefecture.as_deref(),
        Some(expected_pref),
        "parse_location({text:?}) expected pref={expected_pref:?}, got {:?} via {}",
        r.prefecture,
        r.method
    );
}

#[test]
fn indeed_csv_tokyo_samples_parse_to_tokyo() {
    // 2026-04-23 indeed-2026-04-23.csv から実データ抽出
    let tokyo_samples = [
        "東京都 小平市 学園西町",
        "東京都 あきる野市 草花",
        "東京都 千代田区 丸の内",
        "東京都 東村山市 秋津町",
        "東京都 八王子市 長沼町",
        "東京都 国分寺市 西恋ヶ窪",
        "東京都 立川市 高松町",
        "東京都 清瀬市 中清戸",
        "東京都 昭島市 つつじが丘",
        "東京都 西多摩郡 日の出町",
        "東京都 立川市 || この採用企業の、同種の別の求人を見る",
    ];
    for s in &tokyo_samples {
        parse_and_assert(s, "東京都");
    }
}

#[test]
fn kyoto_samples_parse_to_kyoto() {
    // 京都府が正しく京都府にパースされる（東京都に誤分類されない）
    let kyoto_samples = [
        "京都府 京都市 下京区",
        "京都府 宇治市",
        "京都府 京都市 中京区 烏丸通",
    ];
    for s in &kyoto_samples {
        parse_and_assert(s, "京都府");
    }
}

#[test]
fn tokyo_to_kanagawa_no_leak() {
    // 隣接県も正しくパース
    let samples = [
        ("神奈川県 横浜市", "神奈川県"),
        ("神奈川県 藤沢市 鵠沼", "神奈川県"),
    ];
    for (text, expected) in &samples {
        parse_and_assert(text, expected);
    }
}

#[test]
fn no_false_positive_when_text_has_no_prefecture() {
    // 「京都」を含む東京の地名・「東京」を含む他地域はない想定だが念のため
    let r = parse_location("横浜駅", None);
    // 駅名マッチで神奈川県になるはず
    assert_eq!(r.prefecture.as_deref(), Some("神奈川県"));
}

#[test]
fn tokyo_tachikawa_takamatsu_not_kagawa() {
    // 2026-04-24 バグ再現: 「東京都 立川市 高松駅」が station_map の
    // 「高松駅 → 香川県高松市」に引っ張られて香川県に誤分類されていた。
    // 先頭の「東京都」が優先されるべき。
    let samples = [
        "東京都 立川市 高松駅",
        "東京都 立川市 高松町",
        "東京都 立川市 高松町 駅前徒歩5分",
    ];
    for s in &samples {
        let r = parse_location(s, None);
        assert_eq!(
            r.prefecture.as_deref(),
            Some("東京都"),
            "{:?} should resolve to 東京都 not 香川県 (got pref={:?} via {})",
            s,
            r.prefecture,
            r.method
        );
    }
}

#[test]
fn takamatsu_station_still_works_when_no_tokyo_context() {
    // コンテキストが無ければ駅名マッチで香川県高松市に解決される（本来の用途）
    let r = parse_location("高松駅", None);
    assert_eq!(r.prefecture.as_deref(), Some("香川県"));
}

// ========================================================================
// MECE 監査: 47 都道府県 × 主要駅名 / city alias / 共有区名 の cross-check
// 2026-04-24 ユーザー要求「誤変換について徹底的にMECEに対応」対応
// ========================================================================

/// 47 都道府県フル名
const ALL_PREFS: [&str; 47] = [
    "北海道",
    "青森県",
    "岩手県",
    "宮城県",
    "秋田県",
    "山形県",
    "福島県",
    "茨城県",
    "栃木県",
    "群馬県",
    "埼玉県",
    "千葉県",
    "東京都",
    "神奈川県",
    "新潟県",
    "富山県",
    "石川県",
    "福井県",
    "山梨県",
    "長野県",
    "岐阜県",
    "静岡県",
    "愛知県",
    "三重県",
    "滋賀県",
    "京都府",
    "大阪府",
    "兵庫県",
    "奈良県",
    "和歌山県",
    "鳥取県",
    "島根県",
    "岡山県",
    "広島県",
    "山口県",
    "徳島県",
    "香川県",
    "愛媛県",
    "高知県",
    "福岡県",
    "佐賀県",
    "長崎県",
    "熊本県",
    "大分県",
    "宮崎県",
    "鹿児島県",
    "沖縄県",
];

/// station_map 所収の主要駅（他都道府県にも地名として存在しうるもの中心）
const MAJOR_STATIONS: [&str; 21] = [
    "新宿駅",
    "渋谷駅",
    "東京駅",
    "品川駅",
    "上野駅",
    "池袋駅",
    "横浜駅",
    "大阪駅",
    "梅田駅",
    "名古屋駅",
    "札幌駅",
    "仙台駅",
    "福岡駅",
    "高松駅",
    "松山駅",
    "岡山駅",
    "広島駅",
    "鹿児島中央駅",
    "那覇駅",
    "京都駅",
    "神戸駅",
];

/// 政令指定都市の略称（city_alias）
const CITY_ALIASES: [&str; 17] = [
    "名古屋",
    "札幌",
    "仙台",
    "横浜",
    "川崎",
    "福岡",
    "広島",
    "神戸",
    "京都",
    "大阪",
    "熊本",
    "岡山",
    "北九州",
    "新潟",
    "静岡",
    "浜松",
    "さいたま",
];

/// 東京23区 の共有区名（他政令指定都市にも同名の区がある）
const SHARED_WARDS: [&str; 6] = ["北区", "中央区", "港区", "南区", "東区", "西区"];

#[test]
fn mece_prefecture_prefix_beats_station_name() {
    // 各都道府県 + 主要駅名の組み合わせで、先頭の都道府県が必ず勝つこと
    let mut failures: Vec<String> = Vec::new();
    for pref in ALL_PREFS {
        for station in MAJOR_STATIONS {
            let text = format!("{} {}", pref, station);
            let r = parse_location(&text, None);
            if r.prefecture.as_deref() != Some(pref) {
                failures.push(format!(
                    "{:?} expected {:?} got {:?} via {}",
                    text, pref, r.prefecture, r.method
                ));
            }
        }
    }
    assert!(
        failures.is_empty(),
        "MECE 監査: 47pref × {}駅 = {} 組合せで、先頭都道府県が駅名マッチに負けてはいけない\nFailures ({}):\n{}",
        MAJOR_STATIONS.len(),
        ALL_PREFS.len() * MAJOR_STATIONS.len(),
        failures.len(),
        failures.join("\n")
    );
}

#[test]
fn mece_prefecture_prefix_beats_city_alias() {
    // 各都道府県 + city alias の組合せで、先頭の都道府県が勝つこと
    let mut failures: Vec<String> = Vec::new();
    for pref in ALL_PREFS {
        for alias in CITY_ALIASES {
            let text = format!("{} ○○町 {}", pref, alias);
            let r = parse_location(&text, None);
            if r.prefecture.as_deref() != Some(pref) {
                failures.push(format!(
                    "{:?} expected {:?} got {:?} via {}",
                    text, pref, r.prefecture, r.method
                ));
            }
        }
    }
    assert!(
        failures.is_empty(),
        "MECE 監査: 47pref × {}alias 全組合せで都道府県が勝つべき\nFailures:\n{}",
        CITY_ALIASES.len(),
        failures.join("\n")
    );
}

#[test]
fn mece_prefecture_prefix_beats_shared_ward() {
    // 東京以外の 46 都道府県 + 共有区名 で、先頭の都道府県が勝つ
    // （東京都は当然 23区として OK）
    let mut failures: Vec<String> = Vec::new();
    for pref in ALL_PREFS {
        if pref == "東京都" {
            continue;
        }
        for ward in SHARED_WARDS {
            let text = format!("{} ○○市 {}", pref, ward);
            let r = parse_location(&text, None);
            // 都道府県が合えば OK、None でも許容（本来マッチしないケース）
            let got = r.prefecture.as_deref();
            if got != Some(pref) && got.is_some() {
                failures.push(format!(
                    "{:?} expected {:?} got {:?} via {}",
                    text, pref, r.prefecture, r.method
                ));
            }
        }
    }
    assert!(
        failures.is_empty(),
        "MECE 監査: 46pref × {}ward で都道府県が負けないこと\nFailures:\n{}",
        SHARED_WARDS.len(),
        failures.join("\n")
    );
}
