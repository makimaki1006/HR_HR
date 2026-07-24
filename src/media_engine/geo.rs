//! 距離圏(C案)による近隣地域の抽出。
//!
//! Python 版 `neighbor_map.haversine_km` / `neighbors_within` の移植。
//! 市区町村の重心(lat, lon)は外部データ。ここは辞書を受け取るだけで
//! データ源に依存しない(重心欠測でも空で安全に動く)。

use std::collections::HashMap;
use std::path::Path;

/// 平均地球半径(km)。Haversine 用。
const EARTH_RADIUS_KM: f64 = 6371.0088;

/// 市区町村重心 CSV を読み込む(Phase 2b)。
///
/// CSV は列 `name,lat,lon,level,...`。`level` が `municipality` / `ward` の行のみ
/// 採用(`prefecture` は近隣戦略の粒度外なので除外)。name→(lat, lon) の索引を返す。
/// ヘッダ順は先頭行から解決する。緯度経度が数値でない行はスキップする。
pub fn load_centroids(path: &Path) -> anyhow::Result<HashMap<String, (f64, f64)>> {
    let content = std::fs::read_to_string(path)?;
    let mut lines = content.lines();
    let header = lines
        .next()
        .ok_or_else(|| anyhow::anyhow!("centroids CSV is empty: {}", path.display()))?;
    let cols: Vec<&str> = header.split(',').map(str::trim).collect();
    let idx = |name: &str| cols.iter().position(|c| *c == name);
    let (i_name, i_lat, i_lon, i_level) = (
        idx("name").ok_or_else(|| anyhow::anyhow!("centroids CSV missing 'name' column"))?,
        idx("lat").ok_or_else(|| anyhow::anyhow!("centroids CSV missing 'lat' column"))?,
        idx("lon").ok_or_else(|| anyhow::anyhow!("centroids CSV missing 'lon' column"))?,
        idx("level").ok_or_else(|| anyhow::anyhow!("centroids CSV missing 'level' column"))?,
    );
    let mut out = HashMap::new();
    for line in lines {
        if line.trim().is_empty() {
            continue;
        }
        let fields: Vec<&str> = line.split(',').collect();
        let max_i = i_name.max(i_lat).max(i_lon).max(i_level);
        if fields.len() <= max_i {
            continue;
        }
        let level = fields[i_level].trim();
        if level != "municipality" && level != "ward" {
            continue;
        }
        let name = fields[i_name].trim();
        if name.is_empty() {
            continue;
        }
        let (lat, lon) = match (fields[i_lat].trim().parse::<f64>(), fields[i_lon].trim().parse::<f64>()) {
            (Ok(la), Ok(lo)) => (la, lo),
            _ => continue,
        };
        out.insert(name.to_string(), (lat, lon));
    }
    Ok(out)
}

/// 2点間の大圏距離(km)。
pub fn haversine_km(lat1: f64, lon1: f64, lat2: f64, lon2: f64) -> f64 {
    let phi1 = lat1.to_radians();
    let phi2 = lat2.to_radians();
    let d_phi = (lat2 - lat1).to_radians();
    let d_lambda = (lon2 - lon1).to_radians();
    let a = (d_phi / 2.0).sin().powi(2)
        + phi1.cos() * phi2.cos() * (d_lambda / 2.0).sin().powi(2);
    2.0 * EARTH_RADIUS_KM * a.sqrt().asin()
}

/// 近隣1件(地名と基準からの距離km)。
#[derive(Debug, Clone, PartialEq)]
pub struct Neighbor {
    pub name: String,
    pub distance_km: f64,
}

/// 近隣抽出の結果。基準の重心が無ければ `resolved = false`。
#[derive(Debug, Clone, PartialEq)]
pub struct NeighborResult {
    pub base: String,
    pub resolved: bool,
    pub radius_km: f64,
    pub neighbors: Vec<Neighbor>,
}

fn round2(v: f64) -> f64 {
    (v * 100.0).round() / 100.0
}

