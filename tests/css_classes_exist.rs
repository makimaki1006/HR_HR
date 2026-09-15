//! 配っている CSS に無いクラス名を書いていないか、ソースの側で見張る。
//!
//! # なぜ必要か
//! この画面の Tailwind は JIT ではなく `static/css/tailwind-precompiled.css`
//! に焼き込んだ静的 CSS で配っている。**そこに無いクラスを書いても、
//! エラーも警告も出ず、ただ何も起きない。**
//!
//! 2026-09-10、Indeed タブの見た目の修正が軒並み無効になっていた。
//! `sky-*` と `rose-*` は CSS に 1 件も無く、`text-rose-400`（減少の赤）、
//! `bg-sky-900/30`（要点ボックス）、`z-20`、`bg-navy-900/95`、`backdrop-blur`
//! が全部素通りしていた。computed style を読むまで誰も気づけなかった。
//!
//! # 照合の考え方: CSS からクラス名を「抽出」してはいけない
//! `@media` の入れ子・`:hover` の付与・`\` エスケープで必ず壊れる。
//! 実際にそれで `/` `:` `.` を含むクラスを軒並み「無い」と誤検出した。
//! 正解は **トークンごとに生 CSS を検索する** こと。
//! Tailwind は特殊文字を `\` でエスケープして出力する:
//!
//! ```text
//!   bg-blue-500/20    ->  .bg-blue-500\/20
//!   hover:text-white  ->  .hover\:text-white:hover
//!   p-1.5             ->  .p-1\.5
//! ```
//!
//! なので「特殊文字の前の `\` はあってもなくてもよい」形で照合する。
//! 一致したあとに `[\w-]` か `\` が続くなら、それはもっと長い別のクラスなので
//! 採らない（`.bg-blue-500` は `bg-blue-5` の定義ではないし、
//! `.bg-amber-500\/10` は `bg-amber-500` の定義ではない）。
//!
//! # 何を置き換えたか
//! `src/lib.rs` に `css_utility_tests` という同趣旨のテストがあったが、
//! 対象が 5 個のベタ書き配列だった。2026-09-10 に素通りした `sky-*` `rose-*`
//! `z-20` `backdrop-blur` は 1 つも入っておらず、緑のまま何も守っていない。
//! 手で並べる方式をやめ、ソースから総当たりで集める方式にした。
//! 旧テストが見ていた 5 個は `旧lib_rsのベタ書き5個を今も見ている` で押さえる。
//!
//! # 「定義されている」の定義
//! 干し草は `static/css/*.css` + `templates/**/*.html` + `src/**/*.rs` の
//! 文字列リテラル。レポート系ハンドラは CSS を Rust の文字列として持って
//! いる（`.kpi-card` `.tbl-wrap` は `src/handlers/indeed/report.rs` が定義元）
//! ので、そこを外すと定義済みのクラスを大量に誤検出する。
//!
//! したがってこの検査が言えるのは **「どこにも定義が無い」** ことだけで、
//! 「そのページに読み込まれる CSS に定義がある」ことまでは言えない。
//! そこは実ページの stylesheet を読む段 2 (`scripts/audit_css_classes.js`) が見る。
//!
//! # 使用側として見る場所
//! * `src/**/*.rs` の文字列リテラル
//!     1. `class="..."` に直接書かれたトークン
//!     2. リテラルのどこにあっても形が一意に決まるもの
//!        （色ユーティリティ / `z-<数字>` / `backdrop-*`）
//!     3. 「クラス列にしか見えない」リテラル
//!        （`"bg-navy-800/60 border border-slate-500 rounded-lg p-4"` のような、
//!        タプルに入れて `class="{card}"` へ流し込む書き方）
//! * `templates/**/*.html`
//!     - `class="..."` の中身（`{{ }}` `{% %}` は潰す）
//!     - それ以外の行は 2 だけ（`<script>` が付けるクラスを拾うため）
//!     - `<style>` と HTML コメントは使用側から外す
//!
//! # ここが段 1 の限界
//! `format!("{}", if x { "..." } else { "..." })` の片側が色以外のクラス
//! だった場合など、3 でも拾えないものは取りこぼす。`static/js/**` も見ていない。
//! **だから実 DOM を読む段 2 と対で運用する。**

use std::cell::RefCell;
use std::collections::{BTreeMap, BTreeSet};
use std::fs;
use std::path::{Path, PathBuf};

/// 既知の未定義クラス。1 行 1 件。直したらその行を消す。
const ALLOWLIST_PATH: &str = "tests/css_class_allowlist.txt";

/// この行より下は自動生成。`CSS_CLASS_ALLOWLIST=write` で書き直す。
/// 上は手書き（今この瞬間に誰かが直している最中のもの）で、自動生成では触らない。
const GENERATED_MARKER: &str = "# ---- ここから自動生成（着手前から在る分）。手で編集しない ----";

/// ハイフンを含まない Tailwind のユーティリティ。抽出 3 で
/// 「これはクラス列だ」と判断するときに、英文の散文と区別するために使う。
/// `"search assessment"`（expect のメッセージ）を弾くのがここの役目。
const BARE_UTILITIES: &[&str] = &[
    "flex",
    "grid",
    "block",
    "inline",
    "hidden",
    "relative",
    "absolute",
    "fixed",
    "sticky",
    "static",
    "truncate",
    "italic",
    "underline",
    "uppercase",
    "lowercase",
    "capitalize",
    "border",
    "rounded",
    "shadow",
    "container",
    "table",
    "transform",
    "transition",
    "group",
    "peer",
    "visible",
    "invisible",
    "isolate",
    "antialiased",
    "resize",
    "collapse",
];

/// 色ユーティリティの接頭辞。`<接頭辞>-<色名>-<数字>[/<数字>]` の形だけを見る。
const COLOR_PREFIXES: &[&str] = &[
    "text",
    "bg",
    "border",
    "from",
    "to",
    "via",
    "ring",
    "divide",
    "fill",
    "stroke",
    "decoration",
    "outline",
    "shadow",
    "accent",
    "caret",
    "placeholder",
];

/// 修飾子。`hover:bg-slate-700` のように前に付く。
/// ここに無いものが `:` の前に来たら、それはクラスではないと判断する
/// （`style="text-align:right"` の `text-align` を弾くため）。
const VARIANTS: &[&str] = &[
    "sm",
    "md",
    "lg",
    "xl",
    "2xl",
    "hover",
    "focus",
    "focus-visible",
    "focus-within",
    "active",
    "visited",
    "disabled",
    "checked",
    "first",
    "last",
    "odd",
    "even",
    "group-hover",
    "group-focus",
    "peer-hover",
    "peer-focus",
    "print",
    "dark",
    "motion-safe",
    "motion-reduce",
    "rtl",
    "ltr",
    "empty",
];

// ===========================================================================
// ファイル収集
// ===========================================================================

