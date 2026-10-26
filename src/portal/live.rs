//! 顧客のシートの読み取りをプロセス内で共有する (計画 4.2・10 章 PR4)。
//!
//! - 更新の確認: Drive の modifiedTime だけを読む。同じスプレッドシートは [`LiveConfig::version_ttl`] の間
//!   1 回にまとめ、同時の確認は 1 本にまとめる (後から来た要求は先の結果を待つ)。
//! - 値の読み取り: values.batchGet。最終更新時刻が前回の読み取りと同じなら読まない。同時に来ても 1 回だけ読む。
//! - タブの名前: spreadsheets.get。[`LiveConfig::tabs_ttl`] の間使い回す (タブは数字の番号で探す)。
//! - 開いていないシートは読まない (要求が来たときだけ読む)。しばらく使われなかった表はメモリから捨てる。
//!
//! 単一インスタンス前提 (台ごとに別のキャッシュを持つ)。セルの値はログに出さない。
use std::collections::HashMap;
use std::sync::{Arc, Mutex};
use std::time::Duration;

use chrono::{DateTime, Utc};
use tokio::sync::Mutex as AsyncMutex;
use tokio::time::Instant;

use super::sheets::{a1, GoogleError, SheetsApi, TabInfo};
use super::store::SheetReg;

#[derive(Debug, Clone)]
pub struct LiveConfig {
    /// 同じスプレッドシートの更新の確認をまとめる時間
    pub version_ttl: Duration,
    /// 確認に失敗したとき、やり直すまでの時間
    pub error_ttl: Duration,
    /// タブの名前を使い回す時間
    pub tabs_ttl: Duration,
    /// これより長く使われなかった表はメモリから捨てる
    pub idle_drop: Duration,
}

impl Default for LiveConfig {
    fn default() -> Self {
        Self {
            version_ttl: Duration::from_secs(20),
            error_ttl: Duration::from_secs(10),
            tabs_ttl: Duration::from_secs(10 * 60),
            idle_drop: Duration::from_secs(15 * 60),
        }
    }
}

/// 読み取りの失敗
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum ReadError {
    /// サービスアカウントにシートが共有されていない (403 / 404)
    NotShared,
    /// そのタブ (番号) がシートに無い
    TabNotFound,
    Google(GoogleError),
}

impl From<GoogleError> for ReadError {
    fn from(e: GoogleError) -> Self {
        match e {
            GoogleError::Status(403 | 404) => ReadError::NotShared,
            other => ReadError::Google(other),
        }
    }
}

/// どのシートのどこを読んだか (登録が変われば読み直す)
#[derive(Debug, Clone, PartialEq, Eq)]
struct Shape {
    spreadsheet: String,
    sheet_id: i64,
    header_row: u32,
}

impl Shape {
    fn of(reg: &SheetReg) -> Self {
        Self {
            spreadsheet: reg.spreadsheet_id.clone(),
            sheet_id: reg.sheet_id,
            header_row: reg.header_row.max(1),
        }
    }
}

/// 読んだ表。`rows[0]` が見出しの行
#[derive(Debug)]
pub struct Grid {
    shape: Shape,
    /// 読む前に確かめた最終更新時刻
    pub modified: String,
    pub rows: Vec<Vec<String>>,
    pub read_at: DateTime<Utc>,
}

/// 表の読み取りの結果
pub enum GridResult {
    /// 最新 (確かめた最終更新時刻で読んだもの)
    Fresh(Arc<Grid>),
    /// 確かめる・読むのに失敗したので、前に読んだものを返す
    Stale(Arc<Grid>),
}

#[derive(Default)]
struct VersionSlot {
    value: Option<(Instant, Result<String, GoogleError>)>,
}

#[derive(Default)]
struct TabsSlot {
    value: Option<(Instant, Vec<TabInfo>)>,
}

#[derive(Default)]
struct GridSlot {
    value: Option<Arc<Grid>>,
}

type Slots<T> = Mutex<HashMap<String, (Arc<AsyncMutex<T>>, Instant)>>;

pub struct LiveSheets {
    api: Arc<dyn SheetsApi>,
    cfg: LiveConfig,
    /// スプレッドシートの番号 → 最終更新時刻
    versions: Slots<VersionSlot>,
    /// スプレッドシートの番号 → タブの一覧
    tabs: Slots<TabsSlot>,
    /// シートの登録の番号 → 読んだ表
    grids: Slots<GridSlot>,
}

