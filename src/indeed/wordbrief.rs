//! 求人票を書くときに使う「求職者が実際に打っている語」の手引き（試作 2026-09-24）。
//!
//! # なぜ要るか
//! 求人票生成（`src/job_gen/`）の工程③ペルソナ設計と工程⑦ 84 列原稿は、
//! 入力が **① 元の求人票の事実 + ② その原稿から読んだ市場分析** だけで、
//! Indeed のデータが 1 行も入っていない（`grep -rn 'indeed' src/job_gen/` で出るのは
//! 84 列の列名の定義だけ）。「Indeed表示職種名」という列があるのに、
//! Indeed で実際に何が打たれているかを見ずに書いている。
//!
//! ここはその橋渡し。14 か月ぶんの検索語シェアから、
//! **どの語に寄せるか**をデータだけで決める。
//!
//! # 判定は LLM に渡さない
//! 同じリポジトリの既存規律に合わせる。
//!
//! > `coverage_gate.rs:11` 判定方式: LLM を使わない純粋関数
//! > `claim_audit.rs:13` 判定（差し戻すか否か）は LLM ではなくコード側で確定させる
//!
//! この module が決めるのは「どの語が伸びていて、どの語が落ちているか」まで。
//! LLM（M3）がやるのは、確定した語を使って文章を書くことだけ。
//!
//! # 出さないもの
//! 検索語には社名・施設名・公的機関名が混ざる（実測 3,086 語中 205 語 = 6.6%）。
//! 顧客向けの素材に他社名が出るのは避ける必要があるので、この module で落とす。

use crate::db::local_sqlite::LocalDb;
use crate::handlers::helpers::{get_f64_opt, get_i64_opt, get_str};

/// 動きとして扱う最小の幅（ポイント）。
///
/// 14 か月で 3pt 未満の動きは、月ごとの揺れと区別が付かない。
/// シェアは上位 10 語しか取れていないので、11 位の語が 10 位に入れ替わるだけで
/// 1〜2pt は動く。
pub const MIN_DIFF_PT: f64 = 3.0;

/// 向きの一貫度の下限。
///
/// 隣り合う月の差のうち、全体の向きと同じものの割合。
/// 0.6 は「14 か月 13 区間のうち 8 区間以上が同じ向き」にあたる。
pub const MIN_MONOTONIC: f64 = 0.6;

/// 雇用形態の語。職種名ではないので別枠にする。
///
/// 「アルバイト」が複数職種で一斉に伸びているのが実測で分かっており、
/// これは職種名を変える話ではなく**雇用形態の書き方**の話になる。
/// 求人票の直しやすさが違うので、読む側に区別が要る。
pub const KOYOU_TERMS: [&str; 8] = [
    "アルバイト",
    "正社員",
    "パート",
    "派遣",
    "契約社員",
    "業務委託",
    "日雇い",
    "短期",
];

/// 顧客向けに出さない語の印。
///
/// 社名・施設名・公的機関名。`insight_pref_unique` の上位は
/// 「米子鬼太郎空港」「隠岐汽船株式会社」のようなものが並ぶ。
const NG_MARKS: [&str; 10] = [
    "株式会社",
    "有限会社",
    "合同会社",
    "公共職業安定所",
    "ハローワーク",
    "空港",
    "大学",
    "市役所",
    "役場",
    "センター",
];

/// 語の種類。求人票のどこに書く語なのかが違う。
///
/// # なぜ仕分けが要るか
/// 仕分けずに上位語をそのまま並べると、こうなる（実測）:
///
/// ```text
/// ホールスタッフ:「カフェ」「飲食店」「ネイルok」「カフェスタッフ」「高校生ok アルバイト」
///                 → 「職種名はこの語に寄せる」
/// ```
///
/// 「ネイルok」を職種名にすることはない。これは**条件欄**に書く語で、
/// 「アルバイト」は**雇用形態欄**に書く語。書く場所が違うので混ぜて渡せない。
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Kind {
    /// 職種名の候補。求人票の職種名に使う
    Title,
    /// 雇用形態。正社員／アルバイト等の欄に使う
    Koyou,
    /// 働き方の条件・歓迎する人。条件欄や歓迎欄に使う
    Joken,
}

/// 働き方の条件・歓迎する人を表す語の印。
///
/// 実データの上位語から拾った。`insight_kw_attr_trend` の「条件」属性が
/// 全職種で非ゼロ（最大 88.6%・平均 34.9%）なのと対応する。
const JOKEN_MARKS: [&str; 34] = [
    "土日",
    "祝休",
    "休み",
    "シフト",
    "未経験",
    "経験不問",
    "資格不問",
    "学歴不問",
    "髪色",
    "ネイル",
    "ピアス",
    "服装",
    "在宅",
    "リモート",
    "週1",
    "週2",
    "週3",
    "短時間",
    "夜勤",
    "日勤",
    "日払い",
    "週払い",
    "送迎",
    "車通勤",
    "駅近",
    "残業",
    "高収入",
    "高時給",
    "主婦",
    "主夫",
    "シニア",
    "高校生",
    "大学生",
    "フリーター",
];

