//! 手元の確認用 (debug ビルドの `PORTAL_DEV_MEMORY=1` だけ): 顧客のシートの代わりに架空の表を返す。
//! 名前・電話番号・メールアドレスはすべて架空 (example.com・090-0000-000x)。
use super::sheets::{ApiFuture, SheetsApi, TabInfo};

pub struct DevSheet;

fn row(cells: &[&str]) -> Vec<String> {
    cells.iter().map(|s| s.to_string()).collect()
}

impl SheetsApi for DevSheet {
    fn client_email(&self) -> &str {
        "portal-sheets@example-project.iam.gserviceaccount.com"
    }
    fn tabs<'a>(&'a self, _spreadsheet: &'a str) -> ApiFuture<'a, Vec<TabInfo>> {
        Box::pin(async {
            Ok(vec![
                TabInfo {
                    sheet_id: 0,
                    title: "応募者情報管理".into(),
                },
                TabInfo {
                    sheet_id: 1_234_567,
                    title: "集計（社内用）".into(),
                },
            ])
        })
    }
    fn add_tabs<'a>(&'a self, _s: &'a str, _t: &'a [String]) -> ApiFuture<'a, ()> {
        Box::pin(async { Ok(()) })
    }
    fn batch_get<'a>(
        &'a self,
        _spreadsheet: &'a str,
        ranges: &'a [String],
    ) -> ApiFuture<'a, Vec<Vec<Vec<String>>>> {
        Box::pin(async move {
            let grid = vec![
                row(&[
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
                ]),
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
                row(&[
                    "2026/10/20",
                    "engage",
                    "山田 花子",
                    "ヤマダ ハナコ",
                    "090-0000-0001",
                    "sample1@example.com",
                    "ドライバー",
                    "面接調整中",
                    "2026/10/28",
                    "11:00",
                ]),
                row(&[
                    "2026/10/21",
                    "indeed",
                    "鈴木 次郎",
                    "スズキ ジロウ",
                    "090-0000-0002",
                    "sample2@example.com",
                    "倉庫作業",
                    "未連絡",
                    "",
                    "",
                    "夕方以降に連絡",
                ]),
                row(&[
                    "2026/10/22",
                    "求人ボックス",
                    "高橋 三郎",
                    "タカハシ サブロウ",
                    "090-0000-0003",
                    "",
                    "ドライバー",
                    "応募者連絡待ち",
                ]),
            ];
            Ok(ranges.iter().map(|_| grid.clone()).collect())
        })
    }
    fn batch_update<'a>(
        &'a self,
        _s: &'a str,
        _d: &'a [(String, Vec<Vec<String>>)],
    ) -> ApiFuture<'a, ()> {
        Box::pin(async { Ok(()) })
    }
    fn append<'a>(
        &'a self,
        _s: &'a str,
        _r: &'a str,
        _rows: &'a [Vec<String>],
    ) -> ApiFuture<'a, ()> {
        Box::pin(async { Ok(()) })
    }
}
