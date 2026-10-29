//! 顧客向け応募者画面の Google Sheets の呼び出し。
//!
//! 社内用シート (アプリの情報) と顧客のシート (応募者) で、別のサービスアカウントの鍵を使う
//! (書き込みの回数の枠を分け、片方の鍵が漏れても他方に届かないようにする。計画 4.1・9.1)。
//! どちらも [`SheetsApi`] を通し、テストでは偽物に差し替える。
//! 鍵・トークン・セルの値はログに出さない。失敗は HTTP の状態だけを持つ ([`GoogleError`])。
use std::future::Future;
use std::pin::Pin;
use std::sync::Arc;

use serde_json::{json, Value};

pub use crate::handlers::portal_test::google::GoogleError;
use crate::handlers::portal_test::google::{GoogleSa, DRIVE_API, SHEETS_API};

pub type ApiFuture<'a, T> = Pin<Box<dyn Future<Output = Result<T, GoogleError>> + Send + 'a>>;

/// 社内用シートのスプレッドシート ID
pub const ENV_STORE_SPREADSHEET: &str = "PORTAL_STORE_SPREADSHEET_ID";
/// 社内用シートを読み書きするサービスアカウントの鍵 (base64)
pub const ENV_STORE_KEY: &str = "PORTAL_STORE_SA_KEY_B64";
/// 顧客のシートを読み書きするサービスアカウントの鍵 (base64)
pub const ENV_CUSTOMER_KEY: &str = "PORTAL_GOOGLE_SA_KEY_B64";

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct TabInfo {
    pub sheet_id: i64,
    pub title: String,
}

/// 保存前の照合に使うセル。値と数式・入力規則をサーバー内でだけ持つ。
#[derive(Debug, Clone, Default, PartialEq, Eq, serde::Serialize)]
pub struct SheetCell {
    pub text: String,
    pub entered: Value,
    pub validation: Option<Value>,
}

impl SheetCell {
    pub fn literal(text: String) -> Self {
        Self {
            entered: json!({"stringValue": text}),
            text,
            validation: None,
        }
    }

    pub fn formula(&self) -> bool {
        self.entered.get("formulaValue").is_some()
    }

    /// 未対応の入力規則は自由入力にせず、編集不可として扱う。
    pub fn options(&self) -> Result<Option<Vec<String>>, &'static str> {
        let Some(v) = &self.validation else {
            return Ok(None);
        };
        let Some(condition) = v.get("condition") else {
            return Err("入力規則を確認できません");
        };
        if condition.get("type").and_then(Value::as_str) != Some("ONE_OF_LIST") {
            return Err("この入力規則にはまだ対応していません");
        }
        let Some(values) = condition.get("values").and_then(Value::as_array) else {
            return Err("入力規則を確認できません");
        };
        values
            .iter()
            .map(|v| {
                v.get("userEnteredValue")
                    .and_then(Value::as_str)
                    .map(str::to_string)
                    .ok_or("入力規則を確認できません")
            })
            .collect::<Result<Vec<_>, _>>()
            .map(Some)
    }
}

#[derive(Debug, Clone, Default)]
pub struct SheetCells {
    pub rows: Vec<Vec<SheetCell>>,
}

impl SheetCells {
    pub fn texts(&self) -> Vec<Vec<String>> {
        self.rows
            .iter()
            .map(|r| r.iter().map(|c| c.text.clone()).collect())
            .collect()
    }
}