fn walk(dir: &Path, ext: &str, out: &mut Vec<PathBuf>) {
    let Ok(entries) = fs::read_dir(dir) else {
        return;
    };
    for e in entries.flatten() {
        let p = e.path();
        if p.is_dir() {
            walk(&p, ext, out);
        } else if p.extension().map(|x| x == ext).unwrap_or(false) {
            out.push(p);
        }
    }
}

// ===========================================================================
// Rust の文字列リテラルを取り出す
// ===========================================================================

fn prev_is_ident(b: &[u8], i: usize) -> bool {
    i > 0 && (b[i - 1].is_ascii_alphanumeric() || b[i - 1] == b'_')
}

/// ソースから文字列リテラルの中身（エスケープを戻したもの）と開始行を返す。
///
/// コメントと文字リテラルを飛ばさないと位置がずれて全部壊れる。
/// 実在する厄介な例:
///   * `'"' => out.push_str("&quot;")` ... `esc()` の中。素朴に読むと `'"'` で
///     文字列が始まったと誤認し、以降のリテラル境界が全部ずれる
///   * 日本語コメント中の `"` や `'`
///   * 行末 `\` による継続。この表は `class="..."` が継続をまたぐ
fn string_literals(src: &str) -> Vec<(usize, String)> {
    let b = src.as_bytes();
    let mut out: Vec<(usize, String)> = Vec::new();
    let mut i = 0usize;
    let mut line = 1usize;

    while i < b.len() {
        let c = b[i];

        if c == b'\n' {
            line += 1;
            i += 1;
            continue;
        }

        // 行コメント
        if c == b'/' && b.get(i + 1) == Some(&b'/') {
            while i < b.len() && b[i] != b'\n' {
                i += 1;
            }
            continue;
        }

        // ブロックコメント（Rust は入れ子可）
        if c == b'/' && b.get(i + 1) == Some(&b'*') {
            let mut depth = 1usize;
            i += 2;
            while i < b.len() && depth > 0 {
                if b[i] == b'\n' {
                    line += 1;
                    i += 1;
                } else if b[i] == b'/' && b.get(i + 1) == Some(&b'*') {
                    depth += 1;
                    i += 2;
                } else if b[i] == b'*' && b.get(i + 1) == Some(&b'/') {
                    depth -= 1;
                    i += 2;
                } else {
                    i += 1;
                }
            }
            continue;
        }

        // 文字リテラル / ライフタイム
        if c == b'\'' {
            if b.get(i + 1) == Some(&b'\\') {
                // '\n' '\'' '\u{a0}' など
                i += 2;
                while i < b.len() && b[i] != b'\'' {
                    i += 1;
                }
                i += 1;
            } else if b.get(i + 2) == Some(&b'\'') {
                i += 3; // 'x'
            } else {
                i += 1; // 'a （ライフタイム）
            }
            continue;
        }

        // 生文字列 r"..." / r#"..."#
        if c == b'r' && !prev_is_ident(b, i) {
            let mut j = i + 1;
            let hstart = j;
            while b.get(j) == Some(&b'#') {
                j += 1;
            }
            let hashes = j - hstart;
            if b.get(j) == Some(&b'"') {
                j += 1;
                let start = j;
                loop {
                    if j >= b.len() {
                        break;
                    }
                    if b[j] == b'"' {
                        let mut k = j + 1;
                        let mut n = 0usize;
                        while n < hashes && b.get(k) == Some(&b'#') {
                            k += 1;
                            n += 1;
                        }
                        if n == hashes {
                            break;
                        }
                    }
                    j += 1;
                }
                let end = j.min(b.len());
                let content = String::from_utf8_lossy(&b[start..end]).into_owned();
                let nl = content.matches('\n').count();
                out.push((line, content));
                line += nl;
                i = (end + 1 + hashes).min(b.len());
                continue;
            }
            i += 1;
            continue;
        }

        // 通常の文字列
        if c == b'"' {
            let startline = line;
            let mut j = i + 1;
            let mut buf: Vec<u8> = Vec::new();
            while j < b.len() {
                match b[j] {
                    b'\\' => match b.get(j + 1) {
                        Some(b'"') => {
                            buf.push(b'"');
                            j += 2;
                        }
                        Some(b'\\') => {
                            buf.push(b'\\');
                            j += 2;
                        }
                        Some(b'n') => {
                            buf.push(b'\n');
                            j += 2;
                        }
                        Some(b't') => {
                            buf.push(b'\t');
                            j += 2;
                        }
                        Some(b'r') => {
                            buf.push(b'\r');
                            j += 2;
                        }
                        Some(b'\'') => {
                            buf.push(b'\'');
                            j += 2;
                        }
                        // 行末 `\` による継続。次行の先頭空白ごと畳む
                        Some(b'\n') => {
                            line += 1;
                            buf.push(b' ');
                            j += 2;
                            while j < b.len() && (b[j] == b' ' || b[j] == b'\t') {
                                j += 1;
                            }
                        }
                        // \u{..} \0 など。中身は捨ててよい（クラス名には出ない）
                        Some(_) => {
                            buf.push(b' ');
                            j += 2;
                        }
                        None => {
                            j += 1;
                        }
                    },
                    b'"' => break,
                    ch => {
                        if ch == b'\n' {
                            line += 1;
                        }
                        buf.push(ch);
                        j += 1;
                    }
                }
            }
            out.push((startline, String::from_utf8_lossy(&buf).into_owned()));
            i = (j + 1).min(b.len());
            continue;
        }

        i += 1;
    }
    out
}

// ===========================================================================
// CSS に定義があるか（トークン -> 生 CSS を検索）
// ===========================================================================

/// `token` に対応するセレクタが `css` の中にあるか。
///
/// `.` の直後からトークンを 1 文字ずつ照合する。英数字とハイフン以外の文字は
/// 直前に `\` が入っていてもよい（Tailwind の出力がそうなっている）。
/// 一致したあと `[\w-]` が続く場合は別のクラスなので採らない。
fn defined_in_css(css: &[u8], token: &str) -> bool {
    let dots: Vec<usize> = (0..css.len()).filter(|&i| css[i] == b'.').collect();
    defined_at(css, &dots, token)
}

/// 干し草の `.` の位置を先頭文字ごとに仕分けておく索引。
///
/// 素朴に全走査すると、干し草（CSS + テンプレート + Rust の全リテラル）が
/// 数 MB あるため、数千トークンの照合で十数秒かかる。先頭 1 文字で
/// 絞るだけで実測 12.7s → 1s 台になる。照合そのものの考え方は変えていない。
struct DotIndex {
    by_first: BTreeMap<u8, Vec<usize>>,
}

impl DotIndex {
    fn build(css: &[u8]) -> Self {
        let mut by_first: BTreeMap<u8, Vec<usize>> = BTreeMap::new();
        for i in 0..css.len() {
            if css[i] != b'.' {
                continue;
            }
            // `.` の次が `\` なら、それは Tailwind のエスケープなので 1 つ飛ばす
            let mut j = i + 1;
            if css.get(j) == Some(&b'\\') {
                j += 1;
            }
            if let Some(&f) = css.get(j) {
                by_first.entry(f).or_default().push(i);
            }
        }
        Self { by_first }
    }

