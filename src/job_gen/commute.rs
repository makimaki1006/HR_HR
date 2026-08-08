//! 通勤圏レイヤー (仕様 P2-1)。
//!
//! 競合母集団を「同一市区町村 → 都道府県」の2段ではなく、勤務地からの距離で
//! レイヤー化する。直接競合 (通勤圏) と広域ベンチマーク (県) を分けて出すための土台。
//!
//! 判定は市区町村重心 CSV (`data/media_engine/municipality_centroids.csv`) の
//! Haversine 距離のみ。LLM は使わない。重心が引けない地名は距離を返さず
//! [`CommuteLayer::OutOfRange`] にする (それっぽい距離を捏造しない)。
//!
//! 重心 CSV の都道府県表現に注意:
//! - 列は `name,lat,lon,level,parent,source_name` で、**都道府県列は無い**。
//! - 全国で一意な市区町村は `菊川市` のように素の名前で入る (`level=municipality`)。
//! - 同名が複数県にある市区町村だけ `東京府中市` `広島府中市` `宮城川崎町` のように
//!   県名を前置した名前で入り、`parent` に県名の幹 (`東京`/`広島`/`宮城`) が入る
//!   (`level=ward`)。政令市の行政区も `level=ward` だが `parent` は市名 (`大阪市`)。
//!
//! したがって「県で必ず絞る」照合は、`parent` が都道府県の行を (県, 市区町村) の
//! 複合キーとして持ち、素の名前は「全国で一意」な行だけに使う、という形で担保する。
//! 同名市区町村 (府中市・川崎町・伊達市など) は県付きキーでしか引けないようにし、
//! 素の名前へのフォールバックを禁止する。

use std::collections::{HashMap, HashSet};
use std::path::{Path, PathBuf};

use crate::media_engine::geo::haversine_km;

/// 勤務地から見た通勤圏のレイヤー。
#[derive(Debug, Clone, Copy, PartialEq, Eq, serde::Serialize)]
pub enum CommuteLayer {
    /// 15km 圏内。直接競合。
    Direct15km,
    /// 15km 超 30km 圏内。通勤圏内の競合。
    Direct30km,
    /// 同一都道府県で 30km 超。広域ベンチマーク。
    Wide,
    /// 他県、または重心が引けず距離を確定できない。
    OutOfRange,
}

impl CommuteLayer {
    /// 画面表示用のラベル。
    pub fn label(&self) -> &'static str {
        match self {
            CommuteLayer::Direct15km => "15km圏(直接競合)",
            CommuteLayer::Direct30km => "30km圏(通勤圏)",
            CommuteLayer::Wide => "県内広域",
            CommuteLayer::OutOfRange => "圏外・地域不明",
        }
    }
}

/// 15km 圏の上限 (km)。
const DIRECT_15KM: f64 = 15.0;
/// 30km 圏の上限 (km)。
const DIRECT_30KM: f64 = 30.0;

/// 重心 CSV のパス (`CENTROIDS_PATH` 優先)。
///
/// `media_engine::handlers` の同名関数と同じ規約。あちらは private なので同じ既定値を持つ。
fn centroids_path() -> PathBuf {
    match std::env::var("CENTROIDS_PATH") {
        Ok(p) if !p.trim().is_empty() => PathBuf::from(p),
        _ => PathBuf::from("data/media_engine/municipality_centroids.csv"),
    }
}

/// 都道府県名の幹を返す。`静岡県`→`静岡`、`東京都`→`東京`、`京都府`→`京都`。
///
/// `北海道` だけは幹も `北海道` (重心 CSV の `parent` もそう入っている)。
fn prefecture_stem(prefecture: &str) -> String {
    let trimmed: String = prefecture.chars().filter(|c| !c.is_whitespace()).collect();
    if trimmed == "北海道" {
        return trimmed;
    }
    let mut chars = trimmed.chars();
    match chars.next_back() {
        Some('都') | Some('府') | Some('県') => chars.as_str().to_string(),
        _ => trimmed,
    }
}

/// 地名の空白 (半角/全角) を落とす。重心 CSV 側の名前も同じ規則で正規化する。
fn strip_spaces(value: &str) -> String {
    value.chars().filter(|c| !c.is_whitespace()).collect()
}

