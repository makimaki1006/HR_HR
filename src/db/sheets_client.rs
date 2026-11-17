//! Google Sheets API REST クライアント (gspread の Rust 版が無いため手書き)
//!
//! - Service Account JSON (base64) を環境変数 `GOOGLE_SA_KEY_B64` から読込
//! - jsonwebtoken で RS256 JWT 生成 → OAuth2 token endpoint で access_token 取得
//! - access_token は tokio::sync::RwLock で 1 時間キャッシュ (失効 5 分前にリフレッシュ)
//! - Spreadsheets API `values.get` で各シートの全レンジを取得
//!
//! 既存 HR_HR の reqwest は再利用。features = ["json"] を仮定 (rustls-tls は明示)。

use anyhow::{bail, Context, Result};
use base64::{engine::general_purpose::STANDARD as B64, Engine};
use jsonwebtoken::{encode, Algorithm, EncodingKey, Header};
use serde::{Deserialize, Serialize};
use std::collections::{HashMap, HashSet};
use std::sync::Arc;
use std::time::{Duration, SystemTime, UNIX_EPOCH};
use tokio::sync::RwLock;

const TOKEN_URI: &str = "https://oauth2.googleapis.com/token";
const SHEETS_API: &str = "https://sheets.googleapis.com/v4/spreadsheets";
const SCOPES: &str = "https://www.googleapis.com/auth/spreadsheets.readonly";
/// access_token 失効までこれより短いタイミングでリフレッシュ
const REFRESH_BEFORE_EXPIRY: u64 = 300;

/// リトライすべき HTTP ステータスか。
///
/// 2026-08-16 追加。実データでの起動確認中に Sheets API が **503 (UNAVAILABLE)** を返し、
/// 画面が 502 で落ちた。Google 側の一時的な不可用は日常的に起きる。
///
/// - 429 (レート超過) と 5xx (サーバ側の一時障害) は再試行する
/// - それ以外の 4xx は権限・シート名の誤りなど**再試行しても直らない**ので即諦める
///   （無駄な待ち時間を作らないため。401 を4回リトライしても意味がない）
fn is_retryable(status: u16) -> bool {
    status == 429 || (500..600).contains(&status)
}

// ---- Service Account JSON ----------------------------------------------

#[derive(Debug, Deserialize)]
struct ServiceAccountKey {
    client_email: String,
    private_key: String,
    token_uri: Option<String>,
    #[allow(dead_code)]
    project_id: Option<String>,
}

// ---- JWT Claims --------------------------------------------------------

#[derive(Debug, Serialize)]
struct JwtClaims {
    iss: String,
    scope: String,
    aud: String,
    exp: u64,
    iat: u64,
}

// ---- OAuth token レスポンス --------------------------------------------

#[derive(Debug, Deserialize)]
struct TokenResponse {
    access_token: String,
    expires_in: u64,
    #[allow(dead_code)]
    token_type: String,
}

// ---- キャッシュ済み access_token ---------------------------------------

#[derive(Clone)]
struct CachedToken {
    token: String,
    expires_at: u64, // epoch sec
}

// ---- 公開クライアント --------------------------------------------------

pub struct SheetsClient {
    http: reqwest::Client,
    sa_key: ServiceAccountKey,
    spreadsheet_id: String,
    token_cache: Arc<RwLock<Option<CachedToken>>>,
}

impl SheetsClient {
    /// 環境変数から初期化:
    ///   - GOOGLE_SA_KEY_B64 : base64 化された SA JSON
    ///   - SPREADSHEET_ID    : 対象スプシ ID
    pub fn from_env() -> Result<Self> {
        let spreadsheet_id =
            std::env::var("SPREADSHEET_ID").context("環境変数 SPREADSHEET_ID が未設定")?;
        Self::from_env_for(spreadsheet_id)
    }

