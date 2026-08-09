//! 顧客提示用の診断レポート HTML (Customer-Facing Journey Report)。
//!
//! # なぜ社内レビュー用 HTML と分けるか
//!
//! 応募者ジャーニー診断の生成物には、社内で検証を回すための情報が大量に混ざる。
//! 根拠番号 (J1 / C36 / R1)、`evidence_refs`、品質ゲートの合否、再生成の記録、
//! `basis_type` の内部語 (「データ由来」等) は、いずれも**顧客に見せる前提で
//! 書かれていない**。社内 HTML をそのまま渡すと、顧客は意味の分からない記号を
//! 読まされ、こちらの検証プロセスの内情まで開示することになる。
//!
//! 本モジュールは、同じ生成物から**顧客がそのまま受け取れる 1 枚の HTML** だけを
//! 組み立てる。設計・文言・CSS の正本はユーザーレビュー済みの設計モック
//! `render_customer_html.py` で、本実装はその移植版。
//!
//! # 判定方式: LLM を使わない純粋関数
//!
//! [`super::coverage_gate`] 等と同じ方針で、出力は入力 JSON だけから決まる決定論的な
//! 純粋関数にする。「顧客向けに言い換えて」を LLM に任せると、入力に無い数値や
//! 断定が混入し、事実照合を通した意味が消えるため。同じ入力からは常に同じ HTML が
//! 出る (`same_input_is_deterministic` テストで確認)。
//!
//! # 顧客向け出力の規律
//!
//! - **内部記号を出さない**: 根拠番号・`evidence_refs`・ゲート・再生成への言及は
//!   一切載せない。参照するフィールドを絞ることで構造的に担保している
//!   (`no_internal_symbols_leak` テストで逆証明)。
//! - **内部語を顧客語へ変換**: `basis_type` は [`basis_label`] で言い換える
//!   (「データ由来」→「周辺求人・貴社求人の実測から」)。
//! - **中立表現**: 「劣位」「埋もれる」「集中」「縮小」のような評価語を固定文言に
//!   使わない (`no_evaluative_words` テストで逆証明)。
//! - **未確認は未確認と書く**: `client_fact_status` が「未確認」の提案には備考
//!   「要確認」を付け、確認後に反映する前提であることを注記する。
//! - **HTML エスケープ**: 動的文字列は全て [`esc`] を通す。表セルは呼び出し側で
//!   エスケープしてから [`table`] に渡す規約 (モックと同じ)。
//!
//! # 既知の限界
//!
//! - 顧客語への変換は `basis_type` のような**列挙値**にしか効かない。
//!   `countermeasure` 等の自由文に内部記号が書かれていた場合はそのまま出る
//!   (自由文を機械で書き換えると事実が変わるため、あえて手を入れていない)。
//!   自由文側は生成時のプロンプト規約と検証ゲートで塞ぐ前提。
//! - 抜粋は求人票案・note 案とも先頭 1 件のみ。全案の提示は社内 HTML の役割。
//! - 数値は入力の値をそのまま整形するだけで、単位変換や再計算はしない。

use serde_json::Value;
use std::collections::HashMap;

/// 顧客提示用レポートの入力一式。
///
/// 全て参照で受け取る (レポート生成は読み取りのみで、入力を変更しない)。
pub struct CustomerReportInput<'a> {
    /// 案件プロフィール (`company_name` / `job_title` / `prefecture` / `municipality`)。
    pub case_profile: &'a Value,
    /// 照合済み事実マップ `{salary:{value,evidence_quote,status},...}`。
    /// 表に載るのは `status == "verified"` の項目だけ。
    pub facts: &'a Value,
    /// 事実台帳 (`supplementary_conditions` を参照する)。
    pub fact_ledger: &'a Value,
    /// 給与内訳 ([`super::salary_breakdown`] の出力相当)。
    pub salary_breakdown: &'a Value,
    /// 顧客求人の給与位置 (`sample_count` / `median_yen` / `percentile_position` 等)。
    pub client_salary_position: &'a Value,
    /// 比較コホート (`commute_salary_layers` を参照する)。
    pub comparison_cohort: &'a Value,
    /// diagnose の result (`personas` / `client_questions` / `limitations`)。
    pub prepare_result: &'a Value,
    /// 求人内で記載が食い違う項目 (`topic` / `quote_a` / `quote_b`)。
    pub fact_conflicts: &'a [Value],
    /// persona_id → 8段階診断の result。
    ///
    /// [`HashMap`] は反復順が不定のため、出力順は `prepare_result.personas` の
    /// 並び順に固定する (未参照の persona_id はキー昇順で後ろに付ける)。
    pub persona_details: &'a HashMap<String, Value>,
    /// 生成済みで品質ゲートを通過した note 下書きの result (空ならセクションを省略)。
    pub note_drafts: &'a [Value],
    /// 生成済みで品質ゲートを通過した求人票下書きの result (空ならセクションを省略)。
    pub posting_drafts: &'a [Value],
}

/// 事実キーの顧客向け日本語ラベル。ここに無いキーはキー名をそのまま出す。
const FACT_LABELS: &[(&str, &str)] = &[
    ("salary", "給与"),
    ("working_hours", "勤務時間"),
    ("holidays", "休日"),
    ("work_location", "勤務地"),
    ("employment_type", "雇用形態"),
    ("required_qualifications", "必須資格"),
    ("insurance", "保険"),
    ("allowances", "手当"),
    ("bonus", "賞与"),
];

/// 改善提案の最大行数。
const MAX_PROPOSAL_ROWS: usize = 12;

/// 確認事項の最大件数。
const MAX_QUESTIONS: usize = 12;

/// 抜粋に載せるキャッチコピー案・タイトル案の件数。
const MAX_OPTION_EXCERPTS: usize = 2;

/// 空配列アクセス用の共有スライス。
const EMPTY_VALUES: &[Value] = &[];

