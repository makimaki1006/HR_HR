//! 住所パーサー（GAS LocationParser.js移植）
//! 日本語住所・駅名テキストを都道府県・市区町村に正規化

use serde::Serialize;
use std::collections::HashMap;
use std::sync::OnceLock;

// ======== 型定義 ========

#[derive(Debug, Clone, Serialize)]
pub struct ParsedLocation {
    pub original_text: String,
    pub prefecture: Option<String>,
    pub municipality: Option<String>,
    pub region_block: Option<String>,
    pub city_type: Option<String>,
    pub confidence: f64,
    pub method: String,
}

// ======== 都道府県リスト ========

const PREFECTURES: [&str; 47] = [
    "北海道", "青森県", "岩手県", "宮城県", "秋田県", "山形県", "福島県",
    "茨城県", "栃木県", "群馬県", "埼玉県", "千葉県", "東京都", "神奈川県",
    "新潟県", "富山県", "石川県", "福井県", "山梨県", "長野県", "岐阜県",
    "静岡県", "愛知県", "三重県", "滋賀県", "京都府", "大阪府", "兵庫県",
    "奈良県", "和歌山県", "鳥取県", "島根県", "岡山県", "広島県", "山口県",
    "徳島県", "香川県", "愛媛県", "高知県", "福岡県", "佐賀県", "長崎県",
    "熊本県", "大分県", "宮崎県", "鹿児島県", "沖縄県",
];

// ======== 地域ブロック ========

fn prefecture_to_region(pref: &str) -> &'static str {
    match pref {
        "北海道" | "青森県" | "岩手県" | "宮城県" | "秋田県" | "山形県" | "福島県"
            => "北海道・東北",
        "茨城県" | "栃木県" | "群馬県" | "埼玉県" | "千葉県" | "東京都" | "神奈川県"
            => "関東",
        "新潟県" | "富山県" | "石川県" | "福井県" | "山梨県" | "長野県" | "岐阜県" | "静岡県" | "愛知県"
            => "中部",
        "三重県" | "滋賀県" | "京都府" | "大阪府" | "兵庫県" | "奈良県" | "和歌山県"
            => "近畿",
        "鳥取県" | "島根県" | "岡山県" | "広島県" | "山口県"
            => "中国",
        "徳島県" | "香川県" | "愛媛県" | "高知県"
            => "四国",
        "福岡県" | "佐賀県" | "長崎県" | "熊本県" | "大分県" | "宮崎県" | "鹿児島県" | "沖縄県"
            => "九州・沖縄",
        _ => "不明",
    }
}

// ======== 静的データ（遅延初期化） ========

struct StationInfo {
    city: &'static str,
    prefecture: &'static str,
}

