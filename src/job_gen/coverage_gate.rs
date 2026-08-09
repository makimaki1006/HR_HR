//! 重大条件の網羅ゲート (Material Fact Coverage Gate)。
//!
//! # なぜ必要か
//!
//! [`super::validate`] や [`super::claim_audit`] は「書かれた内容が事実と矛盾しないか」を見る。
//! しかし実データ (センコー) でペルソナ別6案を横断したところ、**矛盾は無いのに
//! 「最初の1年は契約社員からスタート」が4案で丸ごと欠落**していた。矛盾検知では
//! 「書かなかったこと」は捕まらない。本モジュールはその逆側、つまり
//! **応募判断に重大な条件 (Material Facts) が原稿に載っているか**の網羅チェックを担う。
//!
//! # 判定方式: LLM を使わない純粋関数
//!
//! 他の検証ゲート ([`super::ng_words`], [`super::validate`]) と同じ方針で、判定は
//! コード側の決定論的処理にする。LLM に「重要な条件が抜けていないか」を尋ねると、
//! 抜けていても「網羅されています」と答える (= 検証にならない) ため。
//!
//! # 「存在しないものは要求しない」
//!
//! [`MaterialFacts`] の各項目は `Option` で、**入力データに実在した条件だけ**を入れる。
//! `None` の項目は一切チェックしない。全項目 `None` なら issue は空になる
//! (`empty_facts_require_nothing` テストで逆証明)。原文に無い条件を「書け」と
//! 要求すると、書き手が事実でない文言を足す方向に働くため。
//!
//! # 既知の限界
//!
//! - 判定は**語と数値の含有**しか見ない。「契約社員」の語があれば通るので、
//!   文脈まで正しいか (例: 「契約社員は募集していません」と書いても通る) は保証しない。
//!   ここは矛盾側のゲート ([`super::claim_audit`]) と併用して塞ぐ前提。
//! - 数値正規化はカンマを全除去するので「254,200」と「254200」は同一視されるが、
//!   桁区切り以外のカンマも消える。含有チェック用途なので実害は無いと判断した。
//! - 空白は除去しない。「254, 200」のように桁区切りの直後に空白がある表記は
//!   検出できない (空白まで消すと無関係な数字が連結し、誤って合格するため)。
//! - 残業時間は範囲の端点いずれかの数字が原稿にあれば通す。「30」は「300」にも
//!   一致するため、緩い方向に倒れる。

/// 入力データに実在した重大条件。存在しない項目は `None` (存在しないものは要求しない)。
#[derive(Debug, Clone, Default)]
pub struct MaterialFacts {
    /// 初年度の雇用形態に関する原文引用 (例:「最初の1年は契約社員からスタート」)。
    pub first_year_employment: Option<String>,
    /// 表示月給の下限 (円)。
    pub salary_min_yen: Option<i64>,
    /// 基本給 (円)。
    pub base_monthly_yen: Option<i64>,
    /// 表示月給に残業代が含まれる旨が原文にあるか。
    pub overtime_pay_included: bool,
    /// 想定残業時間 (月)。
    pub overtime_hours_range: Option<(u32, u32)>,
    /// 休日の事実値 (例:「週休2日制 ┗日曜日＋祝日＋他シフト制」)。
    pub holidays: Option<String>,
    /// 必須資格の事実値 (例:「【必須】 普通運転免許資格保有」)。
    pub required_qualifications: Option<String>,
}

/// 引用の最大文字数 (文字数であってバイト数ではない)。
pub const MAX_QUOTE_CHARS: usize = 60;

/// 引用で手がかり語の手前に含める文脈の文字数。
const CONTEXT_BEFORE_CHARS: usize = 15;

/// 文の区切りとみなす文字 (句点・感嘆符・疑問符・改行)。
const SENTENCE_DELIMS: [char; 6] = ['。', '！', '？', '!', '?', '\n'];

