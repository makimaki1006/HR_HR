//! 職種別知識の抽出(Sheets由来のローカルJSONを引く)。
//!
//! 正本設計: `docs/job_creation_media_engine_generation_pipeline_v1_2026-07-24.md` §2.3。
//!
//! # 設計方針: 「該当職種の行だけ」注入
//! 対象求人の職種名から**該当する職種シート1枚だけ**を選び、これに職種横断で有効な
//! 汎用シート(普遍的なKW/原稿FMT/職種別難易度/業務内容テンプレ等)を足して返す。
//! 無関係な職種の記述を混ぜると、それを取り込んで捏造する温床になるため注入しない
//! (§2.3)。プロンプトも小さく保てる。
//!
//! # データ
//! `data/job_creation_media_engine/knowledge/sheets/`:
//! - `index.json` = `{files:[{slug, sheet, spreadsheet, kind, job_keywords}]}`
//!   - `kind`: `"job_specific"`(職種別) / `"generic"`(職種横断で常に注入) /
//!     `"meta"`・`"attribute"`(参照用に保持するが注入しない)
//!   - `job_keywords`: その職種を職種名文字列から当てる部分一致キーワード
//! - `<slug>.json` = `{sheet, values}` (`values` は Sheets の二次元配列)
//!
//! # 職種の当て方(最長マッチ優先)
//! `job_title` に `job_keywords` のいずれかが部分一致する `job_specific` シートを選ぶ。
//! 複数該当する場合は**マッチしたキーワードが最も長いもの**を採る(例:
//! 「訪問介護スタッフ」は "介護" と "訪問介護" の両方に当たるが、より具体的な
//! 「訪問介護」シートを選ぶ)。該当なし→`category="その他"` + 汎用シートのみ。

use std::path::Path;

use anyhow::{Context, Result};
use serde::Deserialize;

/// プロンプト注入用の知識束。
///
/// `category` は当てた職種名(該当なしは "その他")。`sections` は (見出し, 本文) の列で、
/// 先頭が該当職種シート(あれば)、続いて汎用シート。
pub struct KnowledgeBundle {
    pub category: String,
    pub sections: Vec<(String, String)>,
}

/// 1シートあたり本文の最大文字数(プロンプト肥大防止)。超過分は切って印を付ける。
const MAX_SECTION_CHARS: usize = 4000;
const TRUNCATE_MARK: &str = "…(以下省略)";

#[derive(Deserialize)]
struct IndexFile {
    files: Vec<IndexEntry>,
}

#[derive(Deserialize)]
struct IndexEntry {
    slug: String,
    sheet: String,
    #[serde(default)]
    kind: String,
    #[serde(default)]
    job_keywords: Vec<String>,
}

#[derive(Deserialize)]
struct SheetFile {
    #[serde(default)]
    sheet: String,
    #[serde(default)]
    values: Vec<Vec<serde_json::Value>>,
}

/// 埋め込み知識バンドル(コンパイル時同梱)。
///
/// 正本は `data/job_creation_media_engine/knowledge/sheets/`(Sheetsスナップショット)で、
/// `assets/knowledge_bundle.json` はそこから結合生成したコピー。Render等の公開デプロイで
/// ファイルシステム配置に依存しないための同梱(約1MB)。更新手順は引き継ぎ資料参照。
const EMBEDDED_BUNDLE_JSON: &str = include_str!("../../assets/knowledge_bundle.json");

#[derive(Deserialize)]
struct BundleFile {
    index: IndexFile,
    sheets: std::collections::BTreeMap<String, SheetFile>,
}

/// メモリ上の知識ストア(埋め込みバンドル or 任意のバンドルJSON)。
pub struct KnowledgeStore {
    index: IndexFile,
    sheets: std::collections::BTreeMap<String, SheetFile>,
}

impl KnowledgeStore {
    /// バンドルJSON文字列からストアを作る。
    pub fn from_bundle_str(raw: &str) -> Result<KnowledgeStore> {
        let b: BundleFile =
            serde_json::from_str(raw).with_context(|| "knowledge_bundle のJSON解析失敗")?;
        Ok(KnowledgeStore { index: b.index, sheets: b.sheets })
    }

    /// 埋め込みバンドル(プロセス内で1回だけ解析)。
    pub fn embedded() -> &'static KnowledgeStore {
        static STORE: std::sync::OnceLock<KnowledgeStore> = std::sync::OnceLock::new();
        STORE.get_or_init(|| {
            KnowledgeStore::from_bundle_str(EMBEDDED_BUNDLE_JSON)
                .expect("埋め込み knowledge_bundle.json が不正(ビルド資産の破損)")
        })
    }

    /// 職種名から該当職種の知識束を返す(埋め込み/メモリ版)。
    pub fn lookup(&self, job_title: &str) -> KnowledgeBundle {
        select_and_build(&self.index, job_title, |slug, fallback| {
            Ok(self
                .sheets
                .get(slug)
                .and_then(|sheet| sheet_to_section(sheet, fallback)))
        })
        // クロージャが Err を返さないため unwrap は安全だが、防御的に空束へ倒す。
        .unwrap_or_else(|_| KnowledgeBundle { category: "その他".into(), sections: Vec::new() })
    }
}

