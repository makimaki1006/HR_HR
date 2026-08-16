//! 架電クオリティ: 全シートの共通アクセス層
//!
//! 2026-08-14。GAS 版(`Code.gs` の `buildSheetResponse_`)が 65 シートを
//! 同じ形で読んでいるので、Rust 側も**1つの層**で受ける。
//! シートごとに handler を書くと 65 本になり、GAS の二の舞になる。
//!
//! GAS 版との違い（ここが移行する理由）:
//!   GAS  : ステートレス。CacheService(TTL 6h・90KBチャンク)が切れるたび全行再読込。
//!          集計はブラウザ側。「時間帯ヒート_クロス」は 200,679行 = 33.1MB を毎回送る。
//!   Rust : 常駐メモリに保持。TTL 内は Sheets を叩かない。
//!          絞り込み・集計をサーバ側で行い、必要分だけ返す（実測 6.7KB / 3,407分の1）。
//!
//! 実測値の根拠:
//!   docs\wbs_outputs\ダッシュボード高速化_Rust移行\PoC実測結果_2026-08-14.md

use std::collections::HashMap;
use std::sync::Arc;
use std::time::{Duration, Instant};

use anyhow::{Context, Result};
use serde::{Deserialize, Serialize};
use tokio::sync::RwLock;

use crate::db::sheets_client::SheetsClient;

/// 常駐キャッシュの寿命。
/// GAS の CacheService は 6h だったが、常駐なら再読込が安いので短くして鮮度を優先する。
const CACHE_TTL: Duration = Duration::from_secs(60 * 60);

/// GAS 版 `Code.gs` が読んでいる全シート（2026-08-14 時点で 65 枚）。
///
/// ここに列挙するのは「存在するシート名を型で縛る」ためではなく、
/// **どのシートが画面から参照されているかを1箇所で見えるようにする**ため。
/// GAS 側では 65 個の getter に散っていて全体像が掴めなかった。
pub const KNOWN_SHEETS: &[&str] = &[
    // --- 営業コア ---
    "月次接触規模",
    "曜日別集計",
    "曜日別 owner別",
    "時間帯ヒート",
    "時間帯ヒート_クロス",
    "時間帯ヒート_BPO",
    "N回目架電分析",
    "リサイクル間隔",
    "コンプライアンスパターン",
    "コーラー行動パターン",
    "通話時間バケット",
    "ステージ反復分析",
    "滞留日数",
    "ファネル4段",
    "コホート分析",
    "新規/既存/リサイクル",
    "セグメント_クロス",
    "Recency owner月次",
    "その場失注 owner月次",
    "商談遷移_集計",
    "商談遷移_クロス",
    "成約率_現場ベース",
    "追いかけ停止候補",
    "異常検知",
    "月末予測",
    "kpi_targets",
    // --- Deal Health / リスク ---
    "Deal Health",
    "Deal Health owner月次",
    "リスク統合ボード",
    // --- コンサル ---
    "コンサル行動量_日次",
    "コンサル行動量_月次",
    "コンサル行動量_担当日次",
    "コンサル行動量_担当月次",
    "コンサル勝ち筋分析",
    "コンサル接触率_週次",
    "コンサル接触ログ_週次",
    "コンサル接触ロールアップ",
    "コンサル接触寄与率",
    "コンサル接触寄与率_分布",
    "コンサル接触寄与率_統計",
    "コンサル別ベンチマーク",
    "コンサル別ベンチマーク_月別",
    "コンサル別ベンチマーク_統計",
    "コンサルフェーズKPI",
    "コンサルKGIマトリクス",
    "コンサルMTGタイムライン",
    "コンサルタスク漏れ",
    "コンサルリスクスコア_月次",
    "コンサル健全性_月次",
    "コンサル健全性_月次_active",
    "コンサル担当者360_KPI",
    "コンサル担当者360_Deal一覧",
    "コンサル担当者360_都道府県",
    "コンサル担当者月次推移",
    "コンサル未来アクション",
    "コンサル未来案件_月次",
    "コンサル未来案件_Deal一覧",
    "コンサル未来案件_担当者サマリ",
    // --- 解約分析 ---
    "解約_理由パターン",
    "解約_active予測",
    "解約_コンサル担当別",
    "解約_業界規模マトリクス",
    "解約_モデル指標",
    // --- BPO ---
    "BPO貢献追跡",
    "業種別アクティブ観測",
    // --- 求人・応募 ---
    "求人応募_KPI",
    "求人応募_媒体月次",
    "求人応募_Deal健全性",
    "求人応募_データ品質",
];

