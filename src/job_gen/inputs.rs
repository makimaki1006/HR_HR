//! 入力5形式(自由テキスト/URL/CSV/Excel/PDF/保存HTML)の正規化。
//!
//! 求人票生成パイプラインの最上流。多様な入力を「1求人=1[`NormalizedJob`]」に
//! そろえ、後段([`super::fact_extract`] 以降)が `source_text` だけを見れば
//! 済むようにする。CSV/Excel は複数行→複数求人になりうる。
//!
//! 設計方針(既存モジュールと同じ):
//! - HTML→テキスト化は自前実装([`html_to_text`])。新依存を増やさない・純粋関数。
//! - パース系(CSV/Excel の行→求人、セル→文字列)は非async の純粋関数に分離し
//!   ユニットテスト可能にする。ライブ HTTP(URL 取得)だけが async。

use crate::job_gen::inputs::calamine_shim::Data;
use anyhow::{anyhow, Result};
use base64::Engine;
use serde::{Deserialize, Serialize};
use std::net::{IpAddr, Ipv4Addr, Ipv6Addr, SocketAddr};

/// 一般的なブラウザを騙る User-Agent(素の reqwest だと 403 を返すサイトがある)。
const BROWSER_UA: &str =
    "Mozilla/5.0 (Windows NT 10.0; Win64; x64) AppleWebKit/537.36 \
     (KHTML, like Gecko) Chrome/120.0.0.0 Safari/537.36";

/// タイトル推定・自由テキスト先頭行の最大文字数。
const HINT_MAX_CHARS: usize = 30;

// ── リソース枯渇対策の上限(すべて名前付き定数) ──────────────────────
/// URL取得の応答上限。求人票1件の原文は数十KB程度で足りるため、HTML込みでも
/// 2MBあれば十分。これを超える応答は途中で打ち切る(メモリ枯渇・遅延攻撃対策)。
const MAX_FETCH_BYTES: usize = 2 * 1024 * 1024;
/// アップロード(base64デコード後)の上限。xlsx/PDFの実務サイズは数MB以内で、
/// 15MBあれば通常の求人票資料を賄える。復号後にこれを超えれば拒否。
const MAX_UPLOAD_BYTES: usize = 15 * 1024 * 1024;
/// base64文字列長の上限。デコード後は約3/4になるため、20MB相当(=約27MB文字)を
/// 上限とし、巨大文字列の確保・復号自体を未然に防ぐ。
const MAX_B64_CHARS: usize = 27 * 1024 * 1024;
/// テキスト系入力(自由テキスト/CSV/HTML)の上限。求人原文としては過大な5MBを
/// 超える入力は拒否する。
const MAX_TEXT_BYTES: usize = 5 * 1024 * 1024;

/// 入力の種別。HTTP 層(`/api/jobgen/normalize` の `kind`)と同じ綴りで受けられるよう
/// snake_case の別名を付ける(free_text/url/csv/excel/pdf/html)。
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum InputKind {
    FreeText,
    Url,
    Csv,
    Excel,
    Pdf,
    Html,
}

/// 正規化後の1求人。後段はこの `source_text` のみを事実抽出の原文として扱う。
#[derive(Debug, Clone, Serialize, Deserialize, Default, PartialEq, Eq)]
pub struct NormalizedJob {
    /// 職種名の当たり(知識ルックアップ・進捗表示用)。確定値ではない。
    pub title_hint: String,
    /// 抽出対象の原文(プレーンテキスト)。
    pub source_text: String,
}