fn station_map() -> &'static HashMap<&'static str, StationInfo> {
    static MAP: OnceLock<HashMap<&str, StationInfo>> = OnceLock::new();
    MAP.get_or_init(|| {
        let mut m = HashMap::new();
        // 北海道
        m.insert("札幌駅", StationInfo { city: "札幌市中央区", prefecture: "北海道" });
        m.insert("函館駅", StationInfo { city: "函館市", prefecture: "北海道" });
        m.insert("旭川駅", StationInfo { city: "旭川市", prefecture: "北海道" });
        m.insert("帯広駅", StationInfo { city: "帯広市", prefecture: "北海道" });
        m.insert("釧路駅", StationInfo { city: "釧路市", prefecture: "北海道" });
        m.insert("小樽駅", StationInfo { city: "小樽市", prefecture: "北海道" });
        // 東北
        m.insert("仙台駅", StationInfo { city: "仙台市青葉区", prefecture: "宮城県" });
        m.insert("盛岡駅", StationInfo { city: "盛岡市", prefecture: "岩手県" });
        m.insert("青森駅", StationInfo { city: "青森市", prefecture: "青森県" });
        m.insert("秋田駅", StationInfo { city: "秋田市", prefecture: "秋田県" });
        m.insert("山形駅", StationInfo { city: "山形市", prefecture: "山形県" });
        m.insert("福島駅", StationInfo { city: "福島市", prefecture: "福島県" });
        m.insert("郡山駅", StationInfo { city: "郡山市", prefecture: "福島県" });
        // 関東（埼玉）
        m.insert("大宮駅", StationInfo { city: "さいたま市大宮区", prefecture: "埼玉県" });
        m.insert("浦和駅", StationInfo { city: "さいたま市浦和区", prefecture: "埼玉県" });
        m.insert("川口駅", StationInfo { city: "川口市", prefecture: "埼玉県" });
        m.insert("川越駅", StationInfo { city: "川越市", prefecture: "埼玉県" });
        m.insert("所沢駅", StationInfo { city: "所沢市", prefecture: "埼玉県" });
        m.insert("越谷駅", StationInfo { city: "越谷市", prefecture: "埼玉県" });
        m.insert("草加駅", StationInfo { city: "草加市", prefecture: "埼玉県" });
        m.insert("春日部駅", StationInfo { city: "春日部市", prefecture: "埼玉県" });
        m.insert("熊谷駅", StationInfo { city: "熊谷市", prefecture: "埼玉県" });
        // 関東（千葉）
        m.insert("千葉駅", StationInfo { city: "千葉市中央区", prefecture: "千葉県" });
        m.insert("船橋駅", StationInfo { city: "船橋市", prefecture: "千葉県" });
        m.insert("松戸駅", StationInfo { city: "松戸市", prefecture: "千葉県" });
        m.insert("柏駅", StationInfo { city: "柏市", prefecture: "千葉県" });
        m.insert("市川駅", StationInfo { city: "市川市", prefecture: "千葉県" });
        m.insert("津田沼駅", StationInfo { city: "習志野市", prefecture: "千葉県" });
        m.insert("海浜幕張駅", StationInfo { city: "千葉市美浜区", prefecture: "千葉県" });
        // 関東（東京）
        m.insert("東京駅", StationInfo { city: "千代田区", prefecture: "東京都" });
        m.insert("新宿駅", StationInfo { city: "新宿区", prefecture: "東京都" });
        m.insert("渋谷駅", StationInfo { city: "渋谷区", prefecture: "東京都" });
        m.insert("池袋駅", StationInfo { city: "豊島区", prefecture: "東京都" });
        m.insert("品川駅", StationInfo { city: "港区", prefecture: "東京都" });
        m.insert("上野駅", StationInfo { city: "台東区", prefecture: "東京都" });
        m.insert("秋葉原駅", StationInfo { city: "千代田区", prefecture: "東京都" });
        m.insert("六本木駅", StationInfo { city: "港区", prefecture: "東京都" });
        m.insert("銀座駅", StationInfo { city: "中央区", prefecture: "東京都" });
        m.insert("立川駅", StationInfo { city: "立川市", prefecture: "東京都" });
        m.insert("八王子駅", StationInfo { city: "八王子市", prefecture: "東京都" });
        m.insert("町田駅", StationInfo { city: "町田市", prefecture: "東京都" });
        m.insert("吉祥寺駅", StationInfo { city: "武蔵野市", prefecture: "東京都" });
        m.insert("北千住駅", StationInfo { city: "足立区", prefecture: "東京都" });
        m.insert("錦糸町駅", StationInfo { city: "墨田区", prefecture: "東京都" });
        m.insert("蒲田駅", StationInfo { city: "大田区", prefecture: "東京都" });
        m.insert("恵比寿駅", StationInfo { city: "渋谷区", prefecture: "東京都" });
        m.insert("中野駅", StationInfo { city: "中野区", prefecture: "東京都" });
        m.insert("赤羽駅", StationInfo { city: "北区", prefecture: "東京都" });
        // 関東（神奈川）
        m.insert("横浜駅", StationInfo { city: "横浜市西区", prefecture: "神奈川県" });
        m.insert("川崎駅", StationInfo { city: "川崎市川崎区", prefecture: "神奈川県" });
        m.insert("武蔵小杉駅", StationInfo { city: "川崎市中原区", prefecture: "神奈川県" });
        m.insert("藤沢駅", StationInfo { city: "藤沢市", prefecture: "神奈川県" });
        m.insert("小田原駅", StationInfo { city: "小田原市", prefecture: "神奈川県" });
        m.insert("海老名駅", StationInfo { city: "海老名市", prefecture: "神奈川県" });
        // 中部
        m.insert("名古屋駅", StationInfo { city: "名古屋市中村区", prefecture: "愛知県" });
        m.insert("栄駅", StationInfo { city: "名古屋市中区", prefecture: "愛知県" });
        m.insert("静岡駅", StationInfo { city: "静岡市葵区", prefecture: "静岡県" });
        m.insert("浜松駅", StationInfo { city: "浜松市中央区", prefecture: "静岡県" });
        m.insert("新潟駅", StationInfo { city: "新潟市中央区", prefecture: "新潟県" });
        m.insert("長野駅", StationInfo { city: "長野市", prefecture: "長野県" });
        m.insert("金沢駅", StationInfo { city: "金沢市", prefecture: "石川県" });
        m.insert("富山駅", StationInfo { city: "富山市", prefecture: "富山県" });
        m.insert("岐阜駅", StationInfo { city: "岐阜市", prefecture: "岐阜県" });
        m.insert("甲府駅", StationInfo { city: "甲府市", prefecture: "山梨県" });
        // 近畿
        m.insert("大阪駅", StationInfo { city: "大阪市北区", prefecture: "大阪府" });
        m.insert("梅田駅", StationInfo { city: "大阪市北区", prefecture: "大阪府" });
        m.insert("難波駅", StationInfo { city: "大阪市中央区", prefecture: "大阪府" });
        m.insert("天王寺駅", StationInfo { city: "大阪市天王寺区", prefecture: "大阪府" });
        m.insert("京都駅", StationInfo { city: "京都市下京区", prefecture: "京都府" });
        m.insert("三宮駅", StationInfo { city: "神戸市中央区", prefecture: "兵庫県" });
        m.insert("神戸駅", StationInfo { city: "神戸市中央区", prefecture: "兵庫県" });
        m.insert("姫路駅", StationInfo { city: "姫路市", prefecture: "兵庫県" });
        m.insert("奈良駅", StationInfo { city: "奈良市", prefecture: "奈良県" });
        // 中国・四国・九州
        m.insert("広島駅", StationInfo { city: "広島市南区", prefecture: "広島県" });
        m.insert("岡山駅", StationInfo { city: "岡山市北区", prefecture: "岡山県" });
        m.insert("高松駅", StationInfo { city: "高松市", prefecture: "香川県" });
        m.insert("松山駅", StationInfo { city: "松山市", prefecture: "愛媛県" });
        m.insert("博多駅", StationInfo { city: "福岡市博多区", prefecture: "福岡県" });
        m.insert("天神駅", StationInfo { city: "福岡市中央区", prefecture: "福岡県" });
        m.insert("小倉駅", StationInfo { city: "北九州市小倉北区", prefecture: "福岡県" });
        m.insert("熊本駅", StationInfo { city: "熊本市西区", prefecture: "熊本県" });
        m.insert("鹿児島中央駅", StationInfo { city: "鹿児島市", prefecture: "鹿児島県" });
        m.insert("長崎駅", StationInfo { city: "長崎市", prefecture: "長崎県" });
        m.insert("大分駅", StationInfo { city: "大分市", prefecture: "大分県" });
        m.insert("那覇駅", StationInfo { city: "那覇市", prefecture: "沖縄県" });
        m
    })
}