/// 初年度条件を示す手がかり語の中核。
const CONTRACT_WORD: &str = "契約社員";

/// [`CONTRACT_WORD`] の**直後**に続けば初年度条件とみなす表現。
const START_SUFFIXES: &[&str] = &["からスタート", "スタート", "として入社", "でのスタート"];

/// [`CONTRACT_WORD`] の**直前**が年数表現のとき、その前に付いてよい語。
///
/// 「◯年間は契約社員」だけは前置語を要求しない (「年間」自体が期間を表すため)。
const YEAR_PREFIX_LEADS: &[&str] = &["最初の", "入社後"];

/// 休日の事実値と原稿の両方に共通して現れるべき特徴トークンの候補。
const HOLIDAY_FEATURE_TOKENS: &[&str] = &["日曜", "祝日", "シフト", "土日"];

/// 休日に言及していると認める語。
const HOLIDAY_WORDS: &[&str] = &["休日", "週休", "休み"];

/// 必須資格に言及していると認める語。
const QUALIFICATION_WORDS: &[&str] = &["免許", "資格"];

/// 原文から初年度雇用形態の記載を検出する (存在すれば引用を返す)。
///
/// 検出するのは以下の文脈のみ:
/// - 「契約社員からスタート」「契約社員スタート」「契約社員として入社」「契約社員でのスタート」
/// - 「最初の◯年は契約社員」「入社後◯年は契約社員」「◯年間は契約社員」(◯ は半角/全角の1〜2桁)
///
/// 「契約社員」の語があっても上記文脈が無ければ `None` を返す。雇用形態一覧の
/// 「正社員・契約社員・パート募集」は初年度条件ではないため
/// (`employment_type_list_is_not_first_year_condition` テストで逆証明)。
///
/// 戻り値は原文から切り出した逐語引用 (句点/改行区切りの一文、最大
/// [`MAX_QUOTE_CHARS`] 文字、前後の空白のみ除去)。
pub fn detect_first_year_employment(source_text: &str) -> Option<String> {
    let mut from = 0usize;
    while let Some(rel) = source_text[from..].find(CONTRACT_WORD) {
        let start = from + rel;
        let end = start + CONTRACT_WORD.len();
        let suffix_hit = START_SUFFIXES
            .iter()
            .any(|s| source_text[end..].starts_with(s));
        if suffix_hit || has_first_year_prefix(&source_text[..start]) {
            return Some(quote_sentence(source_text, start, CONTRACT_WORD.len()));
        }
        from = end;
    }
    None
}

/// 「契約社員」の直前が「最初の◯年は」「入社後◯年は」「◯年間は」かを見る。
fn has_first_year_prefix(prefix: &str) -> bool {
    let head = match prefix.strip_suffix('は') {
        Some(h) => h,
        None => return false,
    };
    if let Some(rest) = head.strip_suffix("年間") {
        return trim_trailing_year_number(rest).is_some();
    }
    if let Some(rest) = head.strip_suffix('年') {
        if let Some(before) = trim_trailing_year_number(rest) {
            return YEAR_PREFIX_LEADS.iter().any(|lead| before.ends_with(lead));
        }
    }
    false
}

/// 末尾の1〜2桁の数字 (半角/全角) を落とした残りを返す。数字が無い、または
/// 3桁以上続く場合は `None` (「◯年」は1〜2桁という仕様のため)。
fn trim_trailing_year_number(text: &str) -> Option<&str> {
    let mut cut = text.len();
    let mut digits = 0usize;
    for (i, c) in text.char_indices().rev() {
        if !is_digit_char(c) {
            break;
        }
        digits += 1;
        if digits > 2 {
            return None;
        }
        cut = i;
    }
    if digits == 0 {
        None
    } else {
        Some(&text[..cut])
    }
}

fn is_digit_char(c: char) -> bool {
    c.is_ascii_digit() || ('０'..='９').contains(&c)
}

