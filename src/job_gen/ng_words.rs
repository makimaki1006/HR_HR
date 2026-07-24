//! 求人票の禁止表現(NGワード)検証。
//!
//! `data/job_creation_media_engine/knowledge/ng_words.json`(24グループ/50ルール)を読み、
//! 生成テキストに性別・年齢・出身/居住地・心身差別の恐れがある表現が含まれないかを**コードで**判定する。
//! LLM には判定させない(法的リスクがあるため、機械的・決定論的に検出する)。
//!
//! # 検出セマンティクス(実データに基づく設計判断)
//!
//! グループは `standalone` フラグで2系統に分かれる:
//!
//! - **`standalone = true`**: `major` がテキストに含まれれば違反。
//!   例: 「主婦・ママ」「子供」「障害」「優しい・元気…」。求職者の属性・性格・容姿を
//!   限定する語そのものが問題なので、単独出現で拾う。過検出寄りだが、これは
//!   「求職者に対する限定表現は原則NG(注記に沿う)」という法令趣旨に合わせた意図的な設計。
//!
//! - **`standalone = false`(minors あり)**: `major` の出現末尾から**後方12文字以内**に
//!   いずれかの `minor` が出現したら違反。例:「女性(major)＋歓迎(minor)」。
//!   「女性歓迎」も「女性の方歓迎」も捕まえる一方、「女性が多い職場です」のような
//!   minor を伴わない言及は違反にしない(過検出を避ける)。
//!
//! # `・` `/` `改行` の扱い(代替語区切り)
//!
//! `major`/`minor` 内の `・` `/` `改行` は**代替語の区切り**として展開する。実データ根拠:
//! - 「主婦・ママ」= 主婦 または ママ
//! - 「車通勤・自転車通勤」= どちらか
//! - 「のある・がある」(minor)= どちらか
//! - 「五感\n視覚・聴覚・味覚・触覚・嗅覚・傾聴」= 各語いずれか
//! - 「入社祝い金/入社定着金/入職祝い金」= 各語いずれか
//!
//! # `〇`(ワイルドカード)の扱い
//!
//! `major`/`minor` 内の `〇`(U+3007 漢数字ゼロ)は**1文字以上の数字列**として扱う。
//! 唯一の該当が年齢差別グループの「〇歳・代」+ {以下/以上/まで/未満/…}。
//! ここで `・` を単純分割すると「〇歳」と「代」になり、「代」単独では「代表歓迎」等を
//! 誤検出しうる。実データの `〇` は "数字＋(歳|代)" を意味するため、
//! **元の major が `〇` で始まる場合、`〇` を含まない分割片には先頭に `〇` を補う**
//! ヒューリスティックを入れ、代替語を「〇歳」「〇代」に正規化する。これで
//! 「35歳以下」「40代まで」を拾い、「代表歓迎」を誤検出しない。
//! (このヒューリスティックが影響するのは `〇` 始まりの major のみ=実データでは年齢グループだけ)
//!
//! # 正規化
//!
//! 照合前にテキスト・パターンとも NFKC 正規化し、全角/半角のゆらぎ(「３５歳」→「35歳」、
//! 半角カナ等)を吸収する。`matched` には**元テキスト**の該当付近を入れる(NFKC で
//! 文字位置がずれないよう、元文字→正規化文字の位置対応表を保持する)。
//!
//! # 既知の限界(未解決事項として team-lead へ報告)
//!
//! `notes` に列挙された言い換え例(「〜らしい/〜ならでは」、主夫/パパ、看護婦/カメラマン 等)は
//! v1 では自動展開しない。notes は自由記述で機械展開が壊れやすく、無理な分割は誤検出源に
//! なりうるため。「特定の職種名」「環境依存文字」のような説明的 major はテキストに
//! そのまま現れないので実質不検出(取りこぼし)になる。ここは将来、notes を人手で
//! 構造化データへ落とし込む拡張余地がある。

use std::collections::HashSet;

use serde::{Deserialize, Serialize};
use unicode_normalization::UnicodeNormalization;