/// 基準地名から radius_km 以内の地名を距離昇順で返す。
///
/// フィルタは丸め前の生距離で行い(Python と一致)、格納は小数2桁に丸める。
/// 基準の重心が無ければ resolved=false(近隣は空)。
pub fn neighbors_within(
    base_name: &str,
    centroids: &HashMap<String, (f64, f64)>,
    radius_km: f64,
    include_base: bool,
) -> NeighborResult {
    let base = match centroids.get(base_name) {
        Some(&b) => b,
        None => {
            return NeighborResult {
                base: base_name.to_string(),
                resolved: false,
                radius_km,
                neighbors: Vec::new(),
            }
        }
    };
    let (base_lat, base_lon) = base;
    let mut neighbors: Vec<Neighbor> = Vec::new();
    for (name, &(lat, lon)) in centroids.iter() {
        if name == base_name && !include_base {
            continue;
        }
        let distance = haversine_km(base_lat, base_lon, lat, lon);
        if distance <= radius_km {
            neighbors.push(Neighbor {
                name: name.clone(),
                distance_km: round2(distance),
            });
        }
    }
    // 距離昇順。同距離は決定論のため名前で tie-break(Python は挿入順だが、
    // Rust の HashMap は順不同のため名前で安定化する)。
    neighbors.sort_by(|a, b| {
        a.distance_km
            .partial_cmp(&b.distance_km)
            .unwrap()
            .then_with(|| a.name.cmp(&b.name))
    });
    NeighborResult {
        base: base_name.to_string(),
        resolved: true,
        radius_km,
        neighbors,
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn synthetic() -> HashMap<String, (f64, f64)> {
        // 合成座標(実地理でない)。緯度0.05度≒5.6km、0.1度≒11.1km、0.5度≒55.6km。
        HashMap::from([
            ("基準".to_string(), (35.0, 135.0)),
            ("近い".to_string(), (35.05, 135.0)),
            ("中間".to_string(), (35.1, 135.0)),
            ("遠い".to_string(), (35.5, 135.0)),
        ])
    }

    #[test]
    fn haversine_zero_and_known() {
        assert_eq!(haversine_km(35.0, 135.0, 35.0, 135.0), 0.0);
        assert!((haversine_km(35.0, 135.0, 35.1, 135.0) - 11.1).abs() < 0.3);
    }

    #[test]
    fn within_radius_orders_and_filters() {
        let r = neighbors_within("基準", &synthetic(), 15.0, false);
        assert!(r.resolved);
        let names: Vec<&str> = r.neighbors.iter().map(|n| n.name.as_str()).collect();
        assert_eq!(names, vec!["近い", "中間"]); // 遠い(55km)は除外、基準は既定で除外
    }

    #[test]
    fn base_missing_unresolved() {
        let r = neighbors_within("存在しない", &synthetic(), 15.0, false);
        assert!(!r.resolved);
        assert!(r.neighbors.is_empty());
    }

    #[test]
    fn load_centroids_keeps_only_municipality_and_ward() {
        let dir = std::env::temp_dir().join(format!("jme_geo_{}", std::process::id()));
        std::fs::create_dir_all(&dir).unwrap();
        let path = dir.join("centroids.csv");
        // 列順を name,lat,lon,level,parent,source_name(本番と同じ)。
        std::fs::write(
            &path,
            "name,lat,lon,level,parent,source_name\n\
             渋谷区,35.6640,139.6982,ward,東京都,渋谷区役所\n\
             札幌市,43.0618,141.3545,municipality,,札幌市役所\n\
             東京都,35.6895,139.6917,prefecture,,東京都庁\n\
             壊れ行,abc,139.0,municipality,,x\n",
        )
        .unwrap();
        let map = super::load_centroids(&path).unwrap();
        assert_eq!(map.len(), 2); // prefecture と 壊れ行 は除外
        assert!(map.contains_key("渋谷区"));
        assert!(map.contains_key("札幌市"));
        assert!(!map.contains_key("東京都"));
        let (lat, lon) = map["札幌市"];
        assert!((lat - 43.0618).abs() < 1e-6 && (lon - 141.3545).abs() < 1e-6);
        let _ = std::fs::remove_file(&path);
    }
}