/// 入力を正規化して求人のリストにする。
///
/// - `FreeText`/`Html`: `text` を使う。`Html` はタグ除去してテキスト化。
/// - `Url`: `url` を GET(ブラウザ UA・20秒)→ HTML をテキスト化。
/// - `Csv`: `text`(CSV 本文)を行ごとに求人化。
/// - `Excel`: `data_base64`(xlsx バイナリ)→ 先頭シートを CSV 同様に処理。
/// - `Pdf`: `data_base64`(PDF バイナリ)→ テキスト抽出(ビルド制約で未対応の場合あり)。
pub async fn normalize(
    kind: InputKind,
    text: Option<String>,
    url: Option<String>,
    data_base64: Option<String>,
) -> Result<Vec<NormalizedJob>> {
    match kind {
        InputKind::FreeText => {
            let t = text.ok_or_else(|| anyhow!("free_text: text が必要です"))?;
            ensure_text_size(&t, "自由テキスト")?;
            Ok(vec![NormalizedJob {
                title_hint: first_line_hint(&t),
                source_text: t,
            }])
        }
        InputKind::Html => {
            let t = text.ok_or_else(|| anyhow!("html: text(HTML文字列)が必要です"))?;
            ensure_text_size(&t, "HTML")?;
            let txt = html_to_text(&t);
            Ok(vec![NormalizedJob {
                title_hint: first_line_hint(&txt),
                source_text: txt,
            }])
        }
        InputKind::Url => {
            let u = url.ok_or_else(|| anyhow!("url: url が必要です"))?;
            let html = fetch_url(&u).await?;
            let txt = html_to_text(&html);
            Ok(vec![NormalizedJob {
                title_hint: first_line_hint(&txt),
                source_text: txt,
            }])
        }
        InputKind::Csv => {
            let t = text.ok_or_else(|| anyhow!("csv: text(CSV本文)が必要です"))?;
            ensure_text_size(&t, "CSV")?;
            let rows = parse_csv(&t)?;
            Ok(rows_to_jobs(rows))
        }
        InputKind::Excel => {
            let b64 = data_base64.ok_or_else(|| anyhow!("excel: data_base64 が必要です"))?;
            let bytes = decode_b64(&b64)?;
            let rows = parse_xlsx(&bytes)?;
            Ok(rows_to_jobs(rows))
        }
        InputKind::Pdf => {
            let b64 = data_base64.ok_or_else(|| anyhow!("pdf: data_base64 が必要です"))?;
            let bytes = decode_b64(&b64)?;
            let text = extract_pdf_text(&bytes)?;
            Ok(vec![NormalizedJob {
                title_hint: first_line_hint(&text),
                source_text: text,
            }])
        }
    }
}

/// URL を GET して本文(HTML想定)を返す。ブラウザ UA・20秒タイムアウト。
///
/// SSRF 対策:
/// - スキームは http/https のみ許可。ホスト名 "localhost" は拒否。
/// - ホストを DNS 解決し、いずれかの IP が内部宛([`is_forbidden_ip`])なら拒否。
/// - 解決した(検査済みの)IP に接続を固定(`resolve_to_addrs`)し、reqwest 側の
///   再解決による DNS リバインディングを封じる。
/// - リダイレクトは追わない。3xx は「リダイレクト先を直接指定」の明確なエラーに。
///
/// リソース枯渇対策: Content-Length と実受信バイト数の両方を [`MAX_FETCH_BYTES`] で打ち切る。
async fn fetch_url(url: &str) -> Result<String> {
    let parsed = reqwest::Url::parse(url).map_err(|e| anyhow!("URL が不正です: {e}"))?;

    let scheme = parsed.scheme();
    if scheme != "http" && scheme != "https" {
        return Err(anyhow!(
            "http/https のスキームのみ対応です(指定: {scheme})"
        ));
    }
    let host = parsed
        .host_str()
        .ok_or_else(|| anyhow!("URL にホストがありません"))?;
    if host.eq_ignore_ascii_case("localhost") {
        return Err(anyhow!("localhost へのアクセスは禁止です"));
    }
    let port = parsed
        .port_or_known_default()
        .unwrap_or(if scheme == "https" { 443 } else { 80 });

    // ホスト名を解決し、全解決 IP を検査(1つでも内部宛なら拒否)。
    let addrs: Vec<SocketAddr> = tokio::net::lookup_host((host, port))
        .await
        .map_err(|e| anyhow!("ホスト名を解決できません: {e}"))?
        .collect();
    if addrs.is_empty() {
        return Err(anyhow!("ホスト名を解決できませんでした"));
    }
    for a in &addrs {
        if is_forbidden_ip(a.ip()) {
            return Err(anyhow!("内部ネットワーク宛のURLは取得できません"));
        }
    }

    let client = reqwest::Client::builder()
        .timeout(std::time::Duration::from_secs(20))
        .redirect(reqwest::redirect::Policy::none())
        // 検査済み IP に接続を固定(reqwest の再解決を封じる)。
        .resolve_to_addrs(host, &addrs)
        .build()?;

    let resp = client
        .get(url)
        .header(reqwest::header::USER_AGENT, BROWSER_UA)
        .send()
        .await?;

    let status = resp.status();
    if status.is_redirection() {
        return Err(anyhow!(
            "リダイレクト応答({})です。リダイレクト先URLを直接指定してください",
            status.as_u16()
        ));
    }
    let mut resp = resp.error_for_status()?;

    // Content-Length があれば受信前に上限超過を弾く。
    if let Some(len) = resp.content_length() {
        if len > MAX_FETCH_BYTES as u64 {
            return Err(anyhow!(
                "応答が大きすぎます({len}バイト > 上限{MAX_FETCH_BYTES}バイト)"
            ));
        }
    }

    // Content-Length を偽る/持たない応答に備え、実受信量でも打ち切る。
    let mut buf: Vec<u8> = Vec::new();
    while let Some(chunk) = resp.chunk().await? {
        if buf.len() + chunk.len() > MAX_FETCH_BYTES {
            return Err(anyhow!(
                "応答が上限{MAX_FETCH_BYTES}バイトを超えたため取得を打ち切りました"
            ));
        }
        buf.extend_from_slice(&chunk);
    }
    Ok(String::from_utf8_lossy(&buf).into_owned())
}

