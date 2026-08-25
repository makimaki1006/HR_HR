//! env_example_matches_code.rs
//!
//! `.env.example` が実装とずれていないことを機械的に守る。
//!
//! ## なぜ要るか
//!
//! 2026-08-22 時点で `.env.example` は **8個しか書かれておらず、うち3個は
//! コードに存在しなかった**:
//!
//!     TURSO_DATABASE_URL   ← src/ に無い(Phase3 F2 の「これから作る」計画の名前)
//!     TURSO_AUTH_TOKEN     ← 同上
//!     LOCAL_DB_PATH        ← src/ に無い(2026-05-13 の旧監査ドキュメントにのみ登場)
//!
//! 実装が読んでいるのは 38 個。差が 30 個あった。
//!
//! この状態だと、新しい環境を作る人が `.env.example` を埋めても起動しない。
//! 実際、Mac への移行準備でここを追跡するのに時間がかかった。
//! **本当に必須なのは `GOOGLE_SA_KEY_B64` と `SPREADSHEET_ID`** で、
//! どちらもテンプレートに書かれていなかった。
//!
//! ## このテストがやること
//!
//! `src/**/*.rs` から `env::var("NAME")` を全部拾い、`.env.example` の中に
//! その名前が出てくるかを見る。宣言行(`NAME=`)でもコメント内の言及でもよい
//! (ホスティング側が入れる RENDER 系は、設定させるとかえって誤解を生むので
//!  コメントで説明する形にしてある)。
//!
//! 逆向きも見る: `.env.example` に `NAME=` と書いてあるのに実装が読んでいない
//! ものは、消し忘れか改名漏れなので落とす。
//!
//! ## 通し方
//!
//! 環境変数を増やしたら `.env.example` にも足す。それだけ。

use std::collections::BTreeSet;
use std::fs;
use std::path::{Path, PathBuf};

/// `env::var("NAME")` の NAME を集める。
fn collect_env_vars_in_src(dir: &Path, out: &mut BTreeSet<String>) {
    let Ok(entries) = fs::read_dir(dir) else {
        return;
    };
    for entry in entries.flatten() {
        let path = entry.path();
        if path.is_dir() {
            collect_env_vars_in_src(&path, out);
        } else if path.extension().and_then(|e| e.to_str()) == Some("rs") {
            let Ok(src) = fs::read_to_string(&path) else {
                continue;
            };
            for name in extract_env_var_names(&src) {
                out.insert(name);
            }
        }
    }
}

/// `env::var("NAME")` を素朴に抜き出す。
///
/// 正規表現クレートを増やしたくないので手で走査する。
/// `std::env::var("X")` / `env::var( "X" )` の両方を拾う。
fn extract_env_var_names(src: &str) -> Vec<String> {
    let mut found = Vec::new();
    let needle = "env::var(";
    let bytes = src.as_bytes();
    let mut from = 0usize;

    while let Some(rel) = src[from..].find(needle) {
        let mut i = from + rel + needle.len();
        // 開き括弧のあとの空白を飛ばす
        while i < bytes.len() && (bytes[i] as char).is_whitespace() {
            i += 1;
        }
        if i < bytes.len() && bytes[i] == b'"' {
            i += 1;
            let start = i;
            while i < bytes.len() && bytes[i] != b'"' {
                i += 1;
            }
            if i <= bytes.len() {
                let name = &src[start..i];
                // 環境変数らしい形のものだけ(大文字・数字・アンダースコア)
                if !name.is_empty()
                    && name
                        .chars()
                        .all(|c| c.is_ascii_uppercase() || c.is_ascii_digit() || c == '_')
                {
                    found.push(name.to_string());
                }
            }
        }
        from = from + rel + needle.len();
    }
    found
}

fn repo_root() -> PathBuf {
    PathBuf::from(env!("CARGO_MANIFEST_DIR"))
}

