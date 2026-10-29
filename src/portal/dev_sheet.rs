//! 手元の確認用 (debug ビルドの `PORTAL_DEV_MEMORY=1` だけ): 顧客のシートの代わりに架空の表を返す。
//! 名前・電話番号・メールアドレスはすべて架空 (example.com・090-0000-00xx)。
//! タブは 2 つ (今年の応募者と、去年の応募者)。範囲ごとの読み書きをメモリに保持する。
use super::sheets::{ApiFuture, GoogleError, SheetCell, SheetCells, SheetsApi, TabInfo};
use super::store::{ColumnGroup, ColumnKind, ColumnSetting, SheetReg};
use serde_json::json;
use std::collections::BTreeMap;
use std::sync::{
    atomic::{AtomicU64, Ordering},
    Mutex,
};

pub struct DevSheet {
    grids: Mutex<BTreeMap<String, Vec<Vec<String>>>>,
    revision: AtomicU64,
}

impl Default for DevSheet {
    fn default() -> Self {
        Self {
            grids: Mutex::new(BTreeMap::from([
                (TAB_NOW.to_string(), grid(&NOW_PEOPLE, 1)),
                (TAB_PAST.to_string(), grid(&PAST_PEOPLE, 101)),
            ])),
            revision: AtomicU64::new(0),
        }
    }
}

fn bounds(range: &str) -> (usize, Option<usize>, usize, usize) {
    let cells = range.rsplit_once('!').map(|x| x.1).unwrap_or(range);
    let (first, last) = cells.split_once(':').unwrap_or((cells, cells));
    let part = |s: &str| {
        let letters: String = s.chars().take_while(|c| c.is_ascii_uppercase()).collect();
        let col = letters
            .bytes()
            .fold(0usize, |n, b| n * 26 + (b - b'A' + 1) as usize)
            .saturating_sub(1);
        (
            s[letters.len()..]
                .parse::<usize>()
                .ok()
                .map(|n| n.saturating_sub(1)),
            col,
        )
    };
    let (r, c) = part(first);
    let (last_r, last_c) = part(last);
    (r.unwrap_or(0), last_r, c, last_c)
}

impl DevSheet {
    fn read_range(&self, range: &str) -> Vec<Vec<String>> {
        let g = self.grids.lock().unwrap_or_else(|e| e.into_inner());
        let Some(rows) = g.get(&tab_of(range)) else {
            return Vec::new();
        };
        let (first, last, start_col, end_col) = bounds(range);
        let end = last.map_or(rows.len(), |n| (n + 1).min(rows.len()));
        (first..end)
            .map(|r| {
                rows[r]
                    .iter()
                    .skip(start_col)
                    .take(end_col - start_col + 1)
                    .cloned()
                    .collect()
            })
            .collect()
    }
}

/// 手元の確認で登録するスプレッドシートの番号 (架空)
pub const DEV_SPREADSHEET: &str = "dev-spreadsheet-000000000000";
pub const TAB_NOW: &str = "応募者情報管理";
pub const TAB_PAST: &str = "2025年度";
const TAB_NOW_ID: i64 = 0;
const TAB_PAST_ID: i64 = 2;
/// Drive の最終更新時刻の基準 (架空。保存ごとの連番を末尾に足す)
const MODIFIED: &str = "2026-10-26T00:00:00.000Z";

const HEADERS: [&str; 12] = [
    "応募発生日",
    "応募媒体",
    "応募者名",
    "フリガナ",
    "電話番号",
    "メールアドレス",
    "職種",
    "状況",
    "面接予定日",
    "時間",
    "備考",
    "社内メモ",
];

fn row(cells: &[&str]) -> Vec<String> {
    cells.iter().map(|s| s.to_string()).collect()
}

/// (応募日, 媒体, 名前, フリガナ, 職種, 状況, 面接日, 時間, 備考)
type Person = (
    &'static str,
    &'static str,
    &'static str,
    &'static str,
    &'static str,
    &'static str,
    &'static str,
    &'static str,
    &'static str,
);