pub trait SheetsApi: Send + Sync {
    /// シートに編集者として追加してもらうアドレス
    fn client_email(&self) -> &str;
    /// タブの番号と名前 (spreadsheets.get)
    fn tabs<'a>(&'a self, spreadsheet: &'a str) -> ApiFuture<'a, Vec<TabInfo>>;
    /// タブを足す (spreadsheets.batchUpdate の addSheet)
    fn add_tabs<'a>(&'a self, spreadsheet: &'a str, titles: &'a [String]) -> ApiFuture<'a, ()>;
    /// 範囲ごとの値 (values.batchGet、表示どおりの文字)。行末の空のマスは詰まって返る
    fn batch_get<'a>(
        &'a self,
        spreadsheet: &'a str,
        ranges: &'a [String],
    ) -> ApiFuture<'a, Vec<Vec<Vec<String>>>>;
    /// 範囲ごとに書く (values.batchUpdate、RAW: 数式として解釈させない)
    fn batch_update<'a>(
        &'a self,
        spreadsheet: &'a str,
        data: &'a [(String, Vec<Vec<String>>)],
    ) -> ApiFuture<'a, ()>;
    /// 末尾に行を足す (values.append、RAW)
    fn append<'a>(
        &'a self,
        spreadsheet: &'a str,
        range: &'a str,
        rows: &'a [Vec<String>],
    ) -> ApiFuture<'a, ()>;
    /// スプレッドシートの最終更新時刻 (Drive files.get の modifiedTime だけ)。
    /// 中身が変わったかを Sheets を読まずに確かめる (計画 4.2)
    fn modified_time<'a>(&'a self, spreadsheet: &'a str) -> ApiFuture<'a, String>;
    /// 範囲ごとの表示値・数式・入力規則。実装漏れを保存成功と誤認しない。
    fn cells<'a>(
        &'a self,
        _spreadsheet: &'a str,
        _ranges: &'a [String],
    ) -> ApiFuture<'a, Vec<SheetCells>> {
        Box::pin(async { Err(GoogleError::Status(501)) })
    }
}

/// サービスアカウントの鍵で本物の Google を呼ぶ
pub struct GoogleSheetsApi {
    sa: GoogleSa,
}

impl GoogleSheetsApi {
    /// 環境変数の鍵から作る。無い・読めないときは None
    pub fn from_env_var(name: &str) -> Option<Arc<dyn SheetsApi>> {
        GoogleSa::from_env_var(name).map(|sa| Arc::new(Self { sa }) as Arc<dyn SheetsApi>)
    }
}

fn cell_text(v: &Value) -> String {
    match v {
        Value::String(s) => s.clone(),
        Value::Null => String::new(),
        Value::Bool(b) => if *b { "TRUE" } else { "FALSE" }.to_string(),
        other => other.to_string(),
    }
}

fn grid(v: Option<&Value>) -> Vec<Vec<String>> {
    v.and_then(Value::as_array)
        .map(|rows| {
            rows.iter()
                .map(|r| {
                    r.as_array()
                        .map(|cells| cells.iter().map(cell_text).collect())
                        .unwrap_or_default()
                })
                .collect()
        })
        .unwrap_or_default()
}

impl SheetsApi for GoogleSheetsApi {
    fn client_email(&self) -> &str {
        self.sa.client_email()
    }

