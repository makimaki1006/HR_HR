//! 架電クオリティ: 時間帯×曜日ヒートマップ（サーバ側集計 PoC）
//!
//! 2026-08-14 PoC。検証したいのは1点だけ:
//!   **常駐プロセスでサーバ側集計すると、ブラウザに渡す量と体感がどう変わるか**
//!
//! 現行 GAS の構造:
//!   - シート「時間帯ヒート_クロス」は 200,680行 × 8列
//!   - GAS は `getDataRange().getValues()` で全行を読み、90KB チャンクに分割して
//!     ブラウザへ送り、**集計はブラウザ側の JS** が行う
//!   - GAS はステートレスなので、CacheService(TTL 6h) が切れるたび全行を再読込
//!
//! この PoC:
//!   - 起動後は常駐メモリに全行を保持（TTL 内は Sheets を叩かない）
//!   - 絞り込み（都道府県 / 業種 / 担当者）を**サーバ側で適用**
//!   - 返すのは曜日7 × 時間帯24 = **最大168セル**のみ
//!
//! これで「返却バイト数」と「2回目以降の応答時間」が実測できる。
//! 効果が小さければ PoC はここで終わり、という前提で最小構成にしてある。

use std::collections::HashMap;
use std::sync::Arc;
use std::time::{Duration, Instant};

use anyhow::Result;
use serde::{Deserialize, Serialize};
use tokio::sync::RwLock;

use crate::db::sheets_client::SheetsClient;

/// 対象シート名（スプレッドシート側の実タブ名）
const SHEET_NAME: &str = "時間帯ヒート_クロス";
/// 常駐キャッシュの寿命。GAS の CacheService(6h) より短くして鮮度を優先する。
const CACHE_TTL: Duration = Duration::from_secs(60 * 60);

/// シート1行ぶん。文字列のまま持たず、集計に使う形へ正規化して保持する。
/// （200,680行を String のまま持つとメモリも比較も無駄になるため）
#[derive(Debug, Clone)]
pub struct CrossRow {
    pub owner_id: u64,
    pub weekday: u8, // 0=月 .. 6=日
    pub hour: u8,    // 0..23
    pub prefecture: Arc<str>,
    pub industry: Arc<str>,
    pub dial_count: u32,
    pub connect_count: u32,
    pub apo_count: u32,
}

/// 絞り込み条件。未指定(None)は「絞らない」。
#[derive(Debug, Default, Deserialize, Serialize)]
pub struct HeatmapQuery {
    pub prefecture: Option<String>,
    pub industry: Option<String>,
    /// カンマ区切りの owner_id。GAS 側のメンバー選択に相当。
    pub owners: Option<String>,
}

crate::accepted_params!(HeatmapQuery, heatmap_query_accepted =>
    "prefecture", "industry", "owners");

/// 返却する1セル。曜日×時間帯ごとの集計値。
#[derive(Debug, Serialize)]
pub struct HeatCell {
    pub weekday: u8,
    pub hour: u8,
    pub dial: u32,
    pub connect: u32,
    pub apo: u32,
    /// アポ率(%)。分母0なら null（0% と区別する）
    pub apo_rate: Option<f64>,
}

#[derive(Debug, Serialize)]
pub struct HeatmapResponse {
    pub cells: Vec<HeatCell>,
    /// 集計に使った元行数（絞り込み後）。透明性のため必ず返す。
    pub source_rows: usize,
    /// 元データの総行数（絞り込み前）
    pub total_rows: usize,
    /// キャッシュから応答したか
    pub from_cache: bool,
    /// サーバ側の処理時間(ms)。Sheets 取得を含むかは from_cache で判別する。
    pub elapsed_ms: u128,
    /// 解釈できず捨てた引数名。空でも必ず出す（`TabPayload` と同じ約束）。
    /// ルータが後乗せする。
    pub ignored_params: Vec<String>,
}

/// 常駐キャッシュ。Sheets から読んだ全行を保持する。
pub struct HeatmapCache {
    rows: RwLock<Option<(Instant, Arc<Vec<CrossRow>>)>>,
}

impl HeatmapCache {
    pub fn new() -> Self {
        Self {
            rows: RwLock::new(None),
        }
    }

