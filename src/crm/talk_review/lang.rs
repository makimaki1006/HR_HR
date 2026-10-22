//! 判定結果の説明文・助言文が日本語だけで書かれているかの検査 (ADR-033 追記)。
//! MiniMax が中国語・韓国語・英単語を混ぜることがあるため、混ざった文章は画面に出さない。
//! 文字起こしからの引用 (quote) は原文なので検査しない。引用や文字起こしにある英単語は助言にあっても許す。

/// 画面に出せる文章を作れなかったときの表示
pub const FALLBACK_TEXT: &str = "判定の文章を作れませんでした";

/// 英字のままでよい語 (小文字で比べる)
const ALLOWED_WORDS: &[&str] = &[
    "indeed", "hubspot", "zoom", "airwork", "pdf", "csv", "url", "gmail", "google", "excel",
    "line", "sms", "crm", "kpi", "email", "mail", "web",
];

/// 簡体字にしかない字 (日本語の漢字としては使わないもの)
const SIMPLIFIED_ONLY: &str = "这个们为说对时过还吗让给开发见现观关从么样机间问题应该别实种经办总须户买无协话语请认识谢订记讨论设计达进运动车软单业务员产价优势复习质评钱银长门阳际标线级约热术议导众战听觉读";

fn is_hangul(c: char) -> bool {
    matches!(c as u32, 0x1100..=0x11FF | 0x3130..=0x318F | 0xA960..=0xA97F | 0xAC00..=0xD7FF)
}

/// 文章に日本語以外が混ざっていればその一部を返す。`source` は引用・文字起こしの文字 (英単語の許可に使う)
pub fn find_foreign(text: &str, source: &str) -> Option<String> {
    let source = source.to_lowercase();
    if let Some(c) = text
        .chars()
        .find(|c| is_hangul(*c) || SIMPLIFIED_ONLY.contains(*c))
    {
        return Some(c.to_string());
    }
    let mut word = String::new();
    for c in text.chars().chain(std::iter::once(' ')) {
        if c.is_ascii_alphabetic() {
            word.push(c);
            continue;
        }
        if word.chars().count() >= 3 {
            let lower = word.to_lowercase();
            if !ALLOWED_WORDS.contains(&lower.as_str()) && !source.contains(&lower) {
                return Some(word);
            }
        }
        word.clear();
    }
    None
}

pub fn prose_ok(text: &str, source: &str) -> bool {
    find_foreign(text, source).is_none()
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn plain_japanese_passes() {
        assert!(prose_ok("先に用件を一言伝えてから日程を聞く", ""));
        assert!(prose_ok("", ""));
        // 日本語でも使う漢字は誤検出しない
        assert!(prose_ok("会って話す時間を点検し、関係を見直す", ""));
    }

    #[test]
    fn rejects_chinese_korean_and_stray_english() {
        assert!(!prose_ok("这个问题需要先确认", ""));
        assert!(!prose_ok("먼저 용건을 말한다", ""));
        assert!(!prose_ok("日程を operatorsconciled する", ""));
        assert_eq!(
            find_foreign("日程を operatorsconciled する", "").as_deref(),
            Some("operatorsconciled")
        );
    }

    #[test]
    fn allowlist_and_short_letters_pass() {
        assert!(prose_ok(
            "Indeed の求人と HubSpot の件。Zoom で OK、PDF も AirWork も",
            ""
        ));
        assert!(prose_ok("ID と HR と AI の話", ""));
    }

    #[test]
    fn words_in_the_transcript_or_quote_are_allowed() {
        assert!(!prose_ok("Kintone の話を先に出す", ""));
        assert!(prose_ok(
            "Kintone の話を先に出す",
            "うちは kintone を使っていて"
        ));
    }
}