/// 検出された1違反。`major`/`minor` は元ルールの表記、`matched` は元テキストの該当付近。
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
pub struct NgViolation {
    /// 違反区分(例:「性別差別表現」「年齢差別」)。
    pub reason: String,
    /// 主要語(ルールの `major` 原文。例:「女性」「〇歳・代」)。
    pub major: String,
    /// 修飾語(ルールの `minor` 原文。standalone グループでは空文字)。
    pub minor: String,
    /// 元テキストで実際に該当した部分文字列(例:「女性歓迎」「35歳以下」)。
    pub matched: String,
}

/// ロード済みのNGワードルール集合。内部表現は正規化済みパターンを保持する。
#[derive(Debug, Clone)]
pub struct NgRules {
    groups: Vec<Group>,
}

#[derive(Debug, Clone)]
struct Group {
    reason: String,
    /// 報告用の major 原文。
    major_raw: String,
    /// major を代替語展開・NFKC 正規化したパターン群。
    major_pats: Vec<Vec<char>>,
    standalone: bool,
    /// (minor 原文, その代替語展開パターン群)。報告時は原文を使う。
    minors: Vec<(String, Vec<Vec<char>>)>,
}

/// major 末尾から minor を探索する最大文字距離(後方12文字以内)。
const MINOR_WINDOW: usize = 12;

// ---- JSON パース用 ----

#[derive(Deserialize)]
struct RawFile {
    groups: Vec<RawGroup>,
}

#[derive(Deserialize)]
struct RawGroup {
    reason: String,
    major: String,
    #[serde(default)]
    minors: Vec<String>,
    #[serde(default)]
    standalone: bool,
}

impl NgRules {
    /// `ng_words.json` 文字列からルールを構築する。
    pub fn load_from_str(json: &str) -> anyhow::Result<NgRules> {
        let raw: RawFile = serde_json::from_str(json)?;
        let groups = raw
            .groups
            .into_iter()
            .map(|g| Group {
                reason: g.reason,
                major_pats: expand_alternatives(&g.major),
                major_raw: g.major,
                standalone: g.standalone,
                minors: g
                    .minors
                    .into_iter()
                    .map(|m| {
                        let pats = expand_alternatives(&m);
                        (m, pats)
                    })
                    .collect(),
            })
            .collect();
        Ok(NgRules { groups })
    }

    /// テキストを走査し、検出した違反を返す(同一 (reason,major,minor,matched) は重複排除)。
    pub fn detect(&self, text: &str) -> Vec<NgViolation> {
        let norm = Norm::new(text);
        let hay = &norm.chars;

        let mut out: Vec<NgViolation> = Vec::new();
        let mut seen: HashSet<String> = HashSet::new();

        for g in &self.groups {
            for mpat in &g.major_pats {
                for (ms, me) in find_all(hay, mpat) {
                    if g.standalone {
                        let matched = norm.snippet(ms, me);
                        push_unique(
                            &mut out,
                            &mut seen,
                            NgViolation {
                                reason: g.reason.clone(),
                                major: g.major_raw.clone(),
                                minor: String::new(),
                                matched,
                            },
                        );
                        continue;
                    }
                    // minor を major 末尾から MINOR_WINDOW 文字以内で探す。
                    for (raw_minor, minor_pats) in &g.minors {
                        if let Some(minor_end) = find_minor_near(hay, me, minor_pats) {
                            let matched = norm.snippet(ms, minor_end);
                            push_unique(
                                &mut out,
                                &mut seen,
                                NgViolation {
                                    reason: g.reason.clone(),
                                    major: g.major_raw.clone(),
                                    minor: raw_minor.clone(),
                                    matched,
                                },
                            );
                        }
                    }
                }
            }
        }
        out
    }
}

/// 重複違反を排除しつつ追加する。
fn push_unique(out: &mut Vec<NgViolation>, seen: &mut HashSet<String>, v: NgViolation) {
    let key = format!("{}\u{1}{}\u{1}{}\u{1}{}", v.reason, v.major, v.minor, v.matched);
    if seen.insert(key) {
        out.push(v);
    }
}