/// 既定経路の lookup: env `KNOWLEDGE_DIR`(ng_words.json と sheets/ を含む階層)が
/// あればファイルシステム、なければ埋め込みバンドルを使う。
///
/// 公開デプロイ(Render)ではファイル配置に依存せず埋め込みで動く。ローカルで
/// 知識だけ差し替えたい場合に KNOWLEDGE_DIR で上書きできる。
pub fn lookup_default(job_title: &str) -> Result<KnowledgeBundle> {
    if let Ok(dir) = std::env::var("KNOWLEDGE_DIR") {
        if !dir.trim().is_empty() {
            return lookup(&Path::new(&dir).join("sheets"), job_title);
        }
    }
    Ok(KnowledgeStore::embedded().lookup(job_title))
}

/// 職種名から該当職種の知識束を返す(ファイルシステム版)。
///
/// `data_dir` = `data/job_creation_media_engine/knowledge/sheets`(`index.json` のある階層)。
pub fn lookup(data_dir: &Path, job_title: &str) -> Result<KnowledgeBundle> {
    let index_path = data_dir.join("index.json");
    let raw = std::fs::read_to_string(&index_path)
        .with_context(|| format!("index.json 読み込み失敗: {}", index_path.display()))?;
    let index: IndexFile =
        serde_json::from_str(&raw).with_context(|| "index.json のJSON解析失敗")?;
    select_and_build(&index, job_title, |slug, fallback| {
        load_section(data_dir, slug, fallback)
    })
}

/// 選定コア: 該当職種シートを最長マッチ優先で1枚選び、汎用シートを足す。
///
/// `get_section(slug, fallback_title)` がシート本文の取得を担う(fs/埋め込みで共有)。
fn select_and_build(
    index: &IndexFile,
    job_title: &str,
    mut get_section: impl FnMut(&str, &str) -> Result<Option<(String, String)>>,
) -> Result<KnowledgeBundle> {
    let mut best: Option<(&IndexEntry, usize)> = None;
    for e in &index.files {
        if e.kind != "job_specific" {
            continue;
        }
        // この職種の中で job_title に部分一致する最長キーワード長。
        let matched_len = e
            .job_keywords
            .iter()
            .filter(|kw| !kw.is_empty() && job_title.contains(kw.as_str()))
            .map(|kw| kw.chars().count())
            .max();
        if let Some(len) = matched_len {
            let better = match best {
                Some((_, cur)) => len > cur,
                None => true,
            };
            if better {
                best = Some((e, len));
            }
        }
    }

    let mut sections: Vec<(String, String)> = Vec::new();
    let category = match best {
        Some((e, _)) => {
            if let Some(sec) = get_section(&e.slug, &e.sheet)? {
                sections.push(sec);
            }
            clean_title(&e.sheet)
        }
        None => "その他".to_string(),
    };

    // 汎用シート(職種横断で常に注入)を index の順で追加。
    for e in &index.files {
        if e.kind != "generic" {
            continue;
        }
        if let Some(sec) = get_section(&e.slug, &e.sheet)? {
            sections.push(sec);
        }
    }

    Ok(KnowledgeBundle { category, sections })
}

/// 1シートを読み、(見出し, 本文) に整形。空(全セル空)のシートは None。
fn load_section(data_dir: &Path, slug: &str, fallback_title: &str) -> Result<Option<(String, String)>> {
    let path = data_dir.join(format!("{slug}.json"));
    let raw = std::fs::read_to_string(&path)
        .with_context(|| format!("シートJSON読み込み失敗: {}", path.display()))?;
    let sheet: SheetFile =
        serde_json::from_str(&raw).with_context(|| format!("{slug}.json のJSON解析失敗"))?;
    Ok(sheet_to_section(&sheet, fallback_title))
}

/// SheetFile を (見出し, 本文) に整形(fs/埋め込み共通)。空シートは None。
fn sheet_to_section(sheet: &SheetFile, fallback_title: &str) -> Option<(String, String)> {
    let body = rows_to_text(&sheet.values);
    if body.is_empty() {
        return None;
    }
    let title = if sheet.sheet.trim().is_empty() {
        clean_title(fallback_title)
    } else {
        clean_title(&sheet.sheet)
    };
    Some((title, truncate_chars(&body, MAX_SECTION_CHARS)))
}

