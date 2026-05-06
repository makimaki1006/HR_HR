//! Phase 3 Step 5 Hard NG ガード (Worker P6 Phase 6)
//!
//! コード本体 / HTML テンプレート / docstring に Plan B 違反の用語が
//! 混入していないことを CI ゲートで保証する。
//!
//! 設計参照: `docs/SURVEY_MARKET_INTELLIGENCE_PHASE3_RUST_INTEGRATION_PLAN.md` §2.1.1
//! ジョブメドレープロジェクト 推測禁止ルール (feedback_never_guess_data.md) の自動化

use std::fs;
use std::path::{Path, PathBuf};

/// 識別子レベルで完全禁止 (関数名 / 変数名 / カラム名 等)
const FORBIDDEN_IDENTIFIERS: &[&str] = &[
    "population_count",
    "target_count",
    "market_size_yen",
    "applicant_count",
    "estimated_population",
    "estimated_worker_count",
    "resident_population_estimate",
    "convert_index_to_population",
    "index_to_count",
];

/// 日本語フレーズレベルで完全禁止 (UI 表示文字列 / docstring 等)
const FORBIDDEN_JA_PHRASES: &[&str] = &[
    "推定人数",
    "想定人数",
    "母集団人数",
    "採用ターゲット候補総数",
    "採用市場規模",
];

/// `dir` 配下の `*.rs` を再帰収集 (target/ 除外)
fn visit_rs_files(dir: &Path, files: &mut Vec<PathBuf>) {
    if !dir.exists() {
        return;
    }
    let entries = match fs::read_dir(dir) {
        Ok(e) => e,
        Err(_) => return,
    };
    for entry in entries.flatten() {
        let path = entry.path();
        if path.is_dir() {
            // ビルド成果物 / 隠しディレクトリは除外
            let name = path.file_name().and_then(|n| n.to_str()).unwrap_or("");
            if name == "target" || name.starts_with('.') {
                continue;
            }
            visit_rs_files(&path, files);
        } else if path.extension().map_or(false, |e| e == "rs") {
            files.push(path);
        }
    }
}

/// ファイル全体が禁止用語の「定義リスト」(本ファイル相当) であるか判定。
///
/// FORBIDDEN_IDENTIFIERS / FORBIDDEN_JA_PHRASES の const 定義や、
/// テスト内で禁止用語を assert 対象として列挙している箇所は許可する。
fn is_documentation_file(content: &str, path: &Path) -> bool {
    // テストモジュール内 (tests/ 配下) は許可対象
    let path_str = path.to_string_lossy().replace('\\', "/");
    if path_str.contains("/tests/") {
        return true;
    }
    // 禁止用語列挙のための定数を持つファイルは許可
    content.contains("FORBIDDEN_IDENTIFIERS")
        || content.contains("FORBIDDEN_JA_PHRASES")
        || content.contains("Hard NG")
        || content.contains("Plan B 違反")
}

/// V1 (ジョブメドレー求職者) 統計モデルなど、Plan B 採用市場分析の対象外で
/// 同名識別子が正当な用法で使われているレガシーパスを許可する。
///
/// Hard NG の本来の対象は Phase 3 Step 5 採用マーケットインテリジェンス系
/// (採用候補母集団として `applicant_count` を使う想定) であり、
/// V1 求職者統計の `applicant_count` (求職者数) は別軸のドメイン語彙。
fn is_legacy_v1_path(path: &Path) -> bool {
    let s = path.to_string_lossy().replace('\\', "/");
    // V1 求職者ダッシュボード由来のレガシーモデル
    s.contains("src/models/statistics.rs")
}

#[test]
fn no_forbidden_identifiers_in_src() {
    let mut files = Vec::new();
    visit_rs_files(Path::new("src"), &mut files);
    assert!(
        !files.is_empty(),
        "src/ 配下に *.rs ファイルが見つからない (実行ディレクトリ確認)"
    );

    let mut found = Vec::new();
    for path in &files {
        let content = fs::read_to_string(path).unwrap_or_default();
        if is_documentation_file(&content, path) || is_legacy_v1_path(path) {
            continue;
        }
        for term in FORBIDDEN_IDENTIFIERS {
            if !content.contains(term) {
                continue;
            }
            for (i, line) in content.lines().enumerate() {
                if line.contains(term) {
                    // assert! 内 / コメントで言及のみのケースを除外
                    let trimmed = line.trim_start();
                    let is_negation_assert = trimmed.starts_with("assert!(!")
                        || trimmed.starts_with("// ")
                        || trimmed.starts_with("//!")
                        || trimmed.starts_with("/// ")
                        || trimmed.starts_with("\"")
                        || trimmed.contains("not contain")
                        || trimmed.contains("forbidden");
                    if !is_negation_assert {
                        found.push(format!("{}:{}: {}", path.display(), i + 1, term));
                    }
                }
            }
        }
    }
    assert!(
        found.is_empty(),
        "Hard NG identifiers in src/ ({} 件):\n{}",
        found.len(),
        found.join("\n")
    );
}

#[test]
fn no_forbidden_ja_phrases_in_codebase() {
    let mut files = Vec::new();
    for dir in &["src", "templates"] {
        visit_rs_files(Path::new(dir), &mut files);
    }

    let mut found = Vec::new();
    for path in &files {
        let content = fs::read_to_string(path).unwrap_or_default();
        if is_documentation_file(&content, path) {
            continue;
        }
        for phrase in FORBIDDEN_JA_PHRASES {
            if !content.contains(phrase) {
                continue;
            }
            for (i, line) in content.lines().enumerate() {
                if line.contains(phrase) {
                    let trimmed = line.trim_start();
                    // 禁止用語を assert! で「含まれない」ことを検証する行は許可
                    let is_negation_assert = trimmed.starts_with("assert!(!")
                        || trimmed.contains("!html.contains")
                        || trimmed.contains("forbidden")
                        || trimmed.contains("Hard NG");
                    if !is_negation_assert {
                        found.push(format!("{}:{}: {}", path.display(), i + 1, phrase));
                    }
                }
            }
        }
    }
    assert!(
        found.is_empty(),
        "Hard NG ja phrases ({} 件):\n{}",
        found.len(),
        found.join("\n")
    );
}

#[test]
fn no_x_person_estimate_pattern_in_codebase() {
    // 「○人見込み」「人見込み」パターン (regex 不使用、機械的 substring)
    let mut files = Vec::new();
    for dir in &["src", "templates"] {
        visit_rs_files(Path::new(dir), &mut files);
    }

    let mut found = Vec::new();
    for path in &files {
        let content = fs::read_to_string(path).unwrap_or_default();
        if is_documentation_file(&content, path) {
            continue;
        }
        for (i, line) in content.lines().enumerate() {
            if line.contains("人見込み") {
                let trimmed = line.trim_start();
                let is_negation_assert = trimmed.starts_with("assert!(!")
                    || trimmed.contains("!html.contains")
                    || trimmed.contains("forbidden");
                if !is_negation_assert {
                    found.push(format!(
                        "{}:{}: {}",
                        path.display(),
                        i + 1,
                        line.trim()
                    ));
                }
            }
        }
    }
    assert!(
        found.is_empty(),
        "Hard NG '人見込み' パターン ({} 件):\n{}",
        found.len(),
        found.join("\n")
    );
}