fn slot<T: Default>(map: &Slots<T>, key: &str, idle: Duration) -> Arc<AsyncMutex<T>> {
    let now = Instant::now();
    let mut g = map.lock().unwrap_or_else(|e| e.into_inner());
    // しばらく使われていないもの (待っている人がいないもの) を捨てる
    g.retain(|k, (s, used)| {
        k == key || now.duration_since(*used) < idle || Arc::strong_count(s) > 1
    });
    let e = g
        .entry(key.to_string())
        .or_insert_with(|| (Arc::new(AsyncMutex::new(T::default())), now));
    e.1 = now;
    e.0.clone()
}

impl LiveSheets {
    pub fn new(api: Arc<dyn SheetsApi>, cfg: LiveConfig) -> Self {
        Self {
            api,
            cfg,
            versions: Mutex::new(HashMap::new()),
            tabs: Mutex::new(HashMap::new()),
            grids: Mutex::new(HashMap::new()),
        }
    }

    /// スプレッドシートの最終更新時刻。同じスプレッドシートは `version_ttl` の間 1 回だけ Drive に聞く
    pub async fn modified(&self, spreadsheet: &str) -> Result<String, GoogleError> {
        let s = slot(&self.versions, spreadsheet, self.cfg.idle_drop);
        let mut g = s.lock().await;
        if let Some((at, r)) = &g.value {
            let ttl = if r.is_ok() {
                self.cfg.version_ttl
            } else {
                self.cfg.error_ttl
            };
            if at.elapsed() < ttl {
                return r.clone();
            }
        }
        let r = self.api.modified_time(spreadsheet).await;
        g.value = Some((Instant::now(), r.clone()));
        r
    }

    async fn tab_title(&self, shape: &Shape, refresh: bool) -> Result<String, ReadError> {
        let s = slot(&self.tabs, &shape.spreadsheet, self.cfg.idle_drop);
        let mut g = s.lock().await;
        let cached = g
            .value
            .as_ref()
            .filter(|(at, _)| !refresh && at.elapsed() < self.cfg.tabs_ttl)
            .map(|(_, t)| t.clone());
        let tabs = match cached {
            Some(t) => t,
            None => {
                let t = self.api.tabs(&shape.spreadsheet).await?;
                g.value = Some((Instant::now(), t.clone()));
                t
            }
        };
        tabs.into_iter()
            .find(|t| t.sheet_id == shape.sheet_id)
            .map(|t| t.title)
            .ok_or(ReadError::TabNotFound)
    }

    async fn read(&self, shape: &Shape) -> Result<Vec<Vec<String>>, ReadError> {
        let mut refreshed = false;
        loop {
            let title = match self.tab_title(shape, refreshed).await {
                Err(ReadError::TabNotFound) if !refreshed => {
                    // タブを足した直後かもしれないので、一度だけ一覧を取り直す
                    refreshed = true;
                    continue;
                }
                r => r?,
            };
            let ranges = [a1(&title, &format!("A{}:ZZ", shape.header_row))];
            match self.api.batch_get(&shape.spreadsheet, &ranges).await {
                // タブの名前が変わった直後は範囲が読めない。一度だけ一覧を取り直す
                Err(GoogleError::Status(400)) if !refreshed => {
                    refreshed = true;
                }
                Err(e) => return Err(e.into()),
                Ok(mut got) => return Ok(got.pop().unwrap_or_default()),
            }
        }
    }

    /// 登録したシートの表。`modified` は直前に確かめた最終更新時刻 (確かめられなければ None)。
    /// 前回と同じ時刻なら Sheets を読まない。同じシートへの同時の要求は 1 回の読み取りにまとめる
    pub async fn grid(
        &self,
        reg: &SheetReg,
        modified: Option<&str>,
    ) -> Result<GridResult, ReadError> {
        let shape = Shape::of(reg);
        let s = slot(&self.grids, &reg.id, self.cfg.idle_drop);
        let mut g = s.lock().await;
        let cached = g.value.clone().filter(|c| c.shape == shape);
        if let Some(c) = &cached {
            match modified {
                Some(m) if c.modified == m => return Ok(GridResult::Fresh(c.clone())),
                // 確かめられないときは前のものを出す (Sheets も混んでいる・止まっていることが多い)
                None => return Ok(GridResult::Stale(c.clone())),
                Some(_) => {}
            }
        }
        match self.read(&shape).await {
            Ok(rows) => {
                let grid = Arc::new(Grid {
                    shape,
                    modified: modified.unwrap_or("").to_string(),
                    rows,
                    read_at: Utc::now(),
                });
                g.value = Some(grid.clone());
                Ok(GridResult::Fresh(grid))
            }
            Err(e) => match cached {
                Some(c) if !matches!(e, ReadError::NotShared | ReadError::TabNotFound) => {
                    Ok(GridResult::Stale(c))
                }
                _ => Err(e),
            },
        }
    }
}