/// 1シートぶんの内容。ヘッダと行を分けて持つ。
///
/// `Vec<HashMap<String,String>>` だと 200,679行 × 8列で HashMap を20万個作ることになり
/// メモリも構築時間も無駄なので、**ヘッダは1本・行は Vec<Arc<str>>** で持つ。
#[derive(Debug)]
pub struct SheetData {
    pub header: Vec<String>,
    pub rows: Vec<Vec<Arc<str>>>,
    /// 取得した時刻（鮮度の表示に使う）
    pub fetched_at: Instant,
}

impl SheetData {
    /// 列名 → 添字。無ければ None（位置で決め打ちしない）
    pub fn col(&self, name: &str) -> Option<usize> {
        self.header
            .iter()
            .position(|h| h.trim_start_matches('\u{feff}').trim() == name)
    }

    pub fn get<'a>(&self, row: &'a [Arc<str>], name: &str) -> &'a str {
        self.col(name)
            .and_then(|i| row.get(i))
            .map(|s| s.as_ref())
            .unwrap_or("")
    }
}

/// 全シートの常駐キャッシュ。シート単位で TTL を持つ。
pub struct SheetStore {
    inner: RwLock<HashMap<String, Arc<SheetData>>>,
}

impl SheetStore {
    pub fn new() -> Self {
        Self {
            inner: RwLock::new(HashMap::new()),
        }
    }

    /// キャッシュが生きていれば返す。無ければ Sheets から読んで詰める。
    ///
    /// 戻り値の bool は「キャッシュから返したか」。
    /// 画面に「いつ取得したデータか」を出すために使う
    /// （GAS 版で「読んだ時刻」を「更新時刻」と誤表示していた問題への対応）。
    pub async fn get(&self, client: &SheetsClient, sheet: &str) -> Result<(Arc<SheetData>, bool)> {
        {
            let g = self.inner.read().await;
            if let Some(d) = g.get(sheet) {
                if d.fetched_at.elapsed() < CACHE_TTL {
                    return Ok((Arc::clone(d), true));
                }
            }
        }
        // 書き込みロックを取り直して再確認（同時リクエストで二重fetchしない）
        let mut g = self.inner.write().await;
        if let Some(d) = g.get(sheet) {
            if d.fetched_at.elapsed() < CACHE_TTL {
                return Ok((Arc::clone(d), true));
            }
        }
        let data = fetch_sheet(client, sheet)
            .await
            .with_context(|| format!("シート「{sheet}」の取得に失敗"))?;
        let arc = Arc::new(data);
        g.insert(sheet.to_string(), Arc::clone(&arc));
        Ok((arc, false))
    }

    /// 明示的に破棄する（GAS の `?refresh=1` 相当）。
    pub async fn invalidate(&self, sheet: Option<&str>) {
        let mut g = self.inner.write().await;
        match sheet {
            Some(s) => {
                g.remove(s);
            }
            None => g.clear(),
        }
    }

    /// 常駐中のシートと行数（運用時の可視化用）
    pub async fn stats(&self) -> Vec<(String, usize, u64)> {
        let g = self.inner.read().await;
        let mut v: Vec<_> = g
            .iter()
            .map(|(k, d)| (k.clone(), d.rows.len(), d.fetched_at.elapsed().as_secs()))
            .collect();
        v.sort_by(|a, b| b.1.cmp(&a.1));
        v
    }
}

impl Default for SheetStore {
    fn default() -> Self {
        Self::new()
    }
}

async fn fetch_sheet(client: &SheetsClient, sheet: &str) -> Result<SheetData> {
    // 2026-08-17 是正: 以前は `get_sheet_as_rows`(HashMap) を使い、順序不定を
    //   避けるため `header.sort()` していた。結果、**スプレッドシートを
    //   そのまま見るための画面で列がアルファベット順**になり、原本
    //   (owner_id, year_month, pipeline, call_count …) と並びが違っていた。
    //   数値は正しい列に紐づいていたので誤りとしては現れず、見落とされていた。
    //   `get_sheet_as_table` は原本の列順をそのまま返すのでソートは不要。
    let (header, raw) = client.get_sheet_as_table(sheet).await?;

    // 値の種類が少ない列（都道府県・業種・ステージ名など）を intern して
    // 同じ文字列を使い回す
    let mut interner: HashMap<String, Arc<str>> = HashMap::new();
    let mut intern = |s: &str| -> Arc<str> {
        if let Some(a) = interner.get(s) {
            return Arc::clone(a);
        }
        let a: Arc<str> = Arc::from(s);
        interner.insert(s.to_string(), Arc::clone(&a));
        a
    };

    let mut rows = Vec::with_capacity(raw.len());
    for r in &raw {
        let mut row = Vec::with_capacity(header.len());
        for i in 0..header.len() {
            row.push(intern(r.get(i).map(|s| s.as_str()).unwrap_or("")));
        }
        rows.push(row);
    }

    Ok(SheetData {
        header,
        rows,
        fetched_at: Instant::now(),
    })
}