// 東京23区
const TOKYO_23_WARDS: [&str; 23] = [
    "千代田区", "中央区", "港区", "新宿区", "文京区", "台東区", "墨田区", "江東区",
    "品川区", "目黒区", "大田区", "世田谷区", "渋谷区", "中野区", "杉並区", "豊島区",
    "北区", "荒川区", "板橋区", "練馬区", "足立区", "葛飾区", "江戸川区",
];

// 政令指定都市→都道府県
fn designated_city_pref(city: &str) -> Option<&'static str> {
    match city {
        "札幌市" => Some("北海道"), "仙台市" => Some("宮城県"),
        "さいたま市" => Some("埼玉県"), "千葉市" => Some("千葉県"),
        "横浜市" => Some("神奈川県"), "川崎市" => Some("神奈川県"),
        "相模原市" => Some("神奈川県"), "新潟市" => Some("新潟県"),
        "静岡市" => Some("静岡県"), "浜松市" => Some("静岡県"),
        "名古屋市" => Some("愛知県"), "京都市" => Some("京都府"),
        "大阪市" => Some("大阪府"), "堺市" => Some("大阪府"),
        "神戸市" => Some("兵庫県"), "岡山市" => Some("岡山県"),
        "広島市" => Some("広島県"), "北九州市" => Some("福岡県"),
        "福岡市" => Some("福岡県"), "熊本市" => Some("熊本県"),
        _ => None,
    }
}

