//! テスト用の偽の Google Sheets。1 つのスプレッドシートのタブを文字の表で持つ。
//! 範囲は `'タブ'!A1:G` / `'タブ'!A5:G5` / `'タブ'!A2:B` の形だけを解釈する (アプリが使う形)。
use std::collections::BTreeMap;
use std::sync::Mutex;

use super::sheets::{ApiFuture, GoogleError, SheetsApi, TabInfo};

/// (タブの番号, 行)
pub type Tab = (i64, Vec<Vec<String>>);

#[derive(Default)]
pub struct FakeSheets {
    /// タブの名前 → タブ
    pub tabs: Mutex<BTreeMap<String, Tab>>,
    /// 読み取りを全部失敗させる
    pub fail_reads: Mutex<Option<GoogleError>>,
    /// 書き込みを全部失敗させる
    pub fail_writes: Mutex<Option<GoogleError>>,
    pub calls: Mutex<Vec<String>>,
}

fn col_index(letters: &str) -> usize {
    letters
        .bytes()
        .fold(0usize, |n, b| n * 26 + (b - b'A' + 1) as usize)
        - 1
}

/// (タブ, 開始の行 0 始まり, 終わりの行 (含む) or None, 開始の列, 終わりの列)
fn parse_range(r: &str) -> (String, usize, Option<usize>, usize, usize) {
    let (title, cells) = r.rsplit_once('!').expect("タブ付きの範囲");
    let title = title
        .trim_start_matches('\'')
        .trim_end_matches('\'')
        .replace("''", "'");
    let (a, b) = cells.split_once(':').expect("範囲");
    let split = |x: &str| -> (String, Option<usize>) {
        let letters: String = x.chars().take_while(|c| c.is_ascii_uppercase()).collect();
        let digits = &x[letters.len()..];
        (letters, digits.parse().ok())
    };
    let (ac, ar) = split(a);
    let (bc, br) = split(b);
    (
        title,
        ar.unwrap_or(1) - 1,
        br.map(|r| r - 1),
        col_index(&ac),
        col_index(&bc),
    )
}

impl FakeSheets {
    pub fn with_tab(self, title: &str, sheet_id: i64, rows: Vec<Vec<&str>>) -> Self {
        self.tabs.lock().unwrap().insert(
            title.to_string(),
            (
                sheet_id,
                rows.into_iter()
                    .map(|r| r.into_iter().map(str::to_string).collect())
                    .collect(),
            ),
        );
        self
    }

    pub fn rows(&self, title: &str) -> Vec<Vec<String>> {
        self.tabs
            .lock()
            .unwrap()
            .get(title)
            .map(|t| t.1.clone())
            .unwrap_or_default()
    }

    pub fn set_cell(&self, title: &str, row: usize, col: usize, v: &str) {
        let mut g = self.tabs.lock().unwrap();
        let t = &mut g.get_mut(title).unwrap().1;
        if t.len() <= row {
            t.resize(row + 1, Vec::new());
        }
        if t[row].len() <= col {
            t[row].resize(col + 1, String::new());
        }
        t[row][col] = v.to_string();
    }

    /// 全部のタブの全部のマスをつなげた文字 (値が置かれていないことを確かめる用)
    pub fn all_text(&self) -> String {
        self.tabs
            .lock()
            .unwrap()
            .values()
            .flat_map(|(_, rows)| rows.iter().flatten().cloned())
            .collect::<Vec<_>>()
            .join("\n")
    }

    fn read(&self) -> Result<(), GoogleError> {
        match *self.fail_reads.lock().unwrap() {
            Some(e) => Err(e),
            None => Ok(()),
        }
    }
    fn write(&self) -> Result<(), GoogleError> {
        match *self.fail_writes.lock().unwrap() {
            Some(e) => Err(e),
            None => Ok(()),
        }
    }