const NOW_PEOPLE: [Person; 12] = [
    (
        "2026/10/08",
        "indeed",
        "佐藤 一郎",
        "サトウ イチロウ",
        "ドライバー",
        "採用",
        "2026/10/14",
        "10:00",
        "11/1 から勤務",
    ),
    (
        "2026/10/10",
        "engage",
        "伊藤 美咲",
        "イトウ ミサキ",
        "倉庫作業",
        "不採用",
        "2026/10/16",
        "14:00",
        "",
    ),
    (
        "2026/10/12",
        "求人ボックス",
        "渡辺 健太",
        "ワタナベ ケンタ",
        "ドライバー",
        "辞退",
        "",
        "",
        "他社に決まったとのこと",
    ),
    (
        "2026/10/14",
        "indeed",
        "中村 由美",
        "ナカムラ ユミ",
        "事務",
        "内定",
        "2026/10/21",
        "13:00",
        "入社日を調整中",
    ),
    (
        "2026/10/15",
        "engage",
        "小林 大輔",
        "コバヤシ ダイスケ",
        "倉庫作業",
        "面接済み",
        "2026/10/23",
        "11:00",
        "",
    ),
    (
        "2026/10/17",
        "indeed",
        "加藤 さくら",
        "カトウ サクラ",
        "事務",
        "面接調整中",
        "",
        "",
        "平日午前は連絡がつきやすい",
    ),
    (
        "2026/10/18",
        "airwork",
        "吉田 翔",
        "ヨシダ ショウ",
        "ドライバー",
        "面接予定",
        "2026/10/27",
        "15:00",
        "",
    ),
    (
        "2026/10/20",
        "engage",
        "山田 花子",
        "ヤマダ ハナコ",
        "ドライバー",
        "面接予定",
        "2026/10/28",
        "11:00",
        "中型免許あり",
    ),
    (
        "2026/10/21",
        "indeed",
        "鈴木 次郎",
        "スズキ ジロウ",
        "倉庫作業",
        "未連絡",
        "",
        "",
        "夕方以降に連絡",
    ),
    (
        "2026/10/22",
        "求人ボックス",
        "高橋 三郎",
        "タカハシ サブロウ",
        "ドライバー",
        "応募者連絡待ち",
        "",
        "",
        "",
    ),
    (
        "2026/10/24",
        "indeed",
        "松本 愛",
        "マツモト アイ",
        "事務",
        "未連絡",
        "",
        "",
        "",
    ),
    (
        "2026/10/25",
        "airwork",
        "井上 拓也",
        "イノウエ タクヤ",
        "倉庫作業",
        "未連絡",
        "",
        "",
        "希望の勤務時間: 9時〜17時\n土日は休みたい",
    ),
];

const PAST_PEOPLE: [Person; 5] = [
    (
        "2025/11/04",
        "indeed",
        "木村 直樹",
        "キムラ ナオキ",
        "ドライバー",
        "採用",
        "2025/11/10",
        "10:00",
        "",
    ),
    (
        "2025/12/02",
        "engage",
        "林 麻衣",
        "ハヤシ マイ",
        "事務",
        "辞退",
        "",
        "",
        "",
    ),
    (
        "2026/01/15",
        "indeed",
        "清水 亮",
        "シミズ リョウ",
        "倉庫作業",
        "不採用",
        "2026/01/22",
        "13:00",
        "",
    ),
    (
        "2026/02/20",
        "求人ボックス",
        "山口 真由",
        "ヤマグチ マユ",
        "事務",
        "採用",
        "2026/02/27",
        "11:00",
        "4/1 入社",
    ),
    (
        "2026/03/09",
        "engage",
        "森 健一",
        "モリ ケンイチ",
        "ドライバー",
        "不採用",
        "2026/03/16",
        "15:00",
        "",
    ),
];

fn grid(people: &[Person], phone_from: usize) -> Vec<Vec<String>> {
    let mut g = vec![
        row(&HEADERS),
        row(&[
            "記入例）2026/1/20",
            "indeed",
            "記入例 太郎",
            "キニュウレイ タロウ",
            "090-0000-0000",
            "example@example.com",
            "ドライバー",
            "未連絡",
        ]),
    ];
    for (i, p) in people.iter().enumerate() {
        let n = phone_from + i;
        let phone = format!("090-0000-{n:04}");
        let mail = format!("sample{n}@example.com");
        g.push(row(&[
            p.0,
            p.1,
            p.2,
            p.3,
            &phone,
            &mail,
            p.4,
            p.5,
            p.6,
            p.7,
            p.8,
            "社内だけのメモ（画面には出さない）",
        ]));
    }
    g
}