/// 年代の語（「50代」「60代」）。数字を含むので別に見る。
fn is_nendai(term: &str) -> bool {
    let b: Vec<char> = term.chars().collect();
    b.windows(2).any(|w| w[0].is_ascii_digit() && w[1] == '代')
}

/// この語を求人票のどこに書くか。
///
/// # 優先順位
/// 条件 > 雇用形態 > 職種名。
/// 「主婦パート」は雇用形態語（パート）も条件語（主婦）も含むが、
/// これは「主婦歓迎のパート」を探している人の語なので条件として扱う。
/// 「事務パート」は条件語を含まないので雇用形態つきになる。
pub fn kind_of(term: &str) -> Kind {
    if is_nendai(term) || JOKEN_MARKS.iter().any(|m| term.contains(m)) {
        return Kind::Joken;
    }
    if KOYOU_TERMS.iter().any(|m| term.contains(m)) {
        return Kind::Koyou;
    }
    Kind::Title
}

/// この語を顧客向けに出してよいか。
pub fn is_safe_term(term: &str) -> bool {
    !NG_MARKS.iter().any(|m| term.contains(m))
}

/// 向きの一貫度。隣り合う月の差のうち、全体の向きと同じものの割合。
///
/// 全体が動いていない（始と終が同じ）ときは 0 を返す。
/// 値が 3 つ未満なら判定できないので 0 を返す。
pub fn monotonic(vals: &[f64]) -> f64 {
    if vals.len() < 3 {
        return 0.0;
    }
    let overall = vals[vals.len() - 1] - vals[0];
    if overall == 0.0 {
        return 0.0;
    }
    let up = overall > 0.0;
    let diffs: Vec<f64> = vals.windows(2).map(|w| w[1] - w[0]).collect();
    let same = diffs
        .iter()
        .filter(|d| **d != 0.0 && (**d > 0.0) == up)
        .count();
    same as f64 / diffs.len() as f64
}

/// 動いた語 1 つ。
#[derive(Debug, Clone, PartialEq)]
pub struct Moved {
    pub term: String,
    pub start_pct: f64,
    pub end_pct: f64,
    pub diff_pt: f64,
    /// 0.0〜1.0
    pub monotonic: f64,
    /// 雇用形態の語か（職種名ではない）
    pub is_koyou: bool,
}

/// いま打たれている語 1 つ。
#[derive(Debug, Clone, PartialEq)]
pub struct Current {
    pub term: String,
    pub pct: f64,
    pub clicks: i64,
}

/// 職種 1 つぶんの手引き。
#[derive(Debug, Clone, Default)]
pub struct WordBrief {
    pub title: String,
    pub first_month: String,
    pub last_month: String,
    pub months: usize,
    /// 直近月でシェアの大きい語（雇用形態の語と伏せる語を除く）
    pub current: Vec<Current>,
    pub rising: Vec<Moved>,
    pub falling: Vec<Moved>,
    /// 伏せた語の数。0 でなければ画面に断りを出す
    pub hidden_terms: usize,
}

/// 月ごとのシェア表から、動いた語を拾う。
///
/// `series` は (語, 月ごとのシェア) で、各 Vec は同じ長さ・同じ月の並びであること。
/// 欠けている月がある語は落とす（始と終を比べられないため）。
///
/// # 入力を作る側の責任
/// 月の並びは呼ぶ側が揃える。ここでは並びの正しさを見ない。
pub fn pick_moved(series: &[(String, Vec<Option<f64>>)]) -> (Vec<Moved>, Vec<Moved>) {
    let mut moved: Vec<Moved> = Vec::new();
    for (term, vals) in series {
        if !is_safe_term(term) {
            continue;
        }
        if vals.iter().any(|v| v.is_none()) || vals.len() < 3 {
            continue;
        }
        let v: Vec<f64> = vals.iter().map(|x| x.unwrap()).collect();
        let diff = v[v.len() - 1] - v[0];
        let mono = monotonic(&v);
        if diff.abs() < MIN_DIFF_PT || mono < MIN_MONOTONIC {
            continue;
        }
        moved.push(Moved {
            term: term.clone(),
            start_pct: v[0],
            end_pct: v[v.len() - 1],
            diff_pt: diff,
            monotonic: mono,
            is_koyou: KOYOU_TERMS.contains(&term.as_str()),
        });
    }
    // 動きの大きい順。同じなら語順で固定する（並びが実行ごとに変わらないように）
    moved.sort_by(|a, b| {
        b.diff_pt
            .abs()
            .partial_cmp(&a.diff_pt.abs())
            .unwrap_or(std::cmp::Ordering::Equal)
            .then_with(|| a.term.cmp(&b.term))
    });
    let rising = moved.iter().filter(|m| m.diff_pt > 0.0).cloned().collect();
    let falling = moved.iter().filter(|m| m.diff_pt < 0.0).cloned().collect();
    (rising, falling)
}

