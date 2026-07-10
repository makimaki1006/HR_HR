//! 静的監査テスト (V2ルール: 介護データ・HW由来データを consult に混入させない)
//!
//! consult/ 配下の全ソースを走査し、HW求人・時系列・介護需要 *データ* の識別子が
//! 参照されていないことを保証する。
//!
//! 注意: `hw_db` は hellowork.db への SQLite 接続ハンドルであり、consult ではそこから
//! 公的統計 (v2_external_*) と国勢調査OD のみを読む。HW求人 (postings) や時系列
//! (ts_counts 等)・介護需要 (care_demand) の *データ* は一切読まない。本テストは後者を検出する。
//! そのため接続ハンドル名 `hw_db` は禁止トークンに含めない (データ利用ではない)。

/// HW・介護 *データ* の利用を示す識別子。これらが consult ソースに現れたら失敗。
const FORBIDDEN_DATA_TOKENS: [&str; 12] = [
    "care_demand",
    "fetch_ts_",
    "cross_care",
    "hw_industry_counts",
    "hw_job_type_counts",
    "ts_counts",
    "ts_vacancy",
    "ts_salary",
    "ts_fulfillment",
    "ts_tracking",
    "FROM postings",
    "fetch_care_demand",
];

#[test]
fn consult_sources_do_not_reference_hw_or_care_data() {
    let dir = std::path::Path::new(env!("CARGO_MANIFEST_DIR")).join("src/handlers/consult");
    let mut violations: Vec<String> = Vec::new();
    let mut scanned = 0usize;
    for entry in std::fs::read_dir(&dir).expect("consult ディレクトリを読める") {
        let path = entry.unwrap().path();
        if path.extension().and_then(|e| e.to_str()) != Some("rs") {
            continue;
        }
        let name = path.file_name().and_then(|n| n.to_str()).unwrap_or("");
        // このテスト自身は禁止トークンの一覧を保持するため除外
        if name == "hw_audit_test.rs" {
            continue;
        }
        let src = std::fs::read_to_string(&path).unwrap();
        scanned += 1;
        for tok in FORBIDDEN_DATA_TOKENS {
            if src.contains(tok) {
                violations.push(format!("{}: 禁止トークン \"{}\"", name, tok));
            }
        }
    }
    assert!(
        scanned >= 10,
        "consult のソースを走査できている (scanned={})",
        scanned
    );
    assert!(
        violations.is_empty(),
        "consult に HW/介護データの参照が混入している: {:?}",
        violations
    );
}