/// 二次元配列を「セル|セル|…」の行テキストに整形(空行は落とす)。
fn rows_to_text(values: &[Vec<serde_json::Value>]) -> String {
    let mut lines: Vec<String> = Vec::new();
    for row in values {
        let cells: Vec<String> = row.iter().map(cell_to_string).collect();
        // 全セル空の行は捨てる。
        if cells.iter().all(|c| c.trim().is_empty()) {
            continue;
        }
        lines.push(cells.join(" | "));
    }
    lines.join("\n")
}

/// セル値を文字列化(文字列はそのまま、数値等は表示形)。
fn cell_to_string(v: &serde_json::Value) -> String {
    match v {
        serde_json::Value::String(s) => s.clone(),
        serde_json::Value::Null => String::new(),
        other => other.to_string(),
    }
}

/// 職種名の末尾空白・末尾 `_` を落とす(シート名の表記ゆれ吸収)。
fn clean_title(t: &str) -> String {
    t.trim().trim_end_matches('_').trim().to_string()
}

/// 文字(char)単位で上限に切り、切った場合は印を付ける。
fn truncate_chars(s: &str, max: usize) -> String {
    if s.chars().count() <= max {
        return s.to_string();
    }
    let head: String = s.chars().take(max).collect();
    format!("{head}{TRUNCATE_MARK}")
}

/// 束をプロンプト注入用テキストに整形。
pub fn bundle_to_text(b: &KnowledgeBundle) -> String {
    let mut out = String::new();
    out.push_str(&format!("# 職種: {}\n", b.category));
    for (heading, body) in &b.sections {
        out.push_str(&format!("\n## {heading}\n{body}\n"));
    }
    out
}

#[cfg(test)]
mod tests {
    use super::*;
    use serde_json::json;
    use std::path::PathBuf;

    /// テスト専用の一時ディレクトリ(tempfileクレート非依存)。Drop で自動削除。
    struct TmpDir(PathBuf);
    impl TmpDir {
        fn new(tag: &str) -> Self {
            let mut p = std::env::temp_dir();
            let uniq = format!(
                "jme_knowledge_{}_{}_{}",
                tag,
                std::process::id(),
                std::time::SystemTime::now()
                    .duration_since(std::time::UNIX_EPOCH)
                    .unwrap()
                    .as_nanos()
            );
            p.push(uniq);
            std::fs::create_dir_all(&p).unwrap();
            TmpDir(p)
        }
        fn path(&self) -> &Path {
            &self.0
        }
        fn write(&self, name: &str, contents: &str) {
            std::fs::write(self.0.join(name), contents).unwrap();
        }
    }
    impl Drop for TmpDir {
        fn drop(&mut self) {
            let _ = std::fs::remove_dir_all(&self.0);
        }
    }

    /// ミニチュアの index + シートを書く。
    fn setup(tag: &str) -> TmpDir {
        let d = TmpDir::new(tag);
        let index = json!({
            "files": [
                {"slug":"kaigo","sheet":"介護職","spreadsheet":"原稿改善資料",
                 "kind":"job_specific","job_keywords":["介護職","介護"]},
                {"slug":"houmon_kaigo","sheet":"訪問介護","spreadsheet":"原稿改善資料",
                 "kind":"job_specific","job_keywords":["訪問介護"]},
                {"slug":"kango","sheet":"看護師 ","spreadsheet":"原稿改善資料",
                 "kind":"job_specific","job_keywords":["看護師","看護"]},
                {"slug":"fuhen","sheet":"普遍的なKW","spreadsheet":"原稿改善資料",
                 "kind":"generic","job_keywords":[]},
                {"slug":"nanido","sheet":"職種別難易度","spreadsheet":"求人系",
                 "kind":"generic","job_keywords":[]},
                {"slug":"toc","sheet":"目次","spreadsheet":"原稿改善資料",
                 "kind":"meta","job_keywords":[]}
            ]
        });
        d.write("index.json", &index.to_string());
        d.write("kaigo.json", &json!({
            "sheet":"介護職",
            "values":[["職種名：介護職",""],["主な転職理由","業界の状況"],["","" ],["・人間関係","・非正規依存"]]
        }).to_string());
        d.write("houmon_kaigo.json", &json!({
            "sheet":"訪問介護",
            "values":[["訪問介護のポイント"],["直行直帰"]]
        }).to_string());
        d.write("kango.json", &json!({
            "sheet":"看護師 ",
            "values":[["看護師の訴求"],["夜勤なし"]]
        }).to_string());
        d.write("fuhen.json", &json!({
            "sheet":"普遍的なKW",
            "values":[["未経験歓迎"],["土日休み"]]
        }).to_string());
        d.write("nanido.json", &json!({
            "sheet":"職種別難易度",
            "values":[["採用レベル","職種"],["低","送迎・調理"]]
        }).to_string());
        d.write("toc.json", &json!({
            "sheet":"目次","values":[["これは注入されない"]]
        }).to_string());
        d
    }

