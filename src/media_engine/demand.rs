//! 需要側のロジック(支配的キーワード選定・基準vs近隣の需要マップ)。
//!
//! Python 版 `media_ranking.dominant_keyword` / `demand_map.build_demand_map` の移植。

use std::collections::HashMap;

use crate::media_engine::geo::neighbors_within;

/// 検索ボリューム最大のキーワードを返す(欠測 `None` は無視、同値は先勝ち)。
///
/// 入力は挿入順を保つスライス(`&[(keyword, Option<volume>)]`)。Python の dict
/// 挿入順(＝修飾語順)での「先勝ち」タイブレークを決定論的に再現する。
pub fn dominant_keyword(volumes: &[(String, Option<i64>)]) -> Option<(String, i64)> {
    let mut best: Option<(String, i64)> = None;
    for (keyword, volume) in volumes {
        let Some(v) = volume else { continue };
        match &best {
            Some((_, best_v)) if *best_v >= *v => {}
            _ => best = Some((keyword.clone(), *v)),
        }
    }
    best
}

/// 需要マップの1地域。
#[derive(Debug, Clone, PartialEq)]
pub struct Area {
    pub name: String,
    pub distance_km: f64,
    pub is_base: bool,
    pub resolved: bool,
    pub geo_id: Option<String>,
    pub total_volume: Option<i64>,
}

/// 基準vs近隣の需要マップ。
#[derive(Debug, Clone, PartialEq)]
pub struct DemandMap {
    pub base: String,
    pub resolved: bool,
    pub radius_km: f64,
    pub queried_count: usize,
    pub dropped_count: usize,
    pub dropped_names: Vec<String>,
    pub base_total_volume: Option<i64>,
    pub areas: Vec<Area>,
    /// 需要降順(欠測は除外)。
    pub ranked: Vec<Area>,
    /// 基準より需要が大きい近隣。
    pub leakage: Vec<Area>,
}