/// 顧客提示用の診断レポートを完全な HTML 文書 (`<!DOCTYPE html>` 〜) として返す。
///
/// 構成は固定の 8 セクション。ただし求人票案 (6) と note 案 (7) は下書きが
/// 無ければセクションごと省略する (空セクションを出すと「用意できなかった」
/// ことが顧客側に伝わらないため)。
pub fn render_customer_report(input: &CustomerReportInput) -> String {
    let profile = input.case_profile;
    let company = field_text(profile, "company_name");
    let job_title = field_text(profile, "job_title");
    let area = format!(
        "{}{}",
        field_text(profile, "prefecture"),
        field_text(profile, "municipality")
    );

    let prepare = result_of(input.prepare_result);
    let personas = array_field(prepare, "personas");
    let persona_ids: Vec<&str> = personas
        .iter()
        .filter_map(|p| p.get("id").and_then(Value::as_str))
        .collect();
    let details = ordered_details(&persona_ids, input.persona_details);

    let mut body = String::new();
    body.push_str(&render_header(&company, &job_title, &area));
    body.push_str(&render_facts_section(
        input.facts,
        input.fact_ledger,
        input.salary_breakdown,
    ));
    body.push_str(&render_market_section(
        input.client_salary_position,
        input.comparison_cohort,
    ));
    body.push_str(&render_personas_section(personas));
    body.push_str(&render_questions_section(
        input.fact_conflicts,
        prepare,
        &details,
    ));
    body.push_str(&render_proposals_section(&details));
    if let Some(posting) = input.posting_drafts.first() {
        body.push_str(&render_posting_section(result_of(posting)));
    }
    if let Some(note) = input.note_drafts.first() {
        body.push_str(&render_note_section(result_of(note)));
    }
    body.push_str(&render_limitations_section(prepare));

    format!(
        "<!DOCTYPE html><html lang='ja'><head><meta charset='utf-8'>\
<meta name='viewport' content='width=device-width, initial-scale=1'>\
<title>採用ジャーニー診断レポート（{title}）</title>\
<style>{css}</style></head><body>{body}</body></html>",
        title = esc(&company),
        css = REPORT_CSS,
    )
}

// ---------------------------------------------------------------------------
// セクション生成
// ---------------------------------------------------------------------------

fn render_header(company: &str, job_title: &str, area: &str) -> String {
    format!(
        r#"<header>
<p class="brand">採用ジャーニー診断レポート</p>
<h1>{company}<br><span class="sub">{job_title}（{area}）</span></h1>
<p class="meta">本レポートは、貴社の求人票・周辺の競合求人・公開情報を機械照合と統計処理で分析したものです。
数値と引用はすべて入力データと照合済みで、確認できていない内容は「取材で確認」と明示しています。</p>
</header>"#,
        company = esc(company),
        job_title = esc(job_title),
        area = esc(area),
    )
}