fn read_env_example() -> String {
    let p = repo_root().join(".env.example");
    fs::read_to_string(&p)
        .unwrap_or_else(|e| panic!(".env.example が読めない ({}): {e}", p.display()))
}

/// `.env.example` の中で `NAME=` として宣言されているもの。
fn declared_in_example(text: &str) -> BTreeSet<String> {
    text.lines()
        .map(str::trim)
        .filter(|l| !l.starts_with('#'))
        .filter_map(|l| l.split_once('='))
        .map(|(k, _)| k.trim().to_string())
        .filter(|k| {
            !k.is_empty()
                && k.chars()
                    .all(|c| c.is_ascii_uppercase() || c.is_ascii_digit() || c == '_')
        })
        .collect()
}

#[test]
fn 実装が読む環境変数はすべてenv_exampleに載っている() {
    let mut in_code = BTreeSet::new();
    collect_env_vars_in_src(&repo_root().join("src"), &mut in_code);

    // RUST_LOG は tracing_subscriber の EnvFilter が読むので env::var には出ない
    let main_rs = fs::read_to_string(repo_root().join("src/main.rs")).unwrap_or_default();
    if main_rs.contains("EnvFilter::try_from_default_env") {
        in_code.insert("RUST_LOG".to_string());
    }

    assert!(
        in_code.len() > 20,
        "環境変数の抽出に失敗している疑い(取れたのは {} 個)。\
         抽出ロジックが壊れると、このテストは何も守らなくなる",
        in_code.len()
    );

    let text = read_env_example();
    let missing: Vec<&String> = in_code.iter().filter(|n| !text.contains(n.as_str())).collect();

    assert!(
        missing.is_empty(),
        "実装が読んでいるのに .env.example に無い環境変数がある。\n\
         新しい環境を作る人がここで詰まる。\n\
         不足: {missing:?}"
    );
}

#[test]
fn env_exampleに書いてあるものは実装で使われている() {
    let mut in_code = BTreeSet::new();
    collect_env_vars_in_src(&repo_root().join("src"), &mut in_code);
    in_code.insert("RUST_LOG".to_string());

    let declared = declared_in_example(&read_env_example());
    let stale: Vec<&String> = declared.iter().filter(|n| !in_code.contains(*n)).collect();

    assert!(
        stale.is_empty(),
        "`.env.example` に `NAME=` と書いてあるのに、実装がその名前を読んでいない。\n\
         消し忘れか改名漏れ。値を入れても効かないので誤解を生む。\n\
         (これから作る機能の名前を先に置きたいなら `NAME=` ではなくコメントで書くこと)\n\
         余分: {stale:?}"
    );
}

#[test]
fn 起動に必須の二つが明記されている() {
    // この2つが抜けていたことが、そもそもの問題だった。
    // src/db/sheets_client.rs は未設定なら明示エラーで停止する。
    let text = read_env_example();
    for key in ["GOOGLE_SA_KEY_B64", "SPREADSHEET_ID"] {
        assert!(
            text.contains(key),
            "必須の {key} が .env.example に無い。これが無いと画面が出ない"
        );
    }
}

#[test]
fn 抽出ロジックそのものが動く() {
    // このテストが「何も見つけられずに素通り」しないことの担保。
    let sample = r#"
        let a = std::env::var("FOO_BAR").unwrap_or_default();
        let b = env::var( "BAZ" ).ok();
        let c = env::var("not_upper");     // 環境変数らしくないので拾わない
        let d = some_other::var("QUX");    // env::var ではないので拾わない
    "#;
    let got = extract_env_var_names(sample);
    assert!(got.contains(&"FOO_BAR".to_string()), "取れていない: {got:?}");
    assert!(got.contains(&"BAZ".to_string()), "空白入りが取れていない: {got:?}");
    assert!(!got.contains(&"not_upper".to_string()), "小文字を拾っている: {got:?}");
    assert!(!got.contains(&"QUX".to_string()), "別関数を拾っている: {got:?}");
}