/// 取得先 IP が内部ネットワーク宛(取得を禁止すべき)かを判定する純粋関数。
///
/// 禁止対象: loopback / private(10/8,172.16/12,192.168/16) / link-local(169.254/16,fe80::/10)
/// / CGNAT(100.64/10) / unspecified、および ULA(fc00::/7)。IPv4射影IPv6(::ffff:x)は
/// V4 として判定する。
pub fn is_forbidden_ip(ip: IpAddr) -> bool {
    match ip {
        IpAddr::V4(v4) => is_forbidden_v4(v4),
        IpAddr::V6(v6) => match v6.to_ipv4_mapped() {
            Some(v4) => is_forbidden_v4(v4),
            None => is_forbidden_v6(v6),
        },
    }
}

fn is_forbidden_v4(ip: Ipv4Addr) -> bool {
    let o = ip.octets();
    ip.is_loopback()        // 127.0.0.0/8
        || ip.is_private()  // 10/8, 172.16/12, 192.168/16
        || ip.is_link_local() // 169.254.0.0/16
        || ip.is_unspecified() // 0.0.0.0
        || (o[0] == 100 && (o[1] & 0xC0) == 0x40) // CGNAT 100.64.0.0/10
}

fn is_forbidden_v6(ip: Ipv6Addr) -> bool {
    let seg0 = ip.segments()[0];
    ip.is_loopback()            // ::1
        || ip.is_unspecified()  // ::
        || (seg0 & 0xffc0) == 0xfe80 // link-local fe80::/10
        || (seg0 & 0xfe00) == 0xfc00 // unique local fc00::/7(内部宛)
}

/// テキスト系入力のサイズ上限([`MAX_TEXT_BYTES`])を検査する。
fn ensure_text_size(t: &str, label: &str) -> Result<()> {
    if t.len() > MAX_TEXT_BYTES {
        return Err(anyhow!(
            "{label}の入力が大きすぎます({}バイト > 上限{MAX_TEXT_BYTES}バイト)",
            t.len()
        ));
    }
    Ok(())
}

// ───────────────────────── base64 ─────────────────────────

/// 標準 base64 をデコード。不正入力・過大サイズは明確なエラーにする。
fn decode_b64(s: &str) -> Result<Vec<u8>> {
    // 復号前に文字列長で弾く(巨大文字列の走査・確保自体を防ぐ)。
    if s.len() > MAX_B64_CHARS {
        return Err(anyhow!(
            "アップロードが大きすぎます(base64長 {} > 上限 {MAX_B64_CHARS})",
            s.len()
        ));
    }
    // 改行・空白混入(メール添付やコピペ由来)は許容してから復号する。
    let cleaned: String = s.chars().filter(|c| !c.is_whitespace()).collect();
    let bytes = base64::engine::general_purpose::STANDARD
        .decode(cleaned.as_bytes())
        .map_err(|e| anyhow!("base64 デコードに失敗しました: {e}"))?;
    if bytes.len() > MAX_UPLOAD_BYTES {
        return Err(anyhow!(
            "アップロードが大きすぎます({}バイト > 上限{MAX_UPLOAD_BYTES}バイト)",
            bytes.len()
        ));
    }
    Ok(bytes)
}

// ───────────────────────── HTML → テキスト ─────────────────────────