/// 1. 貴社求人から確認できた内容。
///
/// 事実表に載せるのは `status == "verified"` の項目だけ。未照合の値を顧客に
/// 「確認できた内容」として見せないため。
fn render_facts_section(facts: &Value, fact_ledger: &Value, salary_breakdown: &Value) -> String {
    let mut fact_rows: Vec<Vec<String>> = Vec::new();
    if let Some(map) = facts.as_object() {
        for (key, fact) in map {
            if field_text(fact, "status") != "verified" {
                continue;
            }
            fact_rows.push(vec![esc(&fact_label(key)), esc(&field_text(fact, "value"))]);
        }
    }

    let supplementary = array_field(fact_ledger, "supplementary_conditions");
    let chips = if supplementary.is_empty() {
        String::new()
    } else {
        let items: Vec<String> = supplementary
            .iter()
            .map(|t| format!(r#"<span class="chip">{}</span>"#, esc(&text_of(t))))
            .collect();
        format!("<h3>あわせて確認できた条件</h3><p>{}</p>", items.join(" "))
    };

    let breakdown_rows = render_salary_breakdown_rows(salary_breakdown);
    let breakdown_html = if breakdown_rows.is_empty() {
        String::new()
    } else {
        format!(
            "<h3>給与の内訳</h3>{}",
            table(&["項目", "内容"], &breakdown_rows)
        )
    };

    format!(
        "<section><h2>1. 貴社求人から確認できた内容</h2>{fact_table}{chips}{breakdown_html}\
<p class=\"note\">いずれも貴社求人の記載から機械照合した内容です。記載の解釈に誤りがあればご指摘ください。</p></section>",
        fact_table = table(&["項目", "記載内容"], &fact_rows),
    )
}

/// 給与内訳の表行。値が入力に無い項目は行ごと出さない (「不明」を空欄で見せない)。
fn render_salary_breakdown_rows(salary_breakdown: &Value) -> Vec<Vec<String>> {
    let mut rows: Vec<Vec<String>> = Vec::new();

    if let Some(min) = salary_breakdown
        .get("display_monthly_min_yen")
        .filter(is_set)
    {
        let tail = match salary_breakdown
            .get("display_monthly_max_yen")
            .filter(is_set)
        {
            Some(max) => format!("〜{}", yen(max)),
            None => "以上".to_string(),
        };
        rows.push(vec![esc("表示月給"), format!("{}{}", yen(min), tail)]);
    }
    if let Some(base) = salary_breakdown.get("base_monthly_yen").filter(is_set) {
        rows.push(vec![esc("基本給"), yen(base)]);
    }
    // 「固定残業なし」は求人票に明示があったときだけ言える (未判定と区別する)。
    if salary_breakdown.get("fixed_overtime") == Some(&Value::Bool(false)) {
        rows.push(vec![esc("固定残業制度"), esc("なし（求人票に明示）")]);
    }
    if salary_breakdown
        .get("overtime_pay_included_in_display")
        .and_then(Value::as_bool)
        .unwrap_or(false)
    {
        rows.push(vec![esc("表示月給の残業代"), esc("含む旨の記載あり")]);
    }
    if let Some(range) = salary_breakdown
        .get("overtime_hours_range")
        .and_then(Value::as_array)
    {
        if range.len() >= 2 {
            rows.push(vec![
                esc("想定残業時間（月）"),
                esc(&format!(
                    "{}〜{}時間",
                    number_text(&range[0]),
                    number_text(&range[1])
                )),
            ]);
        }
    }

    rows
}

/// 2. 周辺市場での給与の位置。
fn render_market_section(client_salary_position: &Value, comparison_cohort: &Value) -> String {
    let summary = if is_present_object(client_salary_position) {
        let percentile = client_salary_position
            .get("percentile_position")
            .and_then(Value::as_f64)
            .unwrap_or(0.0);
        format!(
            "<p>同時期に掲載されていた同職種・同雇用形態の求人{count}件と比較すると、\
貴社の表示月給（比較用代表値 {client}）は中央値 {median} に対して上位 {upper:.0}% の位置にあります。</p>",
            count = esc(&field_text(client_salary_position, "sample_count")),
            client = field_yen(client_salary_position, "client_monthly_equivalent_yen"),
            median = field_yen(client_salary_position, "median_yen"),
            upper = 100.0 - percentile,
        )
    } else {
        String::new()
    };

    let layers = array_field(comparison_cohort, "commute_salary_layers");
    let layer_rows: Vec<Vec<String>> = layers
        .iter()
        .map(|layer| {
            let position = match layer.get("client_percentile_position").filter(is_set) {
                Some(p) => esc(&format!("{}パーセンタイル", number_text(p))),
                None => esc("—"),
            };
            vec![
                esc(&field_text(layer, "layer")),
                match layer.get("count").filter(is_set) {
                    Some(c) => esc(&format!("{}件", number_text(c))),
                    None => esc("—"),
                },
                field_yen(layer, "median_yen"),
                position,
            ]
        })
        .collect();

    let layer_table = if layer_rows.is_empty() {
        String::new()
    } else {
        table(&["比較範囲", "件数", "中央値", "貴社の位置"], &layer_rows)
    };

    format!(
        "<section><h2>2. 周辺市場での給与の位置</h2>{summary}{layer_table}\
<p class=\"note\">比較は掲載求人の表示給与同士によるものです。基本給・手当・残業代の内訳構成までは揃えていません。</p></section>"
    )
}

/// 3. この求人市場で想定される応募者像。
fn render_personas_section(personas: &[Value]) -> String {
    let mut out = String::from(
        "<section><h2>3. この求人市場で想定される応募者像</h2>\
<p class=\"note\">周辺求人の実測データと地域統計から導出した仮説です。具体的な経歴・家族構成などの描写は理解を助けるための作例であり、実在の人物ではありません。</p>",
    );
    for persona in personas {
        out.push_str(&format!(
            r#"
<div class="card"><h3>{label}</h3>
{profile}
<dl>
<dt>重視する条件</dt><dd>{conditions}</dd>
<dt>この求人への想定反応</dt><dd>{behavior} — {reason}</dd>
</dl></div>"#,
            label = esc(&field_text(persona, "label")),
            profile = md(&field_text(persona, "profile")),
            conditions = esc(&join_strings(
                array_field(persona, "priority_conditions"),
                "、"
            )),
            behavior = esc(&field_text(persona, "likely_behavior")),
            reason = esc(&field_text(persona, "behavior_reason")),
        ));
    }
    out.push_str("</section>");
    out
}

/// 4. 貴社に確認させていただきたい事項。
///
/// 記載の食い違い → 準備段階の確認事項 → ペルソナ別の確認事項の順に並べ、
/// 文面が完全一致するものは 1 件に畳む (同じ質問を顧客に二度させないため)。
fn render_questions_section(
    fact_conflicts: &[Value],
    prepare: &Value,
    details: &[&Value],
) -> String {
    let mut questions: Vec<String> = Vec::new();
    let push_unique = |q: String, list: &mut Vec<String>| {
        if !q.trim().is_empty() && !list.contains(&q) {
            list.push(q);
        }
    };

    for conflict in fact_conflicts {
        let question = format!(
            "求人内で記載が分かれている「{}」の正しい内容（「{}」と「{}」のどちらが現行か）",
            field_text(conflict, "topic"),
            field_text(conflict, "quote_a"),
            field_text(conflict, "quote_b"),
        );
        push_unique(question, &mut questions);
    }
    for q in array_field(prepare, "client_questions") {
        push_unique(text_of(q), &mut questions);
    }
    for detail in details {
        for q in array_field(detail, "client_questions") {
            push_unique(text_of(q), &mut questions);
        }
    }

    let items: String = questions
        .iter()
        .take(MAX_QUESTIONS)
        .map(|q| format!("<li>{}</li>", esc(q)))
        .collect();

    format!(
        "<section><h2>4. 貴社に確認させていただきたい事項</h2><ol>{items}</ol>\
<p class=\"note\">確認できた内容は、求人票・採用広報の表現に確定事実として反映できます。未確認のまま公開文へ断定的に書くことはしません。</p></section>"
    )
}

/// 5. 求人票・応募対応の改善提案。
///
/// 全ペルソナの `priority_actions` を対策文で重複排除し、優先度 (高→中→低) 順に
/// 並べて最大 [`MAX_PROPOSAL_ROWS`] 行。並べ替えは安定ソートなので、同一優先度の
/// 中では元のペルソナ順が保たれる。
fn render_proposals_section(details: &[&Value]) -> String {
    let mut rows: Vec<Vec<String>> = Vec::new();
    let mut seen: Vec<String> = Vec::new();

    for detail in details {
        for action in array_field(detail, "priority_actions") {
            let countermeasure = field_text(action, "countermeasure");
            if seen.contains(&countermeasure) {
                continue;
            }
            seen.push(countermeasure.clone());

            let remark = if field_text(action, "client_fact_status") == "未確認" {
                "要確認"
            } else {
                ""
            };
            rows.push(vec![
                esc(&field_text(action, "priority")),
                esc(&countermeasure),
                esc(basis_label(&field_text(action, "basis_type"))),
                esc(remark),
            ]);
        }
    }

    rows.sort_by_key(|row| priority_rank(row.first().map_or("", String::as_str)));
    rows.truncate(MAX_PROPOSAL_ROWS);

    format!(
        "<section><h2>5. 求人票・応募対応の改善提案</h2>{table}\
<p class=\"note\">「要確認」の提案は、貴社の実態を確認できた場合のみ反映する前提の内容です。</p></section>",
        table = table(&["優先度", "提案内容", "提案の根拠", "備考"], &rows),
    )
}

/// 6. 求人票の改善イメージ（抜粋）。
fn render_posting_section(posting: &Value) -> String {
    let catches: String = array_field(posting, "catch_copy_options")
        .iter()
        .take(MAX_OPTION_EXCERPTS)
        .map(|c| format!("<li>「{}」</li>", esc(&field_text(c, "text"))))
        .collect();

    format!(
        "<section><h2>6. 求人票の改善イメージ（抜粋）</h2>\
<dl><dt>キャッチコピー案</dt><dd><ul>{catches}</ul></dd>\
<dt>仕事内容の書き方例</dt><dd>{description}</dd></dl>\
<p class=\"note\">【取材で確認: 】とある箇所は、貴社への確認後に確定情報へ差し替える前提の下書きです。</p></section>",
        description = md(&field_text(posting, "job_description_markdown")),
    )
}

/// 7. 採用広報記事（note等）のイメージ。
///
/// 記事案は取材前の下書きなので、必ず「あくまでイメージ」である旨を冒頭に出す。
fn render_note_section(note: &Value) -> String {
    let titles: String = array_field(note, "title_options")
        .iter()
        .take(MAX_OPTION_EXCERPTS)
        .map(|t| format!("<li>{}</li>", esc(&text_of(t))))
        .collect();

    format!(
        "<section><h2>7. 採用広報記事（note等）のイメージ</h2>\
<p class=\"note\">※この記事案はあくまでイメージです。</p>\
<dl><dt>タイトル案</dt><dd><ul>{titles}</ul></dd>\
<dt>リード文</dt><dd>{lead}</dd></dl></section>",
        lead = md(&field_text(note, "lead")),
    )
}

/// 8. この診断の前提と限界。
///
/// 入力側の `limitations` に加え、比較対象の範囲と応募数を保証しない旨は
/// 常に出す (入力が空でも顧客が範囲を誤解しないようにするため)。
fn render_limitations_section(prepare: &Value) -> String {
    let items: String = array_field(prepare, "limitations")
        .iter()
        .map(|x| format!("<li>{}</li>", esc(&text_of(x))))
        .collect();

    format!(
        "<section><h2>8. この診断の前提と限界</h2><ul>{items}\
<li>比較対象は分析時点で媒体に掲載されていた求人であり、求人市場の全体ではありません。</li>\
<li>応募者像・行動予測は統計と実測に基づく仮説であり、応募数を保証するものではありません。</li></ul></section>"
    )
}

// ---------------------------------------------------------------------------
// 顧客語への変換
// ---------------------------------------------------------------------------

/// `basis_type` の内部語を顧客向けの言い方に置き換える。
///
/// 未知の値はそのまま返す (勝手に「一般的な採用実務として」等へ寄せると、
/// 実測由来かどうかの申告が変わってしまうため)。
fn basis_label(basis_type: &str) -> &str {
    match basis_type {
        "データ由来" => "周辺求人・貴社求人の実測から",
        "データからの推論" => "実測からの推論",
        "一般的な採用施策" => "一般的な採用実務として",
        other => other,
    }
}

/// 優先度の並び順 (高→中→低→不明)。
fn priority_rank(priority: &str) -> u8 {
    match priority.trim() {
        "高" => 0,
        "中" => 1,
        "低" => 2,
        _ => 3,
    }
}

/// 事実キーの日本語ラベル。未知のキーはキー名をそのまま返す。
fn fact_label(key: &str) -> String {
    FACT_LABELS
        .iter()
        .find(|(k, _)| *k == key)
        .map_or_else(|| key.to_string(), |(_, label)| (*label).to_string())
}

// ---------------------------------------------------------------------------
// 値の取り出しと整形
// ---------------------------------------------------------------------------

/// `{"result": {...}}` 形式で渡された場合に中身を取り出す。
///
/// 公開 API 上は「result そのもの」を受け取る契約だが、実行結果 JSON をそのまま
/// 渡されても壊れないようにしている (result キーを持つのは包み側だけのため誤検出しない)。
fn result_of(value: &Value) -> &Value {
    match value.get("result") {
        Some(inner) if inner.is_object() => inner,
        _ => value,
    }
}

/// `persona_details` の出力順を決める。
///
/// [`HashMap`] の反復順は不定なので、`prepare_result.personas` の並び順を正とする。
/// personas に載っていない persona_id はキー昇順で末尾に付ける (取りこぼさないため)。
fn ordered_details<'a>(
    persona_ids: &[&str],
    details: &'a HashMap<String, Value>,
) -> Vec<&'a Value> {
    let mut out: Vec<&Value> = Vec::new();
    let mut used: Vec<&str> = Vec::new();
    for id in persona_ids {
        if used.contains(id) {
            continue;
        }
        if let Some(detail) = details.get(*id) {
            out.push(result_of(detail));
            used.push(id);
        }
    }
    let mut rest: Vec<&String> = details
        .keys()
        .filter(|k| !used.contains(&k.as_str()))
        .collect();
    rest.sort();
    for key in rest {
        out.push(result_of(&details[key]));
    }
    out
}