    /// キャッシュが生きていれば返し、無ければ Sheets から読んで詰める。
    async fn get_rows(&self, client: &SheetsClient) -> Result<(Arc<Vec<CrossRow>>, bool)> {
        {
            let guard = self.rows.read().await;
            if let Some((at, rows)) = guard.as_ref() {
                if at.elapsed() < CACHE_TTL {
                    return Ok((Arc::clone(rows), true));
                }
            }
        }
        // 期限切れ or 未取得。書き込みロックを取り直して再確認する
        // （複数リクエストが同時に来たとき二重に Sheets を叩かないため）
        let mut guard = self.rows.write().await;
        if let Some((at, rows)) = guard.as_ref() {
            if at.elapsed() < CACHE_TTL {
                return Ok((Arc::clone(rows), true));
            }
        }
        let fetched = fetch_cross_rows(client).await?;
        let arc = Arc::new(fetched);
        *guard = Some((Instant::now(), Arc::clone(&arc)));
        Ok((arc, false))
    }
}

impl Default for HeatmapCache {
    fn default() -> Self {
        Self::new()
    }
}

/// Sheets から「時間帯ヒート_クロス」を読み、CrossRow に正規化する。
///
/// 列順ではなくヘッダ名で引く。列の増減・並び替えで壊れないようにするため
/// （シートの列を位置で決め打ちしない、という既存の規律に合わせる）。
async fn fetch_cross_rows(client: &SheetsClient) -> Result<Vec<CrossRow>> {
    // 既存 SheetsClient は header をキーにした HashMap の Vec を返す。
    // 列の位置ではなく名前で引く形なので、列の増減・並び替えで壊れない。
    let raw = client.get_sheet_as_rows(SHEET_NAME).await?;

    // 都道府県・業種は値の種類が少ないので Arc<str> を使い回してメモリを抑える
    // （200,680行ぶんの String を個別に持つと無駄が大きい）
    let mut interner: HashMap<String, Arc<str>> = HashMap::new();

    let num = |s: &str| -> u32 { s.trim().parse::<f64>().ok().map(|v| v as u32).unwrap_or(0) };

    let mut out = Vec::with_capacity(raw.len());
    for row in raw {
        let g = |k: &str| -> &str { row.get(k).map(|s| s.as_str()).unwrap_or("").trim() };

        let wd = g("weekday").parse::<u8>().unwrap_or(255);
        let hr = g("hour").parse::<u8>().unwrap_or(255);
        // 曜日/時間帯が読めない行は集計に載せない（黙って0扱いにしない）
        if wd > 6 || hr > 23 {
            continue;
        }

        let mut intern = |s: &str| -> Arc<str> {
            if let Some(a) = interner.get(s) {
                return Arc::clone(a);
            }
            let a: Arc<str> = Arc::from(s);
            interner.insert(s.to_string(), Arc::clone(&a));
            a
        };

        out.push(CrossRow {
            owner_id: g("owner_id").parse::<u64>().unwrap_or(0),
            weekday: wd,
            hour: hr,
            prefecture: intern(g("prefecture")),
            industry: intern(g("industry")),
            dial_count: num(g("dial_count")),
            connect_count: num(g("connect_count")),
            apo_count: num(g("apo_count")),
        });
    }
    Ok(out)
}

/// 絞り込み → 曜日×時間帯で集計。ここが「サーバ側集計」の本体。
pub fn aggregate(rows: &[CrossRow], q: &HeatmapQuery) -> (Vec<HeatCell>, usize) {
    let owner_filter: Option<Vec<u64>> = q.owners.as_ref().map(|s| {
        s.split(',')
            .filter_map(|t| t.trim().parse::<u64>().ok())
            .collect()
    });

    // 7 曜日 × 24 時間 = 168 の固定バケット
    let mut acc = vec![[0u32; 3]; 7 * 24];
    let mut used = 0usize;

    for r in rows {
        if let Some(p) = q.prefecture.as_deref() {
            if !p.is_empty() && &*r.prefecture != p {
                continue;
            }
        }
        if let Some(i) = q.industry.as_deref() {
            if !i.is_empty() && &*r.industry != i {
                continue;
            }
        }
        if let Some(ids) = owner_filter.as_ref() {
            if !ids.is_empty() && !ids.contains(&r.owner_id) {
                continue;
            }
        }
        let k = (r.weekday as usize) * 24 + r.hour as usize;
        acc[k][0] += r.dial_count;
        acc[k][1] += r.connect_count;
        acc[k][2] += r.apo_count;
        used += 1;
    }

    let mut cells = Vec::with_capacity(7 * 24);
    for w in 0..7u8 {
        for h in 0..24u8 {
            let v = acc[(w as usize) * 24 + h as usize];
            // 全部0のセルは返さない（168 固定より軽くなる。描画側は欠損=0で扱う）
            if v[0] == 0 && v[1] == 0 && v[2] == 0 {
                continue;
            }
            cells.push(HeatCell {
                weekday: w,
                hour: h,
                dial: v[0],
                connect: v[1],
                apo: v[2],
                // 分母0のときは 0% でなく null。「架電0でアポ率0%」と誤読させない
                apo_rate: if v[0] > 0 {
                    Some((v[2] as f64) / (v[0] as f64) * 100.0)
                } else {
                    None
                },
            });
        }
    }
    (cells, used)
}