impl WordBrief {
    /// 求人票を書く人へ渡す文。
    ///
    /// # 語の種類ごとに行き先が違う
    /// 職種名・雇用形態・条件を混ぜて「この語に寄せる」と渡すと、
    /// 「ネイルok」が職種名になる。仕分けてから、それぞれの行き先を言う。
    ///
    /// # 数字は書かない
    /// 工程⑦の `validate_generated` は**原文に無い数字**を弾く。
    /// ここで「45.3% → 26.8%」と書くと、その数字が原稿に混ざったときに
    /// 弾かれるか、弾かれずに出て根拠のない数字になる。どちらも困る。
    /// なので渡すのは**語の並びと向きだけ**にして、数字は画面側にだけ出す。
    pub fn guide_line(&self) -> String {
        let mut parts: Vec<String> = Vec::new();

        let by = |k: Kind| -> Vec<&str> {
            self.current
                .iter()
                .filter(|c| kind_of(&c.term) == k)
                .map(|c| c.term.as_str())
                .collect()
        };
        let titles = by(Kind::Title);
        let koyou_now = by(Kind::Koyou);
        // 条件の語には「主婦パート」のように、求職者は打つが求人票には
        // 書けないものが混ざる（性別差別表示）。渡すと工程⑦の NG ワード検査で
        // その列がまるごと空になるので、渡す前に落とす。画面には残す。
        let joken: Vec<&str> = by(Kind::Joken)
            .into_iter()
            .filter(|t| is_writable(t))
            .collect();

        if !titles.is_empty() {
            parts.push(format!(
                "この職種を探している人が実際に打っている職種名は「{}」。求人票の職種名はこの語に寄せる",
                titles.join("」「")
            ));
        }
        if !koyou_now.is_empty() {
            parts.push(format!(
                "職種名と雇用形態を並べた「{}」でも探されている",
                koyou_now.join("」「")
            ));
        }
        if !joken.is_empty() {
            parts.push(format!(
                "条件として打たれているのは「{}」。当てはまるものは条件欄に書く（職種名には入れない）",
                joken.join("」「")
            ));
        }

        // 向き。職種名の語だけを見る（条件語の増減は別の話なので混ぜない）
        let rise: Vec<&str> = self
            .rising
            .iter()
            .filter(|m| kind_of(&m.term) == Kind::Title)
            .map(|m| m.term.as_str())
            .collect();
        if !rise.is_empty() {
            parts.push(format!(
                "打たれる回数が増えている職種名は「{}」",
                rise.join("」「")
            ));
        }
        let fall: Vec<&str> = self
            .falling
            .iter()
            .filter(|m| kind_of(&m.term) == Kind::Title)
            .map(|m| m.term.as_str())
            .collect();
        if !fall.is_empty() {
            parts.push(format!(
                "減っている職種名は「{}」なので、これだけに頼らない",
                fall.join("」「")
            ));
        }
        let koyou_rise: Vec<&str> = self
            .rising
            .iter()
            .filter(|m| kind_of(&m.term) == Kind::Koyou)
            .map(|m| m.term.as_str())
            .collect();
        if !koyou_rise.is_empty() {
            parts.push(format!(
                "雇用形態では「{}」で探す人が増えているので、当てはまるなら明記する",
                koyou_rise.join("」「")
            ));
        }

        if parts.is_empty() {
            return String::new();
        }
        parts.join("。") + "。"
    }
}

/// NG ワード辞書。1 度だけ読む。
///
/// 求人票生成の本番側（`handlers.rs:242` `load_ng_rules`）は
/// 外部ファイルでの差し替えを許しているが、ここは埋め込みだけを見る。
/// 「渡す前に落とす」ための予備の関門なので、本番の判定より緩くても
/// 危険側には倒れない（本番側が最終的に弾く）。
fn ng_rules() -> Option<&'static crate::job_gen::ng_words::NgRules> {
    static RULES: std::sync::OnceLock<Option<crate::job_gen::ng_words::NgRules>> =
        std::sync::OnceLock::new();
    RULES
        .get_or_init(|| {
            const JSON: &str = include_str!("../../assets/ng_words.json");
            match crate::job_gen::ng_words::NgRules::load_from_str(JSON) {
                Ok(r) => Some(r),
                Err(e) => {
                    tracing::error!("ng_words.json を読めませんでした: {e}");
                    None
                }
            }
        })
        .as_ref()
}

/// この語を求人票に書けるか。辞書が読めなければ「書ける」と答える
/// （落とし過ぎて材料が消えるより、本番側の関門に任せる）。
pub fn is_writable(term: &str) -> bool {
    match ng_rules() {
        Some(r) => writable_in_posting(term, r),
        None => true,
    }
}

