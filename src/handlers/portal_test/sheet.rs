//! シートの読み方・書き方の決まり (Google を呼ばない純粋な関数)。
//!
//! 手元の橋渡しサーバー (`scripts/portal_dev/sheet_bridge.py`) と同じ決まり:
//! - 1 行目が見出し。結合した見出しは左端の列の位置にし、結合の幅の中で最初に値のあるマスを読む
//! - 2 行目は記入例なので読まない。応募者は 3 行目から。全部の見出しが空の行は出さない
//! - 画面から書けるのは決めた 6 つの見出しだけ。行は 3 行目以上
//! - 保存の直前にそのマスを読み直し、編集を始めたときの値 (base) と違えば書かない

use std::collections::BTreeMap;

use serde::Serialize;
use serde_json::Value;

/// 読み書きしてよい唯一のシート (社内向けテスト用のダミー)。
pub const ALLOWED_SPREADSHEET_ID: &str = "1m5DRb117MXqCQw4GhJC21spHE6N3lqE0dtqP7zxwLeg";
pub const ALLOWED_SHEET_ID: i64 = 510300204;

/// 見出しの行 (1 始まり)。
pub const HEADER_ROW: usize = 1;
/// 応募者の最初の行 (1 始まり)。2 行目は記入例。
pub const FIRST_APPLICANT_ROW: usize = 3;
/// 画面から書ける見出し。応募者名・フリガナは、外国籍の人などが登録の名前を変えることがあるため書ける。
pub const WRITABLE_HEADERS: &[&str] = &[
    "応募者名",
    "フリガナ",
    "状況",
    "面接予定日",
    "時間",
    "備考",
    "次回アクション",
    "入社予定日",
];

pub fn is_allowed_sheet(spreadsheet_id: &str, sheet_id: i64) -> bool {
    spreadsheet_id == ALLOWED_SPREADSHEET_ID && sheet_id == ALLOWED_SHEET_ID
}

/// 書けないときの理由 (画面にそのまま出す文)。
pub fn check_writable(header: &str, row_number: usize) -> Result<(), &'static str> {
    if !WRITABLE_HEADERS.contains(&header) {
        return Err("この項目は画面から書けません");
    }
    if row_number < FIRST_APPLICANT_ROW {
        return Err("この行には書けません");
    }
    Ok(())
}

/// 保存の直前に読み直した値と base を比べた結果。
#[derive(Debug, PartialEq, Eq)]
pub enum Decision {
    /// 書いてよい
    Write,
    /// シートで先に変わっていた。書かずに今の値を返す
    Conflict(String),
}

pub fn decide(current: &str, base: &str) -> Decision {
    if current == base {
        Decision::Write
    } else {
        Decision::Conflict(current.to_string())
    }
}

/// 0 始まりの列番号 → A1 の列の文字 (0 → A、26 → AA)。
pub fn col_letter(idx: usize) -> String {
    let mut n = idx + 1;
    let mut s = Vec::new();
    while n > 0 {
        let rem = (n - 1) % 26;
        s.push(b'A' + rem as u8);
        n = (n - 1) / 26;
    }
    s.reverse();
    String::from_utf8(s).unwrap_or_default()
}

/// タブ名付きの A1 の範囲 ('名前'!A1)。名前の ' は '' にする。
pub fn a1(title: &str, range: &str) -> String {
    format!("'{}'!{}", title.replace('\'', "''"), range)
}

/// 見出しの名前 → 列の位置 (結合の左端) と、結合の幅。
#[derive(Debug, Default, PartialEq, Eq)]
pub struct HeaderMap {
    pub positions: BTreeMap<String, usize>,
    pub widths: BTreeMap<String, usize>,
}

fn sheet_of(resp: &Value, sheet_id: i64) -> Option<&Value> {
    resp.get("sheets")?
        .as_array()?
        .iter()
        .find(|s| s.pointer("/properties/sheetId").and_then(Value::as_i64) == Some(sheet_id))
}