    fn contains(&self, css: &[u8], token: &str) -> bool {
        let Some(&first) = token.as_bytes().first() else {
            return false;
        };
        match self.by_first.get(&first) {
            Some(dots) => defined_at(css, dots, token),
            None => false,
        }
    }
}

fn defined_at(css: &[u8], dots: &[usize], token: &str) -> bool {
    let t = token.as_bytes();
    if t.is_empty() {
        return false;
    }
    for &i in dots {
        let mut j = i + 1;
        let mut k = 0usize;
        let mut ok = true;
        while k < t.len() {
            let c = t[k];
            let plain = c.is_ascii_alphanumeric() || c == b'-';
            if !plain && css.get(j) == Some(&b'\\') {
                j += 1;
            }
            if css.get(j) != Some(&c) {
                ok = false;
                break;
            }
            j += 1;
            k += 1;
        }
        if ok {
            // 一致した「あと」に何が続くか。ここを間違えると
            // 別の長いクラスを自分の定義だと誤認する。
            //
            //   `.bg-amber-500\/10` は bg-amber-500/10 の定義であって
            //   bg-amber-500 の定義ではない。素朴に「次が [\w-] でなければ可」
            //   とすると、次の文字が `\` なので通ってしまう。
            //   実際これで bg-amber-500 / text-teal-300 など 20 種以上を
            //   「定義済み」と誤判定していた（2026-09-11 の突き合わせで発覚）。
            //
            // 一方 `.hover\:text-white:hover` の末尾の `:` は擬似クラスの
            // 始まりなので、エスケープされていない記号は終端として認める。
            let tail_ok = match css.get(j) {
                None => true,
                // `\` が続く = エスケープされた記号でクラス名がまだ続いている
                Some(&b'\\') => false,
                Some(&n) => !(n.is_ascii_alphanumeric() || n == b'_' || n == b'-'),
            };
            if tail_ok {
                return true;
            }
        }
    }
    false
}

// ===========================================================================
// トークンの形を見る
// ===========================================================================

/// 修飾子を剥がして本体を返す。未知の修飾子が付いていたら None
/// （`text-align:right` のような CSS 宣言をクラスと誤認しないため）。
fn strip_variants(tok: &str) -> Option<&str> {
    let mut rest = tok;
    while let Some(p) = rest.find(':') {
        let (v, after) = rest.split_at(p);
        if !VARIANTS.contains(&v) {
            return None;
        }
        rest = &after[1..];
    }
    if rest.is_empty() {
        None
    } else {
        Some(rest)
    }
}

/// 色ユーティリティ `<接頭辞>-<色名>-<数字>[/<数字>]` なら色名を返す。
fn color_family(tok: &str) -> Option<String> {
    let body = strip_variants(tok)?;
    let (head, tail) = body.split_once('-')?;
    if !COLOR_PREFIXES.contains(&head) {
        return None;
    }
    // 末尾の `/<数字>`（不透明度）を落とす
    let tail = match tail.split_once('/') {
        Some((a, b)) if !b.is_empty() && b.bytes().all(|c| c.is_ascii_digit()) => a,
        Some(_) => return None,
        None => tail,
    };
    let (family, num) = tail.rsplit_once('-')?;
    if num.is_empty() || !num.bytes().all(|c| c.is_ascii_digit()) {
        return None;
    }
    // 方向指定 (border-t-slate-700) を許す。色名は最後のセグメント
    let family = family.rsplit('-').next().unwrap_or(family);
    if family.len() < 2 || !family.bytes().all(|c| c.is_ascii_lowercase()) {
        return None;
    }
    Some(family.to_string())
}

/// 形が一意に決まるので、文字列リテラルのどこにあっても拾ってよいトークンか。
/// 今回の事故で素通りした 5 つ（色 3 種・`z-20`・`backdrop-blur`）を全部覆う。
fn is_unambiguous_utility(tok: &str) -> bool {
    if color_family(tok).is_some() {
        return true;
    }
    let Some(body) = strip_variants(tok) else {
        return false;
    };
    // z-20 / z-50
    if let Some(n) = body.strip_prefix("z-") {
        if !n.is_empty() && n.bytes().all(|c| c.is_ascii_digit()) {
            return true;
        }
    }
    // backdrop-blur / backdrop-blur-sm
    if let Some(rest) = body.strip_prefix("backdrop-") {
        if !rest.is_empty()
            && rest
                .bytes()
                .all(|c| c.is_ascii_lowercase() || c.is_ascii_digit() || c == b'-')
        {
            return true;
        }
    }
    false
}

/// クラス名として通りうる形か（抽出 1・3 のふるい）。
///
/// # 任意値クラス `min-h-[44px]` を落とさないこと
/// 以前 `src/lib.rs` にあったベタ書きのテストが見ていた 5 個のうち 3 個
/// (`min-h-[44px]` `text-[10px]` `text-[11px]`) は角括弧つきの任意値クラス。
/// 記号を一律に弾く実装にすると、**まさに守りたかったものだけが抜ける**。
/// 角括弧の中は 1 組だけ許し、中身は CSS の値として在りうる文字に限る。
fn looks_like_class_name(tok: &str) -> bool {
    if tok.is_empty() || tok.len() > 60 {
        return false;
    }

    // 角括弧の任意値を切り出して、中身と外側を別々に見る
    let (outer, inner) = match (tok.find('['), tok.rfind(']')) {
        (Some(a), Some(z)) if z > a => {
            // 括弧は 1 組だけ。閉じが末尾より後ろに文字があるのは可 (`[&>svg]:size-4`)
            if tok[a + 1..z].contains('[') || tok[z + 1..].contains(']') {
                return false;
            }
            (
                format!("{}{}", &tok[..a], &tok[z + 1..]),
                Some(&tok[a + 1..z]),
            )
        }
        (None, None) => (tok.to_string(), None),
        _ => return false, // 片方だけある = クラス名ではない
    };

    if let Some(v) = inner {
        // 任意値の中身。空白が入るクラス名は存在しない
        if v.is_empty()
            || !v.bytes().all(|c| {
                c.is_ascii_alphanumeric()
                    || matches!(
                        c,
                        b'-' | b'_'
                            | b'.'
                            | b'%'
                            | b'#'
                            | b'/'
                            | b'+'
                            | b'*'
                            | b'('
                            | b')'
                            | b','
                            | b':'
                            | b'&'
                            | b'>'
                            | b'='
                            | b'\''
                            | b'"'
                            | b'!'
                    )
            })
        {
            return false;
        }
    }

    let tok = outer.as_str();
    if tok.is_empty() {
        return false;
    }
    let b = tok.as_bytes();
    if !b[0].is_ascii_lowercase() {
        return false;
    }
    // 角括弧を外すと末尾が区切り記号になることがある (`min-h-[44px]` -> `min-h-`)
    if inner.is_none() && !b[b.len() - 1].is_ascii_alphanumeric() {
        return false;
    }
    let mut prev_sep = false;
    for &c in b {
        let sep = matches!(c, b'-' | b':' | b'/' | b'.');
        if sep {
            if prev_sep {
                return false;
            }
            prev_sep = true;
            continue;
        }
        if !(c.is_ascii_lowercase() || c.is_ascii_digit()) {
            return false;
        }
        prev_sep = false;
    }
    strip_variants(tok).is_some()
}