/// オブジェクトの文字列フィールド (無ければ空文字。null は空文字)。
fn field_text(value: &Value, key: &str) -> String {
    value.get(key).map_or_else(String::new, text_of)
}

/// 配列フィールド (無ければ空スライス)。
fn array_field<'a>(value: &'a Value, key: &str) -> &'a [Value] {
    value
        .get(key)
        .and_then(Value::as_array)
        .map_or(EMPTY_VALUES, Vec::as_slice)
}

/// 値の未エスケープ表示テキスト。
///
/// null は空文字にする (画面に "null" を出さないため)。配列は読点で連結し、
/// オブジェクトは空文字にする (JSON の生表記が顧客の目に触れないようにするため)。
fn text_of(value: &Value) -> String {
    match value {
        Value::Null => String::new(),
        Value::String(s) => s.clone(),
        Value::Bool(b) => b.to_string(),
        Value::Number(_) => number_text(value),
        Value::Array(items) => join_strings(items, "、"),
        Value::Object(_) => String::new(),
    }
}

/// 配列の各要素を区切り文字で連結する (空要素は落とす)。
fn join_strings(items: &[Value], separator: &str) -> String {
    items
        .iter()
        .map(text_of)
        .filter(|s| !s.trim().is_empty())
        .collect::<Vec<String>>()
        .join(separator)
}

/// 数値の表示テキスト。整数はそのまま、小数は不要な末尾の 0 を付けない。
fn number_text(value: &Value) -> String {
    if let Some(i) = value.as_i64() {
        return i.to_string();
    }
    if let Some(u) = value.as_u64() {
        return u.to_string();
    }
    match value.as_f64() {
        Some(f) if f.fract() == 0.0 => format!("{f:.0}"),
        Some(f) => format!("{f}"),
        None => String::new(),
    }
}

/// オブジェクトのフィールドを金額表記で取り出す (無ければ空文字)。
fn field_yen(value: &Value, key: &str) -> String {
    value.get(key).map_or_else(String::new, yen)
}

/// 金額表記 (桁区切り + 「円」)。数値でなければ通常のテキストとして扱う。
///
/// 戻り値はエスケープ済み。
fn yen(value: &Value) -> String {
    match yen_amount(value) {
        Some(amount) => esc(&format!("{}円", with_thousands_separator(amount))),
        None => esc(&text_of(value)),
    }
}