/// 市区町村重心による通勤圏の判定器。
#[derive(Debug, Clone)]
pub struct CommuteClassifier {
    /// (都道府県の幹, 市区町村名) → (lat, lon)。同名市区町村を持つ県付きの行。
    by_prefecture: HashMap<(String, String), (f64, f64)>,
    /// 市区町村名 → (lat, lon)。全国で一意とみなせる行のみ。
    unique_names: HashMap<String, (f64, f64)>,
    /// 複数県に存在する市区町村名。素の名前での照合を禁止する集合。
    ambiguous_names: HashSet<String>,
}

impl CommuteClassifier {
    /// 既定パス (`CENTROIDS_PATH` または `data/media_engine/municipality_centroids.csv`) から読む。
    pub fn load() -> anyhow::Result<Self> {
        Self::load_from_path(&centroids_path())
    }

    /// 指定パスの重心 CSV から読む。
    pub fn load_from_path(path: &Path) -> anyhow::Result<Self> {
        let content = std::fs::read_to_string(path)
            .map_err(|e| anyhow::anyhow!("centroids CSV を読めない ({}): {e}", path.display()))?;
        Self::from_csv_str(&content)
    }

    /// 重心 CSV 本文から構築する。
    ///
    /// 先頭行をヘッダとして列位置を解決する。`level` が `municipality` / `ward` の行のみ採用し、
    /// `parent` が都道府県 (CSV の `level=prefecture` 行から作った幹の集合) の行を県付きキーに、
    /// それ以外を素の名前のキーに入れる。緯度経度が数値でない行は捨てる。
    pub fn from_csv_str(content: &str) -> anyhow::Result<Self> {
        let mut lines = content.lines();
        let header = lines
            .next()
            .ok_or_else(|| anyhow::anyhow!("centroids CSV が空"))?;
        let cols: Vec<&str> = header.split(',').map(str::trim).collect();
        let index_of = |name: &str| {
            cols.iter()
                .position(|c| *c == name)
                .ok_or_else(|| anyhow::anyhow!("centroids CSV に '{name}' 列が無い"))
        };
        let (i_name, i_lat, i_lon, i_level, i_parent) = (
            index_of("name")?,
            index_of("lat")?,
            index_of("lon")?,
            index_of("level")?,
            index_of("parent")?,
        );
        let max_index = i_name.max(i_lat).max(i_lon).max(i_level).max(i_parent);

        // 1周目: 都道府県の幹の集合を作る (parent が県か市かの判定に使う)。
        let mut prefecture_stems: HashSet<String> = HashSet::new();
        for line in content.lines().skip(1) {
            let fields: Vec<&str> = line.split(',').collect();
            if fields.len() <= max_index || fields[i_level].trim() != "prefecture" {
                continue;
            }
            let name = strip_spaces(fields[i_name]);
            if !name.is_empty() {
                prefecture_stems.insert(prefecture_stem(&name));
            }
        }
        if prefecture_stems.is_empty() {
            anyhow::bail!("centroids CSV に level=prefecture の行が無く、県の判別ができない");
        }

        // 2周目: 市区町村・行政区を索引化する。
        let mut by_prefecture = HashMap::new();
        let mut unique_names = HashMap::new();
        let mut ambiguous_names = HashSet::new();
        for line in lines {
            if line.trim().is_empty() {
                continue;
            }
            let fields: Vec<&str> = line.split(',').collect();
            if fields.len() <= max_index {
                continue;
            }
            let level = fields[i_level].trim();
            if level != "municipality" && level != "ward" {
                continue;
            }
            let name = strip_spaces(fields[i_name]);
            if name.is_empty() {
                continue;
            }
            let coord = match (
                fields[i_lat].trim().parse::<f64>(),
                fields[i_lon].trim().parse::<f64>(),
            ) {
                (Ok(lat), Ok(lon)) => (lat, lon),
                _ => continue,
            };
            let parent = strip_spaces(fields[i_parent]);
            if !parent.is_empty() && prefecture_stems.contains(&parent) {
                // 県名を前置した同名市区町村。素の市区町村名は前置分を剥がして得る。
                let bare = name
                    .strip_prefix(parent.as_str())
                    .unwrap_or(&name)
                    .to_string();
                if bare.is_empty() {
                    continue;
                }
                ambiguous_names.insert(bare.clone());
                by_prefecture.insert((parent, bare), coord);
            } else {
                unique_names.insert(name, coord);
            }
        }
        if by_prefecture.is_empty() && unique_names.is_empty() {
            anyhow::bail!("centroids CSV に市区町村の行が無い");
        }
        Ok(Self {
            by_prefecture,
            unique_names,
            ambiguous_names,
        })
    }

