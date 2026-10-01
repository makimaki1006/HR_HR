//! ガイドタブ (使い方ガイド)
//!
//! 章データ (`GuideResponse`) を 1 つだけ持ち、次の 2 経路で使う。
//! - 旧画面 `/tab/guide`: `render_guide_html()` が同じデータから HTML を組み立てる
//! - 新画面 `/app/guide`: `GET /api/guide` が同じデータを JSON で返す
//!
//! 文言の正本は `guide_content.json` (本文・画像パスを含む)。文言を直すのはここ 1 か所。
//! 旧 HTML は env・権限・DB に依存しない静的文書で、リンク (`<a>`) は含まない
//! (他タブへの言及は「📊 地域概況」等のテキストのみ)。
//!
//! スタイルは `*Style` の列挙で持つ。旧画面は Tailwind クラスに、React 画面は CSS クラスに対応づける。

use axum::response::{Html, IntoResponse, Response};
use axum::Json;
use serde::{Deserialize, Serialize};
use ts_rs::TS;

use super::helpers::escape_html;

/// 章データの正本 (コンパイル時に埋め込む。実行時の I/O なし)。
const GUIDE_CONTENT_JSON: &str = include_str!("guide_content.json");

/// `GET /api/guide` の応答。`blocks` は旧画面の最上位 `<div class="space-y-4">` の直下の並び。
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize, TS)]
pub struct GuideResponse {
    pub blocks: Vec<GuideBlock>,
}

/// 文中の強調。
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize, TS)]
#[serde(rename_all = "snake_case")]
pub enum SpanStyle {
    Plain,
    /// `<strong>`
    Strong,
    /// 白字の `<strong>`
    StrongWhite,
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize, TS)]
pub struct GuideSpan {
    pub text: String,
    pub style: SpanStyle,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize, TS)]
#[serde(rename_all = "snake_case")]
pub enum GroupStyle {
    Plain,
    Indent,
    IndentSpaced,
    Callout,
    Gap,
    GapSpaced,
    GapSpacedSmall,
    GapSpacedMuted,
    /// 枠付きの小パネル (サブタブ説明)
    Panel,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize, TS)]
#[serde(rename_all = "snake_case")]
pub enum DetailsStyle {
    /// 大見出しの折りたたみ (カード直下)
    Card,
    /// タブ別の折りたたみ
    Tab,
    /// 画面イメージの折りたたみ
    Shots,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize, TS)]
#[serde(rename_all = "snake_case")]
pub enum HeadingStyle {
    Title,
    Section,
    Sub,
    SubTight,
    Panel,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize, TS)]
#[serde(rename_all = "snake_case")]
pub enum ParaStyle {
    /// `<p>` を作らず文中だけを並べる (callout の見出し行)
    Bare,
    Warn,
    WarnTight,
    Lead,
    Body,
    Note,
    Desc,
    Intro,
    IntroSpaced,
    Label,
    Caption,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize, TS)]
#[serde(rename_all = "snake_case")]
pub enum ListStyle {
    BulletIndent,
    BulletSpaced,
    Numbered,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize, TS)]
#[serde(rename_all = "snake_case")]
pub enum CellStyle {
    Plain,
    White,
    WhiteBold,
    Amber,
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize, TS)]
pub struct GuideCell {
    pub style: CellStyle,
    pub spans: Vec<GuideSpan>,
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize, TS)]
pub struct GuideTable {
    /// 小さめの余白 (パネル内の表)
    pub dense: bool,
    /// 1 列目を幅 1/2 にする
    pub wide_first_col: bool,
    /// 本文行を淡色にする
    pub muted_body: bool,
    pub headers: Vec<String>,
    pub rows: Vec<Vec<GuideCell>>,
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize, TS)]
#[serde(tag = "kind", rename_all = "snake_case")]
pub enum GuideBlock {
    Card {
        blocks: Vec<GuideBlock>,
    },
    Group {
        style: GroupStyle,
        blocks: Vec<GuideBlock>,
    },
    Details {
        style: DetailsStyle,
        summary: String,
        blocks: Vec<GuideBlock>,
    },
    Heading {
        style: HeadingStyle,
        text: String,
    },
    Para {
        style: ParaStyle,
        spans: Vec<GuideSpan>,
    },
    List {
        style: ListStyle,
        items: Vec<Vec<GuideSpan>>,
    },
    Table(GuideTable),
    Image {
        src: String,
        alt: String,
    },
}

