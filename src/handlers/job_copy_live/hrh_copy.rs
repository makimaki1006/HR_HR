//! Same ordered, unescaped CSV body reader as frontend/hrhCopy.ts.
use std::{collections::HashMap, sync::LazyLock};
static COLUMNS: LazyLock<Vec<String>> = LazyLock::new(|| {
    serde_json::from_str(include_str!(
        "../../../frontend/src/screens/job-copy/hrhCopyColumns.json"
    ))
    .expect("checked-in HRハッカー column list must be valid JSON")
});
struct Candidate<'a> {
    line: usize,
    rank: usize,
    value: &'a str,
    length: usize,
    next: Option<usize>,
}
pub(super) fn fields(body: &str) -> HashMap<&str, String> {
    let lines: Vec<_> = body.lines().collect();
    let mut candidates: Vec<_> = lines
        .iter()
        .enumerate()
        .filter_map(|(line, text)| {
            let (name, value) = text.split_once(['：', ':'])?;
            let rank = COLUMNS.iter().position(|column| column == name.trim())?;
            Some(Candidate {
                line,
                rank,
                value,
                length: 1,
                next: None,
            })
        })
        .collect();
    let mut best: Vec<Option<usize>> = vec![None; COLUMNS.len()];
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
        let end = candidate.next.map_or(lines.len(), |j| candidates[j].line);
        let mut value = vec![candidate.value];
        value.extend_from_slice(&lines[candidate.line + 1..end]);
        fields.insert(
            COLUMNS[candidate.rank].as_str(),
            value.join("\n").trim().to_owned(),
        );
        index = candidate.next;
    }
    fields
}
#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    fn job_copy_ordered_fields_preserve_embedded_column_lines() {
        let parsed = fields("仕事内容：\nご案内\n応募資格：未経験可\n補足\n応募資格：普通免許");
        assert_eq!(parsed["仕事内容"], "ご案内\n応募資格：未経験可\n補足");
        assert_eq!(parsed["応募資格"], "普通免許");
    }
}