/// 基準地域＋近隣(距離圏)の地域別需要マップを組み立てる。
///
/// `resolve` は地名→地域ID(無ければ `None`)、`volume` は (キーワード群, 地域ID)→
/// 需要合計。固定半径は都市部で地域数が爆発するため `max_areas` で距離順に丸め、
/// あふれた近隣は `dropped` に記録する(黙って切らない)。
pub fn build_demand_map<R, V>(
    base_name: &str,
    keywords: &[String],
    centroids: &HashMap<String, (f64, f64)>,
    radius_km: f64,
    max_areas: usize,
    resolve: R,
    volume: V,
) -> DemandMap
where
    R: Fn(&str) -> Option<String>,
    V: Fn(&[String], &str) -> i64,
{
    let nb = neighbors_within(base_name, centroids, radius_km, false);
    if !nb.resolved {
        return DemandMap {
            base: base_name.to_string(),
            resolved: false,
            radius_km,
            queried_count: 0,
            dropped_count: 0,
            dropped_names: Vec::new(),
            base_total_volume: None,
            areas: Vec::new(),
            ranked: Vec::new(),
            leakage: Vec::new(),
        };
    }

    // 基準を先頭に、近隣を距離順で。
    let mut ordered: Vec<(String, f64, bool)> = vec![(base_name.to_string(), 0.0, true)];
    for n in &nb.neighbors {
        ordered.push((n.name.clone(), n.distance_km, false));
    }
    let cap = max_areas.max(1);
    let dropped_names: Vec<String> =
        ordered.iter().skip(cap).map(|(n, _, _)| n.clone()).collect();
    let queried: Vec<(String, f64, bool)> = ordered.into_iter().take(cap).collect();

    let mut areas: Vec<Area> = Vec::new();
    for (name, distance_km, is_base) in &queried {
        match resolve(name) {
            Some(geo_id) => {
                let total = volume(keywords, &geo_id);
                areas.push(Area {
                    name: name.clone(),
                    distance_km: *distance_km,
                    is_base: *is_base,
                    resolved: true,
                    geo_id: Some(geo_id),
                    total_volume: Some(total),
                });
            }
            None => areas.push(Area {
                name: name.clone(),
                distance_km: *distance_km,
                is_base: *is_base,
                resolved: false,
                geo_id: None,
                total_volume: None,
            }),
        }
    }

    let mut ranked: Vec<Area> = areas.iter().filter(|a| a.total_volume.is_some()).cloned().collect();
    // 需要降順。安定ソートで同値は入力順を保つ(Python と一致)。
    ranked.sort_by(|a, b| b.total_volume.cmp(&a.total_volume));

    let base_total = areas.iter().find(|a| a.is_base).and_then(|a| a.total_volume);
    let leakage: Vec<Area> = ranked
        .iter()
        .filter(|a| !a.is_base && matches!((base_total, a.total_volume), (Some(bt), Some(v)) if v > bt))
        .cloned()
        .collect();

    DemandMap {
        base: base_name.to_string(),
        resolved: true,
        radius_km,
        queried_count: queried.len(),
        dropped_count: dropped_names.len(),
        dropped_names,
        base_total_volume: base_total,
        areas,
        ranked,
        leakage,
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn vols(pairs: &[(&str, Option<i64>)]) -> Vec<(String, Option<i64>)> {
        pairs.iter().map(|(k, v)| (k.to_string(), *v)).collect()
    }

    #[test]
    fn picks_max_ignoring_none() {
        let v = vols(&[("a 求人", Some(10)), ("a 転職", Some(40)), ("a パート", None)]);
        assert_eq!(dominant_keyword(&v), Some(("a 転職".to_string(), 40)));
    }

    #[test]
    fn first_wins_on_tie() {
        let v = vols(&[("最初", Some(40)), ("後", Some(40))]);
        assert_eq!(dominant_keyword(&v), Some(("最初".to_string(), 40)));
    }

    #[test]
    fn empty_is_none() {
        assert_eq!(dominant_keyword(&[]), None);
        assert_eq!(dominant_keyword(&vols(&[("a", None)])), None);
    }

    fn synthetic() -> HashMap<String, (f64, f64)> {
        HashMap::from([
            ("基準".to_string(), (35.0, 135.0)),
            ("隣A".to_string(), (35.05, 135.0)), // ≒5.6km
            ("隣B".to_string(), (35.1, 135.0)),  // ≒11.1km
            ("遠".to_string(), (35.5, 135.0)),   // ≒55.6km(半径外)
        ])
    }

    #[test]
    fn demand_map_ranks_and_detects_leakage() {
        let ids: HashMap<&str, &str> =
            HashMap::from([("基準", "L0"), ("隣A", "LA"), ("隣B", "LB")]);
        let totals: HashMap<&str, i64> = HashMap::from([("L0", 100), ("LA", 300), ("LB", 50)]);
        let resolve = |name: &str| ids.get(name).map(|s| s.to_string());
        let volume = |_kws: &[String], loc: &str| *totals.get(loc).unwrap_or(&0);

        let dm = build_demand_map("基準", &["a 求人".to_string()], &synthetic(), 15.0, 20, resolve, volume);
        assert!(dm.resolved);
        assert_eq!(dm.base_total_volume, Some(100));
        assert!(!dm.areas.iter().any(|a| a.name == "遠")); // 半径外は含まれない
        let ranked: Vec<&str> = dm.ranked.iter().map(|a| a.name.as_str()).collect();
        assert_eq!(ranked, vec!["隣A", "基準", "隣B"]); // 需要降順
        let leak: Vec<&str> = dm.leakage.iter().map(|a| a.name.as_str()).collect();
        assert_eq!(leak, vec!["隣A"]); // 基準より大きい近隣
    }

    #[test]
    fn demand_map_caps_and_logs_dropped() {
        let resolve = |name: &str| Some(format!("id-{name}"));
        let volume = |_kws: &[String], _loc: &str| 10;
        let dm = build_demand_map("基準", &["a 求人".to_string()], &synthetic(), 15.0, 2, resolve, volume);
        assert_eq!(dm.queried_count, 2); // 基準+最近隣1
        assert_eq!(dm.dropped_count, 1);
        assert_eq!(dm.dropped_names, vec!["隣B".to_string()]); // 黙って切らず記録
    }

    #[test]
    fn demand_map_base_missing_unresolved() {
        let resolve = |_name: &str| Some("x".to_string());
        let volume = |_kws: &[String], _loc: &str| 1;
        let dm = build_demand_map("無い", &["a".to_string()], &synthetic(), 15.0, 12, resolve, volume);
        assert!(!dm.resolved);
        assert!(dm.areas.is_empty());
    }
}