// ---------------------------------------------------------------- 汎用クエリ

/// 汎用の絞り込み・集計指定。
/// GAS 側でブラウザがやっていたことを、そのままサーバ側で受けられるようにする。
#[derive(Debug, Default, Deserialize, Serialize)]
pub struct SheetQuery {
    /// 絞り込み。`列名=値` を複数。値が空なら絞らない。
    #[serde(default)]
    pub filter: HashMap<String, String>,
    /// 集計のキーにする列（複数可）。空なら集計せず行をそのまま返す。
    #[serde(default)]
    pub group_by: Vec<String>,
    /// 合計する列。group_by 指定時のみ有効。
    #[serde(default)]
    pub sum: Vec<String>,
    /// 返す最大行数。集計しない場合の保険（全件を送らないため）。
    pub limit: Option<usize>,
}

// 2026-08-17 注記: **この構造体には現在 HTTP ルートが無い**（`sheets::query` を
// 内部から呼ぶだけ）。そのため応答に `ignored_params` を持たせる先が無く、
// ここでは受理リストと腐り検出テストだけを置いてある。
// ルートを生やすときは、ハンドラで
// `query_audit::audit_query::<SheetQuery>("sheets", raw.as_deref())` を呼び、
// `SheetResponse` に `ignored_params` を足すこと。
//
// なお `filter` は `HashMap<String,String>`、`group_by`/`sum` は `Vec<String>` なので、
// この構造体は **URL クエリ文字列では復元できない**（p7 と同じ罠）。
// ルートを生やすなら POST + JSON にすること。
crate::accepted_params!(SheetQuery, sheet_query_accepted =>
    "filter", "group_by", "sum", "limit");

#[derive(Debug, Serialize)]
pub struct SheetResponse {
    pub header: Vec<String>,
    pub rows: Vec<Vec<String>>,
    /// 絞り込み後・集計前の行数
    pub matched_rows: usize,
    /// シート全体の行数
    pub total_rows: usize,
    pub from_cache: bool,
    pub elapsed_ms: u128,
    /// limit で切り捨てた場合に true。黙って切らないための旗。
    pub truncated: bool,
}

fn num(s: &str) -> f64 {
    s.trim().replace(',', "").parse::<f64>().unwrap_or(0.0)
}