/// major 末尾 `major_end` から後方 [`MINOR_WINDOW`] 文字以内で最初に一致した minor の
/// 終端位置を返す。
fn find_minor_near(hay: &[char], major_end: usize, minor_pats: &[Vec<char>]) -> Option<usize> {
    let limit = (major_end + MINOR_WINDOW).min(hay.len().saturating_sub(1));
    for pat in minor_pats {
        for start in major_end..=limit {
            if let Some(end) = match_at(hay, start, pat) {
                return Some(end);
            }
        }
    }
    None
}

// ---- パターン展開・正規化 ----

/// `major`/`minor` 文字列を代替語(`・` `/` 改行区切り)へ展開し、各片を NFKC 正規化した
/// 文字列パターンにする。詳細な設計判断はモジュールの doc コメントを参照。
fn expand_alternatives(raw: &str) -> Vec<Vec<char>> {
    let trimmed = raw.trim();
    // 「〇歳・代」のように元 major が `〇` 始まりなら、`〇` を持たない分割片に `〇` を補う。
    let propagate_wildcard = trimmed.starts_with('〇');

    let mut out = Vec::new();
    for frag in trimmed.split(['・', '/', '\n']) {
        // 先頭の「～」「~」は「…」を表す装飾なので落とす(例: minor「～〇歳まで」→「〇歳まで」)。
        let frag = frag.trim().trim_start_matches(|c| c == '～' || c == '~').trim();
        if frag.is_empty() {
            continue;
        }
        let mut s = frag.to_string();
        if propagate_wildcard && !s.contains('〇') {
            s = format!("〇{s}");
        }
        let pat = nfkc_chars(&s);
        if !pat.is_empty() {
            out.push(pat);
        }
    }
    out
}

/// 文字列を1文字ずつ NFKC 正規化して `Vec<char>` にする。
fn nfkc_chars(s: &str) -> Vec<char> {
    let mut v = Vec::new();
    for c in s.chars() {
        for nc in c.nfkc() {
            v.push(nc);
        }
    }
    v
}

/// 正規化テキストと、正規化文字→元文字の位置対応表。
struct Norm {
    /// NFKC 正規化後の文字列。
    chars: Vec<char>,
    /// `chars[i]` が元テキストの何文字目由来かを示す(元文字インデックス)。
    map: Vec<usize>,
    /// 元テキストの文字列。
    orig: Vec<char>,
}

impl Norm {
    /// 1文字ずつ NFKC 正規化しながら位置対応表を作る。全角→半角のような 1:多 展開でも、
    /// 各正規化文字は由来した元文字を指すので、`matched` を元テキストから復元できる。
    fn new(text: &str) -> Norm {
        let orig: Vec<char> = text.chars().collect();
        let mut chars = Vec::new();
        let mut map = Vec::new();
        for (i, &c) in orig.iter().enumerate() {
            for nc in c.nfkc() {
                chars.push(nc);
                map.push(i);
            }
        }
        Norm { chars, map, orig }
    }

    /// 正規化文字の範囲 `[ns, ne)` に対応する元テキストの部分文字列を返す。
    fn snippet(&self, ns: usize, ne: usize) -> String {
        if ne <= ns || ns >= self.map.len() {
            return String::new();
        }
        let os = self.map[ns];
        let oe = self.map[ne - 1] + 1;
        self.orig[os..oe.min(self.orig.len())].iter().collect()
    }
}

// ---- パターン照合(`〇` = 数字1文字以上) ----