    fn cells<'a>(
        &'a self,
        spreadsheet: &'a str,
        ranges: &'a [String],
    ) -> ApiFuture<'a, Vec<SheetCells>> {
        Box::pin(async move {
            let url = format!("{SHEETS_API}/{spreadsheet}");
            let mut result = Vec::with_capacity(ranges.len());
            // A single get with several ranges returns GridData blocks in range order.
            let mut query = vec![(
                "fields",
                "sheets(data(rowData(values(formattedValue,userEnteredValue,dataValidation))))",
            )];
            for range in ranges {
                query.push(("ranges", range.as_str()));
            }
            let response = self.sa.get(&url, &query).await?;
            for data in response
                .get("sheets")
                .and_then(Value::as_array)
                .into_iter()
                .flatten()
                .flat_map(|sheet| {
                    sheet
                        .get("data")
                        .and_then(Value::as_array)
                        .into_iter()
                        .flatten()
                })
            {
                let rows = data
                    .get("rowData")
                    .and_then(Value::as_array)
                    .into_iter()
                    .flatten()
                    .map(|row| {
                        row.get("values")
                            .and_then(Value::as_array)
                            .into_iter()
                            .flatten()
                            .map(|cell| SheetCell {
                                text: cell
                                    .get("formattedValue")
                                    .and_then(Value::as_str)
                                    .unwrap_or("")
                                    .to_string(),
                                entered: cell
                                    .get("userEnteredValue")
                                    .cloned()
                                    .unwrap_or(Value::Null),
                                validation: cell.get("dataValidation").cloned(),
                            })
                            .collect()
                    })
                    .collect();
                result.push(SheetCells { rows });
            }
            if result.len() != ranges.len() {
                return Err(GoogleError::Status(502));
            }
            Ok(result)
        })
    }

    fn tabs<'a>(&'a self, spreadsheet: &'a str) -> ApiFuture<'a, Vec<TabInfo>> {
        Box::pin(async move {
            let url = format!("{SHEETS_API}/{spreadsheet}");
            let resp = self
                .sa
                .get(&url, &[("fields", "sheets(properties(sheetId,title))")])
                .await?;
            Ok(resp
                .get("sheets")
                .and_then(Value::as_array)
                .map(|a| {
                    a.iter()
                        .filter_map(|s| {
                            let p = s.get("properties")?;
                            Some(TabInfo {
                                sheet_id: p.get("sheetId")?.as_i64()?,
                                title: p.get("title")?.as_str()?.to_string(),
                            })
                        })
                        .collect()
                })
                .unwrap_or_default())
        })
    }

    fn add_tabs<'a>(&'a self, spreadsheet: &'a str, titles: &'a [String]) -> ApiFuture<'a, ()> {
        Box::pin(async move {
            let url = format!("{SHEETS_API}/{spreadsheet}:batchUpdate");
            let requests: Vec<Value> = titles
                .iter()
                .map(|t| json!({ "addSheet": { "properties": { "title": t } } }))
                .collect();
            self.sa
                .post(&url, &[], &json!({ "requests": requests }))
                .await?;
            Ok(())
        })
    }

    fn batch_get<'a>(
        &'a self,
        spreadsheet: &'a str,
        ranges: &'a [String],
    ) -> ApiFuture<'a, Vec<Vec<Vec<String>>>> {
        Box::pin(async move {
            let url = format!("{SHEETS_API}/{spreadsheet}/values:batchGet");
            let mut query: Vec<(&str, &str)> = vec![
                ("majorDimension", "ROWS"),
                ("valueRenderOption", "FORMATTED_VALUE"),
            ];
            for r in ranges {
                query.push(("ranges", r.as_str()));
            }
            let resp = self.sa.get(&url, &query).await?;
            let mut out: Vec<Vec<Vec<String>>> = resp
                .get("valueRanges")
                .and_then(Value::as_array)
                .map(|a| a.iter().map(|vr| grid(vr.get("values"))).collect())
                .unwrap_or_default();
            out.resize(ranges.len(), Vec::new());
            Ok(out)
        })
    }

    fn batch_update<'a>(
        &'a self,
        spreadsheet: &'a str,
        data: &'a [(String, Vec<Vec<String>>)],
    ) -> ApiFuture<'a, ()> {
        Box::pin(async move {
            let url = format!("{SHEETS_API}/{spreadsheet}/values:batchUpdate");
            let data: Vec<Value> = data
                .iter()
                .map(|(range, values)| json!({ "range": range, "values": values }))
                .collect();
            self.sa
                .post(
                    &url,
                    &[],
                    &json!({ "valueInputOption": "RAW", "data": data }),
                )
                .await?;
            Ok(())
        })
    }

    fn append<'a>(
        &'a self,
        spreadsheet: &'a str,
        range: &'a str,
        rows: &'a [Vec<String>],
    ) -> ApiFuture<'a, ()> {
        Box::pin(async move {
            let url = format!(
                "{SHEETS_API}/{spreadsheet}/values/{}:append",
                urlencoding::encode(range)
            );
            self.sa
                .post(
                    &url,
                    &[
                        ("valueInputOption", "RAW"),
                        ("insertDataOption", "INSERT_ROWS"),
                    ],
                    &json!({ "values": rows }),
                )
                .await?;
            Ok(())
        })
    }

    fn modified_time<'a>(&'a self, spreadsheet: &'a str) -> ApiFuture<'a, String> {
        Box::pin(async move {
            let url = format!("{DRIVE_API}/{}", urlencoding::encode(spreadsheet));
            let resp = self
                .sa
                .get(
                    &url,
                    &[("fields", "modifiedTime"), ("supportsAllDrives", "true")],
                )
                .await?;
            Ok(resp
                .get("modifiedTime")
                .and_then(Value::as_str)
                .unwrap_or("")
                .to_string())
        })
    }
}