/// HTML をプレーンテキストにする(純粋関数)。
///
/// 手順: script/style/noscript を中身ごと除去 → コメント除去 → ブロック境界を改行化
/// → タグ除去 → 主要エンティティ復元 → 連続空白圧縮・空行除去。
///
/// タグ区切り `<` `>` は ASCII のため、UTF-8 の本文(日本語)を壊さずにバイト走査できる。
pub fn html_to_text(html: &str) -> String {
    let b = html.as_bytes();
    let n = b.len();
    let mut out = String::with_capacity(n);
    let mut i = 0usize;
    let mut text_start = 0usize;

    while i < n {
        if b[i] != b'<' {
            i += 1;
            continue;
        }
        // '<' に到達。直前までの本文を確定。
        out.push_str(&html[text_start..i]);

        // コメント <!-- ... -->
        if html[i..].starts_with("<!--") {
            match html[i + 4..].find("-->") {
                Some(rel) => {
                    i = i + 4 + rel + 3;
                }
                None => {
                    i = n;
                }
            }
            text_start = i;
            continue;
        }

        // タグ名を読む(先頭の '/' は閉じタグ)。
        let mut j = i + 1;
        let closing = j < n && b[j] == b'/';
        if closing {
            j += 1;
        }
        let name_start = j;
        while j < n && b[j].is_ascii_alphanumeric() {
            j += 1;
        }
        let tag_name = html[name_start..j].to_ascii_lowercase();

        // タグ終端 '>' を探す。
        let after = match html[i..].find('>') {
            Some(rel) => i + rel + 1,
            None => n,
        };

        // ブロック要素・改行系は境界を改行にする(圧縮段で重複は畳まれる)。
        if is_line_breaking_tag(&tag_name) {
            out.push('\n');
        }

        // script/style/noscript は中身を丸ごと捨てる。
        if !closing && matches!(tag_name.as_str(), "script" | "style" | "noscript") {
            let close_pat = format!("</{tag_name}");
            let lowered = html[after..].to_ascii_lowercase();
            match lowered.find(&close_pat) {
                Some(rel) => {
                    let close_start = after + rel;
                    i = match html[close_start..].find('>') {
                        Some(r) => close_start + r + 1,
                        None => n,
                    };
                }
                None => {
                    i = n;
                }
            }
            text_start = i;
            continue;
        }

        i = after;
        text_start = i;
    }
    if text_start < n {
        out.push_str(&html[text_start..]);
    }

    let decoded = decode_entities(&out);
    normalize_ws(&decoded)
}

/// 改行を挿入すべきブロック/改行系タグか。
fn is_line_breaking_tag(name: &str) -> bool {
    matches!(
        name,
        "br" | "p"
            | "div"
            | "li"
            | "ul"
            | "ol"
            | "tr"
            | "td"
            | "th"
            | "table"
            | "section"
            | "article"
            | "header"
            | "footer"
            | "nav"
            | "aside"
            | "h1"
            | "h2"
            | "h3"
            | "h4"
            | "h5"
            | "h6"
            | "dt"
            | "dd"
            | "dl"
            | "blockquote"
            | "hr"
            | "pre"
    )
}

/// 主要 HTML エンティティを復元する。名前付き5種+`&nbsp;`+`&apos;`と、数値参照(10進/16進)。
fn decode_entities(s: &str) -> String {
    let mut result = String::with_capacity(s.len());
    let mut rest = s;
    while let Some(pos) = rest.find('&') {
        result.push_str(&rest[..pos]);
        let tail = &rest[pos..];

        let named: Option<(&str, usize)> = if tail.starts_with("&amp;") {
            Some(("&", 5))
        } else if tail.starts_with("&lt;") {
            Some(("<", 4))
        } else if tail.starts_with("&gt;") {
            Some((">", 4))
        } else if tail.starts_with("&quot;") {
            Some(("\"", 6))
        } else if tail.starts_with("&#39;") {
            Some(("'", 5))
        } else if tail.starts_with("&apos;") {
            Some(("'", 6))
        } else if tail.starts_with("&nbsp;") {
            Some((" ", 6))
        } else {
            None
        };

        if let Some((rep, len)) = named {
            result.push_str(rep);
            rest = &tail[len..];
        } else if tail.starts_with("&#") {
            match tail.find(';') {
                Some(semi) => {
                    let body = &tail[2..semi];
                    let cp = if body.starts_with('x') || body.starts_with('X') {
                        u32::from_str_radix(&body[1..], 16).ok()
                    } else {
                        body.parse::<u32>().ok()
                    };
                    match cp.and_then(char::from_u32) {
                        Some(c) => {
                            result.push(c);
                            rest = &tail[semi + 1..];
                        }
                        None => {
                            result.push('&');
                            rest = &tail[1..];
                        }
                    }
                }
                None => {
                    result.push('&');
                    rest = &tail[1..];
                }
            }
        } else {
            // 実体参照でない裸の '&'。そのまま残す。
            result.push('&');
            rest = &tail[1..];
        }
    }
    result.push_str(rest);
    result
}

/// 連続空白を1つに畳み、空行を落として行整形する。`&nbsp;`(U+00A0)も空白扱い。
fn normalize_ws(s: &str) -> String {
    let s = s.replace('\r', "\n").replace('\u{00a0}', " ");
    let mut lines: Vec<String> = Vec::new();
    for line in s.split('\n') {
        let mut collapsed = String::with_capacity(line.len());
        let mut prev_space = false;
        for ch in line.chars() {
            if ch.is_whitespace() {
                if !prev_space {
                    collapsed.push(' ');
                    prev_space = true;
                }
            } else {
                collapsed.push(ch);
                prev_space = false;
            }
        }
        let trimmed = collapsed.trim();
        if !trimmed.is_empty() {
            lines.push(trimmed.to_string());
        }
    }
    lines.join("\n")
}

// ───────────────────────── CSV / Excel 共通 ─────────────────────────