/// 金額として解釈できる整数値 (小数は四捨五入。円未満の桁は表示しないため)。
fn yen_amount(value: &Value) -> Option<i64> {
    if let Some(i) = value.as_i64() {
        return Some(i);
    }
    if let Some(u) = value.as_u64() {
        return i64::try_from(u).ok();
    }
    value.as_f64().map(|f| f.round() as i64)
}

/// 桁区切り表記 (例: 254200 → 254,200)。
fn with_thousands_separator(value: i64) -> String {
    let negative = value < 0;
    let digits = value.unsigned_abs().to_string();
    let mut out = String::with_capacity(digits.len() + digits.len() / 3 + 1);
    for (i, c) in digits.chars().enumerate() {
        if i > 0 && (digits.len() - i) % 3 == 0 {
            out.push(',');
        }
        out.push(c);
    }
    if negative {
        format!("-{out}")
    } else {
        out
    }
}

/// 値が「入力に存在する」か (null は存在しない扱い)。
fn is_set(value: &&Value) -> bool {
    !value.is_null()
}

/// 中身のあるオブジェクトか (null・空オブジェクトは false)。
fn is_present_object(value: &Value) -> bool {
    value.as_object().is_some_and(|m| !m.is_empty())
}

// ---------------------------------------------------------------------------
// HTML 組み立て
// ---------------------------------------------------------------------------

/// HTML エスケープ。属性値にも使えるよう引用符も変換する。
fn esc(text: &str) -> String {
    let mut out = String::with_capacity(text.len());
    for c in text.chars() {
        match c {
            '&' => out.push_str("&amp;"),
            '<' => out.push_str("&lt;"),
            '>' => out.push_str("&gt;"),
            '"' => out.push_str("&quot;"),
            '\'' => out.push_str("&#x27;"),
            other => out.push(other),
        }
    }
    out
}

/// 素のテキストを段落 HTML にする。空行で段落を分け、段落内の改行は `<br>`。
///
/// エスケープはこの中で行うので、呼び出し側は生テキストを渡す。
fn md(text: &str) -> String {
    let escaped = esc(text);
    let paragraphs: Vec<String> = escaped
        .split("\n\n")
        .map(str::trim)
        .filter(|p| !p.is_empty())
        .map(|p| format!("<p>{}</p>", p.replace('\n', "<br>")))
        .collect();
    if paragraphs.is_empty() {
        "<p></p>".to_string()
    } else {
        paragraphs.join("")
    }
}

/// 表を組み立てる。
///
/// **セルは呼び出し側でエスケープ済みであること** (見出しは固定文言のみ)。
/// 横スクロール用のラッパで包むので、狭い画面でも本文が横に伸びない。
fn table(headers: &[&str], rows: &[Vec<String>]) -> String {
    let head: String = headers.iter().map(|h| format!("<th>{h}</th>")).collect();
    let body: String = rows
        .iter()
        .map(|row| {
            let cells: String = row.iter().map(|c| format!("<td>{c}</td>")).collect();
            format!("<tr>{cells}</tr>")
        })
        .collect();
    format!(
        r#"<div class="tablewrap"><table><thead><tr>{head}</tr></thead><tbody>{body}</tbody></table></div>"#
    )
}

