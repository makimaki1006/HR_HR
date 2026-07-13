//! 商談準備レポート HTML 出力 (計画書 §12。内部識別子は brief のまま)
//!
//! - 最大8ページ構成 (summary-first)。既存 navy スタイル (page-navy 等) を流用
//! - 各ページに「社内用 — 顧客配布不可」の帯を明記
//! - 顧客向けレポート (report_html) からはリンクしない
//! - 描画は `ConsultAnalysis` (構造化結果) + `AiComposite` (検証済みAI文章) のみを参照し、
//!   原データへ直接アクセスしない (§15.2)
//! - 断定表現禁止 (§19.2)。仮説は全て「〜の可能性」
//!
//! 印刷CSS注意 (feedback_print_css_cascade_trap):
//! navy CSS の @page (size/margin) はそのまま使い、本モジュールでは @page の
//! margin/size を再定義しない。フッター文言 (@bottom-left content) のみ
//! 後勝ちカスケードで上書きする。

use super::ai::AiComposite;
use super::axes::AxisLevel;
use super::evidence::Evidence;
use super::evidence_pack::ConsultAnalysis;
use super::questions::REQUIRED_HEARING_ITEMS;
use crate::handlers::helpers::escape_html;

/// §12.3-4 確認すべき不足情報の基本リスト
const BASE_MISSING_INFO: [&str; 11] = [
    "応募数",
    "表示数",
    "クリック数",
    "面接設定率",
    "面接実施率",
    "内定率",
    "承諾率",
    "初回連絡時間",
    "辞退理由",
    "採用者属性",
    "応募者居住地",
];

/// 商談準備レポート専用の追加CSS (navy CSS の後に読み込む)
fn consult_css() -> &'static str {
    r#"