/// 範囲 `'タブ'!A1:ZZ` のタブ名
fn tab_of(range: &str) -> String {
    range
        .rsplit_once('!')
        .map(|(t, _)| t.trim_matches('\'').replace("''", "'"))
        .unwrap_or_default()
}

/// 手元の確認で顧客「dev」に登録するシート 2 枚と、その項目の設定
pub fn dev_registrations(
    customer_id: &str,
    now: chrono::DateTime<chrono::Utc>,
) -> Vec<(SheetReg, Vec<ColumnSetting>)> {
    [
        ("dev-sheet-now", TAB_NOW_ID, TAB_NOW, "応募者一覧"),
        ("dev-sheet-past", TAB_PAST_ID, TAB_PAST, "2025年度"),
    ]
    .into_iter()
    .map(|(id, sheet_id, tab, label)| {
        let reg = SheetReg {
            id: id.into(),
            customer_id: customer_id.into(),
            spreadsheet_id: DEV_SPREADSHEET.into(),
            sheet_id,
            tab_title: tab.into(),
            label: label.into(),
            header_row: 1,
            example_rows: 1,
            customer_can_edit_columns: false,
            disabled: false,
            created_at: now,
            updated_at: now,
        };
        // (見出し, 画面での名前, 見せる, 一覧に出す, 複数行, 種類, まとまり)
        let cols: [(&str, &str, bool, bool, bool, ColumnKind, ColumnGroup); 12] = [
            (
                "応募者名",
                "",
                true,
                true,
                false,
                ColumnKind::Text,
                ColumnGroup::Summary,
            ),
            (
                "状況",
                "",
                true,
                true,
                false,
                ColumnKind::Choice,
                ColumnGroup::Summary,
            ),
            (
                "応募発生日",
                "応募日",
                true,
                true,
                false,
                ColumnKind::Date,
                ColumnGroup::Summary,
            ),
            (
                "応募媒体",
                "",
                true,
                true,
                false,
                ColumnKind::Text,
                ColumnGroup::Summary,
            ),
            (
                "職種",
                "",
                true,
                true,
                false,
                ColumnKind::Text,
                ColumnGroup::Summary,
            ),
            (
                "フリガナ",
                "",
                true,
                false,
                false,
                ColumnKind::Text,
                ColumnGroup::Summary,
            ),
            (
                "電話番号",
                "",
                true,
                false,
                false,
                ColumnKind::Phone,
                ColumnGroup::Summary,
            ),
            (
                "メールアドレス",
                "",
                true,
                false,
                false,
                ColumnKind::Email,
                ColumnGroup::Summary,
            ),
            (
                "面接予定日",
                "面接日",
                true,
                true,
                false,
                ColumnKind::Date,
                ColumnGroup::Customer,
            ),
            (
                "時間",
                "面接の時間",
                true,
                false,
                false,
                ColumnKind::Text,
                ColumnGroup::Customer,
            ),
            (
                "備考",
                "",
                true,
                false,
                true,
                ColumnKind::Text,
                ColumnGroup::Customer,
            ),
            (
                "社内メモ",
                "",
                false,
                false,
                true,
                ColumnKind::Text,
                ColumnGroup::Other,
            ),
        ];
        let settings = cols
            .into_iter()
            .enumerate()
            .map(
                |(i, (header, label, visible, in_list, multiline, kind, group))| ColumnSetting {
                    sheet_reg_id: id.into(),
                    header: header.into(),
                    label: label.into(),
                    visible,
                    writable: matches!(
                        header,
                        "応募者名" | "フリガナ" | "状況" | "面接予定日" | "時間" | "備考"
                    ),
                    in_list,
                    multiline,
                    kind,
                    group,
                    order: i as u32,
                },
            )
            .collect();
        (reg, settings)
    })
    .collect()
}