/// 求職者は打つが、求人票には書けない語を除ける。
///
/// # なぜ要るか
/// 検索語の上位には「主婦パート」のような語が出る。求職者が実際に打っている語だが、
/// **求人票にそのまま書くと法令上の NG になる**。`assets/ng_words.json` の
/// 24 グループのうち「主婦・ママ」は `standalone: true`（単独で出ただけで違反・
/// 性別差別表示）、「シニア」は `standalone: false`（「歓迎」「活躍中」が
/// 後方 12 文字以内にあれば違反）。
///
/// 求人票生成の工程⑦は `validate_generated` で NG ワードを弾き、
/// 引っかかった列は**値を空にして `review_required`** にする
/// （`hrhacker.rs:283`）。つまりこの語をそのまま渡すと、
/// 生成された原稿がまるごと空になる。渡す前に落とす。
///
/// # 辞書は写さない
/// ここで「主婦」「シニア」と書き写すと、辞書が更新されたときにずれる。
/// `crate::job_gen::ng_words::NgRules::detect` をそのまま呼ぶ。
///
/// # 画面には出す
/// 落とすのは**LLM へ渡す文**からだけ。画面には「求職者は打っているが
/// 求人票には書けない語」として出す。営業が知っておく価値のある事実で、
/// 隠すと「なぜこの語が出てこないのか」が分からなくなる。
pub fn writable_in_posting(term: &str, rules: &crate::job_gen::ng_words::NgRules) -> bool {
    rules.detect(term).is_empty()
}

#[cfg(test)]
mod ng_tests {
    use super::*;

    const NG_JSON: &str = include_str!("../../assets/ng_words.json");

    fn rules() -> crate::job_gen::ng_words::NgRules {
        crate::job_gen::ng_words::NgRules::load_from_str(NG_JSON)
            .expect("ng_words.json を読めること")
    }

    #[test]
    fn 求人票に書けない語を見分ける() {
        let r = rules();
        // 検索語の上位に実際に出るもの
        assert!(!writable_in_posting("主婦パート", &r), "主婦は単独で違反");
        // 問題のないもの
        assert!(writable_in_posting("一般事務", &r));
        assert!(writable_in_posting("土日祝休み", &r));
        assert!(writable_in_posting("未経験", &r));
        assert!(writable_in_posting("カフェ", &r));
    }

    #[test]
    fn 辞書を写さずに判定している() {
        // この module に語を書き写していないこと。
        // 写すと辞書の更新に追随できなくなる。
        let src = include_str!("wordbrief.rs");
        let body = src
            .split("pub fn writable_in_posting")
            .nth(1)
            .expect("関数がある");
        let body = &body[..body.find("\n}").expect("関数の終わり")];
        assert!(
            !body.contains("主婦") && !body.contains("シニア"),
            "判定の中に辞書の語を書き写している: {body}"
        );
    }
}

/// 手引きに出てくる職種名の語をすべて並べる（いまの語 + 動いた語）。
///
/// 「短いほうの語が偶然当たっただけ」を見分けるのに使う。
fn b_terms(b: &WordBrief) -> impl Iterator<Item = String> + '_ {
    b.current
        .iter()
        .map(|c| c.term.clone())
        .chain(b.rising.iter().map(|m| m.term.clone()))
        .chain(b.falling.iter().map(|m| m.term.clone()))
        .filter(|t| kind_of(t) == Kind::Title)
}

/// 監査の指摘 1 件。
#[derive(Debug, Clone, PartialEq)]
pub struct Finding {
    /// 画面に出す一文
    pub text: String,
    /// 根拠になった語
    pub term: String,
    /// 強さ。2 = 直したほうがよい、1 = 見ておく
    pub level: u8,
}