/// `hay` の位置 `start` から `pat` が一致するか試し、一致すれば終端(排他)を返す。
/// `pat` 中の `〇` は1文字以上の ASCII 数字にマッチする(連続する `〇` はまとめて扱う)。
fn match_at(hay: &[char], start: usize, pat: &[char]) -> Option<usize> {
    let mut hi = start;
    let mut pi = 0;
    while pi < pat.len() {
        if pat[pi] == '〇' {
            let mut count = 0;
            while hi < hay.len() && hay[hi].is_ascii_digit() {
                hi += 1;
                count += 1;
            }
            if count == 0 {
                return None;
            }
            while pi < pat.len() && pat[pi] == '〇' {
                pi += 1;
            }
        } else {
            if hi >= hay.len() || hay[hi] != pat[pi] {
                return None;
            }
            hi += 1;
            pi += 1;
        }
    }
    Some(hi)
}

/// `hay` 中の `pat` の一致位置 `(start, end)` を**最左・最長優先で非重なり**に返す。
///
/// `match_at` は `〇`(数字列)を貪欲に消費するため、各開始位置での一致は最長になる。
/// さらに一致成立時はスキャン位置を一致終端まで飛ばすことで、内側から始まる短い重なり一致
/// (例:「28歳」に一致した後、内側の「8歳」)を生成しない。これで
/// 「28歳以下」から「8歳以下」が二重検出される不具合を防ぐ。
fn find_all(hay: &[char], pat: &[char]) -> Vec<(usize, usize)> {
    let mut res = Vec::new();
    if pat.is_empty() {
        return res;
    }
    let mut s = 0;
    while s < hay.len() {
        if let Some(e) = match_at(hay, s, pat) {
            res.push((s, e));
            // 一致は必ず1文字以上消費するので e > s。終端まで飛ばして重なりを避ける。
            s = e.max(s + 1);
        } else {
            s += 1;
        }
    }
    res
}

#[cfg(test)]
mod tests {
    use super::*;

    /// テストは実物の ng_words.json を読み込む(24グループ/構造の回帰も兼ねる)。
    /// HR_HR 統合 (2026-07-24): 埋め込み配布と同じ assets/ を参照する。
    const NG_JSON: &str = include_str!("../../assets/ng_words.json");

    fn rules() -> NgRules {
        NgRules::load_from_str(NG_JSON).expect("ng_words.json をロードできること")
    }

    fn reasons(vs: &[NgViolation]) -> Vec<String> {
        vs.iter().map(|v| v.reason.clone()).collect()
    }

    #[test]
    fn 実ファイルが24グループでパースできる() {
        let r = rules();
        assert_eq!(r.groups.len(), 24, "グループ数");
    }

    #[test]
    fn 女性歓迎_は性別差別で検出() {
        let v = rules().detect("経験者は女性歓迎です");
        assert!(!v.is_empty(), "検出されるべき: {v:?}");
        assert!(reasons(&v).contains(&"性別差別表現".to_string()));
        // matched に元テキストの該当が入る。
        assert!(v.iter().any(|x| x.matched.contains("女性歓迎")), "{v:?}");
    }

    #[test]
    fn 女性の方歓迎_は後方12文字以内で検出() {
        // major「女性」と minor「歓迎」の間に「の方」が挟まっても拾う。
        let v = rules().detect("女性の方歓迎");
        assert!(!v.is_empty(), "検出されるべき: {v:?}");
        assert!(v.iter().any(|x| x.matched == "女性の方歓迎"), "{v:?}");
    }

    #[test]
    fn 女性が多い職場_はルール外の言及で違反なし() {
        // minor(歓迎/応募可能/代表/限定/にピッタリ)を伴わない言及は違反にしない。
        let v = rules().detect("女性が多い職場です");
        assert!(v.is_empty(), "違反ゼロのはず: {v:?}");
    }

    #[test]
    fn 主婦ママ歓迎_はstandaloneで検出() {
        let v = rules().detect("主婦・ママ歓迎の職場");
        assert!(!v.is_empty(), "検出されるべき: {v:?}");
        assert!(reasons(&v).contains(&"性別差別表現".to_string()));
        // standalone は minor 空。
        assert!(v.iter().any(|x| x.major == "主婦・ママ" && x.minor.is_empty()), "{v:?}");
    }

