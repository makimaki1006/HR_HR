//! 生成文の数値照合ゲート(工程⑦[E]の一部)。
//!
//! 移植元: `scripts/job_creation_media_engine/fact_validation.py`
//! (`find_unsupported_generated_numbers` + `find_unsupported_number_units`)。
//!
//! LLM生成文に「原文にない数値・待遇」が混入していないかを機械照合する安全ゲート。
//! 2段構え:
//! - ① 数値存在照合: 生成文中の数字トークンが原文にも存在するか。
//! - ② 数値+単位ペア照合: 「40代」「10日」のような数値+直後の単位の組が原文に実在するか。
//!   ①だけでは「原文の 240,860円 から 40 を取り出して 40代 と書く」流用を検知できないため
//!   ②が必須(2026-07-17 実PoCで実際に発生した捏造パターン)。
//!
//! Rust 版は `regex` クレートに依存しない方針のため、Python の正規表現
//! (`\d+(?:\.\d+)?` / `NUMBER_UNIT_PATTERN`)を手書きスキャナで同等に再現する。

use std::collections::HashSet;

/// 数値+単位ペアで照合する単位。Python `NUMBER_UNIT_PATTERN` と同集合。
/// 前方一致で最長優先にするため、複数文字の単位を先に並べる
/// (`万円`/`千円` は `円` より先、`時間`/`ヶ月`/`か月`/`ヵ月` も1文字単位より先)。
const NUMBER_UNITS: [&str; 18] = [
    "万円", "千円", "時間", "ヶ月", "か月", "ヵ月", // 2文字以上を先に
    "円", "分", "日", "回", "人", "名", "歳", "代", "年", "割", "%", "％",
];

/// 照合用に全角数字・空白・カンマなどを正規化する(Python `normalize_text` 同等)。
///
/// - 全角数字 `０-９` → 半角 `0-9`
/// - 全角記号 `．` → `.`、`：` → `:`
/// - 空白(全角含む)を除去
/// - カンマ(半角 `,`・全角 `，`)を除去
fn normalize_text(value: &str) -> String {
    let mut out = String::with_capacity(value.len());
    for ch in value.chars() {
        match ch {
            '０'..='９' => {
                let digit = b'0' + (ch as u32 - '０' as u32) as u8;
                out.push(digit as char);
            }
            '．' => out.push('.'),
            '：' => out.push(':'),
            ',' | '，' => {} // カンマは除去
            c if c.is_whitespace() => {} // 空白は除去
            c => out.push(c),
        }
    }
    out
}

/// `\d+(?:\.\d+)?` に相当する数字トークンを走査で抽出する。
fn extract_number_tokens(s: &[char]) -> Vec<String> {
    let mut out = Vec::new();
    let mut i = 0;
    while i < s.len() {
        if s[i].is_ascii_digit() {
            let start = i;
            while i < s.len() && s[i].is_ascii_digit() {
                i += 1;
            }
            // 任意の小数部 `.\d+`
            if i + 1 < s.len() && s[i] == '.' && s[i + 1].is_ascii_digit() {
                i += 1;
                while i < s.len() && s[i].is_ascii_digit() {
                    i += 1;
                }
            }
            out.push(s[start..i].iter().collect());
        } else {
            i += 1;
        }
    }
    out
}

/// 円・万円・千円などの数字トークンを抽出する(Python `numbers_in_text` 同等)。
///
/// 原文全体に「万円」「千円」が含まれる場合、整数トークンに ×10000 / ×1000 の
/// 展開値も加える(Python の粗い実装をそのまま踏襲。単位は文字列全体で判定)。
fn numbers_in_text(text: &str) -> HashSet<String> {
    let s = normalize_text(text);
    let chars: Vec<char> = s.chars().collect();
    let nums = extract_number_tokens(&chars);
    let has_man = s.contains("万円");
    let has_sen = s.contains("千円");

    let mut expanded: HashSet<String> = HashSet::new();
    for n in &nums {
        expanded.insert(n.clone());
        if n.contains('.') {
            continue;
        }
        if let Ok(i) = n.parse::<u128>() {
            if has_man {
                if let Some(v) = i.checked_mul(10_000) {
                    expanded.insert(v.to_string());
                }
            }
            if has_sen {
                if let Some(v) = i.checked_mul(1_000) {
                    expanded.insert(v.to_string());
                }
            }
        }
    }
    expanded
}

/// 「40代」「150円」「10日」のような数値+単位トークンを抽出する
/// (Python `number_unit_pairs` 同等)。数値の直後に単位が続く組だけを拾う。
fn number_unit_pairs(text: &str) -> HashSet<String> {
    let s = normalize_text(text);
    let chars: Vec<char> = s.chars().collect();
    let mut out = HashSet::new();
    let mut i = 0;
    while i < chars.len() {
        if chars[i].is_ascii_digit() {
            let start = i;
            while i < chars.len() && chars[i].is_ascii_digit() {
                i += 1;
            }
            if i + 1 < chars.len() && chars[i] == '.' && chars[i + 1].is_ascii_digit() {
                i += 1;
                while i < chars.len() && chars[i].is_ascii_digit() {
                    i += 1;
                }
            }
            let num_end = i;
            // 数値直後の単位を最長優先で照合。
            let rest: String = chars[num_end..].iter().collect();
            for unit in NUMBER_UNITS {
                if rest.starts_with(unit) {
                    let num: String = chars[start..num_end].iter().collect();
                    out.insert(format!("{num}{unit}"));
                    i = num_end + unit.chars().count();
                    break;
                }
            }
            // 単位が無ければ i は num_end のまま(数値の次から走査継続)。
        } else {
            i += 1;
        }
    }
    out
}