/// レポートのスタイル (外部リソースを一切使わない自己完結 CSS)。
const REPORT_CSS: &str = "
body{font-family:'Hiragino Sans','Yu Gothic UI','Meiryo',sans-serif;line-height:1.85;color:#20242e;
 max-width:860px;margin:0 auto;padding:40px 36px;background:#fff;}
.brand{color:#7a8394;font-size:0.8rem;letter-spacing:0.15em;margin:0;}
h1{font-size:1.5rem;margin:6px 0 4px;border-bottom:3px solid #1f4e8c;padding-bottom:10px;}
h1 .sub{font-size:1.0rem;color:#4a5568;font-weight:normal;}
h2{font-size:1.15rem;color:#1f4e8c;border-left:5px solid #1f4e8c;padding-left:10px;margin-top:44px;}
h3{font-size:0.98rem;margin-top:20px;}
.meta{color:#5a6472;font-size:0.85rem;}
.tablewrap{overflow-x:auto;margin:10px 0;}
table{border-collapse:collapse;width:100%;font-size:0.85rem;}
th{background:#f0f4fa;border:1px solid #ccd6e4;padding:7px 9px;text-align:left;white-space:nowrap;}
td{border:1px solid #ccd6e4;padding:7px 9px;vertical-align:top;}
.card{border:1px solid #d8dfeb;border-radius:8px;padding:14px 18px;margin:12px 0;}
.chip{display:inline-block;font-size:0.78rem;background:#eef4fc;color:#1f4e8c;border-radius:12px;padding:2px 10px;margin:2px;}
.note{color:#5a6472;font-size:0.8rem;}
dl{margin:8px 0;} dt{font-weight:bold;font-size:0.85rem;color:#1f4e8c;margin-top:8px;} dd{margin:2px 0 6px;}
ul,ol{padding-left:22px;} li{font-size:0.9rem;margin:3px 0;}
@media print{body{padding:0;} h2{-webkit-print-color-adjust:exact;print-color-adjust:exact;}}
";

#[cfg(test)]
mod tests {
    use super::*;
    use serde_json::json;

    /// テスト用フィクスチャは全て架空の企業・数値。実案件のデータは持ち込まない。
    fn case_profile() -> Value {
        json!({
            "company_name": "株式会社サンプル物流",
            "job_title": "配送ドライバー（サンプル営業所）",
            "prefecture": "架空県",
            "municipality": "見本市"
        })
    }

    fn facts() -> Value {
        json!({
            "salary": {"value": "月給250,000円〜", "evidence_quote": "月給250,000円〜", "status": "verified"},
            "holidays": {"value": "週休2日制", "evidence_quote": "週休2日制", "status": "verified"},
            // 未照合の項目は表に出さない (逆証明用)。
            "bonus": {"value": "賞与年3回", "evidence_quote": "賞与年3回", "status": "unverified"}
        })
    }

    fn prepare_result() -> Value {
        json!({
            "personas": [
                {
                    "id": "persona_1",
                    "label": "近隣在住の経験者",
                    "profile": "見本市の近隣に住む30代。\n通勤時間を重視している。\n\n家族と同居している設定の作例。",
                    "priority_conditions": ["通勤距離", "休日の取りやすさ"],
                    "likely_behavior": "検索・比較する",
                    "behavior_reason": "周辺求人と条件が近く比較検討に入るため"
                },
                {
                    "id": "persona_2",
                    "label": "未経験からの転職希望者",
                    "profile": "別業種からの転職を検討する20代の作例。",
                    "priority_conditions": ["研修体制"],
                    "likely_behavior": "求人閲覧段階で離脱する",
                    "behavior_reason": "研修に関する記載が求人票に見当たらないため"
                }
            ],
            "client_questions": ["研修期間中の待遇をお教えください"],
            "limitations": ["口コミの件数が少なく、傾向の確認に留まります"]
        })
    }

    fn persona_details() -> HashMap<String, Value> {
        let mut map = HashMap::new();
        map.insert(
            "persona_1".to_string(),
            json!({
                "priority_actions": [
                    {
                        "priority": "中",
                        "countermeasure": "通勤手当の支給条件を求人票の待遇欄に明記すべき",
                        "basis_type": "データ由来",
                        "client_fact_status": "確認済み事実",
                        "evidence_refs": ["J1", "C36"]
                    },
                    {
                        "priority": "高",
                        "countermeasure": "年代別の在籍状況を確認し、実績があれば求人票に追加すべき",
                        "basis_type": "データからの推論",
                        "client_fact_status": "未確認",
                        "evidence_refs": ["R1"]
                    }
                ],
                "client_questions": ["通勤手当の上限額をお教えください"]
            }),
        );
        map.insert(
            "persona_2".to_string(),
            json!({
                "priority_actions": [
                    {
                        "priority": "低",
                        "countermeasure": "応募から24時間以内に一次連絡を入れる運用にすべき",
                        "basis_type": "一般的な採用施策",
                        "client_fact_status": "顧客事実ではない",
                        "evidence_refs": []
                    },
                    {
                        // 重複排除の対象 (persona_1 と同一文面)。
                        "priority": "高",
                        "countermeasure": "通勤手当の支給条件を求人票の待遇欄に明記すべき",
                        "basis_type": "データ由来",
                        "client_fact_status": "確認済み事実",
                        "evidence_refs": ["J1"]
                    }
                ],
                // 準備段階と同一の質問 (重複排除の対象)。
                "client_questions": ["研修期間中の待遇をお教えください", "研修担当者の人数をお教えください"]
            }),
        );
        map
    }

    fn fact_conflicts() -> Vec<Value> {
        vec![json!({
            "topic": "賞与",
            "quote_a": "賞与年3回実績",
            "quote_b": "賞与年2回＋決算賞与",
            "explanation": "回数の食い違い"
        })]
    }

    fn posting_drafts() -> Vec<Value> {
        vec![json!({
            "catch_copy_options": [
                {"text": "見本市から通える配送ドライバー"},
                {"text": "週休2日でつづけられる配送の仕事"},
                {"text": "3件目は抜粋に出ない"}
            ],
            "job_description_markdown": "見本市周辺への配送業務です。\n\n【取材で確認: 1日の配送件数】"
        })]
    }

    fn note_drafts() -> Vec<Value> {
        vec![json!({
            "title_options": ["サンプル物流の一日", "配送の現場から", "3件目は抜粋に出ない"],
            "lead": "架空の営業所を舞台にした記事案のリード文です。"
        })]
    }

    struct Fixture {
        case_profile: Value,
        facts: Value,
        fact_ledger: Value,
        salary_breakdown: Value,
        client_salary_position: Value,
        comparison_cohort: Value,
        prepare_result: Value,
        fact_conflicts: Vec<Value>,
        persona_details: HashMap<String, Value>,
        note_drafts: Vec<Value>,
        posting_drafts: Vec<Value>,
    }

    impl Fixture {
        fn new() -> Self {
            Self {
                case_profile: case_profile(),
                facts: facts(),
                fact_ledger: json!({"supplementary_conditions": ["社会保険完備", "マイカー通勤可"]}),
                salary_breakdown: json!({
                    "display_monthly_min_yen": 250_000,
                    "display_monthly_max_yen": 320_000,
                    "base_monthly_yen": 200_000,
                    "fixed_overtime": false,
                    "overtime_pay_included_in_display": true,
                    "overtime_hours_range": [20, 30]
                }),
                client_salary_position: json!({
                    "client_monthly_equivalent_yen": 250_000,
                    "sample_count": 48,
                    "median_yen": 240_000,
                    "percentile_position": 62.5
                }),
                comparison_cohort: json!({
                    "commute_salary_layers": [
                        {"layer": "通勤圏15km以内", "count": 12, "median_yen": 238_000, "client_percentile_position": 60.0},
                        {"layer": "比較範囲全体", "count": 48, "median_yen": 240_000, "client_percentile_position": null}
                    ]
                }),
                prepare_result: prepare_result(),
                fact_conflicts: fact_conflicts(),
                persona_details: persona_details(),
                note_drafts: note_drafts(),
                posting_drafts: posting_drafts(),
            }
        }

        fn render(&self) -> String {
            render_customer_report(&CustomerReportInput {
                case_profile: &self.case_profile,
                facts: &self.facts,
                fact_ledger: &self.fact_ledger,
                salary_breakdown: &self.salary_breakdown,
                client_salary_position: &self.client_salary_position,
                comparison_cohort: &self.comparison_cohort,
                prepare_result: &self.prepare_result,
                fact_conflicts: &self.fact_conflicts,
                persona_details: &self.persona_details,
                note_drafts: &self.note_drafts,
                posting_drafts: &self.posting_drafts,
            })
        }
    }

    #[test]
    fn renders_all_eight_sections() {
        let html = Fixture::new().render();
        for heading in [
            "<h2>1. 貴社求人から確認できた内容</h2>",
            "<h2>2. 周辺市場での給与の位置</h2>",
            "<h2>3. この求人市場で想定される応募者像</h2>",
            "<h2>4. 貴社に確認させていただきたい事項</h2>",
            "<h2>5. 求人票・応募対応の改善提案</h2>",
            "<h2>6. 求人票の改善イメージ（抜粋）</h2>",
            "<h2>7. 採用広報記事（note等）のイメージ</h2>",
            "<h2>8. この診断の前提と限界</h2>",
        ] {
            assert!(html.contains(heading), "見出しが出ていない: {heading}");
        }
        assert!(html.starts_with("<!DOCTYPE html>"), "完全なHTML文書でない");
        assert!(html.ends_with("</body></html>"));
        // 外部リソースを読まない自己完結HTMLであること。
        assert!(!html.contains("<link "), "外部スタイルシートを参照している");
        assert!(!html.contains("http://") && !html.contains("https://"));
    }

    /// 逆証明: フィクスチャに evidence_refs (J1/C36/R1) を入れた上で、
    /// 内部記号・内部語が 1 つも出力に現れないこと。
    #[test]
    fn no_internal_symbols_leak() {
        let html = Fixture::new().render();
        for symbol in [
            "J1",
            "C36",
            "C1",
            "R1",
            "P1",
            "persona_1",
            "evidence_refs",
            "evidence_quote",
            "品質ゲート",
            "quality_gate",
            "データ由来",
            "データからの推論",
            "一般的な採用施策",
            "再生成",
            "差し戻し",
        ] {
            assert!(!html.contains(symbol), "内部記号が漏れている: {symbol}");
        }
        // 内部語は顧客語に変換されて出ていること (単に消えたのではない)。
        assert!(html.contains("周辺求人・貴社求人の実測から"));
        assert!(html.contains("実測からの推論"));
        assert!(html.contains("一般的な採用実務として"));
    }

    /// 逆証明: 顧客の求人を評価する語を固定文言に使っていない。
    #[test]
    fn no_evaluative_words() {
        let html = Fixture::new().render();
        for word in ["劣位", "埋もれ", "見劣り", "弱い"] {
            assert!(!html.contains(word), "評価語が入っている: {word}");
        }
    }

    /// 未確認の事実に触れる提案には備考「要確認」が付く。確認済みには付かない。
    #[test]
    fn unverified_proposal_is_marked_for_confirmation() {
        let html = Fixture::new().render();
        let row_start = html
            .find("年代別の在籍状況")
            .expect("未確認の提案が出ていない");
        let row_end = html[row_start..]
            .find("</tr>")
            .map(|i| row_start + i)
            .expect("行が閉じていない");
        assert!(
            html[row_start..row_end].contains("要確認"),
            "未確認の提案に「要確認」が付いていない"
        );

        let verified_start = html
            .find("通勤手当の支給条件")
            .expect("確認済みの提案が出ていない");
        let verified_end = html[verified_start..]
            .find("</tr>")
            .map(|i| verified_start + i)
            .expect("行が閉じていない");
        assert!(
            !html[verified_start..verified_end].contains("要確認"),
            "確認済みの提案に「要確認」が付いている"
        );
    }

    /// 提案は優先度順 (高→中→低)、対策文が同じものは 1 行に畳む。
    #[test]
    fn proposals_are_deduplicated_and_ordered_by_priority() {
        let html = Fixture::new().render();
        let high = html.find("年代別の在籍状況").expect("高優先度の提案");
        let mid = html.find("通勤手当の支給条件").expect("中優先度の提案");
        let low = html.find("24時間以内に一次連絡").expect("低優先度の提案");
        assert!(high < mid && mid < low, "優先度順に並んでいない");
        // 同一文面は 2 ペルソナに跨って出ているが 1 行だけ。
        assert_eq!(html.matches("通勤手当の支給条件").count(), 1);
    }

    /// 逆証明: note 下書きが無ければセクションごと出ない (空セクションを作らない)。
    #[test]
    fn note_section_is_omitted_without_drafts() {
        let mut fixture = Fixture::new();
        fixture.note_drafts = Vec::new();
        let html = fixture.render();
        assert!(!html.contains("<h2>7. 採用広報記事（note等）のイメージ</h2>"));
        assert!(!html.contains("※この記事案はあくまでイメージです。"));
        // 他のセクションは残る。
        assert!(html.contains("<h2>8. この診断の前提と限界</h2>"));
        assert!(html.contains("<h2>6. 求人票の改善イメージ（抜粋）</h2>"));
    }

    /// 逆証明: 求人票下書きが無ければ 6 のセクションも出ない。
    #[test]
    fn posting_section_is_omitted_without_drafts() {
        let mut fixture = Fixture::new();
        fixture.posting_drafts = Vec::new();
        let html = fixture.render();
        assert!(!html.contains("<h2>6. 求人票の改善イメージ（抜粋）</h2>"));
        assert!(html.contains("<h2>7. 採用広報記事（note等）のイメージ</h2>"));
    }

    #[test]
    fn note_section_states_it_is_an_image() {
        let html = Fixture::new().render();
        assert!(html.contains("※この記事案はあくまでイメージです。"));
    }

    /// HTML エスケープ: 入力の生タグが出力にそのまま現れない。
    #[test]
    fn dynamic_strings_are_escaped() {
        let mut fixture = Fixture::new();
        fixture.case_profile = json!({
            "company_name": "<script>alert('xss')</script>株式会社サンプル物流",
            "job_title": "配送ドライバー",
            "prefecture": "架空県",
            "municipality": "見本市"
        });
        fixture.prepare_result = json!({
            "personas": [{
                "id": "persona_1",
                "label": "<img src=x onerror=alert(1)>",
                "profile": "<script>alert('profile')</script>",
                "priority_conditions": ["<b>太字</b>"],
                "likely_behavior": "応募へ進む",
                "behavior_reason": "<script>alert('reason')</script>"
            }],
            "client_questions": ["<script>alert('q')</script>"],
            "limitations": ["<script>alert('lim')</script>"]
        });
        let html = fixture.render();

        assert!(!html.contains("<script>"), "生の<script>が出力に現れた");
        assert!(!html.contains("</script>"));
        assert!(!html.contains("<img src=x"));
        assert!(!html.contains("<b>太字</b>"));
        assert!(
            html.contains("&lt;script&gt;"),
            "エスケープ結果が見当たらない"
        );
        // <title> 属性側も同様。
        assert!(html.contains("<title>採用ジャーニー診断レポート（&lt;script&gt;"));
    }

    /// 事実表は照合済みの項目だけ。未照合の値は載せない。
    #[test]
    fn only_verified_facts_are_listed() {
        let html = Fixture::new().render();
        assert!(html.contains("<td>給与</td>"));
        assert!(html.contains("月給250,000円〜"));
        assert!(html.contains("<td>休日</td>"));
        // status が verified でない賞与は出ない。
        assert!(!html.contains("賞与年3回実績</td>"));
        assert!(!html.contains("<td>賞与</td>"));
    }

    #[test]
    fn salary_breakdown_rows_are_rendered() {
        let html = Fixture::new().render();
        assert!(html.contains("250,000円〜320,000円"), "表示月給レンジ");
        assert!(html.contains("<td>200,000円</td>"), "基本給");
        assert!(html.contains("なし（求人票に明示）"), "固定残業なしの明示");
        assert!(html.contains("含む旨の記載あり"));
        assert!(html.contains("20〜30時間"));
    }

    /// 上限が無い場合は「以上」。fixed_overtime が未判定なら行を出さない。
    #[test]
    fn salary_breakdown_handles_missing_values() {
        let mut fixture = Fixture::new();
        fixture.salary_breakdown = json!({"display_monthly_min_yen": 250_000});
        let html = fixture.render();
        assert!(html.contains("250,000円以上"));
        assert!(
            !html.contains("固定残業制度"),
            "未判定を「なし」と書いている"
        );
        assert!(!html.contains("null"), "nullが画面に出ている");
    }

    #[test]
    fn salary_position_reports_upper_percentage() {
        let html = Fixture::new().render();
        // percentile_position 62.5 → 上位 38%。
        assert!(html.contains("上位 38% の位置"), "給与位置の表記が想定外");
        assert!(html.contains("求人48件と比較すると"));
        assert!(html.contains("<td>通勤圏15km以内</td>"));
        assert!(html.contains("<td>12件</td>"));
        // 位置が確定しないレイヤーは「—」で、推測値を書かない。
        assert!(html.contains("<td>—</td>"));
        assert!(html.contains("内訳構成までは揃えていません"));
    }

    /// 確認事項は食い違い → 準備段階 → ペルソナ別の順で、同一文面は畳む。
    #[test]
    fn questions_are_ordered_and_deduplicated() {
        let html = Fixture::new().render();
        let conflict = html
            .find("求人内で記載が分かれている「賞与」")
            .expect("食い違いの質問");
        let prepared = html.find("研修期間中の待遇").expect("準備段階の質問");
        let persona = html.find("通勤手当の上限額").expect("ペルソナ別の質問");
        assert!(conflict < prepared && prepared < persona, "順序が想定外");
        assert_eq!(
            html.matches("研修期間中の待遇").count(),
            1,
            "同一質問が重複している"
        );
    }

    #[test]
    fn personas_include_disclaimer_and_details() {
        let html = Fixture::new().render();
        assert!(html.contains("作例であり、実在の人物ではありません"));
        assert!(html.contains("近隣在住の経験者"));
        assert!(
            html.contains("通勤距離、休日の取りやすさ"),
            "重視条件の連結"
        );
        assert!(
            html.contains("検索・比較する — 周辺求人と条件が近く"),
            "想定反応と理由"
        );
        // 段落内の単一改行は <br>、空行は段落分割。
        assert!(html.contains("30代。<br>通勤時間を重視している。"));
        assert!(html.contains("</p><p>家族と同居している設定の作例。</p>"));
    }

    #[test]
    fn excerpts_are_limited_to_two_options() {
        let html = Fixture::new().render();
        assert!(html.contains("「見本市から通える配送ドライバー」"));
        assert!(html.contains("サンプル物流の一日"));
        assert!(
            !html.contains("3件目は抜粋に出ない"),
            "抜粋が2件を超えている"
        );
    }

    /// 入力側の limitations が空でも、範囲の但し書きは必ず出る。
    #[test]
    fn limitations_always_include_fixed_disclaimers() {
        let mut fixture = Fixture::new();
        fixture.prepare_result = json!({"personas": [], "client_questions": [], "limitations": []});
        let html = fixture.render();
        assert!(html.contains("求人市場の全体ではありません"));
        assert!(html.contains("応募数を保証するものではありません"));
    }

    /// 同じ入力からは同じ HTML (HashMap の反復順に依存しない)。
    #[test]
    fn same_input_is_deterministic() {
        let fixture = Fixture::new();
        let first = fixture.render();
        for _ in 0..5 {
            assert_eq!(first, fixture.render(), "出力が入力以外の要因で変わった");
        }
    }

    /// 空入力でも panic せず、null や "None" を画面に出さない。
    #[test]
    fn empty_input_renders_without_placeholders() {
        let empty = Value::Null;
        let details = HashMap::new();
        let html = render_customer_report(&CustomerReportInput {
            case_profile: &empty,
            facts: &empty,
            fact_ledger: &empty,
            salary_breakdown: &empty,
            client_salary_position: &empty,
            comparison_cohort: &empty,
            prepare_result: &empty,
            fact_conflicts: &[],
            persona_details: &details,
            note_drafts: &[],
            posting_drafts: &[],
        });
        assert!(html.starts_with("<!DOCTYPE html>"));
        assert!(!html.contains("null"));
        assert!(!html.contains("None"));
        assert!(html.contains("<h2>1. 貴社求人から確認できた内容</h2>"));
        assert!(html.contains("<h2>8. この診断の前提と限界</h2>"));
        // 下書きが無いので 6・7 は出ない。
        assert!(!html.contains("<h2>6."));
        assert!(!html.contains("<h2>7."));
    }

    /// `{"result": {...}}` で包まれた値を渡されても中身を読む。
    #[test]
    fn wrapped_result_values_are_unwrapped() {
        let mut fixture = Fixture::new();
        fixture.note_drafts = vec![json!({"result": note_drafts()[0].clone()})];
        let mut wrapped_details = HashMap::new();
        for (key, value) in persona_details() {
            wrapped_details.insert(key, json!({"result": value}));
        }
        fixture.persona_details = wrapped_details;
        let html = fixture.render();
        assert!(html.contains("サンプル物流の一日"), "note が読めていない");
        assert!(
            html.contains("通勤手当の支給条件"),
            "priority_actions が読めていない"
        );
    }

    #[test]
    fn formatting_helpers() {
        assert_eq!(with_thousands_separator(250_000), "250,000");
        assert_eq!(with_thousands_separator(999), "999");
        assert_eq!(with_thousands_separator(0), "0");
        assert_eq!(yen(&json!(250_000)), "250,000円");
        assert_eq!(yen(&json!("応相談")), "応相談");
        assert_eq!(yen(&Value::Null), "");
        assert_eq!(number_text(&json!(60.0)), "60");
        assert_eq!(number_text(&json!(62.5)), "62.5");
        assert_eq!(basis_label("データ由来"), "周辺求人・貴社求人の実測から");
        assert_eq!(basis_label("独自の値"), "独自の値");
        assert_eq!(fact_label("salary"), "給与");
        assert_eq!(fact_label("unknown_key"), "unknown_key");
        assert_eq!(md(""), "<p></p>");
        assert_eq!(md("a\nb"), "<p>a<br>b</p>");
        assert_eq!(md("a\n\nb"), "<p>a</p><p>b</p>");
        assert_eq!(esc("<&\"'>"), "&lt;&amp;&quot;&#x27;&gt;");
    }
}