    fn get_range(&self, r: &str) -> Vec<Vec<String>> {
        let (title, r0, r1, c0, c1) = parse_range(r);
        let g = self.tabs.lock().unwrap();
        let Some((_, rows)) = g.get(&title) else {
            return Vec::new();
        };
        let end = r1.map_or(rows.len(), |e| (e + 1).min(rows.len()));
        let mut out: Vec<Vec<String>> = (r0..end)
            .map(|i| {
                let mut cells: Vec<String> = (c0..=c1)
                    .map(|c| rows[i].get(c).cloned().unwrap_or_default())
                    .collect();
                while cells.last().is_some_and(|c| c.is_empty()) {
                    cells.pop();
                }
                cells
            })
            .collect();
        while out.last().is_some_and(|r| r.is_empty()) {
            out.pop();
        }
        out
    }

    fn put_range(&self, r: &str, values: &[Vec<String>]) {
        let (title, r0, _, c0, _) = parse_range(r);
        let mut g = self.tabs.lock().unwrap();
        let rows = &mut g.get_mut(&title).expect("書く先のタブがある").1;
        for (i, v) in values.iter().enumerate() {
            let ri = r0 + i;
            if rows.len() <= ri {
                rows.resize(ri + 1, Vec::new());
            }
            if rows[ri].len() < c0 + v.len() {
                rows[ri].resize(c0 + v.len(), String::new());
            }
            for (j, x) in v.iter().enumerate() {
                rows[ri][c0 + j] = x.clone();
            }
        }
    }
}

impl SheetsApi for FakeSheets {
    fn client_email(&self) -> &str {
        "reader@test-project.iam.gserviceaccount.com"
    }
    fn tabs<'a>(&'a self, _spreadsheet: &'a str) -> ApiFuture<'a, Vec<TabInfo>> {
        Box::pin(async move {
            self.calls.lock().unwrap().push("tabs".into());
            self.read()?;
            Ok(self
                .tabs
                .lock()
                .unwrap()
                .iter()
                .map(|(t, (id, _))| TabInfo {
                    sheet_id: *id,
                    title: t.clone(),
                })
                .collect())
        })
    }
    fn add_tabs<'a>(&'a self, _s: &'a str, titles: &'a [String]) -> ApiFuture<'a, ()> {
        Box::pin(async move {
            self.calls.lock().unwrap().push("add_tabs".into());
            self.write()?;
            let mut g = self.tabs.lock().unwrap();
            let mut next = g.len() as i64 + 100;
            for t in titles {
                g.entry(t.clone()).or_insert_with(|| {
                    next += 1;
                    (next, Vec::new())
                });
            }
            Ok(())
        })
    }
    fn batch_get<'a>(
        &'a self,
        _s: &'a str,
        ranges: &'a [String],
    ) -> ApiFuture<'a, Vec<Vec<Vec<String>>>> {
        Box::pin(async move {
            self.calls.lock().unwrap().push("batch_get".into());
            self.read()?;
            Ok(ranges.iter().map(|r| self.get_range(r)).collect())
        })
    }
    fn batch_update<'a>(
        &'a self,
        _s: &'a str,
        data: &'a [(String, Vec<Vec<String>>)],
    ) -> ApiFuture<'a, ()> {
        Box::pin(async move {
            self.calls.lock().unwrap().push("batch_update".into());
            self.write()?;
            for (r, v) in data {
                self.put_range(r, v);
            }
            Ok(())
        })
    }
    fn append<'a>(
        &'a self,
        _s: &'a str,
        range: &'a str,
        rows: &'a [Vec<String>],
    ) -> ApiFuture<'a, ()> {
        Box::pin(async move {
            self.calls.lock().unwrap().push("append".into());
            self.write()?;
            let (title, ..) = parse_range(range);
            let mut g = self.tabs.lock().unwrap();
            let t = &mut g.get_mut(&title).expect("追記の先のタブがある").1;
            while t.last().is_some_and(|r| r.iter().all(String::is_empty)) {
                t.pop();
            }
            t.extend(rows.iter().cloned());
            Ok(())
        })
    }
}