/// いま出している求人票の職種名を、求職者が打っている語と突き合わせる。
///
/// # なぜ生成ではなく監査なのか
/// 同じ原文で「Indeed の語を渡す／渡さない」を対にして 3 職種 × 4 回ずつ測った
/// （2026-09-24、MiniMax-M3）:
///
/// ```text
/// 職種            条件        打たれている語を含む  伸びている語を含む  条件語の混入
/// 事務            語なし             4/4              3/4            0
/// 事務            語あり             4/4              4/4            1
/// ホールスタッフ   語なし             4/4              0/4            0
/// ホールスタッフ   語あり             4/4              0/4            0
/// 配送ドライバー   語なし             4/4              0/4            0
/// 配送ドライバー   語あり             4/4              0/4            0
/// ```
///
/// **職種名を書かせる用途では差が出なかった。** LLM は「一般事務」「カフェ」
/// を既に知っていて、渡さなくても当てる。渡したほうが 1 回、条件語を
/// 職種名に混ぜて悪化した（「一般事務パート・アルバイト｜土日祝休み」）。
///
/// 一方、**いま出ている求人票の職種名が市場とずれているか**は LLM には判断できない。
/// 「事務」が 14 か月で 45.3% → 26.8% に落ちていることは、データにしか無い。
/// そこでこの関数は生成側ではなく**監査側**に置く。LLM を使わない。
///
/// # 引数
/// `posted` は求人票にいま書いてある職種名。
pub fn audit_title(posted: &str, b: &WordBrief) -> Vec<Finding> {
    let mut out = Vec::new();
    if posted.trim().is_empty() || b.current.is_empty() {
        return out;
    }

    // 1) いま書いてある語が、落ち続けている語そのものか
    for m in &b.falling {
        if kind_of(&m.term) != Kind::Title {
            continue;
        }
        if !posted.contains(&m.term) {
            continue;
        }
        // 「一般事務スタッフ」は「事務」を含む。長いほうの語を使っているなら、
        // 短いほうが当たったのは偶然なので指摘しない。
        // 比べる相手は「その語を丸ごと含む、もっと長い職種名の語」。
        let longer_used = b_terms(b)
            .filter(|t| t.len() > m.term.len() && t.contains(&m.term))
            .any(|t| posted.contains(&t));
        if longer_used {
            continue;
        }
        {
            // 代わりに伸びている語があるなら、それも言う
            let alt = b
                .rising
                .iter()
                .find(|r| kind_of(&r.term) == Kind::Title)
                .map(|r| r.term.clone());
            let text = match &alt {
                Some(a) => format!(
                    "職種名に使っている「{t}」は {n} か月で {s:.1}% → {e:.1}% に落ちています。\
                     代わりに「{a}」が {rs:.1}% → {re:.1}% に伸びています。",
                    t = m.term,
                    n = b.months,
                    s = m.start_pct,
                    e = m.end_pct,
                    a = a,
                    rs = b.rising[0].start_pct,
                    re = b.rising[0].end_pct,
                ),
                None => format!(
                    "職種名に使っている「{t}」は {n} か月で {s:.1}% → {e:.1}% に落ちています。",
                    t = m.term,
                    n = b.months,
                    s = m.start_pct,
                    e = m.end_pct
                ),
            };
            out.push(Finding {
                text,
                term: m.term.clone(),
                level: 2,
            });
        }
    }

    // 2) いちばん打たれている語が職種名に入っていないか
    //
    // 1 位の語だけを見る。2 位以下まで見ると、どの職種でも何かしら出て
    // 「毎回警告が出る」状態になり、読まれなくなる。
    if let Some(top) = b.current.iter().find(|c| kind_of(&c.term) == Kind::Title) {
        if !posted.contains(&top.term) {
            out.push(Finding {
                text: format!(
                    "いちばん打たれている「{t}」（{p:.1}%・{c} クリック）が職種名に入っていません。",
                    t = top.term,
                    p = top.pct,
                    c = top.clicks
                ),
                term: top.term.clone(),
                level: 2,
            });
        }
    }

    // 3) 条件の語を職種名に入れていないか
    for c in b.current.iter().filter(|c| kind_of(&c.term) == Kind::Joken) {
        if posted.contains(&c.term) {
            out.push(Finding {
                text: format!(
                    "「{t}」は条件の語です。職種名ではなく条件欄に書くほうが、\
                     職種で探している人にも条件で探している人にも当たります。",
                    t = c.term
                ),
                term: c.term.clone(),
                level: 1,
            });
        }
    }

    // 4) 雇用形態を並べた語で探されているのに、職種名に雇用形態が無い
    if let Some(k) = b.current.iter().find(|c| kind_of(&c.term) == Kind::Koyou) {
        let has_koyou = KOYOU_TERMS.iter().any(|t| posted.contains(t));
        if !has_koyou {
            out.push(Finding {
                text: format!(
                    "「{t}」のように職種名と雇用形態を並べて探す人がいます（{p:.1}%）。\
                     当てはまるなら職種名に雇用形態を足す余地があります。",
                    t = k.term,
                    p = k.pct
                ),
                term: k.term.clone(),
                level: 1,
            });
        }
    }

    out
}