    #[test]
    fn 年齢_35歳以下_はワイルドカードで検出() {
        let v = rules().detect("35歳以下の方を募集");
        assert!(reasons(&v).contains(&"年齢差別".to_string()), "{v:?}");
        assert!(v.iter().any(|x| x.matched == "35歳以下"), "{v:?}");
    }

    #[test]
    fn 年齢_全角の数字も正規化して検出() {
        // 「３５歳以下」(全角数字)も半角と同じく拾う。
        let v = rules().detect("３５歳以下歓迎");
        assert!(reasons(&v).contains(&"年齢差別".to_string()), "{v:?}");
        // matched は元テキスト(全角のまま)を返す。
        assert!(v.iter().any(|x| x.matched.starts_with("３５歳")), "{v:?}");
    }

    #[test]
    fn 年齢_40代まで_は〇代で検出() {
        let v = rules().detect("40代まで活躍中");
        assert!(reasons(&v).contains(&"年齢差別".to_string()), "{v:?}");
        assert!(v.iter().any(|x| x.matched == "40代まで"), "{v:?}");
    }

    #[test]
    fn 年齢_28歳以下_は最左最長で1件のみ() {
        // 〇=数字の貪欲一致で「28歳」を取った後、内側「8歳」を重ねて数えない(最左・最長優先)。
        let v = rules().detect("28歳以下の方");
        let age: Vec<_> = v.iter().filter(|x| x.reason == "年齢差別").collect();
        assert_eq!(age.len(), 1, "年齢差別は1件のみのはず: {v:?}");
        assert!(age[0].matched.contains("28歳"), "matchedに28歳を含む: {v:?}");
        assert!(
            !v.iter().any(|x| x.matched.starts_with("8歳")),
            "内側の8歳を単独検出してはならない: {v:?}"
        );
    }

    #[test]
    fn 代表歓迎_は〇代を誤検出しない() {
        // 「代」単独ではなく「数字＋代」を要求するので、年齢差別の誤検出は起きない。
        let v = rules().detect("代表歓迎");
        assert!(
            !reasons(&v).contains(&"年齢差別".to_string()),
            "年齢差別の誤検出があってはならない: {v:?}"
        );
    }

    #[test]
    fn シニア限定_は検出_シニア活躍中_は不検出() {
        let hit = rules().detect("シニア限定の求人");
        assert!(reasons(&hit).contains(&"年齢差別".to_string()), "{hit:?}");

        // notes:「～活躍中 はOK」。minor に「活躍中」は無いので違反にしない。
        let ok = rules().detect("シニア活躍中の職場");
        assert!(ok.is_empty(), "違反ゼロのはず: {ok:?}");
    }

    #[test]
    fn 違反なしテキストは空() {
        let v = rules().detect("未経験から始められる倉庫内の軽作業。土日祝休み、交通費支給。");
        assert!(v.is_empty(), "違反ゼロのはず: {v:?}");
    }

    #[test]
    fn standalone_子供_は単独出現で検出() {
        let v = rules().detect("子供が好きな方向けの保育補助");
        assert!(v.iter().any(|x| x.major == "子供"), "{v:?}");
    }

    #[test]
    fn 同一違反は重複排除される() {
        // 「女性歓迎」が2回出ても、同一 matched は1件に畳まれる。
        let v = rules().detect("女性歓迎。女性歓迎。");
        let count = v.iter().filter(|x| x.matched == "女性歓迎").count();
        assert_eq!(count, 1, "重複排除されるべき: {v:?}");
    }

    #[test]
    fn インラインjson_で車通勤できる方を検出() {
        // 代替語区切り(・)と minor 展開の最小確認。
        let json = r#"{"groups":[
            {"reason":"出身・居住地制限","major":"車通勤・自転車通勤","minors":["できる方"],"standalone":false}
        ]}"#;
        let r = NgRules::load_from_str(json).unwrap();
        assert!(!r.detect("自転車通勤できる方").is_empty());
        assert!(!r.detect("車通勤できる方歓迎").is_empty());
        // minor を伴わなければ違反にしない。
        assert!(r.detect("車通勤の場合は駐車場あり").is_empty());
    }
}