    /// `GOOGLE_SA_KEY_B64` の同じサービスアカウントで、`SPREADSHEET_ID` とは別のスプレッドシートを読む
    /// (トークの振り返りの Slack 宛先のシート等。読み取りのみ)
    pub fn from_env_for(spreadsheet_id: String) -> Result<Self> {
        let b64 =
            std::env::var("GOOGLE_SA_KEY_B64").context("環境変数 GOOGLE_SA_KEY_B64 が未設定")?;
        let json_bytes = B64
            .decode(b64.trim())
            .context("GOOGLE_SA_KEY_B64 の base64 デコードに失敗")?;
        let sa_key: ServiceAccountKey =
            serde_json::from_slice(&json_bytes).context("SA JSON のパースに失敗")?;

        let http = reqwest::Client::builder()
            // 2026-08-17: 30秒だと大きいシートの初回取得が落ちる。
            //   「セグメント_クロス」72,633行 /「時間帯ヒート_クロス」約20万行。
            //   キャッシュに載れば以後は速いので、初回だけの話。
            .timeout(Duration::from_secs(120))
            .build()
            .context("reqwest クライアント初期化失敗")?;

        Ok(Self {
            http,
            sa_key,
            spreadsheet_id,
            token_cache: Arc::new(RwLock::new(None)),
        })
    }

    /// access_token を取得 (キャッシュ越し)
    async fn get_access_token(&self) -> Result<String> {
        let now = epoch_now()?;

        // ---- 1) 既存 token が valid なら即返す ----
        {
            let guard = self.token_cache.read().await;
            if let Some(cached) = guard.as_ref() {
                if cached.expires_at > now + REFRESH_BEFORE_EXPIRY {
                    return Ok(cached.token.clone());
                }
            }
        }

        // ---- 2) write lock を取って二重発行を防ぐ ----
        let mut guard = self.token_cache.write().await;
        if let Some(cached) = guard.as_ref() {
            if cached.expires_at > now + REFRESH_BEFORE_EXPIRY {
                return Ok(cached.token.clone());
            }
        }

        // ---- 3) JWT 生成 → token endpoint ----
        let jwt = self.build_jwt(now)?;
        let token_uri = self.sa_key.token_uri.as_deref().unwrap_or(TOKEN_URI);

        let params = [
            ("grant_type", "urn:ietf:params:oauth:grant-type:jwt-bearer"),
            ("assertion", &jwt),
        ];

        let resp = self
            .http
            .post(token_uri)
            .form(&params)
            .send()
            .await
            .context("OAuth token endpoint POST 失敗")?;

        let status = resp.status();
        let body = resp
            .text()
            .await
            .context("token endpoint body 読み込み失敗")?;
        if !status.is_success() {
            bail!("OAuth token 取得失敗: status={} body={}", status, body);
        }
        let token_resp: TokenResponse =
            serde_json::from_str(&body).context("token レスポンスのパース失敗")?;

        let cached = CachedToken {
            token: token_resp.access_token.clone(),
            expires_at: now + token_resp.expires_in,
        };
        *guard = Some(cached);

        Ok(token_resp.access_token)
    }

    fn build_jwt(&self, now: u64) -> Result<String> {
        let claims = JwtClaims {
            iss: self.sa_key.client_email.clone(),
            scope: SCOPES.to_string(),
            aud: self
                .sa_key
                .token_uri
                .clone()
                .unwrap_or_else(|| TOKEN_URI.to_string()),
            iat: now,
            exp: now + 3600,
        };

        let key = EncodingKey::from_rsa_pem(self.sa_key.private_key.as_bytes())
            .context("private_key のパース失敗 (PEM 形式である必要あり)")?;

        let header = Header::new(Algorithm::RS256);
        let token = encode(&header, &claims, &key).context("JWT エンコード失敗")?;
        Ok(token)
    }

    /// 単一シートを取得し header をキーとした `Vec<HashMap<String,String>>` で返す
    /// シートを **原本の列順のまま** (ヘッダ, 行) で返す。
    ///
    /// 2026-08-17 新設。従来は `get_sheet_as_rows` の `HashMap` しか無く、
    /// 呼び出し側がヘッダ順を復元できずアルファベット順にソートしていた。
    /// その結果、スプレッドシートをそのまま見るための画面(データブラウザ)で
    /// **原本と列順が違う**という副作用が出ていた。
    pub async fn get_sheet_as_table(
        &self,
        sheet_name: &str,
    ) -> Result<(Vec<String>, Vec<Vec<String>>)> {
        self.fetch_values(sheet_name, |body| parse_values(body, OwnedCells))
            .await
    }

