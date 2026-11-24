//! 求人チェックの判定を、画面を通さずに求人ごとに当てる (2026-11-25)。
//!
//! 入力 (標準入力、1 行 1 件の JSON): `{"id": "...", "prefecture": "東京都", "body": "列名：値..."}`。
//! `body` は HubSpot の `hrh_kyuujinhyou_honbun` と同じ形。`prefecture` は `todoufuken`。
//! 出力 (標準出力、1 行 1 件の指摘): `{"id", "key", "category", "severity", "detail"}`。
//! 判定は本番の `/app/listing-check` と同じ関数 (`listing_salary_check::check` と
//! `listing_text_check::check`) をそのまま呼ぶ。直したかどうかを後から確かめる巡回が、
//! 指摘を出したときと同じ決まりで判定し直すために使う。HubSpot も外部も呼ばない。
use std::io::{self, BufRead, Write};

use rust_dashboard::{listing_salary_check, listing_text_check};

fn main() {
    let stdin = io::stdin();
    let stdout = io::stdout();
    let mut out = stdout.lock();
    for (n, line) in stdin.lock().lines().enumerate() {
        let line = match line {
            Ok(l) => l,
            Err(e) => {
                eprintln!("{} 行目を読めません: {e}", n + 1);
                std::process::exit(2);
            }
        };
        if line.trim().is_empty() {
            continue;
        }
        let v: serde_json::Value = match serde_json::from_str(&line) {
            Ok(v) => v,
            Err(e) => {
                eprintln!("{} 行目が JSON ではありません: {e}", n + 1);
                std::process::exit(2);
            }
        };
        let id = v.get("id").and_then(|x| x.as_str()).unwrap_or("");
        let pref = v
            .get("prefecture")
            .and_then(|x| x.as_str())
            .filter(|p| !p.is_empty());
        let body = v.get("body").and_then(|x| x.as_str()).unwrap_or("");
        let mut found = listing_salary_check::check(pref, body);
        found.extend(listing_text_check::check(body));
        for f in found {
            let row = serde_json::json!({
                "id": id,
                "key": f.category.key(),
                "category": f.category.label(),
                "severity": f.category.severity().key(),
                "detail": f.detail,
            });
            writeln!(out, "{row}").expect("標準出力に書けません");
        }
    }
}