/// 章データを読む。埋め込み JSON が壊れていればテストで落ちる (実行時は panic せず空を返す)。
pub fn guide_data() -> GuideResponse {
    serde_json::from_str(GUIDE_CONTENT_JSON).unwrap_or_else(|e| {
        tracing::error!("guide_content.json を読めない: {e}");
        GuideResponse { blocks: vec![] }
    })
}

// ---------------------------------------------------------------------------
// 旧 HTML への描画 (Tailwind クラス。旧画面の見た目を保つ)
// ---------------------------------------------------------------------------

fn span_html(spans: &[GuideSpan]) -> String {
    let mut s = String::new();
    for sp in spans {
        let t = escape_html(&sp.text);
        match sp.style {
            SpanStyle::Plain => s.push_str(&t),
            SpanStyle::Strong => s.push_str(&format!("<strong>{t}</strong>")),
            SpanStyle::StrongWhite => {
                s.push_str(&format!(r#"<strong class="text-white">{t}</strong>"#))
            }
        }
    }
    s
}

fn group_class(s: GroupStyle) -> &'static str {
    match s {
        GroupStyle::Plain => "",
        GroupStyle::Indent => "mt-2 ml-4",
        GroupStyle::IndentSpaced => "mt-2 ml-4 space-y-3",
        GroupStyle::Callout => "mt-2 p-2 bg-amber-900/30 rounded text-amber-300 text-xs",
        GroupStyle::Gap => "mt-3",
        GroupStyle::GapSpaced => "mt-3 space-y-4",
        GroupStyle::GapSpacedSmall => "mt-3 space-y-4 text-sm",
        GroupStyle::GapSpacedMuted => "mt-3 text-slate-400 text-sm space-y-4",
        GroupStyle::Panel => "bg-slate-800/50 rounded p-3",
    }
}

fn details_class(s: DetailsStyle) -> &'static str {
    match s {
        DetailsStyle::Card => "",
        DetailsStyle::Tab => "ml-2",
        DetailsStyle::Shots => "mt-4",
    }
}

fn summary_class(s: DetailsStyle) -> &'static str {
    match s {
        DetailsStyle::Card => "text-lg font-bold text-cyan-400 cursor-pointer hover:text-cyan-300",
        DetailsStyle::Tab => "text-white font-semibold cursor-pointer hover:text-cyan-300",
        DetailsStyle::Shots => "text-slate-300 text-sm cursor-pointer hover:text-cyan-300",
    }
}

fn para_class(s: ParaStyle) -> &'static str {
    match s {
        ParaStyle::Bare => "",
        ParaStyle::Warn => "mt-2 text-amber-400 text-xs",
        ParaStyle::WarnTight => "mt-1 text-amber-400 text-xs",
        ParaStyle::Lead => "mt-2 text-slate-300",
        ParaStyle::Body => "mt-2 text-slate-400 text-sm",
        ParaStyle::Note => "mt-2 text-slate-500 text-xs",
        ParaStyle::Desc => "text-slate-400 mt-1",
        ParaStyle::Intro => "text-slate-400 text-sm",
        ParaStyle::IntroSpaced => "text-slate-400 text-sm mb-2",
        ParaStyle::Label => "text-white font-semibold",
        ParaStyle::Caption => "text-xs text-slate-500 mb-1",
    }
}

fn heading_tag_class(s: HeadingStyle) -> (&'static str, &'static str) {
    match s {
        HeadingStyle::Title => ("h2", "text-xl font-bold text-white mb-2"),
        HeadingStyle::Section => ("h3", "text-lg font-bold text-cyan-400 mb-3"),
        HeadingStyle::Sub => ("h4", "text-white font-semibold mb-2"),
        HeadingStyle::SubTight => ("h4", "text-white font-semibold mb-1"),
        HeadingStyle::Panel => ("h5", "text-cyan-300 font-semibold text-sm mb-2"),
    }
}

fn list_tag_class(s: ListStyle) -> (&'static str, &'static str) {
    match s {
        ListStyle::BulletIndent => ("ul", "list-disc list-inside ml-2"),
        ListStyle::BulletSpaced => ("ul", "list-disc list-inside mt-1 space-y-1"),
        ListStyle::Numbered => (
            "ol",
            "list-decimal list-inside text-slate-400 text-sm space-y-1",
        ),
    }
}