    /// `get_sheet_as_table` と同じ表を、同じ値のセルを 1 つの `Arc<str>` にまとめて返す。
    ///
    /// 2026-11-17 新設。常駐キャッシュ（架電クオリティ・コンサルKPI の `SheetStore`）向け。
    /// 従来は 本文の String → JSON の値 (`serde_json::Value`、1 個 72 バイト) →
    /// `Vec<Vec<String>>` → `Arc<str>` の表 と 1 枚につき 3〜4 回作り直し、前の段が
    /// 後の段と同時に生きていた。実データ 12 枚で一時確保の合計 223MB。
    /// 本文のバイト列から最終の表へ直接読むと 98MB（保持量は同じ、確保の回数 −76%）。
    pub async fn get_sheet_as_shared_table(
        &self,
        sheet_name: &str,
    ) -> Result<(Vec<String>, Vec<Vec<Arc<str>>>)> {
        self.fetch_values(sheet_name, |body| {
            parse_values(body, SharedCells(HashSet::new()))
        })
        .await
    }

    /// `values.get` を呼び（429/5xx・送信失敗・本文の読み込み失敗は再試行）、
    /// 成功した本文を `parse` に渡す。
    async fn fetch_values<T>(
        &self,
        sheet_name: &str,
        parse: impl FnOnce(&[u8]) -> serde_json::Result<T>,
    ) -> Result<T> {
        let token = self.get_access_token().await?;

        // シート名に '/' や日本語が含まれるので URL encode
        // single quote escape は Google Sheets A1 notation 仕様
        let range = format!("'{}'", sheet_name.replace('\'', "''"));
        let encoded_range = urlencoding::encode(&range);

        let url = format!(
            "{base}/{ss_id}/values/{range}?majorDimension=ROWS&valueRenderOption=UNFORMATTED_VALUE&dateTimeRenderOption=FORMATTED_STRING",
            base = SHEETS_API,
            ss_id = self.spreadsheet_id,
            range = encoded_range,
        );

        // 2026-08-16 追加: 429/5xx のリトライ。
        //   実データでの起動確認中に **Sheets API が 503 (UNAVAILABLE) を返して
        //   画面が 502 で落ちた**。Google 側の一時的な不可用は日常的に起きるため、
        //   リトライが無いと「たまに画面が真っ白」という再現しにくい不具合になる。
        //   Python 側(consulting_patrol / patrol_data)でも同じ教訓で
        //   429/5xx リトライを入れている。
        //   429 は Retry-After を尊重し、それ以外は指数バックオフ。
        //   4xx(429以外)は再試行しても無駄なので即座に諦める。
        const MAX_RETRIES: u32 = 4;
        let mut last_err = String::new();
        let mut body = None;

        for attempt in 0..=MAX_RETRIES {
            let resp = self.http.get(&url).bearer_auth(&token).send().await;

            // 2026-08-17 追加: **送信そのものの失敗も再試行する**。
            //   従来は `?` で即座に諦めていたため、タイムアウトや接続断が
            //   リトライされなかった。実際に「セグメント_クロス」(72,633行)が
            //   コールドキャッシュで 30秒のクライアント側タイムアウトに掛かり、
            //   画面が 502 で落ちた（再試行2回はいずれも成功したので一過性）。
            //   「時間帯ヒート_クロス」は20万行あり、より踏みやすい。
            //   ステータスコードのリトライ(429/5xx)だけでは届かない経路だった。
            let resp = match resp {
                Ok(r) => r,
                Err(e) => {
                    last_err = format!("送信失敗: {e}");
                    if attempt == MAX_RETRIES {
                        bail!("Sheets API 失敗 ({sheet_name}): {last_err}");
                    }
                    let wait = Duration::from_secs(1u64 << attempt);
                    tracing::warn!(
                        "Sheets API {sheet_name}: {e} のため {}秒後に再試行 ({}/{})",
                        wait.as_secs(),
                        attempt + 1,
                        MAX_RETRIES
                    );
                    tokio::time::sleep(wait).await;
                    continue;
                }
            };

            let status = resp.status();
            // 2026-11-17: 本文は `text()` で String にせず、受け取ったバイト列のまま読む
            //   （`parse_values` が JSON の値を経由せずに表へ直接読む）。
            let bytes = match resp.bytes().await {
                Ok(b) => {
                    crate::mem_watch::received(crate::mem_watch::Peer::Sheets, b.len());
                    b
                }
                Err(e) => {
                    // 本文の読み込み中に切れる場合も同じ扱い（実際に
                    // "error decoding response body" → "operation timed out" が出た）
                    last_err = format!("body 読み込み失敗: {e}");
                    if attempt == MAX_RETRIES {
                        bail!("Sheets API 失敗 ({sheet_name}): {last_err}");
                    }
                    let wait = Duration::from_secs(1u64 << attempt);
                    tracing::warn!(
                        "Sheets API {sheet_name}: {e} のため {}秒後に再試行 ({}/{})",
                        wait.as_secs(),
                        attempt + 1,
                        MAX_RETRIES
                    );
                    tokio::time::sleep(wait).await;
                    continue;
                }
            };

            if status.is_success() {
                body = Some(bytes);
                break;
            }

            let retryable = is_retryable(status.as_u16());
            last_err = format!("status={status} body={}", String::from_utf8_lossy(&bytes));
            if !retryable || attempt == MAX_RETRIES {
                break;
            }

            // 1s, 2s, 4s, 8s
            let wait = Duration::from_secs(1u64 << attempt);
            tracing::warn!(
                "Sheets API {sheet_name}: {status} のため {}秒後に再試行 ({}/{})",
                wait.as_secs(),
                attempt + 1,
                MAX_RETRIES
            );
            tokio::time::sleep(wait).await;
        }

        let Some(body) = body else {
            bail!("Sheets API 失敗 ({sheet_name}): {last_err}");
        };
        parse(&body).with_context(|| format!("ValuesResponse パース失敗 ({sheet_name})"))
    }