fn first_row_cells(sheet: &Value) -> &[Value] {
    sheet
        .pointer("/data/0/rowData/0/values")
        .and_then(Value::as_array)
        .map(Vec::as_slice)
        .unwrap_or(&[])
}

fn formatted(cell: &Value) -> &str {
    cell.get("formattedValue")
        .and_then(Value::as_str)
        .unwrap_or("")
}

fn idx(v: &Value, key: &str) -> usize {
    v.get(key).and_then(Value::as_u64).unwrap_or(0) as usize
}

/// spreadsheets.get (includeGridData、見出しの行だけ) の応答から見出しの対応を作る。
/// そのタブが応答に無ければ None。同じ名前の見出しは左のものを使う。
pub fn header_map(resp: &Value, sheet_id: i64) -> Option<HeaderMap> {
    let sheet = sheet_of(resp, sheet_id)?;
    let header_idx = HEADER_ROW - 1;
    let mut spans: BTreeMap<usize, usize> = BTreeMap::new();
    for m in sheet
        .get("merges")
        .and_then(Value::as_array)
        .map(Vec::as_slice)
        .unwrap_or(&[])
    {
        if idx(m, "startRowIndex") <= header_idx && header_idx < idx(m, "endRowIndex") {
            let start = idx(m, "startColumnIndex");
            spans.insert(start, idx(m, "endColumnIndex").saturating_sub(start).max(1));
        }
    }
    let mut out = HeaderMap::default();
    for (i, cell) in first_row_cells(sheet).iter().enumerate() {
        let name = formatted(cell).trim();
        if name.is_empty() || out.positions.contains_key(name) {
            continue;
        }
        out.positions.insert(name.to_string(), i);
        out.widths
            .insert(name.to_string(), spans.get(&i).copied().unwrap_or(1));
    }
    Some(out)
}

#[derive(Debug, Serialize, PartialEq, Eq)]
#[serde(rename_all = "camelCase")]
pub struct ApplicantRow {
    pub row_number: usize,
    pub values: BTreeMap<String, String>,
}

#[derive(Debug, Default, PartialEq, Eq)]
pub struct SheetRows {
    pub rows: Vec<ApplicantRow>,
    /// プルダウン (入力規則の ONE_OF_LIST) の選択肢。見出しごと、出てきた順、重複なし。
    pub options: BTreeMap<String, Vec<String>>,
}

/// spreadsheets.get (includeGridData、3 行目から) の応答から応募者の行を作る。
pub fn extract_rows(resp: &Value, sheet_id: i64, headers: &HeaderMap) -> SheetRows {
    let mut out = SheetRows::default();
    let Some(sheet) = sheet_of(resp, sheet_id) else {
        return out;
    };
    let data = sheet.pointer("/data/0");
    let start = data
        .and_then(|d| d.get("startRow"))
        .and_then(Value::as_u64)
        .map(|v| v as usize)
        .unwrap_or(FIRST_APPLICANT_ROW - 1);
    let row_data = data
        .and_then(|d| d.get("rowData"))
        .and_then(Value::as_array)
        .map(Vec::as_slice)
        .unwrap_or(&[]);
    for (ri, rd) in row_data.iter().enumerate() {
        let cells = rd
            .get("values")
            .and_then(Value::as_array)
            .map(Vec::as_slice)
            .unwrap_or(&[]);
        let mut values = BTreeMap::new();
        for (name, &pos) in &headers.positions {
            let width = headers.widths.get(name).copied().unwrap_or(1);
            let v = (pos..pos + width)
                .filter_map(|k| cells.get(k))
                .map(formatted)
                .find(|t| !t.is_empty())
                .unwrap_or("");
            values.insert(name.clone(), v.to_string());
            if let Some(cond) = cells
                .get(pos)
                .and_then(|c| c.pointer("/dataValidation/condition"))
            {
                if cond.get("type").and_then(Value::as_str) == Some("ONE_OF_LIST") {
                    let list = out.options.entry(name.clone()).or_default();
                    for x in cond
                        .get("values")
                        .and_then(Value::as_array)
                        .map(Vec::as_slice)
                        .unwrap_or(&[])
                    {
                        let u = x
                            .get("userEnteredValue")
                            .and_then(Value::as_str)
                            .unwrap_or("")
                            .to_string();
                        if !list.contains(&u) {
                            list.push(u);
                        }
                    }
                }
            }
        }
        // 記入例より下の空の行 (書式だけの行) は出さない
        if values.values().any(|v| !v.trim().is_empty()) {
            out.rows.push(ApplicantRow {
                row_number: start + ri + 1,
                values,
            });
        }
    }
    out
}