    /// 索引済みの市区町村数 (県付き + 素の名前)。
    pub fn len(&self) -> usize {
        self.by_prefecture.len() + self.unique_names.len()
    }

    /// 索引が空か。
    pub fn is_empty(&self) -> bool {
        self.len() == 0
    }

    /// (都道府県, 市区町村) の重心を引く。引けなければ `None` (推定しない)。
    ///
    /// 照合順:
    /// 1. 表記を正規化 (空白除去、先頭に付いた自県名の除去)
    /// 2. 完全一致 — 県付きキー → 素の名前 (同名市区町村は素の名前を使わない)
    /// 3. `◯◯郡` を落とした形で 2 を再試行
    /// 4. 前方一致で市まで縮める (`川崎市高津区` → `川崎市`)。ここでも県の絞り込みは外さない。
    pub fn centroid(&self, prefecture: &str, municipality: &str) -> Option<(f64, f64)> {
        let stem = prefecture_stem(prefecture);
        if stem.is_empty() {
            return None;
        }
        for form in self.name_forms(&stem, municipality) {
            if let Some(coord) = self.lookup_exact(&stem, &form) {
                return Some(coord);
            }
        }
        for form in self.name_forms(&stem, municipality) {
            if let Some(coord) = self.lookup_city_prefix(&stem, &form) {
                return Some(coord);
            }
        }
        None
    }

    /// 照合に使う市区町村名の候補形 (原形に近い順)。
    ///
    /// 原形を必ず先頭に置く。県名の剥がしは「静岡県静岡市」→「市」のように
    /// 市名そのものを削ってしまうことがあるので、原形を置き換えるのではなく
    /// 後続の候補として足すだけにする。
    fn name_forms(&self, stem: &str, municipality: &str) -> Vec<String> {
        let mut forms: Vec<String> = Vec::new();
        let normalized = strip_spaces(municipality);
        if normalized.is_empty() {
            return forms;
        }
        forms.push(normalized.clone());
        // 「静岡県菊川市」「静岡菊川市」のように県名が重複して渡された場合の候補。
        // 剥がすのは呼び出し側が指定した県の名前だけなので、他県と取り違えることはない。
        let stripped = normalized
            .strip_prefix(&format!("{stem}都"))
            .or_else(|| normalized.strip_prefix(&format!("{stem}府")))
            .or_else(|| normalized.strip_prefix(&format!("{stem}県")))
            .or_else(|| normalized.strip_prefix(stem))
            .filter(|rest| rest.chars().count() >= 2)
            .map(str::to_string);
        if let Some(rest) = stripped {
            if !forms.contains(&rest) {
                forms.push(rest);
            }
        }
        // 「周智郡森町」→「森町」。郡は重心 CSV に無い粒度なので落とす。
        for base in forms.clone() {
            if let Some(position) = base.rfind('郡') {
                let rest = base[position + '郡'.len_utf8()..].to_string();
                if !rest.is_empty() && !forms.contains(&rest) {
                    forms.push(rest);
                }
            }
        }
        forms
    }

    /// 完全一致の照合。県付きキーを先に見て、素の名前は全国で一意な場合のみ許す。
    fn lookup_exact(&self, stem: &str, name: &str) -> Option<(f64, f64)> {
        if let Some(coord) = self
            .by_prefecture
            .get(&(stem.to_string(), name.to_string()))
        {
            return Some(*coord);
        }
        if self.ambiguous_names.contains(name) {
            // 複数県にある市区町村名。県付きで引けなかった以上、素の名前で拾うと
            // 別県の同名自治体になり得るので照合を打ち切る。
            return None;
        }
        self.unique_names.get(name).copied()
    }

    /// `◯◯市△△区` `◯◯市□□町1-2` のような市より細かい表記を市まで縮めて照合する。
    ///
    /// 名前の途中にある `市` の位置で前方を切り出し、長い候補から順に試す。
    /// 切り出した候補も [`Self::lookup_exact`] を通すので、県の絞り込みは外れない。
    fn lookup_city_prefix(&self, stem: &str, name: &str) -> Option<(f64, f64)> {
        let mut cut_positions: Vec<usize> = name
            .char_indices()
            .filter(|(_, c)| *c == '市')
            .map(|(i, c)| i + c.len_utf8())
            .filter(|end| *end < name.len())
            .collect();
        cut_positions.sort_unstable_by(|a, b| b.cmp(a));
        for end in cut_positions {
            let candidate = &name[..end];
            if candidate.chars().count() < 2 {
                continue;
            }
            if let Some(coord) = self.lookup_exact(stem, candidate) {
                return Some(coord);
            }
        }
        None
    }