    /// 従来互換。列順を要さない呼び出し向けに `HashMap` へ畳む。
    pub async fn get_sheet_as_rows(
        &self,
        sheet_name: &str,
    ) -> Result<Vec<HashMap<String, String>>> {
        let (header, rows) = self.get_sheet_as_table(sheet_name).await?;
        Ok(rows
            .into_iter()
            .map(|cells| {
                header
                    .iter()
                    .zip(cells)
                    .map(|(k, v)| (k.clone(), v))
                    .collect::<HashMap<String, String>>()
            })
            .collect())
    }

    pub fn spreadsheet_id(&self) -> &str {
        &self.spreadsheet_id
    }
}

/// ヘッダを「実際に値がある最大幅」まで伸ばし、名前の無い列に `列N` を付ける。
///
/// Sheets API は行末の空セルを詰めて返すため、1行目が注記1セルだけのシートでは
/// **ヘッダ行そのものが1セル**になる。そのまま使うと2列目以降が丸ごと消える。
/// 実測「最新サマリ」: 39列あるのにヘッダ1列 → **2〜39列目が無言で欠落**していた。
fn normalize_header(header: Vec<String>, widest: usize) -> Vec<String> {
    let mut h = header;
    if widest > h.len() {
        h.resize(widest, String::new());
    }
    h.into_iter()
        .enumerate()
        .map(|(i, x)| {
            if x.trim().is_empty() {
                format!("列{}", i + 1)
            } else {
                x
            }
        })
        .collect()
}

// ---- Sheets API レスポンス (`values.get`) を表へ直接読む ------------------
//
// 2026-11-17: 従来は `ValuesResponse { values: Vec<Vec<serde_json::Value>> }` に
// いったん読み、そこから文字列の表を作っていた。JSON の値の段を作らず、
// 本文のバイト列から最終の表（`String` または同じ値をまとめた `Arc<str>`）へ直接読む。
// セルを文字列にする決まりは従来と同じ:
//   文字列 → そのまま / null・欠け → 空 / 真偽値 → "true"・"false" /
//   数値 → `serde_json::Number` の文字列化 / 配列・オブジェクト → JSON の文字列化。
// 本文の形の決まりも従来の `ValuesResponse` と同じ（`values` は省略可で既定は空、
// `range` は省略可の文字列、ほかのキーは無視、同じキーの重複・末尾のゴミは誤り）。

/// 1 セルの値を作る。
trait CellMaker {
    type Cell: AsRef<str> + Clone;
    fn make(&mut self, s: &str) -> Self::Cell;
}

/// セルを `String` で持つ（`get_sheet_as_table`）。
struct OwnedCells;