/// ハンドラ本体。axum の State から SheetsClient と HeatmapCache を受け取る想定。
pub async fn handle(
    client: &SheetsClient,
    cache: &HeatmapCache,
    q: HeatmapQuery,
) -> Result<HeatmapResponse> {
    let started = Instant::now();
    let (rows, from_cache) = cache.get_rows(client).await?;
    let (cells, used) = aggregate(&rows, &q);
    Ok(HeatmapResponse {
        cells,
        source_rows: used,
        total_rows: rows.len(),
        from_cache,
        elapsed_ms: started.elapsed().as_millis(),
        // ルータが後乗せする（ここは生のクエリ文字列を知らない）
        ignored_params: Vec::new(),
    })
}

#[cfg(test)]
mod tests {
    use super::*;

    fn row(wd: u8, hr: u8, pref: &str, ind: &str, owner: u64, dial: u32, apo: u32) -> CrossRow {
        CrossRow {
            owner_id: owner,
            weekday: wd,
            hour: hr,
            prefecture: Arc::from(pref),
            industry: Arc::from(ind),
            dial_count: dial,
            connect_count: 0,
            apo_count: apo,
        }
    }

    #[test]
    fn 曜日と時間帯で畳まれる() {
        let rows = vec![
            row(0, 10, "東京都", "運輸", 1, 100, 2),
            row(0, 10, "大阪府", "建設", 2, 50, 1),
            row(1, 11, "東京都", "運輸", 1, 30, 0),
        ];
        let (cells, used) = aggregate(&rows, &HeatmapQuery::default());
        assert_eq!(used, 3);
        assert_eq!(cells.len(), 2, "同じ曜日×時間帯は1セルに畳まれる");
        let c = cells.iter().find(|c| c.weekday == 0 && c.hour == 10).unwrap();
        assert_eq!(c.dial, 150);
        assert_eq!(c.apo, 3);
    }

    #[test]
    fn 都道府県で絞れる() {
        let rows = vec![
            row(0, 10, "東京都", "運輸", 1, 100, 2),
            row(0, 10, "大阪府", "建設", 2, 50, 1),
        ];
        let q = HeatmapQuery {
            prefecture: Some("東京都".into()),
            ..Default::default()
        };
        let (cells, used) = aggregate(&rows, &q);
        assert_eq!(used, 1);
        assert_eq!(cells[0].dial, 100);
    }

    #[test]
    fn 担当者で絞れる() {
        let rows = vec![
            row(0, 10, "東京都", "運輸", 1, 100, 2),
            row(0, 10, "東京都", "運輸", 2, 50, 1),
        ];
        let q = HeatmapQuery {
            owners: Some("2".into()),
            ..Default::default()
        };
        let (_, used) = aggregate(&rows, &q);
        assert_eq!(used, 1);
    }

    #[test]
    fn 分母0のアポ率はnullで返る() {
        // 架電0でアポだけある行（実データに存在する）。0% と表示すると誤読される。
        let rows = vec![row(0, 10, "東京都", "運輸", 1, 0, 3)];
        let (cells, _) = aggregate(&rows, &HeatmapQuery::default());
        assert_eq!(cells.len(), 1);
        assert!(cells[0].apo_rate.is_none(), "分母0のとき 0% を返してはいけない");
    }

    #[test]
    fn 空セルは返さない() {
        let rows = vec![row(3, 14, "東京都", "運輸", 1, 10, 0)];
        let (cells, _) = aggregate(&rows, &HeatmapQuery::default());
        assert_eq!(cells.len(), 1, "値のあるセルだけ返す(168固定にしない)");
    }
}