/* ==== 商談準備レポート (社内用) 追加スタイル ==== */
/* @page の size/margin は navy CSS の定義を継承し、フッター文言のみ上書きする */
@page {
  @bottom-left {
    content: "FOR A-CAREER  /  商談準備レポート [社内用 - 顧客配布不可]";
    font-family: "Noto Sans JP", sans-serif;
    font-size: 8pt;
    color: #A8331F;
    letter-spacing: 0.04em;
  }
}
body.theme-navy .consult-internal-band {
  display: flex;
  align-items: center;
  gap: 8px;
  background: #FBEAE6;
  border: 1.5px solid #A8331F;
  color: #A8331F;
  font-size: 9.5pt;
  font-weight: 700;
  letter-spacing: 0.06em;
  padding: 5px 12px;
  margin-bottom: 3mm;
  -webkit-print-color-adjust: exact;
  print-color-adjust: exact;
}
/* 商談準備レポートは最大8ページ。見出しをレポート本体より小ぶりにする */
body.theme-navy .page-navy .page-head { margin-bottom: 5mm; }
body.theme-navy .page-navy .ph-title { font-size: 16pt; }
/* AI複合考察カード */
body.theme-navy .consult-ai-item {
  break-inside: avoid;
  page-break-inside: avoid;
  border: 1px solid var(--rule, #D8D2C4);
  border-left: 3px solid #1F2D4D;
  padding: 2.5mm 3.5mm;
  margin-bottom: 3mm;
  background: #FBFAF6;
  -webkit-print-color-adjust: exact;
  print-color-adjust: exact;
}
body.theme-navy .consult-ai-item .ai-title { font-weight: 700; font-size: 10pt; color: #1F2D4D; }
body.theme-navy .consult-ai-item .ai-body { font-size: 9pt; line-height: 1.7; margin: 1mm 0; }
body.theme-navy .consult-ai-item .ai-meta { font-size: 7.5pt; color: #6A6E7A; }
body.theme-navy .consult-ai-badge {
  display: inline-block; font-size: 7.5pt; font-weight: 700; color: #1F2D4D;
  border: 1px solid #1F2D4D; border-radius: 3px; padding: 0 4px; margin-left: 6px;
}
/* 反証あり考察の注意ラベル */
body.theme-navy .consult-ai-refute-flag {
  display: inline-block; font-size: 7.5pt; font-weight: 700; color: #A8331F;
  border: 1px solid #A8331F; border-radius: 3px; padding: 0 5px; margin-left: 8px;
  background: #FBEAE6;
  -webkit-print-color-adjust: exact; print-color-adjust: exact;
}
/* 逆の見方 / 別の見方 ブロック (考察カード内で視覚的に区別) */
body.theme-navy .consult-ai-counter {
  margin: 1.5mm 0 0.5mm; padding: 1.5mm 2.5mm;
  border-left: 3px solid #6A6E7A; background: #F3F3EF;
  -webkit-print-color-adjust: exact; print-color-adjust: exact;
}
body.theme-navy .consult-ai-counter.refuted {
  border-left-color: #A8331F; background: #FBF3F1;
}
body.theme-navy .consult-ai-counter .counter-label {
  font-size: 8pt; font-weight: 700; color: #6A6E7A; margin-bottom: 0.5mm;
}
body.theme-navy .consult-ai-counter.refuted .counter-label { color: #A8331F; }
body.theme-navy .consult-ai-counter .counter-body {
  font-size: 8.5pt; line-height: 1.6; color: #1F2D4D;
}
body.theme-navy .consult-ai-noreview { font-style: italic; color: #6A6E7A; }
/* 矛盾・質問ブロックの2カラム配置 (縦方向の圧縮) */
body.theme-navy .consult-2col {
  display: grid;
  grid-template-columns: 1fr 1fr;
  gap: 2mm 6mm;
  align-items: start;
}
body.theme-navy .consult-note {
  font-size: 8.5pt;
  color: var(--ink-muted, #6A6E7A);
  margin: 2mm 0 4mm;
  line-height: 1.6;
}
body.theme-navy .consult-axis-level {
  font-family: "Noto Sans JP", sans-serif;
  font-weight: 700;
}
body.theme-navy .consult-fill {
  min-height: 6mm;
  border: 1px dashed var(--rule, #D8D2C4);
  padding: 1.5mm 3mm;
  margin: 1mm 0 2.5mm;
  font-size: 9pt;
  color: var(--ink-soft, #1F2D4D);
}
body.theme-navy .consult-branch {
  margin: 0.5mm 0 1.5mm 5mm;
  font-size: 8.5pt;
  color: var(--ink-soft, #1F2D4D);
}
body.theme-navy .consult-branch li { margin-bottom: 0.5mm; }
body.theme-navy .consult-q-block {
  break-inside: avoid;
  page-break-inside: avoid;
  margin-bottom: 3mm;
}
body.theme-navy .consult-check-grid {
  display: grid;
  grid-template-columns: 1fr 1fr 1fr;
  gap: 1mm 5mm;
  font-size: 8.5pt;
}
body.theme-navy .consult-check-grid-wide {
  grid-template-columns: 1fr 1fr 1fr 1fr;
}
body.theme-navy .consult-evidence-table th,
body.theme-navy .consult-evidence-table td {
  padding: 1px 6px 1px 0;
  font-size: 7pt;
  line-height: 1.5;
  /* P1-8: 通勤流入元上位のような長文セルが折り返さず重なるのを防ぐ */
  white-space: normal;
  overflow-wrap: anywhere;
  word-break: break-word;
}
body.theme-navy .table-navy td, body.theme-navy .table-navy th { vertical-align: top; }
/* 全テーブルの値・指標セルも長文で重ならないよう折り返す (P1-8) */
body.theme-navy .table-navy td { overflow-wrap: anywhere; }
/* 面談の掴み (差別化の核) カード */
body.theme-navy .consult-grip-item {
  break-inside: avoid;
  page-break-inside: avoid;
  border: 1px solid var(--rule, #D8D2C4);
  border-left: 4px solid #A8331F;
  padding: 3mm 4mm;
  margin-bottom: 3.5mm;
  background: #FBF6F4;
  -webkit-print-color-adjust: exact;
  print-color-adjust: exact;
}
body.theme-navy .consult-grip-item .grip-head {
  font-weight: 700; font-size: 11pt; color: #A8331F; margin-bottom: 1.5mm;
}
body.theme-navy .consult-grip-item .grip-no {
  display: inline-block; min-width: 6mm; height: 6mm; line-height: 6mm; text-align: center;
  background: #A8331F; color: #fff; border-radius: 50%; font-size: 8.5pt; margin-right: 6px;
  -webkit-print-color-adjust: exact; print-color-adjust: exact;
}
body.theme-navy .consult-grip-item .grip-talk {
  font-size: 9.5pt; line-height: 1.75; margin: 1mm 0; color: #1F2D4D;
}
body.theme-navy .consult-grip-item .grip-followup {
  font-size: 8.5pt; line-height: 1.6; color: #1F6B43; margin: 1.5mm 0 0.5mm;
}
"#
}

fn axis_level_html(level: AxisLevel) -> String {
    let color = match level {
        AxisLevel::High => "#A8331F",
        AxisLevel::Medium => "#B5731C",
        AxisLevel::Low => "#1F6B43",
        AxisLevel::Unknown => "#6A6E7A",
    };
    format!(
        "<span class=\"consult-axis-level\" style=\"color:{}\">{}</span>",
        color,
        level.label_ja()
    )
}

fn internal_band() -> &'static str {
    r#"<div class="consult-internal-band" role="note">&#128274; 社内用 &#8212; 顧客配布不可 / INTERNAL USE ONLY</div>"#
}

fn page_head(sec: &str, title: &str, sub: &str) -> String {
    format!(
        r#"<div class="page-head">
  <div class="ph-sec">{}</div>
  <div class="ph-title">{}</div>
  <div class="ph-sub">{}</div>
  <div class="ph-rule"></div>
</div>"#,
        escape_html(sec),
        escape_html(title),
        escape_html(sub)
    )
}

/// 証拠IDから表示用の短い文字列を作る (「E-001 求人件数 180件」形式)
fn evidence_chip(evidence: &[Evidence], id: &str) -> String {
    match evidence.iter().find(|e| e.id == id) {
        Some(e) => {
            let unit = if e.unit.is_empty() {
                String::new()
            } else {
                escape_html(&e.unit)
            };
            format!(
                "<span style=\"white-space:nowrap\">[{}]</span> {} {}{}",
                escape_html(&e.id),
                escape_html(&e.metric_name),
                escape_html(&e.value_text),
                unit
            )
        }
        None => format!("[{}]", escape_html(id)),
    }
}

/// 表示するチップの最大数 (超過分は「+他n件」でIDのみ表記。詳細は証拠一覧で参照)
const CHIP_MAX: usize = 3;

fn evidence_chips(evidence: &[Evidence], ids: &[String]) -> String {
    if ids.is_empty() {
        return "&#8212;".to_string();
    }
    let mut parts: Vec<String> = ids
        .iter()
        .take(CHIP_MAX)
        .map(|id| evidence_chip(evidence, id))
        .collect();
    if ids.len() > CHIP_MAX {
        let rest: Vec<String> = ids[CHIP_MAX..].iter().map(|s| escape_html(s)).collect();
        parts.push(format!(
            "+他{}件 ({})",
            ids.len() - CHIP_MAX,
            rest.join(", ")
        ));
    }
    parts.join("<br>")
}

/// 4軸の水準を並べた要約 (機械判定の骨子)。
fn axes_summary_line(analysis: &ConsultAnalysis) -> String {
    let a = &analysis.axes;
    format!(
        "需要は「{}」、人材供給は「{}」、同職種の競争は「{}」、自社の給与面の位置づけは「{}」と観測される市場です",
        a.demand.level.label_ja(),
        a.supply.level.label_ja(),
        a.competition.level.label_ja(),
        a.offer_competitiveness.level.label_ja()
    )
}

/// 最重要シグナル1件の要点文 (発火シグナルのうち掴みに使う優先度で先頭)。
/// P1-8: AI要約が無いときのフォールバックを「4軸の羅列」で終わらせず、最重要シグナルを1つ含める。
fn top_signal_highlight(analysis: &ConsultAnalysis) -> Option<String> {
    // 掴みと同じ優先順で最初に発火しているものを1つ拾う (決定的)。
    const PRIORITY_ORDER: [&str; 8] = [
        "S-06", "S-07", "S-10", "S-12", "S-16", "S-02", "S-30", "S-01",
    ];
    for id in PRIORITY_ORDER {
        if let Some(s) = analysis
            .signals
            .iter()
            .find(|s| s.id == id && s.fired && !s.interpretation.is_empty())
        {
            return Some(s.interpretation.clone());
        }
    }
    None
}

/// 市場の一文要約 (§12.3-1) をテンプレで合成 (AI要約がないときのフォールバック)。
/// 4軸の羅列だけで終わらせず、最重要シグナルの要点を1つ添える (P1-8)。
fn template_one_line_summary(analysis: &ConsultAnalysis) -> String {
    let axes = axes_summary_line(analysis);
    match top_signal_highlight(analysis) {
        Some(highlight) => format!(
            "{}。とくに、{}（いずれも面談前の市場側データに基づく暫定判定）。",
            axes, highlight
        ),
        None => format!("{}（面談前の市場側データに基づく暫定判定）。", axes),
    }
}

/// 商談準備レポートHTML本体 (AI文章化なし版のショートカット。テスト・後方互換用)
pub fn render_consult_brief_html(analysis: &ConsultAnalysis) -> String {
    render_consult_brief_html_with_ai(analysis, &AiComposite::default())
}

/// 商談準備レポートHTML本体 (AI文章化つき)。
/// `ai` が空 (未設定・失敗・全破棄) の場合は AI セクションを省略し1行の注記を出す。
pub fn render_consult_brief_html_with_ai(analysis: &ConsultAnalysis, ai: &AiComposite) -> String {
    let mut html = String::with_capacity(96 * 1024);
    html.push_str("<!DOCTYPE html>\n<html lang=\"ja\" data-theme=\"default\">\n<head>\n");
    html.push_str("<meta charset=\"UTF-8\">\n");
    html.push_str("<meta name=\"viewport\" content=\"width=device-width,initial-scale=1\">\n");
    html.push_str("<meta name=\"robots\" content=\"noindex,nofollow\">\n");
    html.push_str("<title>商談準備レポート（社内用）</title>\n<style>\n");
    html.push_str(&crate::handlers::survey::report_html::navy_css_bundle());
    html.push_str(consult_css());
    html.push_str("</style>\n</head>\n<body class=\"theme-navy\">\n");

    // ページ番号は動的採番 (省略されるページがあってもラベルが連番になるよう counter を回す)。
    let mut page = 0usize;
    let mut sec = || {
        page += 1;
        format!("商談準備レポート / {:02}", page)
    };

    // summary-first: 1ページ目だけで要点 (要約 + 4軸) が分かる構成
    render_page1_summary(&mut html, analysis, ai, &sec());
    // 面談の掴み (差別化の核): 意外な事実 TOP3。発火シグナルが乏しく空なら省略。
    let grip_items = super::grip::select_grip_items(analysis);
    if !grip_items.is_empty() {
        render_page_grip(&mut html, &grip_items, &sec());
    }
    // 複合考察 (AI下書き): AIが空なら白紙ページを丸ごと出さず、後段の証拠一覧脚注に圧縮 (P1-8)。
    if !ai.items.is_empty() {
        render_page2_ai_composite(&mut html, analysis, ai, &sec());
    }
    render_page3_hypotheses(&mut html, analysis, &sec());
    render_page4_contradictions(&mut html, analysis, &sec());
    render_page5_evidence(&mut html, analysis, &sec(), ai);
    render_page6_questions_missing(&mut html, analysis, &sec());

    html.push_str("</body>\n</html>\n");
    html
}

/// ページ1: 市場環境の要約 (§12.3-1) + 4軸判定 (summary-first)
fn render_page1_summary(
    html: &mut String,
    analysis: &ConsultAnalysis,
    ai: &AiComposite,
    sec: &str,
) {
    let meta = &analysis.report_meta;
    html.push_str("<div class=\"page-navy\">\n");
    html.push_str(internal_band());
    html.push_str(&page_head(
        sec,
        "市場環境の要約",
        "面談前に確認できた市場側データの整理（顧客固有の課題判定は含みません）",
    ));

    // 対象・基準日テーブル
    html.push_str("<div class=\"block-title\">対象と基準</div>\n");
    html.push_str("<table class=\"table-navy\"><tbody>\n");
    let region = if meta.municipality.is_empty() {
        meta.prefecture.clone()
    } else {
        format!("{} {}", meta.prefecture, meta.municipality)
    };
    html.push_str(&format!(
        "<tr><th style=\"width:32mm\">対象地域</th><td>{}</td></tr>\n",
        escape_html(&region)
    ));
    let occ = if meta.occupation_note.is_empty() {
        "（CSVから職種は特定していません。面談で確認）".to_string()
    } else {
        meta.occupation_note.clone()
    };
    html.push_str(&format!(
        "<tr><th>対象職種</th><td>{}</td></tr>\n",
        escape_html(&occ)
    ));
    html.push_str(&format!(
        "<tr><th>データ基準日</th><td>{}</td></tr>\n",
        escape_html(&meta.generated_at)
    ));
    if !meta.client_input_summary.is_empty() {
        html.push_str(&format!(
            "<tr><th>顧客からの事前入力</th><td>{}</td></tr>\n",
            meta.client_input_summary
                .iter()
                .map(|s| escape_html(s))
                .collect::<Vec<_>>()
                .join("<br>")
        ));
    }
    html.push_str("</tbody></table>\n");

    // 一文要約 (AI要約があれば自然文を優先し、テンプレ要約も併記)
    html.push_str("<div class=\"block-title block-title-spaced\">市場の一文要約</div>\n");
    match &ai.one_line_summary {
        Some(s) => {
            html.push_str(&format!(
                "<p style=\"font-size:10.5pt;line-height:1.8\">{}<span class=\"consult-ai-badge\">AI要約</span></p>\n",
                escape_html(s)
            ));
            html.push_str(&format!(
                "<p class=\"consult-note\">4軸の機械判定: {}</p>\n",
                escape_html(&template_one_line_summary(analysis))
            ));
        }
        None => {
            html.push_str(&format!(
                "<p style=\"font-size:10.5pt;line-height:1.8\">{}</p>\n",
                escape_html(&template_one_line_summary(analysis))
            ));
        }
    }

    // 4軸判定 (§8.2: 総合点なし)
    html.push_str(
        "<div class=\"block-title block-title-spaced\">4軸判定（総合点は算出しません）</div>\n",
    );
    html.push_str("<table class=\"table-navy\"><thead><tr><th style=\"width:38mm\">軸</th><th style=\"width:20mm\">判定</th><th>判定理由</th><th style=\"width:24mm\">根拠ID</th></tr></thead><tbody>\n");
    for axis in analysis.axes.all() {
        html.push_str(&format!(
            "<tr><td>{}</td><td>{}</td><td>{}</td><td>{}</td></tr>\n",
            escape_html(&axis.axis_label),
            axis_level_html(axis.level),
            escape_html(&axis.reason),
            if axis.evidence_ids.is_empty() {
                "&#8212;".to_string()
            } else {
                axis.evidence_ids
                    .iter()
                    .map(|s| escape_html(s))
                    .collect::<Vec<_>>()
                    .join("<br>")
            }
        ));
    }
    html.push_str("</tbody></table>\n");

    // 利用データ一覧
    html.push_str("<div class=\"block-title block-title-spaced\">利用データ一覧</div>\n");
    html.push_str("<ul style=\"font-size:9pt;line-height:1.7;margin-left:5mm\">\n");
    for src in &meta.data_sources {
        html.push_str(&format!("<li>{}</li>\n", escape_html(src)));
    }
    html.push_str("</ul>\n");
    html.push_str("<p class=\"consult-note\">本レポートは面談前の仮説整理を目的とした社内資料です。記載内容は市場側データから導いた可能性であり、顧客固有の課題の断定ではありません。数値は各出典の基準時点に依存します。</p>\n");
    html.push_str("</div>\n");
}

/// ページ: 面談の掴み (この市場の意外な事実 TOP3) — 差別化の核 (P1-6)
fn render_page_grip(html: &mut String, items: &[super::grip::GripItem], sec: &str) {
    html.push_str("<div class=\"page-navy\">\n");
    html.push_str(internal_band());
    html.push_str(&page_head(
        sec,
        "面談の掴み（この市場の意外な事実）",
        "顧客の想定を裏切る可能性が高い市場側の事実。面談の冒頭で共有する素材",
    ));
    html.push_str("<p class=\"consult-note\">以下は市場側データから拾った「顧客が意外に思う可能性が高い事実」です。断定ではなく、面談の入り口として共有し、続く質問で顧客の実態を引き出してください。各項目の根拠は証拠一覧に対応します。</p>\n");
    for (i, it) in items.iter().enumerate() {
        html.push_str("<div class=\"consult-grip-item\">\n");
        html.push_str(&format!(
            "<div class=\"grip-head\"><span class=\"grip-no\">{}</span>{}</div>\n",
            i + 1,
            escape_html(&it.headline)
        ));
        html.push_str(&format!(
            "<div class=\"grip-talk\">{}</div>\n",
            escape_html(&it.talk_line)
        ));
        html.push_str(&format!(
            "<div class=\"grip-followup\"><strong>この後につなげる質問:</strong> {}</div>\n",
            escape_html(&it.follow_up_question)
        ));
        html.push_str(&format!(
            "<div class=\"ai-meta\">根拠: {}</div>\n",
            it.evidence_ids
                .iter()
                .map(|s| escape_html(s))
                .collect::<Vec<_>>()
                .join(", ")
        ));
        html.push_str("</div>\n");
    }
    html.push_str("</div>\n");
}

/// ページ: 複合考察 (AI下書き) — 複数シグナル・矛盾をつないだ考察。
/// 呼び出しは ai.items が非空のときのみ (P1-8: 空なら白紙ページを出さない)。
fn render_page2_ai_composite(
    html: &mut String,
    _analysis: &ConsultAnalysis,
    ai: &AiComposite,
    sec: &str,
) {
    html.push_str("<div class=\"page-navy\">\n");
    html.push_str(internal_band());
    html.push_str(&page_head(
        sec,
        "複合考察（AI下書き）",
        "複数の市場データを結びつけた考察の下書き。面談での検証を前提とする素材",
    ));

    html.push_str("<p class=\"consult-note\">以下は市場データ（証拠一覧）だけを入力に、複数の観測を結びつけて言語化した下書きです。各項目の根拠IDは証拠一覧で確認できます。断定ではなく、面談で検証する仮説の素材として扱ってください。</p>\n");
    html.push_str("<p class=\"consult-note\">各考察には機械チェック（標本数・データ粒度・反対方向の観測・逆の因果）による検証注記が付きます（自動チェックは参考であり、最終判断は面談で）。</p>\n");
    for item in &ai.items {
        html.push_str("<div class=\"consult-ai-item\">\n");
        // 機械チェック (T1 標本数 / T2 粒度) で指摘があった考察には注意ラベルを付ける。
        if item.refuted {
            html.push_str("<div class=\"ai-title\">");
            html.push_str(&escape_html(&item.title));
            html.push_str(
                "<span class=\"consult-ai-refute-flag\">⚠ 確認が必要 — 面談で検証</span></div>\n",
            );
        } else {
            html.push_str(&format!(
                "<div class=\"ai-title\">{}</div>\n",
                escape_html(&item.title)
            ));
        }
        html.push_str(&format!(
            "<div class=\"ai-body\">{}</div>\n",
            escape_html(&item.body)
        ));
        if !item.caveat.trim().is_empty() {
            html.push_str(&format!(
                "<div class=\"ai-meta\">留意点・不足情報: {}</div>\n",
                escape_html(&item.caveat)
            ));
        }
        // 機械チェックの裁定結果を「確認が必要な点 / 別の見方」ブロックとして併記する
        // (可能性表現のまま。考察は破棄しない)。
        if !item.reviewed {
            // 通常は起きない (道具箱は決定的) が、旧データ等で裁定が無い場合の防御表示。
            html.push_str(
                "<div class=\"ai-meta consult-ai-noreview\">（反証チェック未実施）</div>\n",
            );
        } else if item.refuted {
            // T1/T2 の指摘あり: 指摘文 + 逆・別の解釈を強調ブロックで併記。
            html.push_str("<div class=\"consult-ai-counter refuted\">\n");
            if let Some(reason) = &item.refute_reason {
                html.push_str(&format!(
                    "<div class=\"counter-label\">確認が必要な点:</div><div class=\"counter-body\">{}</div>\n",
                    escape_html(reason)
                ));
            }
            if let Some(alt) = &item.alt_interpretation {
                html.push_str(&format!(
                    "<div class=\"counter-body\">別の見方: {}</div>\n",
                    escape_html(alt)
                ));
            }
            html.push_str("</div>\n");
        } else if let Some(alt) = &item.alt_interpretation {
            // T1/T2 の指摘なし: T3/T4 の逆・別の解釈を「別の見方」として控えめに併記。
            html.push_str("<div class=\"consult-ai-counter\">\n");
            html.push_str(&format!(
                "<div class=\"counter-label\">別の見方:</div><div class=\"counter-body\">{}</div>\n",
                escape_html(alt)
            ));
            html.push_str("</div>\n");
        }
        html.push_str(&format!(
            "<div class=\"ai-meta\">根拠: {}</div>\n",
            item.evidence_ids
                .iter()
                .map(|s| escape_html(s))
                .collect::<Vec<_>>()
                .join(", ")
        ));
        html.push_str("</div>\n");
    }
    html.push_str("<p class=\"consult-note\">文章の生成は確定済みの構造化データ（証拠一覧）のみを入力とし、原データには直接アクセスしていません。数値計算・判定はすべてルールベースで確定しています。</p>\n");
    html.push_str("</div>\n");
}

/// ページ3: 優先仮説TOP5 (§12.3-2)
fn render_page3_hypotheses(html: &mut String, analysis: &ConsultAnalysis, sec: &str) {
    html.push_str("<div class=\"page-navy\">\n");
    html.push_str(internal_band());
    html.push_str(&page_head(
        sec,
        "優先仮説 TOP5",
        "検証優先度と根拠の厚さで並べた採用課題の仮説（面談で検証する）",
    ));

    if analysis.top_hypotheses.is_empty() {
        html.push_str("<p class=\"consult-note\">発火したシグナルが少なく、市場側データからは優先仮説を生成できませんでした。ヒアリング必須項目（最終ページ）から確認を始めてください。</p>\n");
    } else {
        html.push_str("<table class=\"table-navy\"><thead><tr><th style=\"width:8mm\">#</th><th style=\"width:20mm\">カテゴリ</th><th>仮説</th><th style=\"width:42mm\">根拠</th><th style=\"width:34mm\">反証・留意点</th><th style=\"width:12mm\">信頼度</th></tr></thead><tbody>\n");
        for (i, h) in analysis.top_hypotheses.iter().enumerate() {
            let counter = if h.counter_evidence_ids.is_empty() {
                if h.missing_information.is_empty() {
                    "&#8212;".to_string()
                } else {
                    format!("不足: {}", escape_html(&h.missing_information.join("、")))
                }
            } else {
                format!(
                    "{}<br>不足: {}",
                    evidence_chips(&analysis.evidence, &h.counter_evidence_ids),
                    escape_html(&h.missing_information.join("、"))
                )
            };
            let bd = &h.confidence_breakdown;
            let confidence_cell = format!(
                "{}<br><span style=\"font-size:6.5pt;color:#6A6E7A\">出典{}件 / 粒度{} / 反証{}件{}</span>",
                h.confidence.label_ja(),
                bd.independent_sources,
                if bd.granularity_match { "一致" } else { "差あり" },
                bd.counter_count,
                if bd.sample_sufficient { "" } else { " / 標本少" },
            );
            html.push_str(&format!(
                "<tr><td class=\"num\">{}</td><td style=\"font-size:8.5pt\">{}</td><td style=\"font-size:9pt\">{}<br><span style=\"font-size:7.5pt;color:#6A6E7A\">検証優先度: {}</span></td><td style=\"font-size:7.5pt\">{}</td><td style=\"font-size:7.5pt\">{}</td><td style=\"font-size:8pt\">{}</td></tr>\n",
                i + 1,
                escape_html(h.category.label_ja()),
                escape_html(&h.statement),
                h.priority.label_ja(),
                evidence_chips(&analysis.evidence, &h.supporting_evidence_ids),
                counter,
                confidence_cell,
            ));
        }
        html.push_str("</tbody></table>\n");
    }
    html.push_str("<p class=\"consult-note\">信頼度は「根拠の厚さ」を表し、仮説が正しい確率ではありません。全ての仮説は面談での検証を前提とし、支持・否定・保留のいずれかに更新してください。全シグナルの判定内訳を含む完全な証拠データは「証拠データJSON」に保持されています。</p>\n");
    html.push_str("</div>\n");
}

/// ページ4: 注目すべき矛盾 (§12.3-3)
fn render_page4_contradictions(html: &mut String, analysis: &ConsultAnalysis, sec: &str) {
    html.push_str("<div class=\"page-navy\">\n");
    html.push_str(internal_band());
    html.push_str(&page_head(
        sec,
        "注目すべき矛盾",
        "市場データ内の「違和感」の抽出。結論の断定ではなく面談の論点として使う",
    ));

    // ページ厳守のため表示は最大6件。全件 (最大10) は evidence_pack.json に保持。
    const CONTRADICTION_DISPLAY_MAX: usize = 6;
    if analysis.contradictions.is_empty() {
        html.push_str(
            "<p class=\"consult-note\">市場側データからは特筆すべき矛盾は検出されませんでした。</p>\n",
        );
    } else {
        html.push_str("<div class=\"consult-2col\">\n");
        for c in analysis
            .contradictions
            .iter()
            .take(CONTRADICTION_DISPLAY_MAX)
        {
            html.push_str("<div class=\"consult-q-block\">\n");
            html.push_str(&format!(
                "<div class=\"block-title\">{} {}<span style=\"float:right;font-size:8.5pt;font-weight:400\">確からしさ: {}</span></div>\n",
                escape_html(&c.contradiction_id),
                escape_html(&c.title),
                c.confidence.label_ja()
            ));
            html.push_str(&format!(
                "<p style=\"font-size:8pt;color:#6A6E7A;margin:0.5mm 0\">根拠: {}</p>\n",
                c.evidence_ids
                    .iter()
                    .map(|s| escape_html(s))
                    .collect::<Vec<_>>()
                    .join(", ")
            ));
            // ページ厳守のため各カードは解釈・質問とも最大2件を表示 (全件はJSONに保持)
            html.push_str("<p style=\"font-size:8.5pt;margin:0.5mm 0\"><strong>考えられる解釈:</strong></p><ul class=\"consult-branch\">\n");
            for i in c.interpretations.iter().take(2) {
                html.push_str(&format!("<li>{}</li>\n", escape_html(i)));
            }
            html.push_str("</ul>\n");
            html.push_str("<p style=\"font-size:8.5pt;margin:0.5mm 0\"><strong>面談で聞く:</strong></p><ul class=\"consult-branch\">\n");
            for q in c.questions.iter().take(2) {
                html.push_str(&format!("<li>{}</li>\n", escape_html(q)));
            }
            html.push_str("</ul>\n</div>\n");
        }
        html.push_str("</div>\n");
        if analysis.contradictions.len() > CONTRADICTION_DISPLAY_MAX {
            html.push_str(&format!(
                "<p class=\"consult-note\">この他に {} 件の矛盾を検出しています（全件は証拠データJSONに保持）。</p>\n",
                analysis.contradictions.len() - CONTRADICTION_DISPLAY_MAX
            ));
        }
    }
    html.push_str("<p class=\"consult-note\">矛盾は結論ではなく、面談で真偽を確かめる論点です。確からしさは根拠の厚さを表します。</p>\n");
    html.push_str("</div>\n");
}

/// ページ5: 証拠一覧（拡充分を含む全証拠）(§12.3-5 / §15.2)
fn render_page5_evidence(
    html: &mut String,
    analysis: &ConsultAnalysis,
    sec: &str,
    ai: &AiComposite,
) {
    html.push_str("<div class=\"page-navy\">\n");
    html.push_str(internal_band());
    html.push_str(&page_head(
        sec,
        "証拠一覧",
        "本レポートの判定・仮説・AI考察が参照する全データ。原データへのリネージ",
    ));
    // P1-8: AI考察が生成できなかったときは白紙ページを丸ごと出さず、ここで1行に圧縮して注記する。
    if ai.items.is_empty() {
        html.push_str("<p class=\"consult-note\">※ AIによる複合考察は今回は生成されませんでした（ルールベースの分析結果は本レポートの各ページのとおりです）。</p>\n");
    }
    html.push_str(&format!(
        "<p class=\"consult-note\">全 {} 件。区分は 観測値 / 集計値 / 代理指標 / 仮説 の別。粒度（全国・都道府県・市区町村・今回CSV・企業）と出典を各行に明示しています。</p>\n",
        analysis.evidence.len()
    ));
    // table-layout:fixed で列幅を厳守し、長文値は列内で折り返す (P1-8: 値セルの文字重なり防止)
    html.push_str("<table class=\"table-navy consult-evidence-table\" style=\"table-layout:fixed;width:100%\"><thead><tr><th style=\"width:11mm\">ID</th><th style=\"width:12mm\">区分</th><th>指標</th><th style=\"width:30mm\">値</th><th style=\"width:44mm\">出典 / 粒度</th></tr></thead><tbody>\n");
    for e in &analysis.evidence {
        let value = if e.unit.is_empty() {
            e.value_text.clone()
        } else {
            format!("{} {}", e.value_text, e.unit)
        };
        let n = e
            .sample_n
            .map(|n| format!(" (n={})", n))
            .unwrap_or_default();
        // 長文値 (通勤流入元上位の市町村列挙など) は num の右寄せ・nowrap を使わず、
        // 左寄せで折り返す。短い数値はこれまで通り num セルで右寄せ表示する。
        let is_long_text = value.chars().count() > 12 || value.contains('、');
        let value_cell = if is_long_text {
            format!(
                "<td style=\"white-space:normal;text-align:left\">{}</td>",
                escape_html(&value)
            )
        } else {
            format!("<td class=\"num\">{}</td>", escape_html(&value))
        };
        html.push_str(&format!(
            "<tr><td>{}</td><td>{}</td><td>{}</td>{}<td>{} / {}{}</td></tr>\n",
            escape_html(&e.id),
            e.kind.label_ja(),
            escape_html(&e.metric_name),
            value_cell,
            escape_html(&e.source_name),
            escape_html(&e.granularity),
            escape_html(&n),
        ));
    }
    html.push_str("</tbody></table>\n");
    html.push_str("</div>\n");
}

/// ページ6: 不足情報 (§12.3-4) + 面談質問 (§12.3-5) + 必須ヒアリング項目 (§13.1)
fn render_page6_questions_missing(html: &mut String, analysis: &ConsultAnalysis, sec: &str) {
    html.push_str("<div class=\"page-navy\">\n");
    html.push_str(internal_band());
    html.push_str(&page_head(
        sec,
        "不足情報・面談質問・ヒアリング項目",
        "公開データでは埋まらない情報を面談で確認する。回答で仮説を更新する",
    ));

    // 不足情報 (基本リスト + 仮説の missing_information)
    html.push_str("<div class=\"block-title\">確認すべき不足情報</div>\n");
    let mut missing: Vec<String> = BASE_MISSING_INFO.iter().map(|s| s.to_string()).collect();
    for h in &analysis.top_hypotheses {
        for m in &h.missing_information {
            if !missing.contains(m) {
                missing.push(m.clone());
            }
        }
    }
    html.push_str("<div class=\"consult-check-grid consult-check-grid-wide\">\n");
    for m in &missing {
        html.push_str(&format!("<div>&#9744; {}</div>\n", escape_html(m)));
    }
    html.push_str("</div>\n");
    html.push_str("<p class=\"consult-note\">これらは公開データからは取得できない顧客固有の情報です。面談での確認後にはじめて個社の施策を検討できます。</p>\n");

    html.push_str("<div class=\"block-title block-title-spaced\">面談質問と分岐</div>\n");

    if analysis.questions.is_empty() {
        html.push_str("<p class=\"consult-note\">生成された質問はありません。必須ヒアリング項目から確認してください。</p>\n");
    }
    html.push_str("<div class=\"consult-2col\">\n");
    for q in &analysis.questions {
        html.push_str("<div class=\"consult-q-block\">\n");
        html.push_str(&format!(
            "<div class=\"block-title\">{}. {}</div>\n",
            escape_html(&q.question_id),
            escape_html(&q.text)
        ));
        html.push_str(&format!(
            "<p style=\"font-size:8pt;color:#6A6E7A;margin:0.5mm 0\">目的: {} ／ 関連仮説: {}</p>\n",
            escape_html(&q.purpose),
            escape_html(&q.related_hypothesis_id)
        ));
        if !q.branches.is_empty() {
            html.push_str("<ul class=\"consult-branch\">\n");
            for b in &q.branches {
                html.push_str(&format!(
                    "<li><strong>{}</strong> &#8594; {}</li>\n",
                    escape_html(&b.answer_case),
                    escape_html(&b.next_question)
                ));
            }
            html.push_str("</ul>\n");
        }
        html.push_str(
            "<div class=\"consult-fill\" contenteditable=\"true\" spellcheck=\"false\">回答メモ: </div>\n",
        );
        html.push_str("</div>\n");
    }
    html.push_str("</div>\n");

    // 必須ヒアリング15項目
    html.push_str("<div class=\"block-title block-title-spaced\">必須ヒアリング項目（15項目チェックリスト）</div>\n");
    html.push_str("<div class=\"consult-check-grid\">\n");
    for item in REQUIRED_HEARING_ITEMS {
        html.push_str(&format!("<div>&#9744; {}</div>\n", escape_html(item)));
    }
    html.push_str("</div>\n");
    html.push_str("<p class=\"consult-note\">「不明」と「データなし」を区別して記録してください。回答はヒアリング後に仮説の支持・否定・保留の更新に使用します。</p>\n");
    html.push_str("</div>\n");
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::handlers::consult::evidence_pack::analyze;
    use crate::handlers::consult::input::{ClientInput, ConsultInput};

    fn rich_analysis() -> ConsultAnalysis {
        analyze(&crate::handlers::consult::evidence_pack::tests::rich_input())
    }

    fn sparse_analysis() -> ConsultAnalysis {
        // 欠損多数ケース
        analyze(&ConsultInput {
            pref: "大分県".to_string(),
            as_of: "2026-07-10".to_string(),
            total_postings: 5,
            data_sources: vec!["今回の求人CSV集計".to_string()],
            ..Default::default()
        })
    }

    /// §19.2 禁止表現 + サービス名 + 旧名称「ブリーフ」がHTMLに含まれないこと
    #[test]
    fn forbidden_phrases_never_appear_in_html() {
        for analysis in [rich_analysis(), sparse_analysis()] {
            let html = render_consult_brief_html(&analysis);
            for banned in [
                "必ず採用できる",
                "応募が増える",
                "離職率が高い企業",
                "成長企業である",
                "この媒体が最適",
                "SalesNow",
                "salesnow",
                // 名称変更: 表示文字列に旧名「ブリーフ」を残さない (商談準備レポートに統一)
                "ブリーフ",
            ] {
                assert!(
                    !html.contains(banned),
                    "禁止表現/サービス名/旧名称 {} がHTMLに含まれる",
                    banned
                );
            }
            // 新名称が使われていること
            assert!(html.contains("商談準備レポート"), "新表示名が使われている");
        }
    }

    #[test]
    fn every_page_has_internal_band() {
        // rich (grip あり・AIなし) は 6 ページ: 要約 / 掴み / 仮説 / 矛盾 / 証拠 / 質問
        let html = render_consult_brief_html(&rich_analysis());
        let page_count = html.matches("class=\"page-navy\"").count();
        let band_count = html.matches("consult-internal-band").count();
        assert_eq!(page_count, 6, "掴みページを含み6ページ (AIなし・最大8以内)");
        assert!(page_count <= 8, "ページ数は8以内");
        assert!(
            band_count >= page_count,
            "全ページに社内用帯がある (band={}, page={})",
            band_count,
            page_count
        );
        assert!(html.contains("社内用 &#8212; 顧客配布不可"));
        // ページ番号が連番であること (省略ページがあっても採番が飛ばない)
        assert!(html.contains("商談準備レポート / 01"));
        assert!(html.contains("商談準備レポート / 06"));
        assert!(!html.contains("商談準備レポート / 07"));
    }

    #[test]
    fn grip_section_present_for_rich_and_absent_ai_page_when_empty() {
        // P1-6: 掴みセクションが出る。P1-8: AIが空なら白紙AIページを出さず1行注記に圧縮。
        let html = render_consult_brief_html(&rich_analysis());
        assert!(
            html.contains("面談の掴み（この市場の意外な事実）"),
            "掴みセクションがある"
        );
        assert!(
            html.contains("この後につなげる質問"),
            "掴みに追撃質問がある"
        );
        // AIが空なので複合考察ページは出ない
        assert!(
            !html.contains("複合考察（AI下書き）"),
            "空AIの複合考察ページは出さない"
        );
        // 証拠一覧に1行の注記が圧縮表示される
        assert!(html.contains("AIによる複合考察は今回は生成されませんでした"));
    }

    #[test]
    fn sparse_report_omits_grip_and_ai_pages() {
        // シグナル非発火なら掴みもAIも出ない (白紙ページを作らない)
        let html = render_consult_brief_html(&sparse_analysis());
        assert!(!html.contains("面談の掴み（この市場の意外な事実）"));
        assert!(!html.contains("複合考察（AI下書き）"));
        // それでもレポートは成立し判定材料不足が出る
        assert!(html.contains("判定材料不足"));
    }

    #[test]
    fn ai_composite_renders_when_present_and_falls_back_when_empty() {
        let analysis = rich_analysis();
        let real_id = analysis.evidence[0].id.clone();
        let ai = AiComposite {
            one_line_summary: Some("需要と供給の緊張が見られる市場の可能性があります".to_string()),
            items: vec![super::super::ai::AiItem {
                title: "供給の細りと需要の強さが重なる可能性".to_string(),
                body: "複数の指標が同じ方向を示している可能性があります".to_string(),
                evidence_ids: vec![real_id],
                caveat: "応募者の居住地が不明".to_string(),
                ..Default::default()
            }],
        };
        let html = render_consult_brief_html_with_ai(&analysis, &ai);
        assert!(html.contains("複合考察（AI下書き）"));
        assert!(html.contains("AI要約"));
        assert!(html.contains("供給の細りと需要の強さ"));

        // 空のときは複合考察ページを出さず、証拠一覧の1行注記に圧縮する (P1-8)
        let html2 = render_consult_brief_html_with_ai(&analysis, &AiComposite::default());
        assert!(!html2.contains("複合考察（AI下書き）"));
        assert!(html2.contains("AIによる複合考察は今回は生成されませんでした"));
    }

    #[test]
    fn html_is_well_formed_basics() {
        let html = render_consult_brief_html(&rich_analysis());
        assert!(html.starts_with("<!DOCTYPE html>"));
        assert!(html.contains("<body class=\"theme-navy\">"));
        assert!(html.ends_with("</html>\n"));
        // div開閉の均衡 (粗い健全性チェック)
        let open = html.matches("<div").count();
        let close = html.matches("</div>").count();
        assert_eq!(
            open, close,
            "divの開閉が不均衡 (open={}, close={})",
            open, close
        );
        // テーブルの開閉
        assert_eq!(
            html.matches("<table").count(),
            html.matches("</table>").count()
        );
    }

    #[test]
    fn brief_contains_required_sections() {
        let html = render_consult_brief_html(&rich_analysis());
        for needle in [
            "市場環境の要約",
            // AIなし版のため複合考察ページは出ないが、掴みセクションは出る (P1-6)
            "面談の掴み（この市場の意外な事実）",
            "優先仮説 TOP5",
            "注目すべき矛盾",
            "証拠一覧",
            "確認すべき不足情報",
            "面談質問と分岐",
            "必須ヒアリング項目",
            "利用データ一覧",
            "データ基準日",
            "総合点は算出しません",
        ] {
            assert!(html.contains(needle), "必須セクション {} がない", needle);
        }
        // contenteditable 記入欄 (§12.5 回答入力欄)
        assert!(html.contains("contenteditable=\"true\""));
    }

    #[test]
    fn client_note_is_html_escaped() {
        let mut input = crate::handlers::consult::evidence_pack::tests::rich_input();
        input.client = ClientInput {
            note: Some("<script>alert(1)</script>".to_string()),
            ..Default::default()
        };
        let html = render_consult_brief_html(&analyze(&input));
        assert!(!html.contains("<script>alert(1)</script>"));
        assert!(html.contains("&lt;script&gt;"));
    }

    #[test]
    fn no_page_margin_redefinition_in_consult_css() {
        // feedback_print_css_cascade_trap: @page の margin/size を再定義しない
        let css = consult_css();
        let page_block_start = css.find("@page").expect("@page がある");
        // @page 直下 (最初の margin box 定義まで) に margin: / size: が無いこと
        let page_block_end = css[page_block_start..]
            .find("@bottom-left")
            .map(|i| i + page_block_start)
            .expect("@bottom-left がある");
        let head = &css[page_block_start..page_block_end];
        assert!(!head.contains("margin:"), "@page で margin を再定義しない");
        assert!(!head.contains(" size:"), "@page で size を再定義しない");
    }

    /// 視覚確認用フィクスチャ出力:
    /// 環境変数 CONSULT_BRIEF_FIXTURE_OUT にディレクトリを指定してテストを実行すると
    /// 合成データの商談準備レポートHTMLを書き出す (Playwright スクリーンショット検証用)。
    #[test]
    fn write_fixture_when_env_set() {
        let Ok(dir) = std::env::var("CONSULT_BRIEF_FIXTURE_OUT") else {
            return;
        };
        let dir = std::path::PathBuf::from(dir);
        std::fs::create_dir_all(&dir).unwrap();
        let rich = rich_analysis();
        std::fs::write(
            dir.join("consult_brief_rich.html"),
            render_consult_brief_html(&rich),
        )
        .unwrap();
        std::fs::write(
            dir.join("consult_brief_sparse.html"),
            render_consult_brief_html(&sparse_analysis()),
        )
        .unwrap();
        // AI複合考察つきの合成バリアント。生成 (Gemini) 部分だけダミー項目で代替し、
        // 反証チェックは実際の逆証明の道具箱 (apply_toolbox) を実行して描画する。
        // 3状態 (確認が必要 / 別の見方のみ / 注記なし) が揃うよう根拠とタグを選ぶ。
        use crate::handlers::consult::evidence::granularity;
        use crate::handlers::consult::refute_toolbox::apply_toolbox;
        let pick = |gran: &str, n: usize| -> Vec<String> {
            rich.evidence
                .iter()
                .filter(|e| e.granularity == gran)
                .take(n)
                .map(|e| e.id.clone())
                .collect()
        };
        let company_ids = pick(granularity::COMPANY, 2);
        let muni_ids = pick(granularity::MUNICIPALITY, 1);
        // 注記なし状態用: 逆因果辞書の対象シグナルが参照しない市区町村粒度の証拠を選ぶ
        let dict_signals = ["S-01", "S-06", "S-07", "S-12", "S-29"];
        let clean_ids: Vec<String> = rich
            .evidence
            .iter()
            .filter(|e| e.granularity == granularity::MUNICIPALITY)
            .filter(|e| {
                !rich.signals.iter().any(|s| {
                    s.fired
                        && dict_signals.contains(&s.id.as_str())
                        && s.evidence_ids.contains(&e.id)
                })
            })
            .take(1)
            .map(|e| e.id.clone())
            .collect();
        let raw_items = vec![
            super::super::ai::AiItem {
                title: "人員減少と募集継続が並行している可能性".to_string(),
                body: "人員を減らしながら募集を続ける企業が見られ、欠員補充型の採用がこの市場で発生している可能性があります。".to_string(),
                evidence_ids: company_ids,
                caveat: "人員推移は企業データベースの参照時点に依存する参考値です。".to_string(),
                claim_axis: "competition".to_string(),
                claim_direction: "problem".to_string(),
                ..Default::default()
            },
            super::super::ai::AiItem {
                title: "通勤圏の広がりを母集団形成に活かせる可能性".to_string(),
                body: "周辺地域からの働き手の流入が見られ、配信地域を通勤圏まで広げる余地がある可能性があります。".to_string(),
                evidence_ids: muni_ids,
                caveat: "応募者の実際の居住地は不明のため面談で確認が必要です。".to_string(),
                claim_axis: "supply".to_string(),
                claim_direction: "opportunity".to_string(),
                ..Default::default()
            },
            super::super::ai::AiItem {
                title: "地域の人口動態を条件設計の前提に置く".to_string(),
                body: "地域の人口動態は採用計画の背景情報として押さえておく論点の可能性があります。".to_string(),
                evidence_ids: clean_ids,
                caveat: "対象職種の労働層と全体人口の動きは異なる場合があります。".to_string(),
                claim_axis: "other".to_string(),
                claim_direction: "neutral".to_string(),
                ..Default::default()
            },
        ];
        let (items, refuted_count, reviewed_count) = apply_toolbox(raw_items, &rich);
        // フィクスチャに3状態が揃っていることを保証する (スクリーンショット検証の前提)
        assert_eq!(reviewed_count, 3);
        assert_eq!(refuted_count, 1, "状態1: T1 標本数チェックで「確認が必要」");
        assert!(items[0].refuted && items[0].refute_reason.is_some());
        assert!(
            !items[1].refuted && items[1].alt_interpretation.is_some(),
            "状態2: T3/T4 の「別の見方」のみ"
        );
        assert!(
            !items[2].refuted && items[2].alt_interpretation.is_none(),
            "状態3: 注記なし"
        );
        let ai = AiComposite {
            one_line_summary: Some(
                "需要は強い一方で人材供給が細っており、通勤圏の広さと条件の見せ方が論点になり得る市場の可能性があります。".to_string(),
            ),
            items,
        };
        std::fs::write(
            dir.join("consult_brief_ai.html"),
            render_consult_brief_html_with_ai(&rich, &ai),
        )
        .unwrap();
    }
}