/// CSV 本文を行列にパースする(1行目=ヘッダも含む)。クォート内カンマ対応。
fn parse_csv(text: &str) -> Result<Vec<Vec<String>>> {
    let mut rdr = csv::ReaderBuilder::new()
        .has_headers(false)
        .flexible(true)
        .from_reader(text.as_bytes());
    let mut rows = Vec::new();
    for rec in rdr.records() {
        let rec = rec.map_err(|e| anyhow!("CSV パースに失敗しました: {e}"))?;
        rows.push(rec.iter().map(|s| s.to_string()).collect());
    }
    Ok(rows)
}

/// xlsx バイナリの先頭シートを行列にする。
fn parse_xlsx(bytes: &[u8]) -> Result<Vec<Vec<String>>> {
    use calamine::{open_workbook_from_rs, Reader, Xlsx};
    let cursor = std::io::Cursor::new(bytes.to_vec());
    let mut wb: Xlsx<_> =
        open_workbook_from_rs(cursor).map_err(|e| anyhow!("xlsx を開けませんでした: {e}"))?;
    let range = wb
        .worksheet_range_at(0)
        .ok_or_else(|| anyhow!("xlsx にシートがありません"))?
        .map_err(|e| anyhow!("xlsx シート読み取りに失敗しました: {e}"))?;
    let mut rows = Vec::new();
    for row in range.rows() {
        rows.push(row.iter().map(cell_to_string).collect());
    }
    Ok(rows)
}

/// calamine のセル値を文字列にする。数値は整数値なら小数点を付けない。
fn cell_to_string(cell: &Data) -> String {
    match cell {
        Data::Empty => String::new(),
        Data::String(s) => s.clone(),
        Data::Int(i) => i.to_string(),
        Data::Bool(b) => b.to_string(),
        Data::Float(f) => {
            if f.fract() == 0.0 && f.abs() < 1e15 {
                format!("{}", *f as i64)
            } else {
                f.to_string()
            }
        }
        // DateTime/Error/その他バージョン差のある型は Display 任せ。
        other => other.to_string(),
    }
}

/// 行列(1行目ヘッダ)を求人リストにする。CSV/Excel 共通。
fn rows_to_jobs(rows: Vec<Vec<String>>) -> Vec<NormalizedJob> {
    if rows.is_empty() {
        return Vec::new();
    }
    let header = &rows[0];
    let title_idx = find_title_col(header);

    let mut jobs = Vec::new();
    for row in &rows[1..] {
        if row.iter().all(|c| c.trim().is_empty()) {
            continue; // まっさらな行はスキップ
        }
        let source_text = header
            .iter()
            .enumerate()
            .map(|(i, h)| {
                let val = row.get(i).map(|s| s.as_str()).unwrap_or("");
                format!("{}: {}", h.trim(), val.trim())
            })
            .collect::<Vec<_>>()
            .join("\n");

        let title_hint = match title_idx {
            Some(idx) => row
                .get(idx)
                .map(|s| truncate_chars(s.trim(), HINT_MAX_CHARS))
                .unwrap_or_default(),
            None => row
                .iter()
                .find(|c| !c.trim().is_empty())
                .map(|s| truncate_chars(s.trim(), HINT_MAX_CHARS))
                .unwrap_or_default(),
        };

        jobs.push(NormalizedJob {
            title_hint,
            source_text,
        });
    }
    jobs
}

/// 「職種」「案件名」等それらしいヘッダ列の位置を返す。
fn find_title_col(header: &[String]) -> Option<usize> {
    const JP_KEYS: [&str; 6] = ["職種", "案件名", "求人名", "募集職種", "タイトル", "職種名"];
    const EN_KEYS: [&str; 4] = ["title", "job", "position", "role"];
    for (i, h) in header.iter().enumerate() {
        let hl = h.to_ascii_lowercase();
        if JP_KEYS.iter().any(|k| h.contains(k)) || EN_KEYS.iter().any(|k| hl.contains(k)) {
            return Some(i);
        }
    }
    None
}

// ───────────────────────── PDF ─────────────────────────

/// PDF バイナリからテキストを抽出する。
#[cfg(feature = "pdf")]
fn extract_pdf_text(bytes: &[u8]) -> Result<String> {
    let text = pdf_extract::extract_text_from_mem(bytes)
        .map_err(|e| anyhow!("PDF テキスト抽出に失敗しました: {e}"))?;
    let cleaned = normalize_ws(&text);
    if cleaned.trim().is_empty() {
        return Err(anyhow!(
            "PDF からテキストを抽出できませんでした(画像PDFの可能性)"
        ));
    }
    Ok(cleaned)
}