/// 職種 1 つぶんを DB から組み立てる。見つからなければ `None`。
///
/// # 月の並びを揃える
/// 語ごとに出ている月が違う（上位 10 語しか取れていないので、
/// 11 位に落ちた月は行が無い）。月の並びは**その職種に 1 行でもある月の和**を取り、
/// 行の無い月は `None` にする。`pick_moved` が欠けた月のある語を落とすので、
/// 「14 か月ずっと上位 10 位に入っていた語」だけが動きの判定に残る。
///
/// ここを「その語に行がある月だけ」で判定すると、
/// 3 か月しか出ていない語が 14 か月の傾向として並んでしまう。
pub fn load(db: &LocalDb, title: &str) -> Result<Option<WordBrief>, String> {
    let rows = db.query(
        "SELECT report_month, search_term, share_pct, clicks \
         FROM insight_kw_term_monthly WHERE norm_title = ?1 \
         ORDER BY report_month, search_term",
        &[&title],
    )?;
    if rows.is_empty() {
        return Ok(None);
    }

    let mut months: Vec<String> = Vec::new();
    let mut per_term: std::collections::BTreeMap<
        String,
        std::collections::HashMap<String, (f64, i64)>,
    > = std::collections::BTreeMap::new();
    for r in &rows {
        let m = get_str(r, "report_month");
        if months.last().map(|x| x != &m).unwrap_or(true) && !months.contains(&m) {
            months.push(m.clone());
        }
        let term = get_str(r, "search_term");
        if term.is_empty() {
            continue;
        }
        let share = get_f64_opt(r, "share_pct").unwrap_or(0.0);
        let clicks = get_i64_opt(r, "clicks").unwrap_or(0);
        per_term.entry(term).or_default().insert(m, (share, clicks));
    }
    months.sort();
    if months.len() < 3 {
        return Ok(None);
    }
    let latest = months[months.len() - 1].clone();
    let first = months[0].clone();

    let series: Vec<(String, Vec<Option<f64>>)> = per_term
        .iter()
        .map(|(t, by_month)| {
            (
                t.clone(),
                months
                    .iter()
                    .map(|m| by_month.get(m).map(|(s, _)| *s))
                    .collect(),
            )
        })
        .collect();
    let (rising, falling) = pick_moved(&series);

    // 直近月に出ている語。雇用形態の語は「いま打たれている職種名」ではないので外す。
    let mut current: Vec<Current> = per_term
        .iter()
        .filter(|(t, _)| is_safe_term(t) && !KOYOU_TERMS.contains(&t.as_str()))
        .filter_map(|(t, by_month)| {
            by_month.get(&latest).map(|(s, c)| Current {
                term: t.clone(),
                pct: *s,
                clicks: *c,
            })
        })
        .filter(|c| c.pct > 0.0)
        .collect();
    current.sort_by(|a, b| {
        b.pct
            .partial_cmp(&a.pct)
            .unwrap_or(std::cmp::Ordering::Equal)
            .then_with(|| a.term.cmp(&b.term))
    });
    current.truncate(6);

    let hidden = per_term.keys().filter(|t| !is_safe_term(t)).count();

    Ok(Some(WordBrief {
        title: title.to_string(),
        first_month: first,
        last_month: latest,
        months: months.len(),
        current,
        rising: rising.into_iter().take(5).collect(),
        falling: falling.into_iter().take(5).collect(),
        hidden_terms: hidden,
    }))
}

#[cfg(test)]
mod tests {
    use super::*;

    // データを見ない検査にする。実データの中身が変わっても通る/落ちる条件が変わらないこと。

    #[test]
    fn 一本調子に上がる列は一貫度が一になる() {
        assert_eq!(monotonic(&[1.0, 2.0, 3.0, 4.0]), 1.0);
    }

    #[test]
    fn 行って戻る列は一貫度が下がる() {
        // 1→5→1 は最後が始点と同じなので全体の動きが 0
        assert_eq!(monotonic(&[1.0, 5.0, 1.0]), 0.0);
        // 上がって少し戻る: 3 区間中 2 区間が上向き
        let m = monotonic(&[1.0, 3.0, 5.0, 4.0]);
        assert!((m - 2.0 / 3.0).abs() < 1e-9, "実際: {m}");
    }

    #[test]
    fn 値が三つ未満なら判定しない() {
        assert_eq!(monotonic(&[1.0, 9.0]), 0.0);
        assert_eq!(monotonic(&[]), 0.0);
    }

    #[test]
    fn 社名を含む語は落とす() {
        assert!(!is_safe_term("ヤンマーキャステクノ株式会社"));
        assert!(!is_safe_term("出雲公共職業安定所"));
        assert!(!is_safe_term("米子鬼太郎空港"));
        assert!(is_safe_term("一般事務"));
        assert!(is_safe_term("アルバイト"));
    }

    #[test]
    fn 動きが小さい語は拾わない() {
        let s = vec![(
            "事務".to_string(),
            vec![Some(10.0), Some(11.0), Some(12.0), Some(12.5)],
        )];
        let (rising, falling) = pick_moved(&s);
        assert!(
            rising.is_empty(),
            "2.5pt は {MIN_DIFF_PT}pt 未満なので拾わない"
        );
        assert!(falling.is_empty());
    }

    #[test]
    fn 上がった語と下がった語は別々に返る() {
        let s = vec![
            (
                "一般事務".to_string(),
                vec![Some(5.0), Some(10.0), Some(15.0), Some(20.0)],
            ),
            (
                "事務".to_string(),
                vec![Some(45.0), Some(40.0), Some(33.0), Some(27.0)],
            ),
        ];
        let (rising, falling) = pick_moved(&s);
        assert_eq!(rising.len(), 1);
        assert_eq!(rising[0].term, "一般事務");
        assert_eq!(falling.len(), 1);
        assert_eq!(falling[0].term, "事務");
        // 下がった側も差の大きさで並ぶので、符号ではなく絶対値で見ていること
        assert!(falling[0].diff_pt < 0.0);
    }