impl CellMaker for OwnedCells {
    type Cell = String;
    fn make(&mut self, s: &str) -> String {
        s.to_owned()
    }
}

/// 同じ値のセルを 1 つの `Arc<str>` にまとめる（`get_sheet_as_shared_table`）。
struct SharedCells(HashSet<Arc<str>>);

impl CellMaker for SharedCells {
    type Cell = Arc<str>;
    fn make(&mut self, s: &str) -> Arc<str> {
        if let Some(a) = self.0.get(s) {
            return Arc::clone(a);
        }
        let a: Arc<str> = Arc::from(s);
        self.0.insert(Arc::clone(&a));
        a
    }
}

/// (ヘッダ, 行)
type Table<C> = (Vec<String>, Vec<Vec<C>>);

/// `values.get` の本文を (ヘッダ, 行) に読む。行は 2 行未満なら空。
/// ヘッダは `normalize_header` で全行の最大幅まで伸ばし、短い行は空セルで埋める。
fn parse_values<M: CellMaker>(body: &[u8], mut maker: M) -> serde_json::Result<Table<M::Cell>> {
    use serde::de::DeserializeSeed;

    let mut de = serde_json::Deserializer::from_slice(body);
    let (header_raw, mut rows) = values_de::Top(&mut maker).deserialize(&mut de)?;
    de.end()?;

    let Some(header_raw) = header_raw else {
        return Ok((Vec::new(), Vec::new()));
    };
    if rows.is_empty() {
        return Ok((Vec::new(), Vec::new()));
    }
    let header: Vec<String> = header_raw.iter().map(|c| c.as_ref().to_owned()).collect();
    let widest = rows.iter().map(|r| r.len()).max().unwrap_or(0);
    let header = normalize_header(header, widest);
    let empty = maker.make("");
    for r in &mut rows {
        if r.len() < header.len() {
            r.resize(header.len(), empty.clone());
        }
    }
    Ok((header, rows))
}

mod values_de {
    use super::CellMaker;
    use serde::de::value::{MapAccessDeserializer, SeqAccessDeserializer};
    use serde::de::{
        self, DeserializeSeed, Deserializer, IgnoredAny, MapAccess, SeqAccess, Visitor,
    };
    use serde::Deserialize;
    use std::borrow::Cow;
    use std::fmt;

    /// (1 行目, 2 行目以降)
    pub(super) type Values<C> = (Option<Vec<C>>, Vec<Vec<C>>);