/// spreadsheets.get (sheets.properties) の応答から、タブの番号 → タブ名。
pub fn tab_title(resp: &Value, sheet_id: i64) -> Option<String> {
    resp.get("sheets")?.as_array()?.iter().find_map(|s| {
        let p = s.get("properties")?;
        (p.get("sheetId")?.as_i64()? == sheet_id).then(|| {
            p.get("title")
                .and_then(Value::as_str)
                .unwrap_or("")
                .to_string()
        })
    })
}

/// values.get (1 マス) の応答の値。空なら "" 。
pub fn single_value(resp: &Value) -> String {
    resp.pointer("/values/0/0")
        .and_then(Value::as_str)
        .unwrap_or("")
        .to_string()
}

#[cfg(test)]
mod tests {
    use super::*;
    use serde_json::json;

    const GID: i64 = ALLOWED_SHEET_ID;

    fn header_resp() -> Value {
        // 見出し: A=応募日, B-C=氏名 (結合), D=状況, E=(空), F=備考, G=状況 (重複)
        json!({"sheets": [
            {"properties": {"sheetId": 1}, "data": [{"rowData": [{"values": [{"formattedValue": "別のタブ"}]}]}]},
            {"properties": {"sheetId": GID},
             "merges": [
                {"startRowIndex": 0, "endRowIndex": 1, "startColumnIndex": 1, "endColumnIndex": 3},
                {"startRowIndex": 4, "endRowIndex": 6, "startColumnIndex": 3, "endColumnIndex": 6}
             ],
             "data": [{"rowData": [{"values": [
                {"formattedValue": "応募日"},
                {"formattedValue": " 氏名 "},
                {},
                {"formattedValue": "状況"},
                {"formattedValue": ""},
                {"formattedValue": "備考"},
                {"formattedValue": "状況"}
             ]}]}]}
        ]})
    }

    #[test]
    fn 見出しは結合の左端と幅で対応付ける() {
        let h = header_map(&header_resp(), GID).unwrap();
        let pos: Vec<(&str, usize)> = h.positions.iter().map(|(k, v)| (k.as_str(), *v)).collect();
        assert_eq!(
            pos,
            vec![("備考", 5), ("応募日", 0), ("氏名", 1), ("状況", 3)]
        );
        assert_eq!(h.widths["氏名"], 2);
        assert_eq!(h.widths["状況"], 1, "見出しの行より下の結合は数えない");
        assert_eq!(h.widths["応募日"], 1);
    }

    #[test]
    fn 見出しのタブが無ければなし() {
        assert_eq!(header_map(&header_resp(), 999), None);
    }

    fn rows_resp() -> Value {
        json!({"sheets": [{"properties": {"sheetId": GID}, "data": [{"startRow": 2, "rowData": [
            {"values": [
                {"formattedValue": "2026/10/01"},
                {},
                {"formattedValue": "テスト 一郎"},
                {"formattedValue": "面接調整中", "dataValidation": {"condition": {"type": "ONE_OF_LIST", "values": [
                    {"userEnteredValue": "未対応"}, {"userEnteredValue": "面接調整中"}, {"userEnteredValue": "未対応"}
                ]}}},
                {},
                {"formattedValue": "メモ1"}
            ]},
            {"values": [{}, {}, {}, {"dataValidation": {"condition": {"type": "ONE_OF_LIST", "values": [
                {"userEnteredValue": "不採用"}
            ]}}}]},
            {"values": [{"formattedValue": "2026/10/03"}, {"formattedValue": "テスト 二郎"}]}
        ]}]}]})
    }