    #[test]
    fn 雇用形態の語には印が付く() {
        let s = vec![(
            "アルバイト".to_string(),
            vec![Some(15.0), Some(20.0), Some(24.0), Some(27.0)],
        )];
        let (rising, _) = pick_moved(&s);
        assert_eq!(rising.len(), 1);
        assert!(rising[0].is_koyou, "職種名ではなく雇用形態の語として扱う");
    }

    #[test]
    fn 欠けた月がある語は落とす() {
        let s = vec![(
            "事務".to_string(),
            vec![Some(45.0), None, Some(33.0), Some(27.0)],
        )];
        let (rising, falling) = pick_moved(&s);
        assert!(rising.is_empty() && falling.is_empty());
    }

    #[test]
    fn 手引きの文に数字を入れない() {
        // 工程⑦の数値照合は原文に無い数字を弾く。手引きに数字を入れると
        // そのまま原稿に混ざる恐れがあるので、語と向きだけを渡す。
        let b = WordBrief {
            title: "事務".into(),
            current: vec![
                Current {
                    term: "事務".into(),
                    pct: 26.8,
                    clicks: 1000,
                },
                Current {
                    term: "一般事務".into(),
                    pct: 19.9,
                    clicks: 800,
                },
            ],
            rising: vec![Moved {
                term: "一般事務".into(),
                start_pct: 5.3,
                end_pct: 19.9,
                diff_pt: 14.6,
                monotonic: 0.69,
                is_koyou: false,
            }],
            falling: vec![Moved {
                term: "事務".into(),
                start_pct: 45.3,
                end_pct: 26.8,
                diff_pt: -18.5,
                monotonic: 0.69,
                is_koyou: false,
            }],
            ..Default::default()
        };
        let line = b.guide_line();
        assert!(line.contains("一般事務"), "語は入る: {line}");
        assert!(
            !line.chars().any(|c| c.is_ascii_digit()),
            "数字が入っている: {line}"
        );
    }

    #[test]
    fn 材料が無ければ手引きは空になる() {
        let b = WordBrief::default();
        assert!(b.guide_line().is_empty());
    }

    /// 監査のテスト用。数字は検査の条件を作るためのもので、実データではない。
    fn brief_for_test() -> WordBrief {
        WordBrief {
            title: "事務".into(),
            first_month: "2025-07".into(),
            last_month: "2026-08".into(),
            months: 14,
            current: vec![
                Current {
                    term: "事務".into(),
                    pct: 26.8,
                    clicks: 201874,
                },
                Current {
                    term: "一般事務".into(),
                    pct: 19.9,
                    clicks: 149958,
                },
                Current {
                    term: "事務パート".into(),
                    pct: 4.9,
                    clicks: 36605,
                },
                Current {
                    term: "土日祝休み".into(),
                    pct: 2.7,
                    clicks: 20544,
                },
            ],
            rising: vec![Moved {
                term: "一般事務".into(),
                start_pct: 5.3,
                end_pct: 19.9,
                diff_pt: 14.6,
                monotonic: 0.69,
                is_koyou: false,
            }],
            falling: vec![Moved {
                term: "事務".into(),
                start_pct: 45.3,
                end_pct: 26.8,
                diff_pt: -18.5,
                monotonic: 0.69,
                is_koyou: false,
            }],
            hidden_terms: 0,
        }
    }

    #[test]
    fn 落ち続けている語を使っていたら指摘する() {
        let f = audit_title("事務スタッフ", &brief_for_test());
        let hit = f
            .iter()
            .find(|x| x.term == "事務")
            .expect("落ちている語の指摘が無い");
        assert_eq!(hit.level, 2);
        assert!(
            hit.text.contains("一般事務"),
            "代わりの語を出す: {}",
            hit.text
        );
    }

    #[test]
    fn 伸びている語を使っていれば落ちた語の指摘は出ない() {
        let f = audit_title("一般事務スタッフ", &brief_for_test());
        assert!(
            !f.iter()
                .any(|x| x.level == 2 && x.term == "事務" && x.text.contains("落ちて")),
            "「一般事務」は「事務」を含むので誤検知しやすい: {f:?}"
        );
    }

    #[test]
    fn 条件の語を職種名に入れていたら指摘する() {
        let f = audit_title("一般事務（土日祝休み）", &brief_for_test());
        let hit = f
            .iter()
            .find(|x| x.term == "土日祝休み")
            .expect("条件語の指摘が無い");
        assert_eq!(hit.level, 1, "直さないと駄目とまでは言わない");
        assert!(hit.text.contains("条件欄"), "行き先を言う: {}", hit.text);
    }