    /// 2つの市区町村の重心間距離 (km)。どちらかの重心が引けなければ `None`。
    pub fn distance_km(
        &self,
        pref_a: &str,
        muni_a: &str,
        pref_b: &str,
        muni_b: &str,
    ) -> Option<f64> {
        let (lat_a, lon_a) = self.centroid(pref_a, muni_a)?;
        let (lat_b, lon_b) = self.centroid(pref_b, muni_b)?;
        Some(haversine_km(lat_a, lon_a, lat_b, lon_b))
    }

    /// 顧客勤務地から見た相手求人の通勤圏レイヤー。
    ///
    /// 県が違えば距離を測らず [`CommuteLayer::OutOfRange`]。同一県内は
    /// 15km / 30km / それ以遠で層別し、重心が引けなければ `OutOfRange`。
    pub fn layer(
        &self,
        client_pref: &str,
        client_muni: &str,
        other_pref: &str,
        other_muni: &str,
    ) -> CommuteLayer {
        let client_stem = prefecture_stem(client_pref);
        let other_stem = prefecture_stem(other_pref);
        if client_stem.is_empty() || client_stem != other_stem {
            return CommuteLayer::OutOfRange;
        }
        match self.distance_km(client_pref, client_muni, other_pref, other_muni) {
            Some(distance) if distance <= DIRECT_15KM => CommuteLayer::Direct15km,
            Some(distance) if distance <= DIRECT_30KM => CommuteLayer::Direct30km,
            Some(_) => CommuteLayer::Wide,
            None => CommuteLayer::OutOfRange,
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    /// 実データ (`data/media_engine/municipality_centroids.csv`) で構築する。
    /// `cargo test` の作業ディレクトリはクレートルートなので相対パスで読める。
    fn classifier() -> CommuteClassifier {
        CommuteClassifier::load().expect("重心 CSV を読み込めること")
    }

    #[test]
    fn loads_real_centroids() {
        let c = classifier();
        assert!(c.len() > 1_800, "市区町村の索引数が少なすぎる: {}", c.len());
        // 同名市区町村は県付きキーだけに入り、素の名前には入らない。
        assert!(c.ambiguous_names.contains("府中市"));
        assert!(c.ambiguous_names.contains("川崎町"));
        assert!(!c.unique_names.contains_key("府中市"));
        assert!(!c.unique_names.contains_key("川崎町"));
    }

    /// 菊川市 (34.757721, 138.084539) ⇔ 掛川市 (34.768717, 137.998403)。
    /// 重心間の実測は 7.963 km なので 15km 圏。
    #[test]
    fn kikugawa_to_kakegawa_is_direct_commute() {
        let c = classifier();
        let distance = c
            .distance_km("静岡県", "菊川市", "静岡県", "掛川市")
            .expect("両市とも重心が引けること");
        assert!(
            (distance - 7.963).abs() < 0.01,
            "菊川⇔掛川の実測距離が想定外: {distance}"
        );
        let layer = c.layer("静岡県", "菊川市", "静岡県", "掛川市");
        assert!(
            matches!(layer, CommuteLayer::Direct15km | CommuteLayer::Direct30km),
            "菊川⇔掛川が通勤圏に入らない: {layer:?}"
        );
        assert_eq!(layer, CommuteLayer::Direct15km);
    }

    /// 菊川市 ⇔ 静岡市 (34.975473, 138.382388) の実測は 36.396 km。
    /// 掛川 (7.963 km) より遠く、同一県内なので広域ベンチマーク扱い。
    #[test]
    fn shizuoka_city_is_farther_than_kakegawa() {
        let c = classifier();
        let to_kakegawa = c
            .distance_km("静岡県", "菊川市", "静岡県", "掛川市")
            .unwrap();
        let to_shizuoka = c
            .distance_km("静岡県", "菊川市", "静岡県", "静岡市")
            .unwrap();
        assert!(
            to_shizuoka > to_kakegawa,
            "静岡市 ({to_shizuoka}) が掛川市 ({to_kakegawa}) より近いのはおかしい"
        );
        assert!(
            (to_shizuoka - 36.396).abs() < 0.01,
            "菊川⇔静岡市の実測距離が想定外: {to_shizuoka}"
        );
        assert_eq!(
            c.layer("静岡県", "菊川市", "静岡県", "静岡市"),
            CommuteLayer::Wide
        );
    }

    /// 菊川市 ⇔ 磐田市 の実測は 21.749 km で 15km 圏と県内広域の中間。
    #[test]
    fn middle_distance_falls_into_30km_layer() {
        let c = classifier();
        let distance = c
            .distance_km("静岡県", "菊川市", "静岡県", "磐田市")
            .unwrap();
        assert!(
            (distance - 21.749).abs() < 0.01,
            "菊川⇔磐田の実測距離が想定外: {distance}"
        );
        assert_eq!(
            c.layer("静岡県", "菊川市", "静岡県", "磐田市"),
            CommuteLayer::Direct30km
        );
    }

    #[test]
    fn other_prefecture_is_out_of_range() {
        let c = classifier();
        assert_eq!(
            c.layer("静岡県", "菊川市", "神奈川県", "川崎市"),
            CommuteLayer::OutOfRange
        );
        assert_eq!(
            c.layer("静岡県", "菊川市", "愛知県", "豊橋市"),
            CommuteLayer::OutOfRange
        );
        // 県が違っても距離そのものは測れる (レイヤー判定だけが圏外)。
        assert!(c
            .distance_km("静岡県", "菊川市", "神奈川県", "川崎市")
            .is_some());
    }

    #[test]
    fn unknown_municipality_yields_none() {
        let c = classifier();
        assert!(c
            .distance_km("静岡県", "存在しない市", "静岡県", "掛川市")
            .is_none());
        assert!(c.centroid("静岡県", "架空町").is_none());
        assert_eq!(
            c.layer("静岡県", "存在しない市", "静岡県", "掛川市"),
            CommuteLayer::OutOfRange
        );
        // 空文字も推定しない。
        assert!(c.centroid("静岡県", "").is_none());
        assert!(c.centroid("", "菊川市").is_none());
    }

    /// 逆証明: 同名の別自治体を県で必ず切り分ける。
    ///
    /// 川崎市 (神奈川) / 川崎町 (宮城) / 川崎町 (福岡) は重心 CSV 上
    /// `川崎市` `宮城川崎町` `福岡川崎町` として別行で入っている。
    /// 実測 川崎市⇔宮城川崎町 = 305.971 km、宮城川崎町⇔福岡川崎町 = 1020.431 km。
    #[test]
    fn kawasaki_city_and_kawasaki_towns_are_not_confused() {
        let c = classifier();
        let kawasaki_shi = c.centroid("神奈川県", "川崎市").expect("川崎市");
        let miyagi = c.centroid("宮城県", "川崎町").expect("宮城県川崎町");
        let fukuoka = c.centroid("福岡県", "川崎町").expect("福岡県川崎町");
        assert_ne!(kawasaki_shi, miyagi);
        assert_ne!(miyagi, fukuoka);
        assert!(
            (c.distance_km("神奈川県", "川崎市", "宮城県", "川崎町")
                .unwrap()
                - 305.971)
                .abs()
                < 0.01
        );
        assert!(
            (c.distance_km("宮城県", "川崎町", "福岡県", "川崎町")
                .unwrap()
                - 1020.431)
                .abs()
                < 0.01
        );
        // 川崎町を持たない県からは引けない (素の名前へ落ちて別県を拾わない)。
        assert!(c.centroid("静岡県", "川崎町").is_none());
        assert!(c.centroid("神奈川県", "川崎町").is_none());
        // 「川崎町」は前方一致フォールバックでも「川崎市」に化けない
        // (切り出しは名前中の `市` の位置でしか行わないため)。
        assert!(c.centroid("宮城県", "川崎町大字前川").is_none());
    }

    /// 同名市 (府中市) も県付きで別物として解決される。実測 東京⇔広島 = 580.604 km。
    #[test]
    fn fuchu_city_resolves_per_prefecture() {
        let c = classifier();
        let tokyo = c.centroid("東京都", "府中市").expect("東京都府中市");
        let hiroshima = c.centroid("広島県", "府中市").expect("広島県府中市");
        assert_ne!(tokyo, hiroshima);
        let distance = c
            .distance_km("東京都", "府中市", "広島県", "府中市")
            .unwrap();
        assert!(
            (distance - 580.604).abs() < 0.01,
            "東京府中⇔広島府中の実測距離が想定外: {distance}"
        );
        // 府中市を持たない県からは引けない。
        assert!(c.centroid("静岡県", "府中市").is_none());
        // 広島県府中町は府中市とは別自治体。
        let fuchu_cho = c.centroid("広島県", "府中町").expect("広島県府中町");
        assert_ne!(fuchu_cho, hiroshima);
    }

    /// 区付き表記は市まで前方一致で縮める。`川崎市高津区` は重心 CSV に無く、
    /// `川崎市` (35.530865, 139.703031) に落ちる。
    #[test]
    fn ward_suffix_falls_back_to_city() {
        let c = classifier();
        let city = c.centroid("神奈川県", "川崎市").unwrap();
        assert_eq!(c.centroid("神奈川県", "川崎市高津区"), Some(city));
        assert_eq!(c.centroid("神奈川県", "川崎市 高津区"), Some(city));
        assert_eq!(c.centroid("神奈川県", "川崎市　高津区"), Some(city));
        // 静岡市葵区も CSV に無いので静岡市へ縮む。
        let shizuoka = c.centroid("静岡県", "静岡市").unwrap();
        assert_eq!(c.centroid("静岡県", "静岡市葵区"), Some(shizuoka));
        // 大阪市の行政区は CSV に実在するので、縮めずその区の重心が返る。
        let osaka_city = c.centroid("大阪府", "大阪市").unwrap();
        let chuo = c.centroid("大阪府", "大阪市中央区").unwrap();
        assert_ne!(osaka_city, chuo);
    }

    /// 郡付き・県名重複の表記ゆれ。静岡県周智郡森町は CSV 上 `静岡森町` (parent=静岡)。
    #[test]
    fn gun_prefix_and_duplicated_prefecture_are_normalized() {
        let c = classifier();
        let mori = c.centroid("静岡県", "森町").expect("静岡県森町");
        assert_eq!(c.centroid("静岡県", "周智郡森町"), Some(mori));
        assert_eq!(c.centroid("静岡県", "静岡県森町"), Some(mori));
        assert_eq!(c.centroid("静岡県", "静岡県周智郡森町"), Some(mori));
        // 北海道森町は別自治体。
        let hokkaido = c.centroid("北海道", "森町").expect("北海道森町");
        assert_ne!(hokkaido, mori);
        // 森町を持たない県からは引けない。
        assert!(c.centroid("愛知県", "森町").is_none());
    }

    /// 既知の限界を明示するテスト (直せていない箇所を隠さないため)。
    ///
    /// 政令市・東京23区の行政区のうち `中央区` `北区` `緑区` の3つは、重心 CSV に
    /// 市名を冠した行 (`大阪市北区` 等) と、市名の無い素の行 (`北区`) の両方がある。
    /// 素の行の座標は東京都のもの (北区 = 35.752805, 139.733657) だが、CSV に
    /// 都道府県列が無いためこの行を「東京都の行」と機械的に確認する手段が無い。
    /// その結果、市名を伴わない `北区` はどの県から引いても同じ座標を返してしまう。
    ///
    /// 市名付き (`大阪市北区`) で渡されれば正しい区が返るので、呼び出し側は
    /// 政令市の区を市名付きで渡すこと。恒久対応は重心 CSV への都道府県列追加
    /// (本モジュールの範囲外)。
    #[test]
    fn bare_ward_names_cannot_be_prefecture_checked() {
        let c = classifier();
        let tokyo_kita = c.centroid("東京都", "北区").expect("素の北区");
        // 限界: 大阪府から引いても東京都の北区が返る (県で弾けない)。
        assert_eq!(c.centroid("大阪府", "北区"), Some(tokyo_kita));
        // 市名付きなら正しく別の区として解決される。
        let osaka_kita = c.centroid("大阪府", "大阪市北区").expect("大阪市北区");
        assert_ne!(osaka_kita, tokyo_kita);
        let kyoto_kita = c.centroid("京都府", "京都市北区").expect("京都市北区");
        assert_ne!(kyoto_kita, osaka_kita);
    }

    #[test]
    fn prefecture_stem_handles_all_suffixes() {
        assert_eq!(prefecture_stem("静岡県"), "静岡");
        assert_eq!(prefecture_stem("東京都"), "東京");
        assert_eq!(prefecture_stem("京都府"), "京都");
        assert_eq!(prefecture_stem("大阪府"), "大阪");
        assert_eq!(prefecture_stem("北海道"), "北海道");
        assert_eq!(prefecture_stem(" 静岡県 "), "静岡");
        // 幹のまま渡されても同じ結果になる (レイヤー判定の県一致に使う)。
        assert_eq!(prefecture_stem("静岡"), "静岡");
        assert_eq!(prefecture_stem(""), "");
    }

    /// 県表記が「静岡県」「静岡」で揺れても同一県として扱う。
    #[test]
    fn prefecture_notation_variants_match() {
        let c = classifier();
        assert_eq!(
            c.layer("静岡", "菊川市", "静岡県", "掛川市"),
            CommuteLayer::Direct15km
        );
    }

    /// CSV に level=prefecture が無いと県の判別ができないので読み込みを失敗させる。
    #[test]
    fn csv_without_prefecture_rows_is_rejected() {
        let csv = "name,lat,lon,level,parent,source_name\n菊川市,34.757721,138.084539,municipality,,菊川市役所\n";
        assert!(CommuteClassifier::from_csv_str(csv).is_err());
    }

    /// 壊れた行 (緯度経度が数値でない・列不足) は捨てて読み進む。
    #[test]
    fn broken_rows_are_skipped() {
        let csv = concat!(
            "name,lat,lon,level,parent,source_name\n",
            "静岡県,34.976944,138.383056,prefecture,,\n",
            "菊川市,34.757721,138.084539,municipality,,菊川市役所\n",
            "掛川市,NaN値,137.998403,municipality,,掛川市役所\n",
            "壊れ行\n",
            "\n",
        );
        let c = CommuteClassifier::from_csv_str(csv).expect("読めること");
        assert!(c.centroid("静岡県", "菊川市").is_some());
        assert!(c.centroid("静岡県", "掛川市").is_none());
        assert_eq!(c.len(), 1);
    }

    #[test]
    fn layer_boundaries_are_inclusive() {
        let csv = concat!(
            "name,lat,lon,level,parent,source_name\n",
            "静岡県,34.976944,138.383056,prefecture,,\n",
            "基準市,35.0,138.0,municipality,,基準市役所\n",
            // 緯度 +0.1348° ≒ 15.0km、+0.2697° ≒ 30.0km、+0.4° ≒ 44.5km
            "十五キロ市,35.1348,138.0,municipality,,十五キロ市役所\n",
            "三十キロ市,35.2697,138.0,municipality,,三十キロ市役所\n",
            "遠方市,35.4,138.0,municipality,,遠方市役所\n",
        );
        let c = CommuteClassifier::from_csv_str(csv).expect("読めること");
        let d15 = c
            .distance_km("静岡県", "基準市", "静岡県", "十五キロ市")
            .unwrap();
        let d30 = c
            .distance_km("静岡県", "基準市", "静岡県", "三十キロ市")
            .unwrap();
        assert!(
            (d15 - 15.0).abs() < 0.05,
            "15km 相当の緯度差がずれている: {d15}"
        );
        assert!(
            (d30 - 30.0).abs() < 0.05,
            "30km 相当の緯度差がずれている: {d30}"
        );
        assert_eq!(
            c.layer("静岡県", "基準市", "静岡県", "十五キロ市"),
            CommuteLayer::Direct15km
        );
        assert_eq!(
            c.layer("静岡県", "基準市", "静岡県", "三十キロ市"),
            CommuteLayer::Direct30km
        );
        assert_eq!(
            c.layer("静岡県", "基準市", "静岡県", "遠方市"),
            CommuteLayer::Wide
        );
        assert_eq!(
            c.layer("静岡県", "基準市", "静岡県", "基準市"),
            CommuteLayer::Direct15km
        );
    }

    #[test]
    fn labels_are_distinct() {
        let labels = [
            CommuteLayer::Direct15km.label(),
            CommuteLayer::Direct30km.label(),
            CommuteLayer::Wide.label(),
            CommuteLayer::OutOfRange.label(),
        ];
        let unique: HashSet<&&str> = labels.iter().collect();
        assert_eq!(unique.len(), labels.len());
    }
}