    /// 本文のオブジェクト
    pub(super) struct Top<'m, M>(pub(super) &'m mut M);

    impl<'de, M: CellMaker> DeserializeSeed<'de> for Top<'_, M> {
        type Value = Values<M::Cell>;
        fn deserialize<D: Deserializer<'de>>(self, d: D) -> Result<Self::Value, D::Error> {
            d.deserialize_map(self)
        }
    }

    impl<'de, M: CellMaker> Visitor<'de> for Top<'_, M> {
        type Value = Values<M::Cell>;
        fn expecting(&self, f: &mut fmt::Formatter) -> fmt::Result {
            f.write_str("struct ValuesResponse")
        }
        fn visit_map<A: MapAccess<'de>>(self, mut m: A) -> Result<Self::Value, A::Error> {
            let mut values: Option<Values<M::Cell>> = None;
            let mut range_seen = false;
            while let Some(k) = m.next_key::<Cow<'de, str>>()? {
                match k.as_ref() {
                    "values" => {
                        if values.is_some() {
                            return Err(de::Error::duplicate_field("values"));
                        }
                        values = Some(m.next_value_seed(Rows(&mut *self.0))?);
                    }
                    "range" => {
                        if range_seen {
                            return Err(de::Error::duplicate_field("range"));
                        }
                        m.next_value::<String>()?;
                        range_seen = true;
                    }
                    _ => {
                        m.next_value::<IgnoredAny>()?;
                    }
                }
            }
            Ok(values.unwrap_or((None, Vec::new())))
        }
    }

    /// `values` の配列。1 行目（ヘッダ）は分けて持つ。
    struct Rows<'m, M>(&'m mut M);

    impl<'de, M: CellMaker> DeserializeSeed<'de> for Rows<'_, M> {
        type Value = Values<M::Cell>;
        fn deserialize<D: Deserializer<'de>>(self, d: D) -> Result<Self::Value, D::Error> {
            d.deserialize_seq(self)
        }
    }

    impl<'de, M: CellMaker> Visitor<'de> for Rows<'_, M> {
        type Value = Values<M::Cell>;
        fn expecting(&self, f: &mut fmt::Formatter) -> fmt::Result {
            f.write_str("a sequence")
        }
        fn visit_seq<A: SeqAccess<'de>>(self, mut s: A) -> Result<Self::Value, A::Error> {
            let Some(header) = s.next_element_seed(Row {
                maker: &mut *self.0,
                cap: 0,
            })?
            else {
                return Ok((None, Vec::new()));
            };
            // 2 行目以降はヘッダの幅で確保しておく（短い行を後で埋めても確保し直さない）
            let cap = header.len();
            let mut rows = Vec::new();
            while let Some(r) = s.next_element_seed(Row {
                maker: &mut *self.0,
                cap,
            })? {
                rows.push(r);
            }
            Ok((Some(header), rows))
        }
    }

    /// 1 行
    struct Row<'m, M> {
        maker: &'m mut M,
        cap: usize,
    }

    impl<'de, M: CellMaker> DeserializeSeed<'de> for Row<'_, M> {
        type Value = Vec<M::Cell>;
        fn deserialize<D: Deserializer<'de>>(self, d: D) -> Result<Self::Value, D::Error> {
            d.deserialize_seq(self)
        }
    }

    impl<'de, M: CellMaker> Visitor<'de> for Row<'_, M> {
        type Value = Vec<M::Cell>;
        fn expecting(&self, f: &mut fmt::Formatter) -> fmt::Result {
            f.write_str("a sequence")
        }
        fn visit_seq<A: SeqAccess<'de>>(self, mut s: A) -> Result<Self::Value, A::Error> {
            let mut row = Vec::with_capacity(self.cap);
            while let Some(c) = s.next_element_seed(Cell(&mut *self.maker))? {
                row.push(c);
            }
            Ok(row)
        }
    }

    /// 1 セル
    struct Cell<'m, M>(&'m mut M);

    impl<'de, M: CellMaker> DeserializeSeed<'de> for Cell<'_, M> {
        type Value = M::Cell;
        fn deserialize<D: Deserializer<'de>>(self, d: D) -> Result<Self::Value, D::Error> {
            d.deserialize_any(self)
        }
    }

    impl<'de, M: CellMaker> Visitor<'de> for Cell<'_, M> {
        type Value = M::Cell;
        fn expecting(&self, f: &mut fmt::Formatter) -> fmt::Result {
            f.write_str("any valid JSON value")
        }
        fn visit_str<E>(self, v: &str) -> Result<M::Cell, E> {
            Ok(self.0.make(v))
        }
        fn visit_unit<E>(self) -> Result<M::Cell, E> {
            Ok(self.0.make(""))
        }
        fn visit_bool<E>(self, v: bool) -> Result<M::Cell, E> {
            Ok(self.0.make(if v { "true" } else { "false" }))
        }
        fn visit_i64<E>(self, v: i64) -> Result<M::Cell, E> {
            Ok(self.0.make(&v.to_string()))
        }
        fn visit_u64<E>(self, v: u64) -> Result<M::Cell, E> {
            Ok(self.0.make(&v.to_string()))
        }
        fn visit_f64<E>(self, v: f64) -> Result<M::Cell, E> {
            // 従来の `Value` と同じ: 表せない値は null 扱い (= 空)
            let s = serde_json::Number::from_f64(v)
                .map(|n| n.to_string())
                .unwrap_or_default();
            Ok(self.0.make(&s))
        }
        fn visit_seq<A: SeqAccess<'de>>(self, s: A) -> Result<M::Cell, A::Error> {
            let v = serde_json::Value::deserialize(SeqAccessDeserializer::new(s))?;
            Ok(self.0.make(&v.to_string()))
        }
        fn visit_map<A: MapAccess<'de>>(self, m: A) -> Result<M::Cell, A::Error> {
            let v = serde_json::Value::deserialize(MapAccessDeserializer::new(m))?;
            Ok(self.0.make(&v.to_string()))
        }
    }
}