    #[test]
    fn hits_job_specific_and_includes_generic() {
        let d = setup("hit");
        let b = lookup(d.path(), "介護職スタッフ募集").unwrap();
        assert_eq!(b.category, "介護職");
        let headings: Vec<&str> = b.sections.iter().map(|(h, _)| h.as_str()).collect();
        // 先頭が職種シート、続いて汎用2枚。meta(目次)は入らない。
        assert_eq!(headings, vec!["介護職", "普遍的なKW", "職種別難易度"]);
        // 職種本文が入っている。
        assert!(b.sections[0].1.contains("主な転職理由 | 業界の状況"));
        // 空行(全セル空)は落ちている。
        assert!(!b.sections[0].1.contains("\n\n"));
    }

    #[test]
    fn longest_keyword_wins() {
        let d = setup("longest");
        // "介護" にも "訪問介護" にも当たるが、より具体的な訪問介護シートを選ぶ。
        let b = lookup(d.path(), "訪問介護のヘルパー").unwrap();
        assert_eq!(b.category, "訪問介護");
        assert_eq!(b.sections[0].0, "訪問介護");
    }

    #[test]
    fn no_match_falls_back_to_other_and_generic_only() {
        let d = setup("nomatch");
        let b = lookup(d.path(), "宇宙飛行士").unwrap();
        assert_eq!(b.category, "その他");
        // 職種シートは無く、汎用のみ。
        let headings: Vec<&str> = b.sections.iter().map(|(h, _)| h.as_str()).collect();
        assert_eq!(headings, vec!["普遍的なKW", "職種別難易度"]);
    }

    #[test]
    fn cleans_trailing_underscore_and_space_in_category() {
        let d = setup("clean");
        let b = lookup(d.path(), "病院の看護師").unwrap();
        // シート名 "看護師 "(末尾空白)が "看護師" に整形される。
        assert_eq!(b.category, "看護師");
    }

    #[test]
    fn truncates_long_section() {
        let d = TmpDir::new("trunc");
        // 1セルに MAX 超えの本文を持つシート。
        let long_cell: String = "あ".repeat(MAX_SECTION_CHARS + 500);
        d.write("index.json", &json!({
            "files":[{"slug":"big","sheet":"介護職","spreadsheet":"x",
                      "kind":"job_specific","job_keywords":["介護"]}]
        }).to_string());
        d.write("big.json", &json!({
            "sheet":"介護職","values":[[long_cell]]
        }).to_string());
        let b = lookup(d.path(), "介護職").unwrap();
        let body = &b.sections[0].1;
        assert!(body.ends_with(TRUNCATE_MARK));
        // 上限文字数 + 印 の長さちょうど。
        assert_eq!(body.chars().count(), MAX_SECTION_CHARS + TRUNCATE_MARK.chars().count());
    }

    #[test]
    fn bundle_to_text_contains_category_and_sections() {
        let d = setup("text");
        let b = lookup(d.path(), "介護のお仕事").unwrap();
        let t = bundle_to_text(&b);
        assert!(t.contains("# 職種: 介護職"));
        assert!(t.contains("## 普遍的なKW"));
        assert!(t.contains("未経験歓迎"));
    }

    // ---- 埋め込みバンドル(実データ同梱)の検証 ----

    #[test]
    fn embedded_bundle_が解析でき実データ規模を持つ() {
        let s = KnowledgeStore::embedded();
        assert!(s.index.files.len() >= 160, "index件数が想定より少ない: {}", s.index.files.len());
        assert!(s.sheets.len() >= 160, "シート数が想定より少ない: {}", s.sheets.len());
    }

    #[test]
    fn embedded_lookup_介護職が当たり汎用も付く() {
        let b = KnowledgeStore::embedded().lookup("介護員【無資格者】");
        assert_eq!(b.category, "介護職");
        assert!(!b.sections.is_empty());
        let headings: Vec<&str> = b.sections.iter().map(|(h, _)| h.as_str()).collect();
        assert!(headings.iter().any(|h| h.contains("介護")), "職種シートが先頭にない: {headings:?}");
    }

    #[test]
    fn embedded_lookup_該当なしはその他で汎用のみ() {
        let b = KnowledgeStore::embedded().lookup("宇宙飛行士");
        assert_eq!(b.category, "その他");
        assert!(!b.sections.is_empty(), "汎用シートは付くはず");
    }
}
