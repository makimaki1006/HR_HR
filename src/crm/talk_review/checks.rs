//! 判定項目の定義。`checks.json` が唯一の定義 (試作 `scripts/call_advice/call_advice.py` も同じファイルを読む)。

use std::sync::OnceLock;

use serde::Deserialize;
use sha2::{Digest, Sha256};

use super::types::TalkCheckDef;

const RAW: &str = include_str!("checks.json");

#[derive(Debug, Clone, Deserialize)]
pub struct Baseline {
    pub pct: f64,
    pub basis: String,
}

#[derive(Debug, Clone, Deserialize)]
pub struct Check {
    pub id: String,
    pub stage: String,
    pub title: String,
    pub rule: String,
    pub na_when: String,
    pub baseline: Option<Baseline>,
}

#[derive(Debug, Clone, Deserialize)]
pub struct Checks {
    pub min_talk_sec: u32,
    pub checks: Vec<Check>,
}

pub struct Loaded {
    pub checks: Checks,
    /// 内容のハッシュ (先頭 8 文字)
    pub version: String,
}

pub fn version_of(raw: &str) -> String {
    let d = Sha256::digest(raw.as_bytes());
    format!("{d:x}").chars().take(8).collect()
}

pub fn loaded() -> &'static Loaded {
    static L: OnceLock<Loaded> = OnceLock::new();
    L.get_or_init(|| Loaded {
        checks: serde_json::from_str(RAW).expect("checks.json が読めない"),
        version: version_of(RAW),
    })
}

pub fn defs() -> Vec<TalkCheckDef> {
    loaded()
        .checks
        .checks
        .iter()
        .map(|c| TalkCheckDef {
            id: c.id.clone(),
            stage: c.stage.clone(),
            title: c.title.clone(),
            rule: c.rule.clone(),
            baseline_pct: c.baseline.as_ref().map(|b| b.pct),
            baseline_basis: c.baseline.as_ref().map(|b| b.basis.clone()),
        })
        .collect()
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn checks_json_is_the_single_definition() {
        let l = loaded();
        assert_eq!(l.checks.min_talk_sec, 70);
        assert_eq!(l.checks.checks.len(), 9);
        assert_eq!(l.checks.checks[0].id, "reception_name");
        let d = defs();
        assert_eq!(d[2].id, "early_date");
        assert_eq!(d[2].baseline_pct, Some(81.0));
        assert_eq!(d[0].baseline_pct, None);
        assert_eq!(l.version.len(), 8);
    }

    #[test]
    fn version_changes_with_content() {
        assert_ne!(version_of("a"), version_of("b"));
        assert_eq!(version_of("a"), version_of("a"));
    }
}