/// 求人票原稿に対する重大条件の網羅チェック。
///
/// 欠落項目ごとに日本語の issue 文字列を返す (空なら合格)。`posting_text` は
/// キャッチ・見出し・仕事内容・セクション本文を連結したテキストを想定する。
pub fn material_fact_coverage_issues(facts: &MaterialFacts, posting_text: &str) -> Vec<String> {
    let normalized = normalize_numeric(posting_text);
    let mut issues = Vec::new();

    // 1. 初年度雇用形態。
    if let Some(quote) = facts.first_year_employment.as_deref() {
        if !posting_text.contains(CONTRACT_WORD) {
            issues.push(format!(
                "求人票に初年度の雇用形態(契約社員スタート)が記載されていません。応募判断に重大な条件のため本文に明記してください。原文: 「{}」",
                quote.trim()
            ));
        }
    }

    // 2. 給与レンジの下限。
    if let Some(min_yen) = facts.salary_min_yen {
        if !contains_amount(&normalized, min_yen) {
            issues.push(format!(
                "求人票に月給の下限({}円)が記載されていません。「{}円」または「{}」の形で本文に明記してください。",
                with_thousands_separator(min_yen),
                with_thousands_separator(min_yen),
                man_notation(min_yen)
            ));
        }
    }

    // 3. 基本給 (表示月給に残業代が含まれる場合のみ)。
    if let Some(base_yen) = facts.base_monthly_yen {
        if facts.overtime_pay_included
            && !posting_text.contains("基本給")
            && !contains_amount(&normalized, base_yen)
        {
            issues.push(format!(
                "求人票に基本給({}円)が記載されていません。表示月給に残業代が含まれるため、基本給の明示が必要です(残業代を除いた固定部分が分からないと応募判断できません)。",
                with_thousands_separator(base_yen)
            ));
        }
    }

    // 4. 想定残業時間。
    if let Some((lo, hi)) = facts.overtime_hours_range {
        let has_word = posting_text.contains("残業");
        let has_number =
            normalized.contains(&lo.to_string()) || normalized.contains(&hi.to_string());
        if !has_word || !has_number {
            issues.push(format!(
                "求人票に想定残業時間(月{lo}〜{hi}時間)が記載されていません。応募判断に重大な条件のため本文に明記してください。"
            ));
        }
    }

    // 5. 休日。
    if let Some(holidays) = facts.holidays.as_deref() {
        let has_word = HOLIDAY_WORDS.iter().any(|w| posting_text.contains(w));
        // 事実値に現れる特徴トークンのうち、原稿にも現れるものがあるか。
        // 事実値がどのトークンも含まない場合は要求しない (存在しない条件は要求しない方針)。
        let fact_tokens: Vec<&str> = HOLIDAY_FEATURE_TOKENS
            .iter()
            .copied()
            .filter(|t| holidays.contains(t))
            .collect();
        let has_token =
            fact_tokens.is_empty() || fact_tokens.iter().any(|t| posting_text.contains(t));
        if !has_word || !has_token {
            issues.push(format!(
                "求人票の休日の記載が事実値に届いていません。事実値「{}」に沿って本文に明記してください。",
                holidays.trim()
            ));
        }
    }

    // 6. 必須資格。
    if let Some(required) = facts.required_qualifications.as_deref() {
        if !QUALIFICATION_WORDS.iter().any(|w| posting_text.contains(w)) {
            issues.push(format!(
                "求人票に必須資格が記載されていません。事実値「{}」を本文に明記してください(応募可否が変わる条件です)。",
                required.trim()
            ));
        }
    }

    issues
}

/// 金額が原稿 (正規化済み) に含まれるか。
///
/// 許容表記は「そのままの数字」「カンマ区切り」「万円換算の小数1桁」。
/// カンマ区切りは [`normalize_numeric`] でカンマが落ちるため素の数字と同一になる。
/// 万円換算は切り捨て・四捨五入の両方を許し、小数第1位が 0 のときは
/// 整数表記 (例:「30万」) も許す。
fn contains_amount(normalized_posting: &str, yen: i64) -> bool {
    amount_notations(yen)
        .iter()
        .any(|n| normalized_posting.contains(n))
}

