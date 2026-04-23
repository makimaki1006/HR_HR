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