// ---- A1 の表記 ----

/// 0 始まりの列番号 → 列の文字 (0 → A、26 → AA)
pub fn col_letter(idx: usize) -> String {
    crate::handlers::portal_test::sheet::col_letter(idx)
}

/// タブ名付きの範囲 ('名前'!A1)。名前の ' は '' にする
pub fn a1(title: &str, range: &str) -> String {
    crate::handlers::portal_test::sheet::a1(title, range)
}

// ---- 顧客のシートの URL ----

/// スプレッドシートの URL から (spreadsheetId, タブの番号)。
/// `https://docs.google.com/spreadsheets/d/<id>/...` だけを受け付け、それ以外は None。
pub fn parse_sheet_url(raw: &str) -> Option<(String, Option<i64>)> {
    let url = raw.trim();
    if url.len() > 2048 {
        return None;
    }
    let rest = url.strip_prefix("https://docs.google.com/spreadsheets/d/")?;
    let id: String = rest
        .chars()
        .take_while(|c| c.is_ascii_alphanumeric() || *c == '-' || *c == '_')
        .collect();
    if !(20..=128).contains(&id.len()) {
        return None;
    }
    let after = &rest[id.len()..];
    if !(after.is_empty()
        || after.starts_with('/')
        || after.starts_with('?')
        || after.starts_with('#'))
    {
        return None;
    }
    let gid = after.find("gid=").and_then(|i| {
        let digits: String = after[i + 4..]
            .chars()
            .take_while(|c| c.is_ascii_digit())
            .collect();
        digits.parse().ok()
    });
    Some((id, gid))
}

// ---- 試しに読む ----