/// 金額の許容表記の一覧 (正規化後の文字列として照合する形)。
fn amount_notations(yen: i64) -> Vec<String> {
    let mut out = vec![yen.to_string()];
    if yen <= 0 {
        return out;
    }
    // 切り捨てと四捨五入の両方 (原稿側がどちらで丸めているか分からないため)。
    for tenths in [yen / 1_000, (yen + 500) / 1_000] {
        let notation = format!("{}.{}万", tenths / 10, tenths % 10);
        if !out.contains(&notation) {
            out.push(notation);
        }
        if tenths % 10 == 0 {
            let integer = format!("{}万", tenths / 10);
            if !out.contains(&integer) {
                out.push(integer);
            }
        }
    }
    out
}

/// メッセージ用の万円表記 (小数1桁、切り捨て)。
fn man_notation(yen: i64) -> String {
    if yen <= 0 {
        return format!("{yen}円");
    }
    let tenths = yen / 1_000;
    if tenths % 10 == 0 {
        format!("{}万円", tenths / 10)
    } else {
        format!("{}.{}万円", tenths / 10, tenths % 10)
    }
}

/// メッセージ用の桁区切り表記 (例: 254200 → 254,200)。
fn with_thousands_separator(value: i64) -> String {
    let negative = value < 0;
    let digits = value.abs().to_string();
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

/// 数値照合用の正規化: 全角数字→半角、全角ピリオド→半角、カンマ (半角/全角) を除去。
///
/// 日本語の語はそのまま残るので、語の含有チェックにも使える。
fn normalize_numeric(text: &str) -> String {
    text.chars()
        .filter_map(|c| match c {
            '０'..='９' => Some(char::from(b'0' + (c as u32 - '０' as u32) as u8)),
            '．' => Some('.'),
            ',' | '，' => None,
            other => Some(other),
        })
        .collect()
}

/// 手がかり語を含む一文を原文から逐語で切り出す (最大 [`MAX_QUOTE_CHARS`] 文字)。
///
/// 戻り値は必ず `text` の部分文字列 (前後の空白除去のみ)。
fn quote_sentence(text: &str, cue_start: usize, cue_len: usize) -> String {
    let cue_end = cue_start + cue_len;

    let sent_start = text[..cue_start]
        .rfind(SENTENCE_DELIMS)
        .map(|i| i + text[i..].chars().next().map_or(1, char::len_utf8))
        .unwrap_or(0);
    let sent_end = text[cue_end..]
        .find(SENTENCE_DELIMS)
        .map(|i| cue_end + i)
        .unwrap_or(text.len());

    if text[sent_start..sent_end].chars().count() <= MAX_QUOTE_CHARS {
        return text[sent_start..sent_end].trim().to_string();
    }

    // 長文は手がかり語の周辺だけを窓で切る (手がかり語は必ず窓に入る)。
    let win_start = nth_char_back(text, sent_start, cue_start, CONTEXT_BEFORE_CHARS);
    let win_end = nth_char_forward(text, win_start, sent_end, MAX_QUOTE_CHARS).max(cue_end);
    text[win_start..win_end].trim().to_string()
}

/// `to` から後方へ最大 `n` 文字戻ったバイト位置 (`floor` 未満には戻らない)。
fn nth_char_back(text: &str, floor: usize, to: usize, n: usize) -> usize {
    text[floor..to]
        .char_indices()
        .rev()
        .take(n)
        .last()
        .map_or(to, |(i, _)| floor + i)
}

/// `from` から前方へ最大 `n` 文字進んだバイト位置 (`ceil` は超えない)。
fn nth_char_forward(text: &str, from: usize, ceil: usize, n: usize) -> usize {
    text[from..ceil]
        .char_indices()
        .nth(n)
        .map_or(ceil, |(i, _)| from + i)
}

#[cfg(test)]
mod tests {
    use super::*;

    /// 実データ (センコー) の雇用形態欄。「職業紹介(正社員)」と併記されていても
    /// 初年度が契約社員である事実を拾う。
    const SENKO_SOURCE: &str = "≪雇用形態≫ 職業紹介（正社員） 試用・研修の詳細情報：最初の1年は契約社員からスタートになります！給与は正社員と変わりません";

    /// 全項目を満たす原稿 (網羅合格の基準)。
    const FULL_POSTING: &str =
        "月給254,200円〜（基本給180,000円＋固定残業代35時間分）。想定残業は月30〜40時間。\
最初の1年は契約社員からスタートし、2年目に正社員へ登用されます。\
週休2日制で日曜＋祝日、そのほかはシフト制です。普通自動車運転免許が必須です。";

    fn senko_facts() -> MaterialFacts {
        MaterialFacts {
            first_year_employment: detect_first_year_employment(SENKO_SOURCE),
            salary_min_yen: Some(254_200),
            base_monthly_yen: Some(180_000),
            overtime_pay_included: true,
            overtime_hours_range: Some((30, 40)),
            holidays: Some("週休2日制 ┗日曜日＋祝日＋他シフト制".to_string()),
            required_qualifications: Some("【必須】 普通運転免許資格保有".to_string()),
        }
    }

    #[test]
    fn detects_senko_first_year_contract() {
        let quote = detect_first_year_employment(SENKO_SOURCE).expect("初年度条件を検出すること");
        assert!(
            quote.contains("契約社員からスタート"),
            "引用に初年度条件の文言が入っていない: {quote}"
        );
        // 引用は原文の逐語部分文字列であること (省略記号等を足さない)。
        assert!(
            SENKO_SOURCE.contains(&quote),
            "引用が原文に実在しない: {quote}"
        );
        assert!(quote.chars().count() <= MAX_QUOTE_CHARS);
    }

    #[test]
    fn detects_year_count_patterns() {
        for source in [
            "最初の1年は契約社員として勤務いただきます",
            "入社後2年は契約社員での雇用となります",
            "1年間は契約社員です",
            "３年間は契約社員となります",
            "契約社員スタートですが待遇は変わりません",
            "まずは契約社員として入社していただきます",
            "契約社員でのスタートとなります",
        ] {
            let quote = detect_first_year_employment(source)
                .unwrap_or_else(|| panic!("検出できていない: {source}"));
            assert!(source.contains(&quote), "引用が原文に実在しない: {quote}");
        }
    }

    /// 誤検出防止 (逆証明): 雇用形態の一覧に「契約社員」があるだけでは初年度条件ではない。
    #[test]
    fn employment_type_list_is_not_first_year_condition() {
        assert_eq!(
            detect_first_year_employment("正社員・契約社員・パート募集中"),
            None
        );
        assert_eq!(
            detect_first_year_employment("≪雇用形態≫ 正社員 / 契約社員 / アルバイト"),
            None
        );
        // 契約社員の語が無ければ当然 None。
        assert_eq!(
            detect_first_year_employment("最初の1年は研修期間となります"),
            None
        );
        // 3桁以上の年数は年数表現として扱わない。
        assert_eq!(
            detect_first_year_employment("創業100年は契約社員も含め全社員で祝いました"),
            None
        );
    }

    /// 長文でも引用は 60 文字以内で、かつ原文の逐語部分文字列。
    #[test]
    fn quote_is_bounded_and_verbatim() {
        let long = format!(
            "{}{}",
            "あ".repeat(200),
            "最初の1年は契約社員からスタートします"
        );
        let quote = detect_first_year_employment(&long).expect("検出すること");
        assert!(
            quote.chars().count() <= MAX_QUOTE_CHARS,
            "引用が長すぎる: {quote}"
        );
        assert!(long.contains(&quote));
        assert!(quote.contains("契約社員"));
    }

    #[test]
    fn full_coverage_has_no_issues() {
        let issues = material_fact_coverage_issues(&senko_facts(), FULL_POSTING);
        assert!(
            issues.is_empty(),
            "網羅済みの原稿で issue が出た: {issues:?}"
        );
    }

    /// 逆証明: 実害が出たケース (契約社員に触れない原稿) を確実に捕まえる。
    #[test]
    fn missing_first_year_employment_is_detected() {
        let posting = "月給254,200円〜（基本給180,000円＋固定残業代）。想定残業は月30〜40時間。\
週休2日制で日曜＋祝日、そのほかはシフト制です。普通自動車運転免許が必須です。正社員として活躍できます。";
        let issues = material_fact_coverage_issues(&senko_facts(), posting);
        assert_eq!(issues.len(), 1, "初年度条件だけが欠落のはず: {issues:?}");
        assert!(
            issues[0].contains("初年度の雇用形態"),
            "issue の文言が想定外: {}",
            issues[0]
        );
        // issue に原文引用が入っていること (書き手が何を書けばよいか分かる)。
        assert!(issues[0].contains("契約社員からスタート"));
    }

    #[test]
    fn man_notation_satisfies_salary_range() {
        let facts = MaterialFacts {
            salary_min_yen: Some(254_200),
            ..Default::default()
        };
        assert!(material_fact_coverage_issues(&facts, "月給25.4万円〜スタート").is_empty());
        // 全角数字・全角ピリオドでも通る。
        assert!(material_fact_coverage_issues(&facts, "月給２５．４万円〜スタート").is_empty());
        // 素の数字・カンマ区切りでも通る。
        assert!(material_fact_coverage_issues(&facts, "月給254200円〜").is_empty());
        assert!(material_fact_coverage_issues(&facts, "月給254,200円〜").is_empty());
        // 小数第1位が 0 の額は整数の万表記でも通る。
        let round = MaterialFacts {
            salary_min_yen: Some(300_000),
            ..Default::default()
        };
        assert!(material_fact_coverage_issues(&round, "月給30万円〜").is_empty());
        // 金額に触れなければ検出される。
        let issues = material_fact_coverage_issues(&facts, "しっかり稼げる好待遇です");
        assert_eq!(issues.len(), 1);
        assert!(issues[0].contains("254,200"), "{}", issues[0]);
    }

    /// 逆証明: 事実が無い項目は一切要求しない。
    #[test]
    fn empty_facts_require_nothing() {
        let facts = MaterialFacts::default();
        assert!(material_fact_coverage_issues(&facts, "").is_empty());
        assert!(
            material_fact_coverage_issues(&facts, "未経験歓迎！アットホームな職場です").is_empty()
        );
    }

    /// 基本給は「表示月給に残業代が含まれる」ときだけ要求する。
    #[test]
    fn base_salary_required_only_when_overtime_is_included() {
        let with_overtime = MaterialFacts {
            base_monthly_yen: Some(180_000),
            overtime_pay_included: true,
            ..Default::default()
        };
        let issues =
            material_fact_coverage_issues(&with_overtime, "月給254,200円〜。がっつり稼げます");
        assert_eq!(issues.len(), 1, "{issues:?}");
        assert!(issues[0].contains("基本給"), "{}", issues[0]);
        assert!(issues[0].contains("残業代が含まれる"), "{}", issues[0]);
        // 「基本給」の語があれば通る。
        assert!(material_fact_coverage_issues(
            &with_overtime,
            "基本給に固定残業代を加えた月給です"
        )
        .is_empty());
        // 金額の明示でも通る。
        assert!(
            material_fact_coverage_issues(&with_overtime, "内訳は180,000円＋残業代").is_empty()
        );
        // 残業代が含まれないなら要求しない。
        let without_overtime = MaterialFacts {
            overtime_pay_included: false,
            ..with_overtime.clone()
        };
        assert!(material_fact_coverage_issues(&without_overtime, "月給254,200円〜").is_empty());
    }

    #[test]
    fn overtime_hours_need_word_and_number() {
        let facts = MaterialFacts {
            overtime_hours_range: Some((30, 40)),
            ..Default::default()
        };
        assert!(material_fact_coverage_issues(&facts, "残業は月30時間程度です").is_empty());
        assert!(material_fact_coverage_issues(&facts, "残業は月40時間が上限です").is_empty());
        // 語だけ・数値だけでは通らない。
        let word_only = material_fact_coverage_issues(&facts, "残業は少なめです");
        assert_eq!(word_only.len(), 1);
        assert!(word_only[0].contains("残業時間"), "{}", word_only[0]);
        assert_eq!(
            material_fact_coverage_issues(&facts, "1日30分の朝礼があります").len(),
            1
        );
    }

    #[test]
    fn holidays_need_word_and_shared_feature_token() {
        let facts = MaterialFacts {
            holidays: Some("週休2日制 ┗日曜日＋祝日＋他シフト制".to_string()),
            ..Default::default()
        };
        assert!(material_fact_coverage_issues(&facts, "週休2日制。日曜はお休みです").is_empty());
        assert!(material_fact_coverage_issues(&facts, "休日はシフト制で調整します").is_empty());
        // 「休日充実」だけでは事実値の特徴 (日曜/祝日/シフト) に届かない。
        let issues = material_fact_coverage_issues(&facts, "休日はしっかり取れます");
        assert_eq!(issues.len(), 1, "{issues:?}");
        assert!(issues[0].contains("休日"), "{}", issues[0]);
        // 休日に一切触れない原稿も検出する。
        assert_eq!(
            material_fact_coverage_issues(&facts, "毎日日曜のような働きやすさ").len(),
            1
        );
        // 事実値が特徴トークンを含まない場合は語の言及だけで通す。
        let vague = MaterialFacts {
            holidays: Some("年間休日120日".to_string()),
            ..Default::default()
        };
        assert!(material_fact_coverage_issues(&vague, "年間休日は120日です").is_empty());
    }

    #[test]
    fn required_qualifications_need_licence_or_qualification_word() {
        let facts = MaterialFacts {
            required_qualifications: Some("【必須】 普通運転免許資格保有".to_string()),
            ..Default::default()
        };
        assert!(material_fact_coverage_issues(&facts, "普通自動車運転免許が必要です").is_empty());
        assert!(
            material_fact_coverage_issues(&facts, "必要な資格は入社前にご確認ください").is_empty()
        );
        let issues = material_fact_coverage_issues(&facts, "未経験から始められます");
        assert_eq!(issues.len(), 1, "{issues:?}");
        assert!(issues[0].contains("必須資格"), "{}", issues[0]);
    }

    /// 複数欠落は複数 issue になる (最初の1件で打ち切らない)。
    #[test]
    fn multiple_gaps_are_all_reported() {
        let issues =
            material_fact_coverage_issues(&senko_facts(), "アットホームな職場で一緒に働きませんか");
        assert_eq!(issues.len(), 6, "全6項目が欠落のはず: {issues:?}");
    }

    #[test]
    fn number_formatting_helpers() {
        assert_eq!(with_thousands_separator(254_200), "254,200");
        assert_eq!(with_thousands_separator(1_000), "1,000");
        assert_eq!(with_thousands_separator(999), "999");
        assert_eq!(with_thousands_separator(0), "0");
        assert_eq!(man_notation(254_200), "25.4万円");
        assert_eq!(man_notation(300_000), "30万円");
        assert!(amount_notations(254_200).contains(&"25.4万".to_string()));
        assert!(amount_notations(254_200).contains(&"254200".to_string()));
        assert_eq!(normalize_numeric("２５４，２００円"), "254200円");
        assert_eq!(normalize_numeric("254,200円"), "254200円");
    }
}