/// 絞り込み → （指定があれば）集計 して返す。
pub fn query(data: &SheetData, q: &SheetQuery) -> SheetResponse {
    let started = Instant::now();

    // --- 絞り込み ---
    let filters: Vec<(usize, &str)> = q
        .filter
        .iter()
        .filter(|(_, v)| !v.is_empty())
        .filter_map(|(k, v)| data.col(k).map(|i| (i, v.as_str())))
        .collect();

    let matched: Vec<&Vec<Arc<str>>> = data
        .rows
        .iter()
        .filter(|row| {
            filters
                .iter()
                .all(|(i, v)| row.get(*i).map(|c| c.as_ref()) == Some(*v))
        })
        .collect();
    let matched_rows = matched.len();

    // --- 集計しない場合 ---
    if q.group_by.is_empty() {
        let limit = q.limit.unwrap_or(usize::MAX);
        let truncated = matched_rows > limit;
        let rows = matched
            .iter()
            .take(limit)
            .map(|r| r.iter().map(|c| c.to_string()).collect())
            .collect();
        return SheetResponse {
            header: data.header.clone(),
            rows,
            matched_rows,
            total_rows: data.rows.len(),
            from_cache: true,
            elapsed_ms: started.elapsed().as_millis(),
            truncated,
        };
    }

    // --- 集計する場合 ---
    let gi: Vec<usize> = q.group_by.iter().filter_map(|c| data.col(c)).collect();
    let si: Vec<usize> = q.sum.iter().filter_map(|c| data.col(c)).collect();

    let mut acc: HashMap<Vec<Arc<str>>, Vec<f64>> = HashMap::new();
    for row in &matched {
        let key: Vec<Arc<str>> = gi
            .iter()
            .map(|i| row.get(*i).cloned().unwrap_or_else(|| Arc::from("")))
            .collect();
        let e = acc.entry(key).or_insert_with(|| vec![0.0; si.len()]);
        for (n, i) in si.iter().enumerate() {
            e[n] += num(row.get(*i).map(|s| s.as_ref()).unwrap_or(""));
        }
    }

    let mut header: Vec<String> = q.group_by.clone();
    header.extend(q.sum.clone());

    let mut rows: Vec<Vec<String>> = acc
        .into_iter()
        .map(|(k, v)| {
            let mut r: Vec<String> = k.iter().map(|s| s.to_string()).collect();
            // 整数で表せる値は小数点を出さない（画面で 12.0 と出ると読みにくい）
            r.extend(v.iter().map(|x| {
                if x.fract() == 0.0 {
                    format!("{}", *x as i64)
                } else {
                    format!("{x}")
                }
            }));
            r
        })
        .collect();
    // 順序を安定させる（HashMap のままだと毎回並びが変わる）
    rows.sort();

    SheetResponse {
        header,
        rows,
        matched_rows,
        total_rows: data.rows.len(),
        from_cache: true,
        elapsed_ms: started.elapsed().as_millis(),
        truncated: false,
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn sample() -> SheetData {
        let header = vec![
            "owner_id".to_string(),
            "prefecture".to_string(),
            "dial".to_string(),
            "apo".to_string(),
        ];
        let mk = |o: &str, p: &str, d: &str, a: &str| -> Vec<Arc<str>> {
            vec![Arc::from(o), Arc::from(p), Arc::from(d), Arc::from(a)]
        };
        SheetData {
            header,
            rows: vec![
                mk("1", "東京都", "100", "2"),
                mk("1", "東京都", "50", "1"),
                mk("2", "大阪府", "30", "0"),
            ],
            fetched_at: Instant::now(),
        }
    }

    #[test]
    fn 列は名前で引ける() {
        let d = sample();
        assert_eq!(d.col("prefecture"), Some(1));
        assert_eq!(d.col("存在しない列"), None, "無い列は None（位置で決め打ちしない）");
    }

    #[test]
    fn 絞り込みができる() {
        let d = sample();
        let mut f = HashMap::new();
        f.insert("prefecture".to_string(), "東京都".to_string());
        let r = query(&d, &SheetQuery { filter: f, ..Default::default() });
        assert_eq!(r.matched_rows, 2);
        assert_eq!(r.total_rows, 3);
    }

    #[test]
    fn 集計ができる() {
        let d = sample();
        let r = query(
            &d,
            &SheetQuery {
                group_by: vec!["owner_id".into()],
                sum: vec!["dial".into(), "apo".into()],
                ..Default::default()
            },
        );
        assert_eq!(r.rows.len(), 2, "owner ごとに畳まれる");
        let owner1 = r.rows.iter().find(|x| x[0] == "1").unwrap();
        assert_eq!(owner1[1], "150");
        assert_eq!(owner1[2], "3");
    }

    #[test]
    fn 空文字の絞り込みは無視される() {
        // 画面の「全て」に相当。空文字で0件になると使い物にならない
        let d = sample();
        let mut f = HashMap::new();
        f.insert("prefecture".to_string(), String::new());
        let r = query(&d, &SheetQuery { filter: f, ..Default::default() });
        assert_eq!(r.matched_rows, 3);
    }

    #[test]
    fn limitで切ったらtruncatedが立つ() {
        // 黙って切り捨てない（GAS 版で「上位N件のみ」を明示せず誤読させた反省）
        let d = sample();
        let r = query(&d, &SheetQuery { limit: Some(2), ..Default::default() });
        assert_eq!(r.rows.len(), 2);
        assert!(r.truncated);
    }

    #[test]
    fn 集計結果の並びが安定する() {
        let d = sample();
        let q = || SheetQuery {
            group_by: vec!["owner_id".into()],
            sum: vec!["dial".into()],
            ..Default::default()
        };
        let a = query(&d, &q());
        let b = query(&d, &q());
        assert_eq!(a.rows, b.rows, "同じ入力なら同じ並びで返る");
    }

    #[test]
    fn 全シートが列挙されている() {
        // GAS 側 Code.gs の buildSheetResponse_ 呼び出し数と一致すること（2026-08-14 時点 69枚）。
        //
        // 注意: 突き合わせるときの抽出は **改行を跨ぐ呼び出しも拾う** こと。
        //   buildSheetResponse_(
        //     CACHE_KEY_X,
        //     'シート名'
        //   )
        // という書き方が実在し、1行だけを見る正規表現では 65枚に見えて
        // 「コンサル勝ち筋分析 / コンサル行動量_担当日次 / コンサル行動量_担当月次」の
        // 3枚を取りこぼした（実際に一度間違えた）。
        assert_eq!(
            KNOWN_SHEETS.len(),
            69,
            "KNOWN_SHEETS の枚数が変わっている。GAS 側 Code.gs と突き合わせること"
        );
    }

    #[test]
    fn シート名に重複がない() {
        let mut v: Vec<&str> = KNOWN_SHEETS.to_vec();
        v.sort();
        let n = v.len();
        v.dedup();
        assert_eq!(v.len(), n, "KNOWN_SHEETS に重複がある");
    }
}