// 政令指定都市の略称
#[allow(dead_code)]
fn resolve_city_alias(name: &str) -> Option<&'static str> {
    match name {
        "札幌" => Some("札幌市"), "仙台" => Some("仙台市"),
        "さいたま" => Some("さいたま市"), "横浜" => Some("横浜市"),
        "川崎" => Some("川崎市"), "名古屋" => Some("名古屋市"),
        "京都" => Some("京都市"), "大阪" => Some("大阪市"),
        "神戸" => Some("神戸市"), "広島" => Some("広島市"),
        "福岡" => Some("福岡市"), "熊本" => Some("熊本市"),
        "岡山" => Some("岡山市"), "北九州" => Some("北九州市"),
        "新潟" => Some("新潟市"), "静岡" => Some("静岡市"),
        "浜松" => Some("浜松市"), "堺" => Some("堺市"),
        "千葉" => Some("千葉市"), "相模原" => Some("相模原市"),
        _ => None,
    }
}

// ======== メインパース関数 ========

/// 住所テキストを解析して構造化データに変換
pub fn parse_location(text: &str, context_pref: Option<&str>) -> ParsedLocation {
    if text.is_empty() {
        return empty_location();
    }

    let text = text.trim();

    // 1. 曖昧表現チェック（リモート、首都圏、都内等）
    if let Some(r) = try_ambiguous(text) { return r; }

    // 2. 駅名マッチ
    if let Some(r) = try_station(text) { return r; }

    // 3. 都道府県直接マッチ
    let prefecture = extract_prefecture(text);

    // 4. 東京23区マッチ
    if let Some(r) = try_tokyo_ward(text, prefecture.as_deref()) { return r; }

    // 5. 政令指定都市マッチ
    if let Some(r) = try_designated_city(text, prefecture.as_deref()) { return r; }

    // 6. 市区町村パターンマッチ
    if let Some(r) = try_municipality_pattern(text, prefecture.as_deref()) { return r; }

    // 7. 都道府県のみ
    if let Some(pref) = &prefecture {
        return ParsedLocation {
            original_text: text.to_string(),
            prefecture: Some(pref.clone()),
            municipality: None,
            region_block: Some(prefecture_to_region(pref).to_string()),
            city_type: Some("都道府県".to_string()),
            confidence: 0.6,
            method: "prefecture_only".to_string(),
        };
    }

    // 8. コンテキスト都道府県フォールバック
    if let Some(ctx) = context_pref {
        return ParsedLocation {
            original_text: text.to_string(),
            prefecture: Some(ctx.to_string()),
            municipality: None,
            region_block: Some(prefecture_to_region(ctx).to_string()),
            city_type: Some("コンテキスト推定".to_string()),
            confidence: 0.3,
            method: "context_fallback".to_string(),
        };
    }

    empty_location_with_text(text)
}

fn empty_location() -> ParsedLocation {
    ParsedLocation {
        original_text: String::new(),
        prefecture: None, municipality: None,
        region_block: None, city_type: None,
        confidence: 0.0, method: "empty".to_string(),
    }
}

fn empty_location_with_text(text: &str) -> ParsedLocation {
    ParsedLocation {
        original_text: text.to_string(),
        prefecture: None, municipality: None,
        region_block: None, city_type: None,
        confidence: 0.0, method: "unmatched".to_string(),
    }
}

// ======== パース関数群 ========