fn epoch_now() -> Result<u64> {
    SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .map(|d| d.as_secs())
        .context("システム時刻取得失敗")
}

#[cfg(test)]
mod tests {
    use super::*;

    // 2026-11-17: 架電クオリティの `sheet_from_table` のテストを移した
    //   （同じ値をまとめる処理が本文の読み取りに移ったため）。
    #[test]
    fn table_preserves_values_order_and_shares_equal_cells() {
        let body = r#"{"range":"x","majorDimension":"ROWS","values":[
            ["z","a","b"],
            ["東京都","","東京都","ignored"],
            ["大阪府","東京都"],
            []
        ]}"#;
        let (header, rows) = parse_values(body.as_bytes(), SharedCells(HashSet::new())).unwrap();
        assert_eq!(header, ["z", "a", "b", "列4"]);
        let values: Vec<Vec<&str>> = rows
            .iter()
            .map(|row| row.iter().map(AsRef::as_ref).collect())
            .collect();
        assert_eq!(
            values,
            vec![
                vec!["東京都", "", "東京都", "ignored"],
                vec!["大阪府", "東京都", "", ""],
                vec!["", "", "", ""]
            ]
        );
        assert!(Arc::ptr_eq(&rows[0][0], &rows[0][2]));
        assert!(Arc::ptr_eq(&rows[0][0], &rows[1][1]));
        assert!(Arc::ptr_eq(&rows[0][1], &rows[1][2]));
        assert!(Arc::ptr_eq(&rows[0][1], &rows[2][0]));
        assert!(!Arc::ptr_eq(&rows[0][0], &rows[1][0]));
    }

    #[test]
    fn ヘッダが短くても列を落とさない() {
        // 「最新サマリ」の実際の形: 1行目が注記1セル、実データは39列
        let got = normalize_header(vec!["最終更新: 2026-08-16".to_string()], 39);
        assert_eq!(got.len(), 39, "39列あるのにヘッダ1列では2〜39列目が消える");
        assert_eq!(got[0], "最終更新: 2026-08-16", "元の名前は書き換えない");
        assert_eq!(got[1], "列2");
        assert_eq!(got[38], "列39");
    }

    #[test]
    fn 空文字ヘッダにも位置名を付ける() {
        let got = normalize_header(
            vec![
                "owner_id".into(),
                "".into(),
                "  ".into(),
                "call_count".into(),
            ],
            4,
        );
        assert_eq!(got, vec!["owner_id", "列2", "列3", "call_count"]);
    }

    #[test]
    fn ヘッダの方が長いときは縮めない() {
        // 全行が空でも、ヘッダにある列は残す
        let got = normalize_header(vec!["a".into(), "b".into(), "c".into()], 1);
        assert_eq!(got, vec!["a", "b", "c"]);
    }

    /// JWT claims 構造の sanity check (実 SA 鍵不要)
    #[test]
    fn jwt_claims_serialize() {
        let claims = JwtClaims {
            iss: "test@example.iam.gserviceaccount.com".into(),
            scope: SCOPES.into(),
            aud: TOKEN_URI.into(),
            iat: 1700000000,
            exp: 1700003600,
        };
        let s = serde_json::to_string(&claims).unwrap();
        assert!(s.contains("\"iss\":\"test@example.iam.gserviceaccount.com\""));
        assert!(s.contains("\"exp\":1700003600"));
    }
    #[test]
    fn 一時的な失敗はリトライする() {
        // 実際に踏んだのは 503。429 はレート超過。
        assert!(is_retryable(503), "Sheets API が返す UNAVAILABLE");
        assert!(is_retryable(500));
        assert!(is_retryable(502));
        assert!(is_retryable(429), "レート超過は待てば通る");
    }

    #[test]
    fn 再試行しても直らないものは諦める() {
        // 権限不足やシート名の誤りは、何度叩いても同じ結果になる。
        // リトライすると無駄に待たせるだけ(401を4回で15秒)。
        assert!(!is_retryable(401), "認証エラー");
        assert!(!is_retryable(403), "権限不足");
        assert!(!is_retryable(404), "シートが無い");
        assert!(!is_retryable(400));
    }

    #[test]
    fn 成功はリトライ対象外() {
        assert!(!is_retryable(200));
        assert!(!is_retryable(204));
    }
}