/// PDF 対応がビルドされていない場合の明示エラー(動かないものを動くように見せない)。
#[cfg(not(feature = "pdf"))]
fn extract_pdf_text(_bytes: &[u8]) -> Result<String> {
    Err(anyhow!("PDF入力は未対応(ビルド制約)"))
}

// ───────────────────────── 小物 ─────────────────────────

/// 先頭の非空行を最大 [`HINT_MAX_CHARS`] 文字で切ってタイトル当たりにする。
fn first_line_hint(s: &str) -> String {
    let line = s
        .lines()
        .map(|l| l.trim())
        .find(|l| !l.is_empty())
        .unwrap_or("");
    truncate_chars(line, HINT_MAX_CHARS)
}

/// 文字(コードポイント)単位で先頭 n 文字に切り詰める。
fn truncate_chars(s: &str, n: usize) -> String {
    s.chars().take(n).collect()
}

// ───────────────────────── calamine 型の別名 ─────────────────────────
// バージョン差(DataType→Data)を1箇所に閉じ込める。
mod calamine_shim {
    pub use calamine::Data;
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn html_to_text_scriptとstyleを中身ごと除去() {
        let html = "<html><head><style>.a{color:red}</style>\
            <script>var x = 1 < 2;</script></head>\
            <body><p>本文テキスト</p></body></html>";
        let text = html_to_text(html);
        assert!(text.contains("本文テキスト"), "本文が残る: {text:?}");
        assert!(!text.contains("color"), "styleが消える: {text:?}");
        assert!(!text.contains("var x"), "scriptが消える: {text:?}");
    }

    #[test]
    fn html_to_text_タグ除去とエンティティ復元() {
        let html = "<div>給与 &amp; 手当 &lt;重要&gt; &quot;固定&quot; &#39;円&#39; &nbsp; 末尾</div>";
        let text = html_to_text(html);
        assert_eq!(text, "給与 & 手当 <重要> \"固定\" '円' 末尾");
    }

    #[test]
    fn html_to_text_数値実体参照を復元() {
        // &#26085; = 日, &#x672c; = 本
        let text = html_to_text("<span>&#26085;&#x672c;</span>");
        assert_eq!(text, "日本");
    }

    #[test]
    fn html_to_text_ブロック要素で改行される() {
        let text = html_to_text("<p>一行目</p><p>二行目</p><br>三行目");
        assert_eq!(text, "一行目\n二行目\n三行目");
    }

    #[tokio::test]
    async fn free_text_はそのまま1件title_hintは先頭行30字() {
        let long = "とても長い職種名がここに続いていて三十文字を確実に超えるようにする余分な文字列";
        let body = format!("{long}\n給与: 25万円\n勤務地: 東京");
        let jobs = normalize(InputKind::FreeText, Some(body.clone()), None, None)
            .await
            .unwrap();
        assert_eq!(jobs.len(), 1);
        assert_eq!(jobs[0].source_text, body);
        assert_eq!(jobs[0].title_hint.chars().count(), 30);
        assert!(long.starts_with(&jobs[0].title_hint));
    }

    #[test]
    fn csv_複数行が複数求人になる() {
        let csv = "職種,給与,勤務地\n介護スタッフ,25万円,東京\nドライバー,30万円,大阪\n";
        let rows = parse_csv(csv).unwrap();
        let jobs = rows_to_jobs(rows);
        assert_eq!(jobs.len(), 2);
        assert_eq!(jobs[0].title_hint, "介護スタッフ");
        assert_eq!(jobs[1].title_hint, "ドライバー");
        assert!(jobs[0].source_text.contains("職種: 介護スタッフ"));
        assert!(jobs[0].source_text.contains("給与: 25万円"));
        assert!(jobs[0].source_text.contains("勤務地: 東京"));
    }

    #[test]
    fn csv_クォート内カンマを1フィールドとして扱う() {
        let csv = "職種,給与\n\"営業, 法人\",\"300,000円\"\n";
        let rows = parse_csv(csv).unwrap();
        assert_eq!(rows[1], vec!["営業, 法人".to_string(), "300,000円".to_string()]);
        let jobs = rows_to_jobs(rows);
        assert_eq!(jobs.len(), 1);
        assert_eq!(jobs[0].title_hint, "営業, 法人");
        assert!(jobs[0].source_text.contains("給与: 300,000円"));
    }

    #[test]
    fn csv_title列がなければ先頭の非空値を使う() {
        let csv = "col1,col2\n,値B\n";
        let rows = parse_csv(csv).unwrap();
        assert!(find_title_col(&rows[0]).is_none());
        let jobs = rows_to_jobs(rows);
        assert_eq!(jobs.len(), 1);
        assert_eq!(jobs[0].title_hint, "値B");
    }