    #[test]
    fn 雇用形態が入っていれば余地の指摘は出ない() {
        let with_koyou = audit_title("一般事務パート", &brief_for_test());
        assert!(
            !with_koyou.iter().any(|x| x.term == "事務パート"),
            "既に雇用形態が入っている: {with_koyou:?}"
        );
        let without = audit_title("一般事務", &brief_for_test());
        assert!(
            without.iter().any(|x| x.term == "事務パート"),
            "雇用形態が無いときは余地を言う: {without:?}"
        );
    }

    #[test]
    fn 材料が無ければ何も言わない() {
        assert!(audit_title("一般事務", &WordBrief::default()).is_empty());
        assert!(audit_title("", &brief_for_test()).is_empty());
    }

    #[test]
    fn 語は書く場所で三つに分かれる() {
        // 実データの上位語をそのまま並べると「ネイルok」が職種名として渡っていた。
        assert_eq!(kind_of("一般事務"), Kind::Title);
        assert_eq!(kind_of("カフェ"), Kind::Title);
        assert_eq!(kind_of("データ入力"), Kind::Title);

        assert_eq!(kind_of("アルバイト"), Kind::Koyou);
        assert_eq!(
            kind_of("事務パート"),
            Kind::Koyou,
            "職種名＋雇用形態は雇用形態側"
        );
        assert_eq!(kind_of("短期 アルバイト"), Kind::Koyou);

        assert_eq!(kind_of("ネイルok"), Kind::Joken);
        assert_eq!(kind_of("土日祝休み"), Kind::Joken);
        assert_eq!(kind_of("未経験"), Kind::Joken);
        assert_eq!(kind_of("50代"), Kind::Joken, "年代は条件側");
    }

    #[test]
    fn 条件語と雇用形態語が両方ある語は条件側にする() {
        // 「主婦パート」は主婦（条件）とパート（雇用形態）の両方を含む。
        // これは「主婦歓迎のパート」を探している語なので条件として扱う。
        assert_eq!(kind_of("主婦パート"), Kind::Joken);
        assert_eq!(kind_of("高校生ok アルバイト"), Kind::Joken);
    }

    #[test]
    fn 求人票に書けない条件語は渡す文に入れない() {
        // 「主婦パート」は検索語の上位に実際に出るが、求人票に書くと
        // 性別差別表示で NG になる。工程⑦はその列を空にするので、
        // 渡す文には入れない。
        let b = WordBrief {
            current: vec![
                Current {
                    term: "販売スタッフ".into(),
                    pct: 5.2,
                    clicks: 20289,
                },
                Current {
                    term: "主婦パート".into(),
                    pct: 3.2,
                    clicks: 12598,
                },
                Current {
                    term: "土日祝休み".into(),
                    pct: 2.7,
                    clicks: 20544,
                },
            ],
            ..Default::default()
        };
        let line = b.guide_line();
        assert!(
            !line.contains("主婦"),
            "書けない語が渡す文に入っている: {line}"
        );
        assert!(line.contains("土日祝休み"), "書ける条件語は残す: {line}");
        assert!(line.contains("販売スタッフ"), "職種名は残す: {line}");
    }

    #[test]
    fn 条件の語を職種名として渡さない() {
        let b = WordBrief {
            current: vec![
                Current {
                    term: "カフェ".into(),
                    pct: 10.8,
                    clicks: 15146,
                },
                Current {
                    term: "ネイルok".into(),
                    pct: 3.1,
                    clicks: 4370,
                },
                Current {
                    term: "高校生ok アルバイト".into(),
                    pct: 1.9,
                    clicks: 2697,
                },
            ],
            ..Default::default()
        };
        let line = b.guide_line();
        let i_title = line
            .find("職種名はこの語に寄せる")
            .expect("職種名の話が無い");
        let i_nail = line.find("ネイルok").expect("条件の語が消えている");
        assert!(
            i_nail > i_title,
            "条件の語が職種名の並びに入っている: {line}"
        );
        assert!(
            line.contains("条件欄に書く"),
            "行き先が書かれていない: {line}"
        );
    }

    #[test]
    fn 雇用形態の語は職種名の並びと混ぜない() {
        let b = WordBrief {
            rising: vec![
                Moved {
                    term: "アルバイト".into(),
                    start_pct: 15.0,
                    end_pct: 27.0,
                    diff_pt: 12.0,
                    monotonic: 0.7,
                    is_koyou: true,
                },
                Moved {
                    term: "ホールスタッフ".into(),
                    start_pct: 2.0,
                    end_pct: 8.0,
                    diff_pt: 6.0,
                    monotonic: 0.7,
                    is_koyou: false,
                },
            ],
            ..Default::default()
        };
        let line = b.guide_line();
        let i_job = line.find("ホールスタッフ").expect("職種名が無い");
        let i_koyou = line.find("雇用形態では").expect("雇用形態の話が無い");
        assert!(i_job < i_koyou, "職種名の話が先、雇用形態の話が後: {line}");
    }
}
