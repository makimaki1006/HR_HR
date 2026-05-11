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
                    found.push(format!("{}:{}: {}", path.display(), i + 1, line.trim()));
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

/// MarketIntelligence 印刷 / PDF P1 専用ガード (Worker F 追加 2026-05-06)
///
/// 印刷向けブロック (`mi-print-summary` / `mi-print-annotations` / `mi-print-only`)
/// を含むファイルにおいて、印刷文脈で人数換算 / 推定母集団系の Hard NG 用語が
/// 混入していないことを保証する。
///
/// 設計参照:
/// - `docs/MARKET_INTELLIGENCE_PRINT_PDF_P1_SPEC.md` §7 (Hard NG 維持)
/// - `docs/MARKET_INTELLIGENCE_UI_P1_P2_BACKLOG.md` 共通完了基準
///
/// 既存の `no_forbidden_identifiers_in_src` / `no_forbidden_ja_phrases_in_codebase`
/// はファイル全体を検査するが、本テストは印刷ブロック近傍 (前後 8 行) に絞った
/// 重複ガードであり、印刷向け新規実装で NG 用語が紛れ込む事故を早期検知する。
#[test]
fn no_forbidden_terms_near_mi_print_blocks() {
    const PRINT_MARKERS: &[&str] = &[
        "mi-print-summary",
        "mi-print-annotations",
        "mi-print-only",
        "render_mi_print", // 将来追加されうる関数名 prefix
    ];
    const NG_NEAR_PRINT: &[&str] = &[
        "推定人数",
        "想定人数",
        "母集団人数",
        "target_count",
        "estimated_population",
        "estimated_worker_count",
        "resident_population_estimate",
        "convert_index_to_population",
        "人見込み",
    ];

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
        let lines: Vec<&str> = content.lines().collect();
        // 印刷ブロックを含む行の周辺 (前後 8 行) を検査範囲とする
        let mut print_line_idx: Vec<usize> = Vec::new();
        for (i, line) in lines.iter().enumerate() {
            if PRINT_MARKERS.iter().any(|m| line.contains(m)) {
                print_line_idx.push(i);
            }
        }
        if print_line_idx.is_empty() {
            continue;
        }
        for &center in &print_line_idx {
            let start = center.saturating_sub(8);
            let end = (center + 8).min(lines.len().saturating_sub(1));
            for i in start..=end {
                let line = lines[i];
                let trimmed = line.trim_start();
                let is_negation_assert = trimmed.starts_with("assert!(!")
                    || trimmed.contains("!html.contains")
                    || trimmed.contains("forbidden")
                    || trimmed.contains("Hard NG")
                    || trimmed.contains("NG_NEAR_PRINT")
                    || trimmed.contains("FORBIDDEN_");
                if is_negation_assert {
                    continue;
                }
                for term in NG_NEAR_PRINT {
                    if line.contains(term) {
                        found.push(format!(
                            "{}:{}: {} (印刷ブロック近傍)",
                            path.display(),
                            i + 1,
                            term
                        ));
                    }
                }
            }
        }
    }
    assert!(
        found.is_empty(),
        "印刷ブロック近傍の Hard NG 用語 ({} 件):\n{}",
        found.len(),
        found.join("\n")
    );
}

/// P0 (2026-05-06): Print/PDF P1 客観レビュー C 判定対応。
///
/// 内部 fallback 文言 (「データ不足」「要件再確認」「データ準備中」「未集計」
/// 「参考表示なし」「本条件では表示対象がありません」「Sample」) が、
/// 採用マーケットインテリジェンスの表示 HTML 出力ファイル
/// (`src/handlers/survey/report_html/market_intelligence.rs`) に
/// 出力文字列リテラルとして残っていないことを保証する。
///
/// スコープ:
/// - 対象: `src/handlers/survey/report_html/market_intelligence.rs`
/// - 除外: docs/、tests/、その他 src (テキスト引用やドメイン語彙のため)
/// - 許容: コメント (`//` 以降)、`assert!(!...)` の否定検証、
///   FORBIDDEN_*/NG_* 定数定義行、test 関数本体
///
/// 設計参照:
/// - 客観レビュー指摘: PDF page 16 結論ブロック / ヒーロー第 2 枠
/// - `docs/MARKET_INTELLIGENCE_PRINT_PDF_P1_SPEC.md` §7
#[test]
fn no_internal_fallback_terms_in_market_intelligence_render() {
    const INTERNAL_FALLBACK_NG: &[&str] = &[
        "データ不足",
        "要件再確認",
        "データ準備中",
        "未集計",
        "参考表示なし",
        "本条件では表示対象がありません",
        "Sample",
    ];

    // スコープ限定: market_intelligence.rs のみ
    let target = Path::new("src/handlers/survey/report_html/market_intelligence.rs");
    assert!(
        target.exists(),
        "対象ファイルが見つからない: {}",
        target.display()
    );

    let content = fs::read_to_string(target).expect("market_intelligence.rs 読み込み失敗");

    // `#[cfg(test)]` モジュールに入った時点以降の行はテストコードとして除外。
    // (テスト本体で NG 用語を assert 検証する目的で参照する正当な用法のため)
    let cfg_test_line: Option<usize> = content
        .lines()
        .position(|l| l.trim_start().starts_with("#[cfg(test)]"));

    let mut found = Vec::new();
    for (i, line) in content.lines().enumerate() {
        // テストモジュール内は対象外
        if let Some(cfg_idx) = cfg_test_line {
            if i >= cfg_idx {
                break;
            }
        }
        let trimmed = line.trim_start();

        // コメント行は許可 (実装意図の説明として用語を引用するため)
        if trimmed.starts_with("//") || trimmed.starts_with("/*") || trimmed.starts_with("*") {
            continue;
        }
        // 否定 assertion / 禁止用語の列挙定数 / テスト用変数は許可
        let is_negation_assert = trimmed.starts_with("assert!(!")
            || trimmed.contains("!html.contains")
            || trimmed.contains("forbidden")
            || trimmed.contains("Hard NG")
            || trimmed.contains("INTERNAL_FALLBACK_NG")
            || trimmed.contains("FORBIDDEN_")
            || trimmed.contains("NG_NEAR_PRINT");
        if is_negation_assert {
            continue;
        }
        // 「実測値準備中」は workplace measured fallback の固有ラベルで NG ではない
        // (タスク NG リストには含まれないドメイン語彙)
        // 「サンプル件数」(統計の n=サンプル件数) は統計用語で NG ではない
        let line_for_check = line
            .replace("実測値準備中", "")
            .replace("サンプル件数", "")
            .replace("業界サンプル件数", "");

        for term in INTERNAL_FALLBACK_NG {
            if line_for_check.contains(term) {
                found.push(format!(
                    "{}:{}: '{}' が表示 HTML 出力に混入: {}",
                    target.display(),
                    i + 1,
                    term,
                    line.trim()
                ));
            }
        }
    }

    assert!(
        found.is_empty(),
        "内部 fallback 文言が market_intelligence.rs の出力に混入 ({} 件):\n{}",
        found.len(),
        found.join("\n")
    );
}