fn cell_class(s: CellStyle, dense: bool) -> String {
    let pad = if dense { "py-1 px-2" } else { "py-2 px-3" };
    match s {
        CellStyle::Plain => pad.to_string(),
        CellStyle::White => format!("{pad} text-white"),
        CellStyle::WhiteBold => format!("{pad} font-semibold text-white"),
        CellStyle::Amber => format!("{pad} text-amber-400"),
    }
}

fn table_html(t: &GuideTable) -> String {
    let pad = if t.dense { "py-1 px-2" } else { "py-2 px-3" };
    let mut h = String::from(
        r#"<table class="w-full text-sm"><thead><tr class="border-b border-slate-700">"#,
    );
    for (i, head) in t.headers.iter().enumerate() {
        let half = if t.wide_first_col && i == 0 {
            " w-1/2"
        } else {
            ""
        };
        h.push_str(&format!(
            r#"<th class="text-left {pad} text-slate-300{half}">{}</th>"#,
            escape_html(head)
        ));
    }
    h.push_str("</tr></thead>");
    if t.muted_body {
        h.push_str(r#"<tbody class="text-slate-400">"#);
    } else {
        h.push_str("<tbody>");
    }
    let last = t.rows.len().saturating_sub(1);
    for (i, row) in t.rows.iter().enumerate() {
        if i < last {
            h.push_str(r#"<tr class="border-b border-slate-800">"#);
        } else {
            h.push_str("<tr>");
        }
        for c in row {
            h.push_str(&format!(
                r#"<td class="{}">{}</td>"#,
                cell_class(c.style, t.dense),
                span_html(&c.spans)
            ));
        }
        h.push_str("</tr>");
    }
    h.push_str("</tbody></table>");
    h
}

fn block_html(b: &GuideBlock) -> String {
    match b {
        GuideBlock::Card { blocks } => {
            format!(r#"<div class="stat-card">{}</div>"#, blocks_html(blocks))
        }
        GuideBlock::Group { style, blocks } => {
            let c = group_class(*style);
            if c.is_empty() {
                format!("<div>{}</div>", blocks_html(blocks))
            } else {
                format!(r#"<div class="{c}">{}</div>"#, blocks_html(blocks))
            }
        }
        GuideBlock::Details {
            style,
            summary,
            blocks,
        } => {
            let c = details_class(*style);
            let open = if c.is_empty() {
                "<details>".to_string()
            } else {
                format!(r#"<details class="{c}">"#)
            };
            format!(
                r#"{open}<summary class="{}">{}</summary>{}</details>"#,
                summary_class(*style),
                escape_html(summary),
                blocks_html(blocks)
            )
        }
        GuideBlock::Heading { style, text } => {
            let (tag, c) = heading_tag_class(*style);
            format!(r#"<{tag} class="{c}">{}</{tag}>"#, escape_html(text))
        }
        GuideBlock::Para { style, spans } => match style {
            ParaStyle::Bare => span_html(spans),
            _ => format!(
                r#"<p class="{}">{}</p>"#,
                para_class(*style),
                span_html(spans)
            ),
        },
        GuideBlock::List { style, items } => {
            let (tag, c) = list_tag_class(*style);
            let lis: String = items
                .iter()
                .map(|i| format!("<li>{}</li>", span_html(i)))
                .collect();
            format!(r#"<{tag} class="{c}">{lis}</{tag}>"#)
        }
        GuideBlock::Table(t) => table_html(t),
        GuideBlock::Image { src, alt } => format!(
            r#"<img src="{}" alt="{}" class="rounded border border-slate-700 w-full" loading="lazy">"#,
            escape_html(src),
            escape_html(alt)
        ),
    }
}

fn blocks_html(bs: &[GuideBlock]) -> String {
    bs.iter().map(block_html).collect()
}

/// 章データから旧画面の HTML を組み立てる。
pub fn render_guide_html(data: &GuideResponse) -> String {
    format!(
        r#"<div class="space-y-4">{}</div>"#,
        blocks_html(&data.blocks)
    )
}

/// ガイドHTMLを構築 (章データ → HTML)。
pub fn build_guide_html() -> String {
    render_guide_html(&guide_data())
}

/// ガイドタブ: 取扱説明書をHTML形式で表示
pub async fn tab_guide() -> Html<String> {
    Html(build_guide_html())
}

/// `GET /api/guide`: 同じ章データを JSON で返す (React 画面用)。
pub async fn api_guide() -> Response {
    Json(guide_data()).into_response()
}

#[cfg(test)]
mod tests {
    use super::*;

    /// 旧 `build_guide_html()` (章データ化の前) の出力。分割前のソースから取り出した固定物。
    const OLD_HTML: &str = include_str!("../../tests/fixtures/guide/old_tab_guide.html");

    fn unescape(s: &str) -> String {
        s.replace("&lt;", "<")
            .replace("&gt;", ">")
            .replace("&quot;", "\"")
            .replace("&#x27;", "'")
            .replace("&amp;", "&")
    }

    /// 比較用の正規化: コメント除去、タグ間の空白除去、class の順序無視、実体参照の展開。
    /// 文中の空白 (`</strong>` の前後) は残す。
    fn normalize(html: &str) -> String {
        let mut s = String::new();
        let mut rest = html;
        while let Some(i) = rest.find("<!--") {
            s.push_str(&rest[..i]);
            let j = rest[i..].find("-->").expect("コメントが閉じていない") + i + 3;
            rest = &rest[j..];
        }
        s.push_str(rest);
        let mut toks: Vec<(bool, String)> = Vec::new();
        let mut cur = s.as_str();
        while !cur.is_empty() {
            if cur.starts_with('<') {
                let end = cur.find('>').expect("タグが閉じていない");
                toks.push((true, cur[1..end].to_string()));
                cur = &cur[end + 1..];
            } else {
                let end = cur.find('<').unwrap_or(cur.len());
                toks.push((false, cur[..end].to_string()));
                cur = &cur[end..];
            }
        }
        let is_strong =
            |t: &(bool, String)| t.0 && t.1.trim_start_matches('/').starts_with("strong");
        let mut out = String::new();
        for (i, (is_tag, t)) in toks.iter().enumerate() {
            if *is_tag {
                let mut parts = t.splitn(2, char::is_whitespace);
                let name = parts.next().unwrap();
                let mut a = parts.next().unwrap_or("").trim().to_string();
                if let Some(p) = a.find("class=\"") {
                    let q = a[p + 7..].find('"').unwrap() + p + 7;
                    let mut cls: Vec<&str> = a[p + 7..q].split_whitespace().collect();
                    cls.sort();
                    a = format!("{}class=\"{}\"{}", &a[..p], cls.join(" "), &a[q + 1..]);
                }
                out.push('<');
                out.push_str(name);
                if !a.is_empty() {
                    out.push(' ');
                    out.push_str(&a);
                }
                out.push('>');
            } else {
                let collapsed = unescape(&t.split_whitespace().collect::<Vec<_>>().join(" "));
                if collapsed.is_empty() {
                    continue;
                }
                let lead = t.starts_with(char::is_whitespace);
                let trail = t.ends_with(char::is_whitespace);
                if lead && i > 0 && is_strong(&toks[i - 1]) {
                    out.push(' ');
                }
                out.push_str(&collapsed);
                if trail && toks.get(i + 1).is_some_and(is_strong) {
                    out.push(' ');
                }
            }
        }
        out
    }

    fn walk<'a>(bs: &'a [GuideBlock], f: &mut impl FnMut(&'a GuideBlock)) {
        for b in bs {
            f(b);
            match b {
                GuideBlock::Card { blocks }
                | GuideBlock::Group { blocks, .. }
                | GuideBlock::Details { blocks, .. } => walk(blocks, f),
                _ => {}
            }
        }
    }

    /// 章データ化の前後で HTML が一致する。
    #[test]
    fn 分割前後でhtmlが一致する() {
        assert_eq!(normalize(OLD_HTML), normalize(&build_guide_html()));
    }

    /// 比較関数自体が差を検出できること (逆証明)。
    #[test]
    fn 正規化比較は文言の差を検出する() {
        let broken = build_guide_html().replace("欠員補充率", "欠員補充率X");
        assert_ne!(normalize(OLD_HTML), normalize(&broken));
        let broken2 = build_guide_html().replace("trend_sub3.png", "trend_sub9.png");
        assert_ne!(normalize(OLD_HTML), normalize(&broken2));
    }

    /// 章データの具体値 (React 画面が受け取る JSON と同じもの)。
    #[test]
    fn 章データの具体値() {
        let d = guide_data();
        let (mut tables, mut details, mut cards) = (0, 0, 0);
        let mut images: Vec<(String, String)> = vec![];
        let mut first_title = None;
        walk(&d.blocks, &mut |b| match b {
            GuideBlock::Table(_) => tables += 1,
            GuideBlock::Details { .. } => details += 1,
            GuideBlock::Card { .. } => cards += 1,
            GuideBlock::Image { src, alt } => images.push((src.clone(), alt.clone())),
            GuideBlock::Heading {
                style: HeadingStyle::Title,
                text,
            } => first_title = Some(text.clone()),
            _ => {}
        });
        assert_eq!(cards, 9);
        assert_eq!(tables, 17);
        assert_eq!(details, 17);
        assert_eq!(first_title.as_deref(), Some("📖 取扱説明書"));
        let expect: Vec<(String, String)> = [
            ("trend_sub1.png", "トレンド: 量の変化"),
            ("trend_sub2.png", "トレンド: 質の変化"),
            ("trend_sub3.png", "トレンド: 構造の変化"),
            ("trend_sub4.png", "トレンド: シグナル"),
            ("trend_sub5.png", "トレンド: 外部比較"),
            ("trend_tokyo.png", "トレンド: 東京都"),
        ]
        .iter()
        .map(|(f, a)| (format!("/static/guide/{f}"), a.to_string()))
        .collect();
        assert_eq!(images, expect);
        // 逆引きインデックス (2 枚目のカード): 10 行、先頭行と最終行
        let GuideBlock::Card { blocks } = &d.blocks[1] else {
            panic!("2 番目はカード")
        };
        let GuideBlock::Table(t) = &blocks[1] else {
            panic!("インデックス表")
        };
        assert_eq!(t.headers, vec!["知りたいこと", "見るべきタブ"]);
        assert!(t.wide_first_col);
        assert_eq!(t.rows.len(), 10);
        assert_eq!(t.rows[0][0].spans[0].text, "この地域の求人市場の全体像");
        assert_eq!(t.rows[0][1].spans[0].text, "📊 地域概況");
        assert_eq!(t.rows[9][1].spans[0].text, "📈 トレンド → 外部比較");
    }

    /// 画像パスが実在する (static/guide/*.png)。
    #[test]
    fn 画像ファイルが存在する() {
        let mut srcs = vec![];
        walk(&guide_data().blocks, &mut |b| {
            if let GuideBlock::Image { src, .. } = b {
                srcs.push(src.clone());
            }
        });
        assert_eq!(srcs.len(), 6);
        for s in srcs {
            let p =
                std::path::Path::new(env!("CARGO_MANIFEST_DIR")).join(s.trim_start_matches('/'));
            assert!(p.exists(), "{}", p.display());
        }
    }

    /// JSON の形 (tag 名・snake_case)。
    #[tokio::test]
    async fn api_guideのjson契約() {
        let res = api_guide().await;
        let bytes = axum::body::to_bytes(res.into_body(), 1 << 22)
            .await
            .unwrap();
        let v: serde_json::Value = serde_json::from_slice(&bytes).unwrap();
        let blocks = v["blocks"].as_array().unwrap();
        assert_eq!(blocks.len(), 9);
        assert_eq!(blocks[0]["kind"], "card");
        assert_eq!(blocks[0]["blocks"][0]["kind"], "heading");
        assert_eq!(blocks[0]["blocks"][0]["style"], "title");
        assert_eq!(blocks[0]["blocks"][0]["text"], "📖 取扱説明書");
        assert_eq!(blocks[1]["blocks"][1]["kind"], "table");
        assert_eq!(blocks[1]["blocks"][1]["rows"].as_array().unwrap().len(), 10);
        let back: GuideResponse = serde_json::from_slice(&bytes).unwrap();
        assert_eq!(back, guide_data());
    }

    #[test]
    fn ts型の宣言() {
        let cfg = ts_rs::Config::default();
        let d = GuideBlock::decl(&cfg);
        assert!(d.contains("\"kind\": \"card\""), "{d}");
        assert!(d.contains("\"kind\": \"image\""), "{d}");
        assert!(GuideResponse::decl(&cfg).contains("blocks: Array<GuideBlock>"));
    }
}