    #[test]
    fn 行は3行目からで空の行は出さず結合の中の値を拾う() {
        let h = header_map(&header_resp(), GID).unwrap();
        let r = extract_rows(&rows_resp(), GID, &h);
        assert_eq!(r.rows.len(), 2);
        assert_eq!(r.rows[0].row_number, 3);
        assert_eq!(r.rows[0].values["氏名"], "テスト 一郎", "結合の 2 列目の値");
        assert_eq!(r.rows[0].values["状況"], "面接調整中");
        assert_eq!(r.rows[0].values["備考"], "メモ1");
        assert_eq!(
            r.rows[1].row_number, 5,
            "空の 4 行目は飛ばすが番号はずらさない"
        );
        assert_eq!(r.rows[1].values["氏名"], "テスト 二郎");
        assert_eq!(r.rows[1].values["状況"], "");
        assert_eq!(r.options["状況"], vec!["未対応", "面接調整中", "不採用"]);
    }

    #[test]
    fn 書ける見出しと行() {
        assert_eq!(check_writable("状況", 3), Ok(()));
        assert_eq!(check_writable("入社予定日", 120), Ok(()));
        assert_eq!(check_writable("応募者名", 3), Ok(()));
        assert_eq!(check_writable("フリガナ", 40), Ok(()));
        assert_eq!(
            check_writable("氏名", 3),
            Err("この項目は画面から書けません")
        );
        assert_eq!(
            check_writable("電話番号", 10),
            Err("この項目は画面から書けません")
        );
        assert_eq!(
            check_writable("状況", 2),
            Err("この行には書けません"),
            "記入例の行"
        );
        assert_eq!(
            check_writable("状況", 1),
            Err("この行には書けません"),
            "見出しの行"
        );
    }

    #[test]
    fn 競合の判定() {
        assert_eq!(decide("面接調整中", "面接調整中"), Decision::Write);
        assert_eq!(decide("", ""), Decision::Write);
        assert_eq!(
            decide("不採用", "面接調整中"),
            Decision::Conflict("不採用".into())
        );
        assert_eq!(decide("", "メモ"), Decision::Conflict(String::new()));
    }

    #[test]
    fn 読み書きできるのは決めた1枚だけ() {
        assert!(is_allowed_sheet(ALLOWED_SPREADSHEET_ID, ALLOWED_SHEET_ID));
        assert!(!is_allowed_sheet(ALLOWED_SPREADSHEET_ID, 0));
        assert!(!is_allowed_sheet("other", ALLOWED_SHEET_ID));
    }

    #[test]
    fn 列の文字とa1() {
        assert_eq!(col_letter(0), "A");
        assert_eq!(col_letter(25), "Z");
        assert_eq!(col_letter(26), "AA");
        assert_eq!(col_letter(701), "ZZ");
        assert_eq!(col_letter(702), "AAA");
        assert_eq!(a1("応募者'一覧", "D3"), "'応募者''一覧'!D3");
    }

    #[test]
    fn タブ名と1マスの値() {
        let r = json!({"sheets": [{"properties": {"sheetId": 1, "title": "x"}}, {"properties": {"sheetId": GID, "title": "応募者"}}]});
        assert_eq!(tab_title(&r, GID).as_deref(), Some("応募者"));
        assert_eq!(tab_title(&r, 7), None);
        assert_eq!(
            single_value(&json!({"values": [["面接調整中"]]})),
            "面接調整中"
        );
        assert_eq!(single_value(&json!({"range": "A1"})), "");
    }
}