// ===========================================================================
// 抽出
// ===========================================================================

/// `{...}` のプレースホルダを空白に潰す。`class="{td} tabular-nums {jc}"` から
/// 実際に書かれているクラスだけを残すため。
fn drop_placeholders(s: &str) -> String {
    let mut out = String::with_capacity(s.len());
    let mut depth = 0usize;
    for ch in s.chars() {
        match ch {
            '{' => {
                depth += 1;
                out.push(' ');
            }
            '}' => {
                depth = depth.saturating_sub(1);
                out.push(' ');
            }
            _ if depth > 0 => out.push(' '),
            c => out.push(c),
        }
    }
    out
}

/// 抽出 1: `class="..."` に直接書かれたトークン。文脈でクラスと確定する。
fn from_class_attr(content: &str) -> Vec<String> {
    let mut out = Vec::new();
    let bytes = content.as_bytes();
    let mut i = 0usize;
    while let Some(p) = content[i..].find("class=") {
        let at = i + p + "class=".len();
        i = at;
        let Some(&q) = bytes.get(at) else { break };
        if q != b'"' && q != b'\'' {
            continue;
        }
        let Some(endrel) = content[at + 1..].find(q as char) else {
            // 閉じ引用符が別のリテラルにある書き方（push_str の分割）。
            // 中途半端に拾うと誤検出になるので、この出現は捨てる
            break;
        };
        let inner = &content[at + 1..at + 1 + endrel];
        for t in drop_placeholders(inner).split_whitespace() {
            out.push(t.to_string());
        }
        i = at + 1 + endrel;
    }
    out
}

/// 文字列を「クラス名になりうる断片」に割る。区切りは英数字と `-:/.` 以外。
fn candidate_tokens(content: &str) -> Vec<&str> {
    content
        .split(|c: char| !(c.is_ascii_alphanumeric() || matches!(c, '-' | ':' | '/' | '.')))
        .filter(|s| !s.is_empty())
        .collect()
}

// ===========================================================================
// 許可リスト
// ===========================================================================

/// 許可リストの 1 行。
///
/// * `クラス名` だけ ... どのファイルでも許す（手書き部分で使う。
///   段 1 の視界外＝テンプレートや JS 側のものもここに入る）
/// * `クラス名 ファイル` ... そのファイルでだけ許す（自動生成部分で使う）
///
/// ファイル単位にしてあるのは、クラス名だけで許すと
/// 「よそで使われている `text-rose-400` を Indeed タブに新しく持ち込む」が
/// 素通りしてしまうため。それは今回の事故そのものなので、赤にしたい。
#[derive(Default)]
struct Allow {
    global: BTreeSet<String>,
    scoped: BTreeSet<(String, String)>,
}

impl Allow {
    fn parse(s: &str) -> Self {
        let mut a = Allow::default();
        for line in s.lines() {
            let body = line.split('#').next().unwrap_or("").trim();
            if body.is_empty() {
                continue;
            }
            let mut it = body.split_whitespace();
            let Some(tok) = it.next() else { continue };
            match it.next() {
                Some(file) => {
                    a.scoped.insert((tok.to_string(), file.to_string()));
                }
                None => {
                    a.global.insert(tok.to_string());
                }
            }
        }
        a
    }

    fn allows(&self, tok: &str, file: &str) -> bool {
        self.global.contains(tok) || self.scoped.contains(&(tok.to_string(), file.to_string()))
    }
}

/// 許可リストを「手書き部分」と「自動生成部分」に分けて読む。
///
/// * 手書き部分 ... 今まさに誰かが直している最中のもの。理由と日付を添えて足す。
///   直ったら行を消す。自動生成では絶対に触らない
/// * 自動生成部分 ... この検査を入れる前から在った分。ラチェットの土台。
///   `CSS_CLASS_ALLOWLIST=write cargo test --test css_classes_exist` で書き直す
fn load_allowlist() -> (String, Allow, Allow) {
    let Ok(text) = fs::read_to_string(ALLOWLIST_PATH) else {
        return (String::new(), Allow::default(), Allow::default());
    };
    let (head, tail) = match text.find(GENERATED_MARKER) {
        Some(p) => (&text[..p], &text[p + GENERATED_MARKER.len()..]),
        None => (&text[..], ""),
    };
    (head.to_string(), Allow::parse(head), Allow::parse(tail))
}

// ===========================================================================
// 本体
// ===========================================================================

/// クラス名 -> ファイル -> そのファイルでの最初の行
type Hits = BTreeMap<String, BTreeMap<String, usize>>;

struct Found {
    hits: Hits,
    /// CSS にある色名の在庫
    families: BTreeSet<String>,
}

fn scan() -> Found {
    // ---- 干し草を作る -----------------------------------------------------
    let mut css_files = Vec::new();
    walk(Path::new("static/css"), "css", &mut css_files);
    css_files.sort();

    let mut tailwind_css = String::new();
    let mut haystack = String::new();
    for p in &css_files {
        if let Ok(t) = fs::read_to_string(p) {
            if p.file_name().map(|n| n == "tailwind-precompiled.css") == Some(true) {
                tailwind_css.push_str(&t);
            }
            haystack.push('\n');
            haystack.push_str(&t);
        }
    }

    let mut tpl_files = Vec::new();
    walk(Path::new("templates"), "html", &mut tpl_files);
    tpl_files.sort();
    for p in &tpl_files {
        if let Ok(t) = fs::read_to_string(p) {
            haystack.push('\n');
            haystack.push_str(&t);
        }
    }

    let mut rs_files = Vec::new();
    walk(Path::new("src"), "rs", &mut rs_files);
    rs_files.sort();

    // Rust 側は文字列リテラルの中身だけを干し草にする。
    // エスケープを戻してあるので `.hover\:text-white` がそのまま入る
    let mut units: Vec<Unit> = Vec::new();
    for p in &rs_files {
        let Ok(src) = fs::read_to_string(p) else {
            continue;
        };
        let file = p.display().to_string().replace('\\', "/");
        for (line, content) in string_literals(&src) {
            haystack.push('\n');
            haystack.push_str(&content);
            units.push(Unit {
                file: file.clone(),
                line,
                text: content,
                kind: Kind::RustLiteral,
            });
        }
    }

    // テンプレートも使用側として見る。
    // 2026-09-11 に Python 版の棚卸しと突き合わせたところ、私が取りこぼした
    // 40 種のうち 32 種が templates/tabs/*.html の class 属性だった。
    // 外枠 (dashboard_inline.html) は直っても、タブ側は同じ状態のまま。
    for p in &tpl_files {
        let Ok(src) = fs::read_to_string(p) else {
            continue;
        };
        units.extend(html_units(p, &src));
    }

    let hay = haystack.as_bytes();

    // ---- 色名の在庫（失敗メッセージに出す） --------------------------------
    // Tailwind のビルド済み CSS だけを見る。ここが「使える色名」の正本
    let mut families = BTreeSet::new();
    for seg in tailwind_css.split('.') {
        let tok: String = seg
            .chars()
            .take_while(|c| c.is_ascii_alphanumeric() || matches!(c, '-' | '/' | '\\'))
            .filter(|c| *c != '\\')
            .collect();
        if let Some(f) = color_family(&tok) {
            families.insert(f);
        }
    }

    let hits = find_undefined(&units, hay);
    Found { hits, families }
}