/// 生成文中の数値のうち、原文で裏付けられないものを列挙する(2段照合)。
///
/// - 段①: 数字トークンが原文に存在しない → その数字
/// - 段②: 数値+単位ペアが原文に存在しない → その「数値+単位」文字列
///
/// 返り値は段①(数字を長さ→辞書順)→段②(辞書順)の連結。空なら数値面は合格。
/// Python では `find_unsupported_generated_numbers` と `find_unsupported_number_units`
/// の2関数だが、契約 API は単一関数のため両段の結果を結合して返す。
pub fn find_unsupported_numbers(source_text: &str, generated_text: &str) -> Vec<String> {
    let mut out = Vec::new();

    // 段①: 数字存在照合。
    let source_nums = numbers_in_text(source_text);
    let mut plain: Vec<String> = numbers_in_text(generated_text)
        .into_iter()
        .filter(|n| !source_nums.contains(n))
        .collect();
    plain.sort_by(|a, b| (a.chars().count(), a).cmp(&(b.chars().count(), b)));
    out.extend(plain);

    // 段②: 数値+単位ペア照合。
    let source_pairs = number_unit_pairs(source_text);
    let mut pairs: Vec<String> = number_unit_pairs(generated_text)
        .into_iter()
        .filter(|p| !source_pairs.contains(p))
        .collect();
    pairs.sort();
    out.extend(pairs);

    out
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn 原文に存在する数値は合格() {
        let source = "月給250,000円 勤務8時間 年間休日120日";
        let gen = "月給250,000円で8時間勤務、休日120日です";
        assert!(find_unsupported_numbers(source, gen).is_empty());
    }

    #[test]
    fn 原文にない数値を検知する() {
        let source = "月給250,000円";
        let gen = "月給250,000円、賞与年3回あり";
        // "3" は原文に無い。
        let un = find_unsupported_numbers(source, gen);
        assert!(un.contains(&"3".to_string()), "got {un:?}");
    }

    #[test]
    fn 数値単位ペア捏造を検知する_60代原文40代生成() {
        // 原文には「60代」しかないのに生成文が「40代」と書く捏造。
        // ただし「40」という数字自体は原文の別箇所(240,860円→240860)には現れないので
        // 段①でも段②でも検知される。ここでは段②の「40代」検知を確認。
        let source = "対象は60代の方が活躍中。月給240,860円";
        let gen = "40代の方が活躍中です";
        let un = find_unsupported_numbers(source, gen);
        assert!(un.contains(&"40代".to_string()), "got {un:?}");
    }

    #[test]
    fn 段2のみが捕まえる単位付替え() {
        // 「40」という数字自体は原文(40時間)に存在するが、「40代」という組は無い。
        // 段①は通り、段②の「40代」だけが検知される。
        let source = "残業は月40時間程度";
        let gen = "40代活躍中、残業40時間程度";
        let un = find_unsupported_numbers(source, gen);
        assert!(
            !un.contains(&"40".to_string()),
            "40 は原文にあるので段①では出ないはず: {un:?}"
        );
        assert!(un.contains(&"40代".to_string()), "got {un:?}");
        assert!(!un.contains(&"40時間".to_string()), "40時間は原文にある: {un:?}");
    }

    #[test]
    fn 全角数字は正規化して照合する() {
        let source = "月給２５万円";
        let gen = "月給25万円スタート";
        // 全角/半角の差は正規化で吸収され、25 と 250000(万円展開)が一致する。
        assert!(find_unsupported_numbers(source, gen).is_empty());
    }

    #[test]
    fn 万円展開は段1の数字照合を吸収する() {
        // 原文「25万円」→ numbers に 25 と 250000(万円展開)。生成が同じ「25万円」表記なら
        // 段①(数字)・段②(数値+単位)とも一致して合格。
        let source = "月給25万円";
        let gen = "初任給25万円からスタート";
        assert!(find_unsupported_numbers(source, gen).is_empty());
    }

    #[test]
    fn 段2は単位表記の付替えに厳格_万円を円に開くと検知() {
        // Python `find_unsupported_number_units` に忠実: 原文「25万円」に対し生成「250,000円」は
        // 数字(250000)は段①で吸収されるが、"250000円" というペアは原文に無いため段②で検知される。
        // 生成プロンプトが「原文の数値表記をそのまま使う」を課すのはこの厳格さのため。
        let source = "月給25万円";
        let gen = "月給250,000円からスタート";
        let un = find_unsupported_numbers(source, gen);
        assert!(!un.contains(&"250000".to_string()), "数字自体は段①で吸収: {un:?}");
        assert!(un.contains(&"250000円".to_string()), "ペアは段②で検知: {un:?}");
    }

    #[test]
    fn 数値を含まない生成文は常に合格() {
        let source = "介護のお仕事です";
        let gen = "未経験歓迎、アットホームな職場です";
        assert!(find_unsupported_numbers(source, gen).is_empty());
    }
}