impl SheetsApi for DevSheet {
    fn client_email(&self) -> &str {
        "portal-sheets@example-project.iam.gserviceaccount.com"
    }
    fn tabs<'a>(&'a self, spreadsheet: &'a str) -> ApiFuture<'a, Vec<TabInfo>> {
        Box::pin(async move {
            if spreadsheet != DEV_SPREADSHEET {
                return Err(GoogleError::Status(404));
            }
            Ok(vec![
                TabInfo {
                    sheet_id: TAB_NOW_ID,
                    title: TAB_NOW.into(),
                },
                TabInfo {
                    sheet_id: TAB_PAST_ID,
                    title: TAB_PAST.into(),
                },
                TabInfo {
                    sheet_id: 1_234_567,
                    title: "集計（社内用）".into(),
                },
            ])
        })
    }
    fn cells<'a>(
        &'a self,
        spreadsheet: &'a str,
        ranges: &'a [String],
    ) -> ApiFuture<'a, Vec<SheetCells>> {
        Box::pin(async move {
            if spreadsheet != DEV_SPREADSHEET {
                return Err(GoogleError::Status(404));
            }
            Ok(ranges.iter().map(|range| {
                let (first,_,start_col,_) = bounds(range);
                let rows = self.read_range(range).into_iter().enumerate().map(|(i,row)| row.into_iter().enumerate().map(|(j,text)| {
                    let mut cell = SheetCell::literal(text);
                    if first+i > 0 && start_col+j == 7 {
                        cell.validation = Some(json!({"condition":{"type":"ONE_OF_LIST","values": (["未連絡","応募者連絡待ち","面接調整中","面接予定","面接済み","内定","採用","不採用","辞退"].into_iter().map(|v|json!({"userEnteredValue":v})).collect::<Vec<_>>())}}));
                    }
                    cell
                }).collect()).collect();
                SheetCells { rows }
            }).collect())
        })
    }
    fn add_tabs<'a>(&'a self, _s: &'a str, _t: &'a [String]) -> ApiFuture<'a, ()> {
        Box::pin(async { Err(GoogleError::Status(501)) })
    }
    fn batch_get<'a>(
        &'a self,
        _spreadsheet: &'a str,
        ranges: &'a [String],
    ) -> ApiFuture<'a, Vec<Vec<Vec<String>>>> {
        Box::pin(async move {
            if _spreadsheet != DEV_SPREADSHEET {
                return Err(GoogleError::Status(404));
            }
            Ok(ranges.iter().map(|r| self.read_range(r)).collect())
        })
    }
    fn batch_update<'a>(
        &'a self,
        _s: &'a str,
        data: &'a [(String, Vec<Vec<String>>)],
    ) -> ApiFuture<'a, ()> {
        Box::pin(async move {
            if _s != DEV_SPREADSHEET {
                return Err(GoogleError::Status(404));
            }
            let mut grids = self.grids.lock().unwrap_or_else(|e| e.into_inner());
            for (range, values) in data {
                let (first, _, start_col, _) = bounds(range);
                let rows = grids
                    .get_mut(&tab_of(range))
                    .ok_or(GoogleError::Status(404))?;
                for (i, row) in values.iter().enumerate() {
                    if rows.len() <= first + i {
                        rows.resize(first + i + 1, Vec::new());
                    }
                    if rows[first + i].len() < start_col + row.len() {
                        rows[first + i].resize(start_col + row.len(), String::new());
                    }
                    for (j, v) in row.iter().enumerate() {
                        rows[first + i][start_col + j] = v.clone();
                    }
                }
            }
            self.revision.fetch_add(1, Ordering::SeqCst);
            Ok(())
        })
    }
    fn append<'a>(
        &'a self,
        _s: &'a str,
        _r: &'a str,
        _rows: &'a [Vec<String>],
    ) -> ApiFuture<'a, ()> {
        Box::pin(async { Err(GoogleError::Status(501)) })
    }
    fn modified_time<'a>(&'a self, _s: &'a str) -> ApiFuture<'a, String> {
        Box::pin(async move {
            if _s != DEV_SPREADSHEET {
                return Err(GoogleError::Status(404));
            }
            Ok(format!(
                "{MODIFIED}|{}",
                self.revision.load(Ordering::SeqCst)
            ))
        })
    }
}