/// 検査する単位。どこから来たかで、何を抽出してよいかが変わる。
#[derive(Clone, Copy, PartialEq)]
enum Kind {
    /// Rust の文字列リテラル。抽出 1・2・3 を全部かける
    RustLiteral,
    /// `class="..."` の中身そのもの。全トークンがクラスだと確定している
    ClassAttr,
    /// HTML の 1 行など、文脈が無いテキスト。形が一意なものだけ拾う
    Loose,
}

struct Unit {
    file: String,
    line: usize,
    text: String,
    kind: Kind,
}

/// HTML から検査単位を作る。
///
/// * `class="..."` の中身は `ClassAttr`（全トークンを見る）
/// * それ以外の行は `Loose`（色 / `z-数字` / `backdrop-*` だけ見る）
///
/// `<style>` は定義側なので使用側からは外す。中の宣言をクラスと誤認しないため。
/// `<script>` は残す。JS が付けるクラス（`el.className = "bg-sky-900/30"`）は
/// まさに段 1 で見たいもの。
fn html_units(path: &Path, src: &str) -> Vec<Unit> {
    let file = path.display().to_string().replace('\\', "/");
    // コメントと <style> を空白に潰す。長さを変えないので行番号がずれない
    let mut buf: Vec<u8> = src.as_bytes().to_vec();
    for (open, close) in [("<!--", "-->"), ("<style", "</style>")] {
        let mut from = 0usize;
        while let Some(a) = find_ci(&buf, open.as_bytes(), from) {
            let end = find_ci(&buf, close.as_bytes(), a + open.len())
                .map(|z| z + close.len())
                .unwrap_or(buf.len());
            for b in &mut buf[a..end] {
                if *b != b'\n' {
                    *b = b' ';
                }
            }
            from = end;
        }
    }
    let text = String::from_utf8_lossy(&buf).into_owned();

    let mut out = Vec::new();
    for (i, line) in text.lines().enumerate() {
        let no = i + 1;
        for v in class_attr_values(line) {
            out.push(Unit {
                file: file.clone(),
                line: no,
                text: v,
                kind: Kind::ClassAttr,
            });
        }
        out.push(Unit {
            file: file.clone(),
            line: no,
            text: line.to_string(),
            kind: Kind::Loose,
        });
    }
    out
}

fn find_ci(hay: &[u8], needle: &[u8], from: usize) -> Option<usize> {
    if needle.is_empty() || from >= hay.len() {
        return None;
    }
    (from..=hay.len().saturating_sub(needle.len())).find(|&i| {
        hay[i..i + needle.len()]
            .iter()
            .zip(needle)
            .all(|(a, b)| a.eq_ignore_ascii_case(b))
    })
}

/// `class="..."` / `class='...'` の中身を返す。テンプレートの `{{ }}` `{% %}` は
/// `drop_placeholders` が潰す。
fn class_attr_values(text: &str) -> Vec<String> {
    let mut out = Vec::new();
    let bytes = text.as_bytes();
    let mut i = 0usize;
    while let Some(p) = text[i..].find("class=") {
        let at = i + p + "class=".len();
        i = at;
        let Some(&q) = bytes.get(at) else { break };
        if q != b'"' && q != b'\'' {
            continue;
        }
        let Some(endrel) = text[at + 1..].find(q as char) else {
            break;
        };
        out.push(text[at + 1..at + 1 + endrel].to_string());
        i = at + 1 + endrel;
    }
    out
}

/// 検査単位の集まりから、干し草に定義の無いクラスを拾う。
///
/// `scan()` から切り出してあるのは、合成したソースを食わせて
/// 「わざと壊したら落ちること」を毎回の CI で確かめられるようにするため。
fn find_undefined(units: &[Unit], hay: &[u8]) -> Hits {
    let index = DotIndex::build(hay);
    let cache: RefCell<BTreeMap<String, bool>> = RefCell::new(BTreeMap::new());
    let is_defined = |tok: &str| -> bool {
        if let Some(&v) = cache.borrow().get(tok) {
            return v;
        }
        let v = index.contains(hay, tok);
        cache.borrow_mut().insert(tok.to_string(), v);
        v
    };

    let mut hits: Hits = BTreeMap::new();

    for unit in units {
        let content = &unit.text;
        let mut report: Vec<String> = Vec::new();

        // 抽出 1: class="..." に書かれたもの。全トークンがクラスだと確定している
        match unit.kind {
            // HTML から取り出した class 属性の中身そのもの
            Kind::ClassAttr => {
                for t in drop_placeholders(content).split_whitespace() {
                    if looks_like_class_name(t) {
                        report.push(t.to_string());
                    }
                }
            }
            // Rust のリテラルは、中から class="..." を探すところから
            Kind::RustLiteral => {
                for t in from_class_attr(content) {
                    if looks_like_class_name(&t) {
                        report.push(t);
                    }
                }
            }
            Kind::Loose => {}
        }

        // 抽出 2: 形が一意に決まるもの（色 / z-数字 / backdrop-*）。
        // ClassAttr は上で全部見ているので重複させない
        if unit.kind != Kind::ClassAttr {
            for t in candidate_tokens(content) {
                if is_unambiguous_utility(t) {
                    report.push(t.to_string());
                }
            }
        }

        // 抽出 3: クラス列にしか見えない文字列リテラル
        //   例: "bg-navy-800/60 border border-slate-500 rounded-lg p-4"
        //   タプルに入れて class="{card}" へ流し込む書き方が拾えないと
        //   このリポジトリでは大半を取りこぼす（`metric_card` がその形）。
        //
        //   ふるいは 3 つ。全部通らないとクラス列とみなさない。
        //     (1) 全トークンがクラス名の形。かつハイフン無しのトークンは
        //         Tailwind の素のユーティリティ（flex / border など）に限る。
        //         これが無いと `.expect("search assessment")` の英文が通ってしまう
        //         （`search` は dashboard.css に実在するので「半数以上定義済み」
        //          を単独で満たしてしまう）
        //     (2) ハイフンを含むトークンのうち 1 つ以上が CSS にある
        //     (3) 半数以上が CSS にある
        let toks: Vec<&str> = content.split_whitespace().collect();
        let shape_ok = unit.kind == Kind::RustLiteral
            && toks.len() >= 2
            && toks.iter().all(|t| {
                looks_like_class_name(t) && (t.contains('-') || BARE_UTILITIES.contains(t))
            });
        if shape_ok {
            let mut defined = 0usize;
            let mut hyphen_defined = 0usize;
            for t in &toks {
                if is_defined(t) {
                    defined += 1;
                    if t.contains('-') {
                        hyphen_defined += 1;
                    }
                }
            }
            if hyphen_defined >= 1 && defined * 2 >= toks.len() {
                for t in &toks {
                    report.push((*t).to_string());
                }
            }
        }

        for t in report {
            if is_defined(&t) {
                continue;
            }
            hits.entry(t)
                .or_default()
                .entry(unit.file.clone())
                .or_insert(unit.line);
        }
    }

    hits
}