    #[test]
    fn rows_to_jobs_空行をスキップ() {
        let rows = vec![
            vec!["職種".to_string(), "給与".to_string()],
            vec!["".to_string(), "".to_string()],
            vec!["調理".to_string(), "22万".to_string()],
        ];
        let jobs = rows_to_jobs(rows);
        assert_eq!(jobs.len(), 1);
        assert_eq!(jobs[0].title_hint, "調理");
    }

    #[test]
    fn cell_to_string_数値は整数表記() {
        assert_eq!(cell_to_string(&Data::Float(250000.0)), "250000");
        assert_eq!(cell_to_string(&Data::Float(1.5)), "1.5");
        assert_eq!(cell_to_string(&Data::Int(42)), "42");
        assert_eq!(cell_to_string(&Data::String("東京".into())), "東京");
        assert_eq!(cell_to_string(&Data::Empty), "");
    }

    #[test]
    fn base64_不正入力はエラー() {
        // '!' は base64 アルファベット外。
        let err = decode_b64("not_valid_base64!!!");
        assert!(err.is_err());
        // 正常系(空白混入も許容)。
        let ok = decode_b64("aGVsbG8=").unwrap();
        assert_eq!(ok, b"hello");
        let ok2 = decode_b64("aGVs\nbG8=").unwrap();
        assert_eq!(ok2, b"hello");
    }

    #[tokio::test]
    async fn html種別はテキスト化して1件() {
        let jobs = normalize(
            InputKind::Html,
            Some("<h1>介護職</h1><p>月給25万円</p>".to_string()),
            None,
            None,
        )
        .await
        .unwrap();
        assert_eq!(jobs.len(), 1);
        assert_eq!(jobs[0].title_hint, "介護職");
        assert!(jobs[0].source_text.contains("月給25万円"));
    }

    #[tokio::test]
    async fn pdf_data_base64が無ければ引数エラー() {
        let e = normalize(InputKind::Pdf, None, None, None).await;
        assert!(e.is_err());
    }

    #[tokio::test]
    async fn pdf_壊れたバイナリは明確なエラー() {
        // 有効な base64 だが PDF ではないバイト列。
        let b64 = base64::engine::general_purpose::STANDARD.encode(b"not a pdf at all");
        let e = normalize(InputKind::Pdf, None, None, Some(b64)).await;
        assert!(e.is_err(), "PDFでないバイナリは抽出エラーになる");
    }

    /// バイトオフセット付き xref を持つ最小の有効PDFを組み立てる(テスト用)。
    #[cfg(feature = "pdf")]
    fn build_min_pdf(text: &str) -> Vec<u8> {
        let content = format!("BT /F1 24 Tf 72 700 Td ({text}) Tj ET");
        let objs: Vec<String> = vec![
            "<</Type/Catalog/Pages 2 0 R>>".into(),
            "<</Type/Pages/Kids[3 0 R]/Count 1>>".into(),
            "<</Type/Page/Parent 2 0 R/MediaBox[0 0 612 792]/Resources<</Font<</F1 4 0 R>>>>/Contents 5 0 R>>".into(),
            "<</Type/Font/Subtype/Type1/BaseFont/Helvetica>>".into(),
            format!("<</Length {}>>stream\n{}\nendstream", content.len(), content),
        ];
        let mut pdf = Vec::new();
        pdf.extend_from_slice(b"%PDF-1.4\n");
        let mut offsets = Vec::new();
        for (i, body) in objs.iter().enumerate() {
            offsets.push(pdf.len());
            pdf.extend_from_slice(format!("{} 0 obj\n{}\nendobj\n", i + 1, body).as_bytes());
        }
        let xref_pos = pdf.len();
        let size = objs.len() + 1;
        pdf.extend_from_slice(format!("xref\n0 {size}\n").as_bytes());
        pdf.extend_from_slice(b"0000000000 65535 f \n"); // 各エントリ20バイト固定
        for off in &offsets {
            pdf.extend_from_slice(format!("{off:010} 00000 n \n").as_bytes());
        }
        pdf.extend_from_slice(
            format!("trailer<</Size {size}/Root 1 0 R>>\nstartxref\n{xref_pos}\n%%EOF").as_bytes(),
        );
        pdf
    }

    // pdf feature 有効時に、合成PDFから実際にテキストを抽出できることを確認する。
    #[cfg(feature = "pdf")]
    #[tokio::test]
    async fn pdf_合成pdfからテキスト抽出() {
        let pdf = build_min_pdf("Hello PDF");
        let b64 = base64::engine::general_purpose::STANDARD.encode(&pdf);
        let jobs = normalize(InputKind::Pdf, None, None, Some(b64)).await.unwrap();
        assert_eq!(jobs.len(), 1);
        assert!(
            jobs[0].source_text.contains("Hello"),
            "抽出テキスト: {:?}",
            jobs[0].source_text
        );
    }

