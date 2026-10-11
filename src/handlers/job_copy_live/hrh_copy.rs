//! Same ordered, unescaped CSV body reader as frontend/hrhCopy.ts.
use std::{collections::HashMap, sync::LazyLock};
static COLUMNS: LazyLock<Vec<String>> = LazyLock::new(|| {
    serde_json::from_str(include_str!(
        "../../../frontend/src/screens/job-copy/hrhCopyColumns.json"
    ))
    .expect("checked-in HRハッカー column list must be valid JSON")
});
struct Candidate {
    line: usize,
    rank: usize,
    start: usize,
    length: usize,
    next: Option<usize>,
}
pub(super) fn fields(body: &str) -> HashMap<&str, String> {
    let columns: Vec<&str> = COLUMNS.iter().map(String::as_str).collect();
    ordered_fields(body, &columns)
        .into_iter()
        .map(|(key, value)| (key, value.trim().to_owned()))
        .collect()
}
pub(super) fn fields84(body: &str) -> HashMap<&str, String> {
    ordered_fields(body, &crate::job_gen::hrhacker::HRHACKER_COLUMNS)
}
fn ordered_fields<'a>(body: &str, columns: &[&'a str]) -> HashMap<&'a str, String> {
    let mut offset = 0;
    let lines: Vec<_> = body
        .split_inclusive('\n')
        .map(|text| {
            let start = offset;
            offset += text.len();
            (
                start,
                text.trim_end_matches('\n')
                    .strip_suffix('\r')
                    .unwrap_or(text.trim_end_matches('\n')),
            )
        })
        .collect();
    let mut candidates: Vec<_> = lines
        .iter()
        .enumerate()
        .filter_map(|(line, &(offset, text))| {
            let (name, _) = text.split_once(['：', ':'])?;
            let separator = text[name.len()..].chars().next()?;
            let rank = columns.iter().position(|column| *column == name.trim())?;
            Some(Candidate {
                line,
                rank,
                start: offset + name.len() + separator.len_utf8(),
                length: 1,
                next: None,
            })
        })
        .collect();
    let mut best: Vec<Option<usize>> = vec![None; columns.len()];
    for i in (0..candidates.len()).rev() {
        let mut next = None;
        let mut length = 1;
        for &following in &best[candidates[i].rank + 1..] {
            if let Some(j) = following {
                if candidates[j].length + 1 > length {
                    length = candidates[j].length + 1;
                    next = Some(j);
                }
            }
        }
        candidates[i].length = length;
        candidates[i].next = next;
        let rank = candidates[i].rank;
        if best[rank].is_none_or(|j| length > candidates[j].length) {
            best[rank] = Some(i);
        }
    }
    let mut fields = HashMap::new();
    let mut index = (!candidates.is_empty()).then_some(0);
    while let Some(i) = index {
        let candidate = &candidates[i];
        let mut end = candidate
            .next
            .map_or(body.len(), |j| lines[candidates[j].line].0);
        // 項目間の区切り一つだけを除く。値内の空白・改行コードは保持する。
        if candidate.next.is_some() && body.as_bytes().get(end.wrapping_sub(1)) == Some(&b'\n') {
            end -= 1;
            if body.as_bytes().get(end.wrapping_sub(1)) == Some(&b'\r') {
                end -= 1;
            }
        }
        fields.insert(
            columns[candidate.rank],
            body[candidate.start..end].to_owned(),
        );
        index = candidate.next;
    }
    fields
}
#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    fn raw_fields_distinguish_crlf_boundaries_and_value_whitespace() {
        let parsed = fields84("勤務時間： \t9〜18時　\r\n休憩あり \r\n勤務時間帯： 日勤　\r\n");
        assert_eq!(parsed["勤務時間"], " \t9〜18時　\r\n休憩あり ");
        assert_eq!(parsed["勤務時間帯"], " 日勤　\r\n");
    }
    #[test]
    fn job_copy_ordered_fields_preserve_embedded_column_lines() {
        let parsed = fields("仕事内容：\nご案内\n応募資格：未経験可\n補足\n応募資格：普通免許");
        assert_eq!(parsed["仕事内容"], "ご案内\n応募資格：未経験可\n補足");
        assert_eq!(parsed["応募資格"], "普通免許");
    }
}