fn render_failure(hits: &Hits, families: &BTreeSet<String>) -> String {
    let mut s = String::new();
    s.push_str(
        "\n配っている CSS に定義が無いクラスが書かれています。\n\
         この画面の Tailwind は静的 CSS なので、無いクラスは何も起こさず素通りします。\n\n",
    );
    // 在庫の無い色名を先に名指しする。置換先がその場で決まるように
    let mut missing_fams: BTreeSet<String> = BTreeSet::new();
    for tok in hits.keys() {
        if let Some(f) = color_family(tok) {
            if !families.contains(&f) {
                missing_fams.insert(f);
            }
        }
    }
    if !missing_fams.is_empty() {
        s.push_str(&format!(
            "  CSS に無い色名 : {}\n  使える色名     : {}\n\n",
            missing_fams.iter().cloned().collect::<Vec<_>>().join(", "),
            families.iter().cloned().collect::<Vec<_>>().join(", ")
        ));
    }
    for (tok, places) in hits {
        let where_ = places
            .iter()
            .take(3)
            .map(|(f, l)| format!("{f}:{l}"))
            .collect::<Vec<_>>()
            .join("  ");
        s.push_str(&format!("  {tok:<30} {where_}\n"));
    }
    s.push_str(&format!(
        "\n直し方は 2 つのどちらか。\n\
         \u{20} 1. 在庫のあるクラスに置き換える（上の「使える色名」を見る）\n\
         \u{20} 2. どうしても必要なら static/css/ に定義を足す\n\
         別担当が対応中などで今は直せない場合だけ、{ALLOWLIST_PATH} に\n\
         理由と日付を添えて 1 行足してください。直ったらその行を消します。\n"
    ));
    s
}

// ===========================================================================
// テスト
// ===========================================================================

/// 許可リストの自動生成部分を書き直す。`CSS_CLASS_ALLOWLIST=write` のときだけ。
///
/// 行番号は書かない。行番号を入れると無関係な編集のたびに差分が出て、
/// レビューで「本当に増えたのか」が読めなくなる。
fn rewrite_generated_section(head: &str, hits: &Hits, manual: &Allow) -> usize {
    let mut out = String::from(head);
    if !out.ends_with('\n') {
        out.push('\n');
    }
    out.push_str(GENERATED_MARKER);
    out.push('\n');
    out.push_str(
        "# 形式: <クラス名> <そのクラスを書いているファイル>\n\
         # 再生成: CSS_CLASS_ALLOWLIST=write cargo test --test css_classes_exist\n\
         # 1 件直すごとにここから消えるので、再生成して差分をコミットすれば減る。\n",
    );
    let mut n = 0usize;
    for (tok, places) in hits {
        for file in places.keys() {
            if manual.allows(tok, file) {
                continue;
            }
            out.push_str(&format!("{tok:<32}{file}\n"));
            n += 1;
        }
    }
    fs::write(ALLOWLIST_PATH, out).expect("許可リストを書けません");
    n
}

#[test]
fn cssに定義の無いクラスを書いていない() {
    let found = scan();
    let (head, manual, generated) = load_allowlist();

    // 色名の在庫が取れていないなら、照合そのものが壊れている。
    // 「何も拾えなくて緑」を潰すための番人
    assert!(
        found.families.len() >= 5,
        "tailwind-precompiled.css から色名をほとんど拾えていません。\
         CSS の場所か解析が壊れています（取れた色名: {:?}）",
        found.families
    );

    if std::env::var("CSS_CLASS_ALLOWLIST").as_deref() == Ok("write") {
        let n = rewrite_generated_section(&head, &found.hits, &manual);
        eprintln!(
            "{ALLOWLIST_PATH} の自動生成部分を書き直しました（{n} 行）。\
             差分を確認してコミットしてください。"
        );
        return;
    }

    // 許可されていない (クラス, ファイル) の組だけを残す
    let mut hits: Hits = BTreeMap::new();
    for (tok, places) in &found.hits {
        for (file, line) in places {
            if manual.allows(tok, file) || generated.allows(tok, file) {
                continue;
            }
            hits.entry(tok.clone())
                .or_default()
                .insert(file.clone(), *line);
        }
    }

    assert!(
        hits.is_empty(),
        "{}",
        render_failure(&hits, &found.families)
    );
}

/// 直ったのに自動生成部分に残っている行を知らせる。
///
/// 放っておくと許可リストが「昔あった問題の墓場」になり、
/// 新しく足された未定義クラスがその中に紛れて見えなくなる。
/// 落ちたら再生成コマンドを打つだけで直る。
///
/// # 手書き部分をここで見ない理由
/// 手書き部分には、段 1 の視界の外（`static/js/**` や、JS が組み立てて付ける
/// クラス）で使われているものも入る。ここで見ると「直っていないのに直ったと
/// 言う」ことになるので、手書き部分の後始末は実 DOM を読む段 2
/// (`scripts/audit_css_classes.js`) が受け持つ。
/// 自動生成部分は段 1 自身が作ったものなので、視界は完全で、ここで見てよい。
#[test]
fn 許可リストの自動生成部分に不要な行が残っていない() {
    if std::env::var("CSS_CLASS_ALLOWLIST").as_deref() == Ok("write") {
        return;
    }
    let found = scan();
    let (_, _, generated) = load_allowlist();

    let stale: Vec<String> = generated
        .scoped
        .iter()
        .filter(|(tok, file)| {
            found
                .hits
                .get(tok)
                .map(|places| !places.contains_key(file))
                .unwrap_or(true)
        })
        .map(|(tok, file)| format!("{tok}  {file}"))
        .collect();

    assert!(
        stale.is_empty(),
        "\n自動生成部分に不要な行が {} 件残っています（直ったのに消えていない）。\n\
         次を実行して {ALLOWLIST_PATH} の差分をコミットしてください:\n\
         \u{20}   CSS_CLASS_ALLOWLIST=write cargo test --test css_classes_exist\n\n  {}\n",
        stale.len(),
        stale.join("\n  ")
    );
}