    // ── SSRF: is_forbidden_ip の境界値 ──────────────────────
    fn v4(s: &str) -> IpAddr {
        IpAddr::V4(s.parse::<Ipv4Addr>().unwrap())
    }
    fn v6(s: &str) -> IpAddr {
        IpAddr::V6(s.parse::<Ipv6Addr>().unwrap())
    }

    #[test]
    fn is_forbidden_ip_private_loopback_linklocal_unspecifiedを拒否() {
        // private の3レンジ + 境界
        assert!(is_forbidden_ip(v4("10.0.0.1")));
        assert!(is_forbidden_ip(v4("172.16.0.1")));
        assert!(is_forbidden_ip(v4("172.31.255.255")));
        assert!(is_forbidden_ip(v4("192.168.1.1")));
        // loopback / link-local / unspecified
        assert!(is_forbidden_ip(v4("127.0.0.1")));
        assert!(is_forbidden_ip(v4("169.254.0.1")));
        assert!(is_forbidden_ip(v4("0.0.0.0")));
        // IPv6
        assert!(is_forbidden_ip(v6("::1")));
        assert!(is_forbidden_ip(v6("::")));
        assert!(is_forbidden_ip(v6("fe80::1")));
        assert!(is_forbidden_ip(v6("fc00::1"))); // ULA
    }

    #[test]
    fn is_forbidden_ip_cgnatの境界() {
        // 100.64.0.0/10 = 100.64.0.0 〜 100.127.255.255
        assert!(is_forbidden_ip(v4("100.64.0.0")));
        assert!(is_forbidden_ip(v4("100.127.255.255")));
        // 直下・直上は公開扱い(許可)
        assert!(!is_forbidden_ip(v4("100.63.255.255")));
        assert!(!is_forbidden_ip(v4("100.128.0.0")));
    }

    #[test]
    fn is_forbidden_ip_公開ipは許可() {
        assert!(!is_forbidden_ip(v4("8.8.8.8")));
        assert!(!is_forbidden_ip(v4("1.1.1.1")));
        // private 隣接の公開帯
        assert!(!is_forbidden_ip(v4("172.15.255.255")));
        assert!(!is_forbidden_ip(v4("172.32.0.0")));
        assert!(!is_forbidden_ip(v4("11.0.0.1")));
        // 公開 IPv6
        assert!(!is_forbidden_ip(v6("2606:4700:4700::1111")));
    }

    #[test]
    fn is_forbidden_ip_ipv4射影ipv6はv4として判定() {
        assert!(is_forbidden_ip(v6("::ffff:127.0.0.1"))); // loopback
        assert!(is_forbidden_ip(v6("::ffff:10.0.0.1"))); // private
        assert!(!is_forbidden_ip(v6("::ffff:8.8.8.8"))); // 公開
    }

    // ── リソース枯渇: サイズ上限 ──────────────────────
    #[test]
    fn base64_文字列長の上限で拒否() {
        let big = "A".repeat(MAX_B64_CHARS + 1);
        let e = decode_b64(&big);
        assert!(e.is_err(), "上限超過の base64 は拒否される");
        // 上限内の正常な base64 は通る。
        assert_eq!(decode_b64("aGVsbG8=").unwrap(), b"hello");
    }

    #[test]
    fn text_入力の上限で拒否() {
        let big = "あ".repeat(MAX_TEXT_BYTES); // 1文字3バイト → 上限を確実に超える
        assert!(big.len() > MAX_TEXT_BYTES);
        assert!(ensure_text_size(&big, "自由テキスト").is_err());
        // 上限内は通る。
        assert!(ensure_text_size("短いテキスト", "自由テキスト").is_ok());
    }

    #[tokio::test]
    async fn free_text_上限超過はnormalizeでエラー() {
        let big = "a".repeat(MAX_TEXT_BYTES + 1);
        let e = normalize(InputKind::FreeText, Some(big), None, None).await;
        assert!(e.is_err());
    }

    #[tokio::test]
    async fn url_httpスキーム以外は拒否() {
        // ネットワークに出る前にスキーム検査で弾かれる(ライブ通信なし)。
        let e = normalize(
            InputKind::Url,
            None,
            Some("file:///etc/passwd".to_string()),
            None,
        )
        .await;
        assert!(e.is_err());
        let e2 = normalize(InputKind::Url, None, Some("ftp://x/y".to_string()), None).await;
        assert!(e2.is_err());
    }

    #[tokio::test]
    async fn url_localhostは拒否() {
        // "localhost" はDNS解決前にホスト名で弾く(ライブ通信なし)。
        let e = normalize(
            InputKind::Url,
            None,
            Some("http://localhost:8080/admin".to_string()),
            None,
        )
        .await;
        assert!(e.is_err());
    }
}
