//! 録音 URL の取り出しと、Zoom の文字起こしの話者分け。

use serde_json::Value;

use super::types::TalkTurn;

/// `hs_call_body` にある `.../audio/play/<数字>/<YYYYMMDD-uuid>` から Zoom の call log id を取る
/// (試作の `REC_RE = /audio/play/\d+/(\d{8}-[0-9a-f-]{36})`)。
pub fn extract_call_log_id(body: &str) -> Option<String> {
    const KEY: &str = "/audio/play/";
    let mut rest = body;
    while let Some(i) = rest.find(KEY) {
        let after = &rest[i + KEY.len()..];
        let digits = after.bytes().take_while(u8::is_ascii_digit).count();
        if digits > 0 && after.as_bytes().get(digits) == Some(&b'/') {
            let id: &str = &after[digits + 1..];
            let b = id.as_bytes();
            if b.len() >= 45
                && b[..8].iter().all(u8::is_ascii_digit)
                && b[8] == b'-'
                && b[9..45]
                    .iter()
                    .all(|c| c.is_ascii_digit() || (b'a'..=b'f').contains(c) || *c == b'-')
            {
                return Some(id[..45].to_string());
            }
        }
        rest = after;
    }
    None
}

/// 営業か (試作の `is_rep`)。名前が空・電話番号・`Unknown Speaker` は先方
pub fn is_rep(username: &str) -> bool {
    let u = username.trim();
    if u.is_empty() || u == "Unknown Speaker" {
        return false;
    }
    !u.chars().all(|c| {
        c.is_ascii_digit() || matches!(c, '+' | '-' | ' ' | '(' | ')') || c.is_whitespace()
    })
}

/// Zoom の `timeline` を発言にする。連続する同じ話者はまとめ、1 から番号を振る
pub fn turns_from(tx: &Value) -> Vec<TalkTurn> {
    let mut out: Vec<TalkTurn> = Vec::new();
    let Some(tl) = tx.get("timeline").and_then(Value::as_array) else {
        return out;
    };
    for t in tl {
        let user = t
            .pointer("/users/0/username")
            .and_then(Value::as_str)
            .or_else(|| t.get("username").and_then(Value::as_str))
            .unwrap_or("");
        let text = t.get("text").and_then(Value::as_str).unwrap_or("").trim();
        if text.is_empty() {
            continue;
        }
        let speaker = if is_rep(user) { "rep" } else { "customer" };
        match out.last_mut() {
            Some(last) if last.speaker == speaker => last.text.push_str(text),
            _ => out.push(TalkTurn {
                n: 0,
                speaker: speaker.to_string(),
                text: text.to_string(),
            }),
        }
    }
    for (i, t) in out.iter_mut().enumerate() {
        t.n = i as u32 + 1;
    }
    out
}

/// 営業と先方の両方が話している (どちらかが欠けると判定できない)
pub fn has_both_sides(turns: &[TalkTurn]) -> bool {
    turns.iter().any(|t| t.speaker == "rep") && turns.iter().any(|t| t.speaker == "customer")
}

#[cfg(test)]
mod tests {
    use super::*;
    use serde_json::json;

    const ID: &str = "20261015-0a1b2c3d-4e5f-4a6b-8c7d-9e0f1a2b3c4d";

    #[test]
    fn extracts_call_log_id_from_body() {
        let body = format!(
            "Call from X\nRecording Url: https://applications.zoom.us/integration/phone/hubspotphone/audio/play/0/{ID}\nend"
        );
        assert_eq!(extract_call_log_id(&body).as_deref(), Some(ID));
    }

    #[test]
    fn no_id_when_missing_or_malformed() {
        assert_eq!(extract_call_log_id(""), None);
        assert_eq!(extract_call_log_id("/audio/play/0/short"), None);
        assert_eq!(
            extract_call_log_id("/audio/play//20261015-0a1b2c3d-4e5f-4a6b-8c7d-9e0f1a2b3c4d"),
            None
        );
        // 大文字の uuid は対象外 (試作と同じ)
        assert_eq!(
            extract_call_log_id("/audio/play/0/20261015-0A1B2C3D-4E5F-4A6B-8C7D-9E0F1A2B3C4D"),
            None
        );
    }

    #[test]
    fn rep_detection() {
        assert!(is_rep("田中 太郎"));
        assert!(!is_rep("+81 3-1234-5678"));
        assert!(!is_rep("0312345678"));
        assert!(!is_rep("Unknown Speaker"));
        assert!(!is_rep(""));
    }

    #[test]
    fn turns_merge_consecutive_speakers_and_number() {
        let tx = json!({"timeline": [
            {"users": [{"username": "田中 太郎"}], "text": "お世話になります。"},
            {"users": [{"username": "田中 太郎"}], "text": "採用のご担当者様はいらっしゃいますか。"},
            {"users": [{"username": "0312345678"}], "text": "少々お待ちください。"},
            {"users": [{"username": "Unknown Speaker"}], "text": "  "},
            {"users": [{"username": "Unknown Speaker"}], "text": "はい。"},
            {"username": "田中 太郎", "text": "来週はいかがですか。"}
        ]});
        let t = turns_from(&tx);
        assert_eq!(t.len(), 3);
        assert_eq!((t[0].n, t[0].speaker.as_str()), (1, "rep"));
        assert_eq!(
            t[0].text,
            "お世話になります。採用のご担当者様はいらっしゃいますか。"
        );
        assert_eq!((t[1].n, t[1].speaker.as_str()), (2, "customer"));
        assert_eq!(t[1].text, "少々お待ちください。はい。");
        assert_eq!((t[2].n, t[2].speaker.as_str()), (3, "rep"));
        assert!(has_both_sides(&t));
        assert!(!has_both_sides(&t[..1]));
    }
}