/// 検査そのものが機能しているかを、逆から確かめる。
///
/// 「テストが緑」は「検査が働いている」ではない。照合が壊れて何も拾えなく
/// なっても緑になる。壊れていることを検出するために、実在しないクラスと
/// 実在するクラスの両方を通す。
#[test]
fn 照合が機能している() {
    let mut css_files = Vec::new();
    walk(Path::new("static/css"), "css", &mut css_files);
    let mut css = String::new();
    for p in &css_files {
        if let Ok(t) = fs::read_to_string(p) {
            css.push('\n');
            css.push_str(&t);
        }
    }
    let hay = css.as_bytes();

    // 事故のクラスは「無い」と言えること
    for t in ["text-rose-400", "bg-sky-900/30", "text-zzz-999"] {
        assert!(
            !defined_in_css(hay, t),
            "{t} は CSS に無いはずなのに「ある」と判定されました"
        );
    }
    // あるものは「ある」と言えること
    for t in ["text-slate-400", "bg-navy-800", "tabular-nums"] {
        assert!(
            defined_in_css(hay, t),
            "{t} は CSS にあるはずなのに「無い」と判定されました"
        );
    }
    // 前方一致で誤って当たらないこと
    assert!(
        !defined_in_css(b".bg-blue-500{color:red}", "bg-blue-5"),
        "bg-blue-5 が bg-blue-500 に誤って当たっています"
    );
    // `\` エスケープの有無どちらでも拾えること
    assert!(defined_in_css(b".bg-blue-500\\/20{}", "bg-blue-500/20"));
    assert!(defined_in_css(b".bg-blue-500/20{}", "bg-blue-500/20"));
    assert!(defined_in_css(
        b".hover\\:text-white:hover{}",
        "hover:text-white"
    ));
    assert!(defined_in_css(b".p-1\\.5{}", "p-1.5"));

    // 長いクラスの定義を、短いクラスの定義と取り違えないこと。
    // `.bg-amber-500\/10` は bg-amber-500/10 の定義であって bg-amber-500 のもの
    // ではない。ここを通していたせいで 20 種以上を「定義済み」と誤判定していた
    assert!(
        !defined_in_css(b".bg-amber-500\\/10{background:#f59e0b1a}", "bg-amber-500"),
        "bg-amber-500 が bg-amber-500/10 の定義に当たっています"
    );
    assert!(defined_in_css(b".bg-amber-500\\/10{}", "bg-amber-500/10"));
}

/// 以前 `src/lib.rs` にベタ書きされていた 5 個を、今の実装でも見ていること。
///
/// # 経緯
/// `src/lib.rs` に `css_utility_tests::utility_classes_used_for_layout_are_actually_defined`
/// があり、この 5 個だけを配列で持って「CSS に在るか」を見ていた。
/// 2026-09-10 に素通りした `sky-*` `rose-*` `z-20` `backdrop-blur` は 1 つも
/// 入っておらず、テストは緑のまま何も守っていなかった。
/// 本ファイルの汎用検査で置き換えたので、**置き換え前が見ていたものが
/// 抜けていないこと**をここで固定する。
///
/// 角括弧つきの任意値クラスは、記号を一律に弾く実装にすると真っ先に落ちる。
/// 守りたかったものだけが抜ける形になるので、明示的に押さえる。
#[test]
fn 旧lib_rsのベタ書き5個を今も見ている() {
    // かつて lib.rs が持っていた組（クラス名, CSS 上のエスケープ形）
    let required = [
        ("min-h-[44px]", r".min-h-\[44px\]"),
        ("mt-0.5", r".mt-0\.5"),
        ("gap-1.5", r".gap-1\.5"),
        ("text-[10px]", r".text-\[10px\]"),
        ("text-[11px]", r".text-\[11px\]"),
    ];

    for (name, selector) in required {
        // 1) クラス名として認識できること。ここで落ちると抽出されず素通りする
        assert!(
            looks_like_class_name(name),
            "{name} をクラス名と認識できていません（抽出されず素通りします）"
        );
        // 2) エスケープ形の定義を「ある」と言えること
        let css = format!("{selector}{{min-height:44px}}");
        assert!(
            defined_in_css(css.as_bytes(), name),
            "{name} が {selector} の定義に当たりません"
        );
        // 3) 定義が無ければ「無い」と言えること（逆証明）
        assert!(
            !defined_in_css(b".unrelated{}", name),
            "{name} が無関係な CSS に当たっています"
        );
    }

    // 4) 実際に配っている CSS で、今この 5 個が定義されていること。
    //    旧テストが見ていたのはここ。同じ結論を出せることを確かめる
    let mut css_files = Vec::new();
    walk(Path::new("static/css"), "css", &mut css_files);
    let mut css = String::new();
    for p in &css_files {
        if let Ok(t) = fs::read_to_string(p) {
            css.push('\n');
            css.push_str(&t);
        }
    }
    for (name, _) in required {
        assert!(
            defined_in_css(css.as_bytes(), name),
            "クラス {name} が CSS に定義されていません（HTML で使っても効きません）"
        );
    }

    // 5) 使用側でも拾えること。class 属性に書けば検出経路に乗る
    let hay = b".text-\\[10px\\]{font-size:10px}";
    let hits = find_undefined(
        &[Unit {
            file: "src/probe.rs".into(),
            line: 1,
            text: "<div class=\"text-[10px] min-h-[44px]\">".into(),
            kind: Kind::RustLiteral,
        }],
        hay,
    );
    assert!(
        hits.contains_key("min-h-[44px]"),
        "定義の無い任意値クラスを見逃しました: {hits:?}"
    );
    assert!(
        !hits.contains_key("text-[10px]"),
        "定義のある任意値クラスを誤検出しました: {hits:?}"
    );
}