/// 曖昧表現チェック
fn try_ambiguous(text: &str) -> Option<ParsedLocation> {
    let checks: &[(&str, Option<&str>, Option<&str>, &str)] = &[
        ("フルリモート", None, Some("リモート"), "フルリモート"),
        ("完全在宅", None, Some("リモート"), "フルリモート"),
        ("在宅勤務", None, Some("リモート"), "リモート"),
        ("テレワーク", None, Some("リモート"), "リモート"),
        ("リモート", None, Some("リモート"), "リモート"),
        ("在宅", None, Some("リモート"), "リモート"),
        ("23区内", Some("東京都"), Some("関東"), "東京23区"),
        ("23区", Some("東京都"), Some("関東"), "東京23区"),
        ("東京都内", Some("東京都"), Some("関東"), "東京都内"),
        ("都内", Some("東京都"), Some("関東"), "東京都内"),
        ("道内", Some("北海道"), Some("北海道・東北"), "北海道内"),
        ("首都圏", None, Some("関東"), "首都圏"),
        ("関東圏", None, Some("関東"), "関東圏"),
        ("関西圏", None, Some("近畿"), "関西圏"),
        ("近畿圏", None, Some("近畿"), "近畿圏"),
        ("東海エリア", None, Some("中部"), "東海"),
        ("東海", None, Some("中部"), "東海"),
        ("全国", None, Some("全国"), "全国"),
        ("各地", None, Some("全国"), "全国"),
    ];

    for &(keyword, pref, region, city_type) in checks {
        if text.contains(keyword) {
            return Some(ParsedLocation {
                original_text: text.to_string(),
                prefecture: pref.map(|s| s.to_string()),
                municipality: None,
                region_block: region.map(|s| s.to_string()),
                city_type: Some(city_type.to_string()),
                confidence: 0.7,
                method: "ambiguous".to_string(),
            });
        }
    }
    None
}

/// 駅名マッチ
fn try_station(text: &str) -> Option<ParsedLocation> {
    // 「XX駅」パターンを検出
    let station_pos = text.find('駅')?;
    // 駅の前の文字列から駅名を抽出（スペースや数字で区切る）
    let before = &text[..station_pos + '駅'.len_utf8()];
    // 末尾から「駅」を含む最長の駅名を探す
    let map = station_map();
    for len in (2..=before.chars().count()).rev() {
        let start = before.chars().count().saturating_sub(len);
        let candidate: String = before.chars().skip(start).collect();
        if let Some(info) = map.get(candidate.as_str()) {
            return Some(ParsedLocation {
                original_text: text.to_string(),
                prefecture: Some(info.prefecture.to_string()),
                municipality: Some(info.city.to_string()),
                region_block: Some(prefecture_to_region(info.prefecture).to_string()),
                city_type: Some("駅名マッチ".to_string()),
                confidence: 0.9,
                method: "station".to_string(),
            });
        }
    }
    None
}

/// 都道府県抽出（東京/京都問題対策付き）
fn extract_prefecture(text: &str) -> Option<String> {
    // 「東京」が含まれる場合は先に東京都チェック（京都との衝突防止）
    if text.contains("東京") {
        return Some("東京都".to_string());
    }
    // 長い順にマッチ（和歌山県→山県の誤マッチ防止）
    let mut sorted = PREFECTURES.to_vec();
    sorted.sort_by(|a, b| b.chars().count().cmp(&a.chars().count()));
    for pref in &sorted {
        if text.contains(pref) {
            return Some(pref.to_string());
        }
    }
    // 略称マッチ
    if text.contains("北海道") { return Some("北海道".to_string()); }
    if text.contains("大阪") { return Some("大阪府".to_string()); }
    if text.contains("京都") && !text.contains("東京") {
        return Some("京都府".to_string());
    }
    None
}

/// 東京23区マッチ
fn try_tokyo_ward(text: &str, pref: Option<&str>) -> Option<ParsedLocation> {
    for ward in &TOKYO_23_WARDS {
        if text.contains(ward) {
            // 共有区名（北区、中央区等）は東京都コンテキストの場合のみ
            let is_shared = matches!(*ward, "北区" | "中央区" | "港区");
            if is_shared && pref != Some("東京都") && !text.contains("東京") {
                continue;
            }
            return Some(ParsedLocation {
                original_text: text.to_string(),
                prefecture: Some("東京都".to_string()),
                municipality: Some(ward.to_string()),
                region_block: Some("関東".to_string()),
                city_type: Some("東京23区".to_string()),
                confidence: 0.9,
                method: "tokyo_ward".to_string(),
            });
        }
    }
    None
}