/// 見出しの下に見せる応募者の行の数
pub const PREVIEW_ROWS: u32 = 5;
pub const MAX_HEADER_ROW: u32 = 20;
pub const MAX_EXAMPLE_ROWS: u32 = 10;
/// 画面に出す 1 マスの最大の文字数
const PREVIEW_CELL_CHARS: usize = 200;
/// 読む列の最大 (ZZ)
const PREVIEW_COLS: usize = 702;

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Preview {
    pub tab_title: String,
    /// 見出しの行の各マス (空は空のまま)
    pub headers: Vec<String>,
    /// 見出しより上の行 (見出しの行を選び直すときの目安)
    pub above: Vec<Vec<String>>,
    /// 記入例の行
    pub examples: Vec<Vec<String>>,
    /// 先頭の応募者の行 (最大 [`PREVIEW_ROWS`])
    pub rows: Vec<Vec<String>>,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub enum ProbeError {
    /// サービスアカウントにシートが共有されていない (403 / 404)
    NotShared,
    /// そのタブ (番号) がシートに無い
    TabNotFound,
    Google(GoogleError),
}

impl From<GoogleError> for ProbeError {
    fn from(e: GoogleError) -> Self {
        match e {
            GoogleError::Status(403 | 404) => ProbeError::NotShared,
            other => ProbeError::Google(other),
        }
    }
}

/// 見出しの比べ方をそろえる: セル内の改行・タブなどの制御文字と続く空白を空白 1 つにし、前後の空白を落とす。
/// 登録・項目の作成・項目の保存・(PR4 以降の) シートの読み取りで同じものを使う
pub fn clean_header(s: &str) -> String {
    s.split(|c: char| c.is_whitespace() || c.is_control())
        .filter(|w| !w.is_empty())
        .collect::<Vec<_>>()
        .join(" ")
}

fn clip(s: &str) -> String {
    let t: String = s.chars().take(PREVIEW_CELL_CHARS).collect();
    if t.len() < s.len() {
        format!("{t}…")
    } else {
        t
    }
}

/// 登録したシートの見出しと先頭の数行を読む (1 回目に spreadsheets.get でタブの名前、
/// 2 回目に values.batchGet で先頭の行)。個人情報を含むので、呼び出し側は管理者だけに返し、ログに値を出さない。
pub async fn read_preview(
    api: &dyn SheetsApi,
    spreadsheet: &str,
    sheet_id: i64,
    header_row: u32,
    example_rows: u32,
) -> Result<Preview, ProbeError> {
    let header_row = header_row.clamp(1, MAX_HEADER_ROW);
    let example_rows = example_rows.min(MAX_EXAMPLE_ROWS);
    let tabs = api.tabs(spreadsheet).await?;
    let title = tabs
        .into_iter()
        .find(|t| t.sheet_id == sheet_id)
        .map(|t| t.title)
        .ok_or(ProbeError::TabNotFound)?;
    let last = header_row + example_rows + PREVIEW_ROWS;
    let ranges = [a1(&title, &format!("A1:ZZ{last}"))];
    let mut got = api.batch_get(spreadsheet, &ranges).await?;
    let mut top = got.pop().unwrap_or_default();
    top.resize(last as usize, Vec::new());
    let width = top
        .iter()
        .map(Vec::len)
        .max()
        .unwrap_or(0)
        .min(PREVIEW_COLS);
    let norm = |r: &Vec<String>| -> Vec<String> {
        let mut v: Vec<String> = r.iter().take(width).map(|c| clip(c)).collect();
        v.resize(width, String::new());
        v
    };
    let h = header_row as usize - 1;
    let e = h + 1 + example_rows as usize;
    Ok(Preview {
        tab_title: title,
        headers: norm(&top[h]).iter().map(|c| clean_header(c)).collect(),
        above: top[..h].iter().map(norm).collect(),
        examples: top[h + 1..e].iter().map(norm).collect(),
        rows: top[e..]
            .iter()
            .map(norm)
            .filter(|r| r.iter().any(|c| !c.trim().is_empty()))
            .collect(),
    })
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn 見出しの改行や制御文字は空白1つにそろえる() {
        assert_eq!(clean_header(" 応募\n日 "), "応募 日");
        assert_eq!(clean_header("面接\r\n\t予定日"), "面接 予定日");
        assert_eq!(clean_header("状況\u{7}"), "状況");
        assert_eq!(clean_header("\n \t"), "");
    }

    #[test]
    fn シートのurlから番号を取る() {
        let id = "1AbCdEfGhIjKlMnOpQrStUvWxYz0123456789_-ab";
        assert_eq!(
            parse_sheet_url(&format!(
                "https://docs.google.com/spreadsheets/d/{id}/edit#gid=12345"
            )),
            Some((id.to_string(), Some(12345)))
        );
        assert_eq!(
            parse_sheet_url(&format!(
                " https://docs.google.com/spreadsheets/d/{id}/edit?usp=sharing "
            )),
            Some((id.to_string(), None))
        );
        assert_eq!(
            parse_sheet_url(&format!("https://docs.google.com/spreadsheets/d/{id}")),
            Some((id.to_string(), None))
        );
        // ほかのホスト・http・短すぎる番号・おかしな文字は受け付けない
        for bad in [
            format!("http://docs.google.com/spreadsheets/d/{id}/edit"),
            format!("https://evil.example/spreadsheets/d/{id}/edit"),
            format!("https://docs.google.com.evil.example/spreadsheets/d/{id}"),
            "https://docs.google.com/spreadsheets/d/short/edit".to_string(),
            format!("https://docs.google.com/spreadsheets/d/{id}%2F..%2F"),
            format!("https://docs.google.com/spreadsheets/d/{id}.json"),
        ] {
            assert_eq!(parse_sheet_url(&bad), None, "{bad}");
        }
    }
}