/// 壊したソースを食わせて「落ちること」を確かめる（逆証明）。
///
/// 実際のソースに `sky-*` を 1 つ足して落ちるのを手で確認したうえで、
/// その確認を毎回の CI で繰り返せるように合成ソースで固定してある。
/// 検査が黙って無力化されたら、ここが落ちる。
#[test]
fn わざと壊したら落ちる() {
    // 干し草は「本物の Tailwind が持っているもの」だけを並べた最小の CSS。
    // sky と rose はここに無い（本番の tailwind-precompiled.css と同じ状況）
    let css = b".text-slate-400{color:#94a3b8}.bg-navy-800{background:#0f172a}\
                .border{border-width:1px}.rounded-lg{border-radius:.5rem}\
                .p-4{padding:1rem}.text-sm{font-size:.875rem}.mt-1{margin-top:.25rem}\
                .tabular-nums{font-variant-numeric:tabular-nums}";

    let lit = |line: usize, s: &str| Unit {
        file: "src/probe.rs".into(),
        line,
        text: s.to_string(),
        kind: Kind::RustLiteral,
    };

    // (1) class 属性に直接書いた場合
    let hits = find_undefined(&[lit(1, "<div class=\"text-sm mt-1\">")], css);
    assert!(hits.is_empty(), "正しいクラスで落ちています: {hits:?}");

    let hits = find_undefined(&[lit(1, "<div class=\"text-sm z-20\">")], css);
    assert!(hits.contains_key("z-20"), "z-20 を見逃しました: {hits:?}");

    // (2) 条件で組み立てる断片（事故の本体）。class 属性の外にある
    let hits = find_undefined(&[lit(7, "text-rose-400")], css);
    assert!(
        hits.contains_key("text-rose-400"),
        "class 属性の外の色クラスを見逃しました: {hits:?}"
    );

    // (3) タプルに入れて流し込むクラス列。1 つだけ在庫の無い色に差し替える
    let ok = "bg-navy-800 border rounded-lg p-4";
    let hits = find_undefined(&[lit(3, ok)], css);
    assert!(hits.is_empty(), "正しいクラス列で落ちています: {hits:?}");

    let broken = "bg-navy-800 border border-sky-500 rounded-lg p-4";
    let hits = find_undefined(&[lit(3, broken)], css);
    assert!(
        hits.contains_key("border-sky-500"),
        "クラス列に混ぜた sky を見逃しました: {hits:?}"
    );
    assert_eq!(
        hits.len(),
        1,
        "壊したのは 1 つだけなのに他も落ちています: {hits:?}"
    );

    // (4) 誤検出しないこと。expect のメッセージや style 宣言はクラスではない
    for noise in [
        ".expect(\"search assessment\")",
        "error key missing in citycode-absent case",
        "text-align:right",
        "min-width:720px",
        "overflow-x:auto",
        "SELECT name FROM t",
    ] {
        let hits = find_undefined(&[lit(9, noise)], css);
        assert!(
            hits.is_empty(),
            "{noise:?} をクラスと誤認しました: {hits:?}"
        );
    }

    // (5) テンプレート側。class 属性と、<style> を使用側から外していること
    let html = "<div class=\"p-4 text-sm\">a</div>\n\
                <div class=\"p-4 bg-sky-700\">b</div>\n\
                <style>.p-4{padding:1rem}</style>\n\
                <script>el.className = 'text-rose-400';</script>\n";
    let hits = find_undefined(&html_units(Path::new("templates/t.html"), html), css);
    assert!(
        hits.contains_key("bg-sky-700"),
        "テンプレートの class 属性を見逃しました: {hits:?}"
    );
    assert!(
        hits.contains_key("text-rose-400"),
        "テンプレートの <script> が付けるクラスを見逃しました: {hits:?}"
    );
    assert!(
        !hits.contains_key("p-4"),
        "定義のあるクラスを誤検出しました: {hits:?}"
    );
    assert_eq!(hits.len(), 2, "想定外のものまで落ちています: {hits:?}");

    // テンプレートの {{ }} {% %} をクラスと誤認しないこと
    let tpl = "<div class=\"p-4 {{EXTRA}}\">x</div>\n\
               <div class=\"{% if a %}p-4{% endif %}\">y</div>\n";
    let hits = find_undefined(&html_units(Path::new("templates/t.html"), tpl), css);
    assert!(
        hits.is_empty(),
        "テンプレートの置換記法をクラスと誤認しました: {hits:?}"
    );

    // HTML コメントの中は見ないこと
    let commented = "<!-- <div class=\"bg-sky-700\"></div> -->\n";
    let hits = find_undefined(&html_units(Path::new("templates/t.html"), commented), css);
    assert!(hits.is_empty(), "HTML コメントの中を拾っています: {hits:?}");
}

/// 形の判定が意図通りか。ここが緩むと誤検出が出る。
#[test]
fn クラスらしさの判定が意図通り() {
    // 色ユーティリティとして拾うもの
    assert_eq!(color_family("text-rose-400").as_deref(), Some("rose"));
    assert_eq!(color_family("bg-sky-900/30").as_deref(), Some("sky"));
    assert_eq!(color_family("hover:bg-slate-700").as_deref(), Some("slate"));
    assert_eq!(color_family("border-t-slate-700").as_deref(), Some("slate"));
    // 色ではないもの
    assert_eq!(color_family("text-sm").as_deref(), None);
    assert_eq!(color_family("text-2xl").as_deref(), None);
    assert_eq!(color_family("w-1/2").as_deref(), None);
    assert_eq!(color_family("to-100").as_deref(), None);

    // 事故で素通りした残り 2 つ
    assert!(is_unambiguous_utility("z-20"));
    assert!(is_unambiguous_utility("backdrop-blur"));

    // CSS 宣言をクラスと誤認しない（未知の修飾子は弾く）
    assert!(!looks_like_class_name("text-align:right"));
    assert!(!looks_like_class_name("min-width:720px"));
    assert!(!looks_like_class_name("Uppercase"));
    // 正しいクラスは通す
    assert!(looks_like_class_name("sm:gap-4"));
    assert!(looks_like_class_name("p-1.5"));
    assert!(looks_like_class_name("bg-navy-800/60"));
}

/// 文字列リテラルの切り出しが `'"'` やコメントで壊れないこと。
#[test]
fn 文字列リテラルの切り出しが壊れない() {
    let src = concat!(
        "// コメントの中の \" は無視する\n",
        "fn esc(c: char) -> &'static str {\n",
        "    match c { '\"' => \"&quot;\", '\\\\' => \"x\", _ => \"y\" }\n",
        "}\n",
        "/* ブロック \" コメント */\n",
        "const A: &str = \"bg-navy-800/60 border\";\n",
        "const B: &str = \"<div class=\\\"kpi-card\\\">\";\n",
        "const C: &str = r#\"raw \" string\"#;\n"
    );
    let lits: Vec<String> = string_literals(src).into_iter().map(|(_, s)| s).collect();
    assert!(
        lits.iter().any(|s| s == "bg-navy-800/60 border"),
        "通常の文字列が取れていません: {lits:?}"
    );
    assert!(
        lits.iter().any(|s| s == "<div class=\"kpi-card\">"),
        "エスケープを戻せていません: {lits:?}"
    );
    assert!(
        lits.iter().any(|s| s == "raw \" string"),
        "生文字列が取れていません: {lits:?}"
    );
    assert!(
        !lits.iter().any(|s| s.contains("コメント")),
        "コメントを文字列として拾っています: {lits:?}"
    );
    // class 属性の抜き出し
    assert_eq!(
        from_class_attr("<div class=\"{td} tabular-nums {jc}\">"),
        vec!["tabular-nums".to_string()]
    );
}