/// 政令指定都市マッチ
fn try_designated_city(text: &str, _pref: Option<&str>) -> Option<ParsedLocation> {
    // 正式名称でマッチ
    let designated_cities = [
        "札幌市", "仙台市", "さいたま市", "千葉市", "横浜市", "川崎市", "相模原市",
        "新潟市", "静岡市", "浜松市", "名古屋市", "京都市", "大阪市", "堺市",
        "神戸市", "岡山市", "広島市", "北九州市", "福岡市", "熊本市",
    ];

    for city in &designated_cities {
        if text.contains(city) {
            let city_pref = designated_city_pref(city)?;
            return Some(ParsedLocation {
                original_text: text.to_string(),
                prefecture: Some(city_pref.to_string()),
                municipality: Some(city.to_string()),
                region_block: Some(prefecture_to_region(city_pref).to_string()),
                city_type: Some("政令指定都市".to_string()),
                confidence: 0.85,
                method: "designated_city".to_string(),
            });
        }
    }

    // 略称マッチ（テキスト中に都市略称が含まれるか直接チェック）
    let city_aliases = [
        ("名古屋", "名古屋市"), ("札幌", "札幌市"), ("仙台", "仙台市"),
        ("横浜", "横浜市"), ("川崎", "川崎市"), ("福岡", "福岡市"),
        ("広島", "広島市"), ("神戸", "神戸市"), ("京都", "京都市"),
        ("大阪", "大阪市"), ("熊本", "熊本市"), ("岡山", "岡山市"),
        ("北九州", "北九州市"), ("新潟", "新潟市"), ("静岡", "静岡市"),
        ("浜松", "浜松市"), ("さいたま", "さいたま市"),
    ];
    for (alias, city) in &city_aliases {
        if text.contains(alias) {
            if let Some(city_pref) = designated_city_pref(city) {
                return Some(ParsedLocation {
                    original_text: text.to_string(),
                    prefecture: Some(city_pref.to_string()),
                    municipality: Some(city.to_string()),
                    region_block: Some(prefecture_to_region(city_pref).to_string()),
                    city_type: Some("政令指定都市".to_string()),
                    confidence: 0.7,
                    method: "city_alias".to_string(),
                });
            }
        }
    }
    None
}

/// 市区町村パターンマッチ（XX市、XX区、XX町、XX村）
fn try_municipality_pattern(text: &str, pref: Option<&str>) -> Option<ParsedLocation> {
    // 都道府県名を除いたテキストで探す
    let clean = if let Some(p) = pref {
        text.replace(p, "")
    } else {
        text.to_string()
    };

    // 市 > 区 > 町 > 村 の優先順
    for suffix in &["市", "区", "町", "村"] {
        if let Some(pos) = clean.find(suffix) {
            // suffixの前の文字列（最大8文字）を市区町村名として抽出
            let before = &clean[..pos];
            let chars: Vec<char> = before.chars().collect();
            // 末尾から最大8文字分のひらがな・カタカナ・漢字を取得
            let start = chars.len().saturating_sub(8);
            let name_chars: Vec<char> = chars[start..].iter()
                .rev()
                .take_while(|c| !c.is_whitespace() && **c != '・' && **c != '/')
                .copied()
                .collect::<Vec<_>>().into_iter().rev().collect();
            if !name_chars.is_empty() {
                let muni_name = format!("{}{}", name_chars.iter().collect::<String>(), suffix);
                let prefecture = pref.map(|s| s.to_string());
                let region = pref.map(|p| prefecture_to_region(p).to_string());
                return Some(ParsedLocation {
                    original_text: text.to_string(),
                    prefecture,
                    municipality: Some(muni_name),
                    region_block: region,
                    city_type: Some(format!("{}マッチ", suffix)),
                    confidence: 0.6,
                    method: "pattern".to_string(),
                });
            }
        }
    }
    None
}

