// UTF-8 safe な文字列操作 helper。
// 2026-05-13: HTML debug snippet 用に `&s[..N]` で byte 切りすると
// 日本語の途中で panic する事故が複数発生したため、共通化。

/// 最大 `max_bytes` byte までを切り出すが、UTF-8 char boundary を割らないよう
/// 直前の boundary まで縮める。`max_bytes >= s.len()` の場合は `s` をそのまま返す。
pub fn truncate_char_safe(s: &str, max_bytes: usize) -> &str {
    if s.len() <= max_bytes {
        return s;
    }
    let mut boundary = max_bytes;
    while boundary > 0 && !s.is_char_boundary(boundary) {
        boundary -= 1;
    }
    &s[..boundary]
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn ascii_within_limit_returns_whole() {
        assert_eq!(truncate_char_safe("hello", 100), "hello");
    }

    #[test]
    fn ascii_over_limit_truncates_exact() {
        assert_eq!(truncate_char_safe("abcdef", 3), "abc");
    }

    #[test]
    fn multibyte_truncate_does_not_split_char() {
        // 「あ」= 3 bytes。max_bytes=4 だと「あ」(3 bytes) + 1 byte が境界外。
        // 期待: 「あ」のみ返す (boundary=3)。
        let s = "あいう";
        let out = truncate_char_safe(s, 4);
        assert_eq!(out, "あ");
        assert!(out.len() <= 4);
    }

    #[test]
    fn multibyte_truncate_at_zero_byte() {
        let s = "あ";
        let out = truncate_char_safe(s, 2);
        // 「あ」は 3 bytes、2 では収まらず boundary=0
        assert_eq!(out, "");
    }

    #[test]
    fn multibyte_exact_boundary() {
        let s = "あい";
        assert_eq!(truncate_char_safe(s, 3), "あ");
        assert_eq!(truncate_char_safe(s, 6), "あい");
    }
}