// ======== テスト ========

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_station() {
        let r = parse_location("新宿駅から徒歩5分", None);
        assert_eq!(r.prefecture.as_deref(), Some("東京都"));
        assert_eq!(r.municipality.as_deref(), Some("新宿区"));
        assert_eq!(r.method, "station");
    }

    #[test]
    fn test_tokyo_ward() {
        let r = parse_location("東京都渋谷区", None);
        assert_eq!(r.prefecture.as_deref(), Some("東京都"));
        assert_eq!(r.municipality.as_deref(), Some("渋谷区"));
    }

    #[test]
    fn test_designated_city() {
        let r = parse_location("横浜市西区", None);
        assert_eq!(r.prefecture.as_deref(), Some("神奈川県"));
        assert!(r.municipality.as_deref().unwrap().starts_with("横浜市"));
    }

    #[test]
    fn test_ambiguous_remote() {
        let r = parse_location("フルリモート", None);
        assert_eq!(r.city_type.as_deref(), Some("フルリモート"));
        assert_eq!(r.region_block.as_deref(), Some("リモート"));
    }

    #[test]
    fn test_ambiguous_23ku() {
        let r = parse_location("23区内", None);
        assert_eq!(r.prefecture.as_deref(), Some("東京都"));
        assert_eq!(r.city_type.as_deref(), Some("東京23区"));
    }

    #[test]
    fn test_prefecture_only() {
        let r = parse_location("愛知県内の工場", None);
        assert_eq!(r.prefecture.as_deref(), Some("愛知県"));
    }

    #[test]
    fn test_tokyo_kyoto_conflict() {
        let r1 = parse_location("東京都千代田区", None);
        assert_eq!(r1.prefecture.as_deref(), Some("東京都"));

        let r2 = parse_location("京都府京都市", None);
        assert_eq!(r2.prefecture.as_deref(), Some("京都府"));
    }

    #[test]
    fn test_context_fallback() {
        let r = parse_location("駅チカの事務所", Some("大阪府"));
        assert_eq!(r.prefecture.as_deref(), Some("大阪府"));
        assert_eq!(r.method, "context_fallback");
    }

    #[test]
    fn test_empty() {
        let r = parse_location("", None);
        assert_eq!(r.confidence, 0.0);
    }

    // ======== エッジケース ========

    #[test]
    fn test_ekichika_no_station() {
        // 「駅チカ」は駅名ではない
        let r = parse_location("駅チカの事務所", None);
        // 駅名マッチしないことを確認
        assert_ne!(r.method, "station");
    }

    #[test]
    fn test_shared_ward_kita() {
        // 「北区」は東京都コンテキストなしでは東京都にマッチしない
        let r = parse_location("北区赤羽", None);
        // 東京の文字がないので23区マッチはスキップされるべき
        // ただしパターンマッチで「北区」は拾える
        assert!(r.municipality.is_some());
    }

    #[test]
    fn test_osaka_kita_ku() {
        let r = parse_location("大阪市北区", None);
        assert_eq!(r.prefecture.as_deref(), Some("大阪府"));
        assert!(r.municipality.as_deref().unwrap().contains("大阪市"));
    }

    #[test]
    fn test_telework() {
        let r = parse_location("完全在宅・テレワーク", None);
        assert_eq!(r.region_block.as_deref(), Some("リモート"));
    }

    #[test]
    fn test_city_alias() {
        let r = parse_location("名古屋の工場", None);
        assert_eq!(r.prefecture.as_deref(), Some("愛知県"));
        assert_eq!(r.municipality.as_deref(), Some("名古屋市"));
    }

    #[test]
    fn test_with_address_detail() {
        let r = parse_location("東京都新宿区西新宿2-8-1", None);
        assert_eq!(r.prefecture.as_deref(), Some("東京都"));
        assert_eq!(r.municipality.as_deref(), Some("新宿区"));
    }

    #[test]
    fn test_hokkaido() {
        let r = parse_location("北海道札幌市", None);
        assert_eq!(r.prefecture.as_deref(), Some("北海道"));
    }

    #[test]
    fn test_nationwide() {
        let r = parse_location("全国各地", None);
        assert_eq!(r.city_type.as_deref(), Some("全国"));
    }

    #[test]
    fn test_municipality_pattern_machi() {
        let r = parse_location("群馬県吾妻郡草津町", Some("群馬県"));
        assert_eq!(r.prefecture.as_deref(), Some("群馬県"));
        assert!(r.municipality.is_some());
    }
}
