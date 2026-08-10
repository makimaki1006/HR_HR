//! 検索エンジンからの見え方チェック (2026-08-10、URL Seed PoC の製品化)。
//!
//! 求人ページのURLを Google Ads の keyword idea 生成に渡すと「そのページから連想される
//! 広告キーワード」が返る。PoC (自社20求人) で確認した知見:
//! - 生の返却はサイト共通ノイズ (「正社員 求人」等) とページ骨格由来のゴミ
//!   (URLパス断片「offers jobs」、分断語「職 業」) が上位を支配する
//! - ノイズを除去すると、ページごとに勤務地・職種・特徴語が正しく浮かぶ
//! - 職種認識のズレ (電気機械修理工がドライバー/工場と認識される等) が実際に検出できた
//!
//! 本モジュールはその前処理と判定を決定論の純関数で行う (LLM不使用)。
//!
//! # 表現の規律
//! - 出力文言は現場・顧客向けのため「Google」を名指しせず「検索エンジン」と表現する
//! - 返却は「検索エンジンの意味理解」であり実流入データではない。判定文は必ず
//!   「〜の可能性があります」の仮説形に固定し、応募効果を主張しない

use std::collections::HashSet;

/// 汎用求人語 (これらのトークンだけで構成される語はどの求人にも出るノイズ)。
/// 「未経験」は意図的に含めない (未経験認識の有無はページ固有のシグナルになる)。
const GENERIC_RECRUIT_TOKENS: [&str; 12] = [
    "求人",
    "正社員",
    "仕事",
    "転職",
    "採用",
    "情報",
    "募集",
    "社員",
    "バイト",
    "アルバイト",
    "パート",
    "中途",
];

/// 語のトークンがすべて汎用求人語なら true (例: 「正社員 求人」「求人 情報」)。
pub fn is_generic_recruit_term(term: &str) -> bool {
    let mut any = false;
    for token in term.split_whitespace() {
        any = true;
        if !GENERIC_RECRUIT_TOKENS.contains(&token) {
            return false;
        }
    }
    any
}

/// ページ骨格由来のゴミ判定。
/// - URLに含まれる英字トークン (パス断片「offers」「jobs」やドメイン断片) を含む語
/// - 1文字だけの日本語トークンを含む語 (「職 業」のような分断語)
pub fn is_fragment_term(term: &str, url: &str) -> bool {
    let url_lower = url.to_lowercase();
    let mut all_ascii = true;
    let mut any = false;
    for token in term.split_whitespace() {
        any = true;
        if token.chars().count() == 1 && token.chars().all(|c| !c.is_ascii()) {
            return true;
        }
        if !token.is_ascii() {
            all_ascii = false;
        }
        if token.is_ascii()
            && token.chars().count() >= 3
            && url_lower.contains(&token.to_lowercase())
        {
            return true;
        }
    }
    // 全トークンが英字の語はURLパス・ブランド断片とみなす (「offers jobs」「fora career」)。
    // 「jal 採用」のような英字+日本語の混在は意味ある固有語なので残す。
    any && all_ascii
}

/// ページ固有語の抽出: 汎用求人語・骨格ゴミ・兄弟URLとの共通語を除去する。
/// 兄弟URL (同一サイトの別職種の求人) の読み取り語にも出る語はサイト共通とみなす。
pub fn unique_page_terms(
    page_terms: &[(String, i64)],
    sibling_sets: &[HashSet<String>],
    url: &str,
) -> Vec<(String, i64)> {
    page_terms
        .iter()
        .filter(|(term, _)| !is_generic_recruit_term(term))
        .filter(|(term, _)| !is_fragment_term(term, url))
        .filter(|(term, _)| !sibling_sets.iter().any(|set| set.contains(term)))
        .cloned()
        .collect()
}

/// ニッチ職種の市場語フォールバック候補 (2026-08-10 ユーザー指摘対応)。
/// 「電気機械修理工 求人」のような複合職種語は検索量が10未満に丸められ突合が形骸化する。
/// 役割接尾辞 (工・員・者・士・師・職・手) を剥がし、末尾の意味語へ段階的に短縮した
/// 候補を返す (検索量が計上される関連語まで追いかけるため。最大2候補)。
pub fn fallback_job_terms(job: &str) -> Vec<String> {
    const ROLE_SUFFIXES: [char; 7] = ['工', '員', '者', '士', '師', '職', '手'];
    let mut out: Vec<String> = Vec::new();
    let mut push = |candidate: String| {
        let trimmed = candidate.trim().to_string();
        if trimmed.chars().count() >= 2 && trimmed != job && !out.contains(&trimmed) {
            out.push(trimmed);
        }
    };
    // 1) 役割接尾辞を剥がす (電気機械修理工 → 電気機械修理)
    let stripped: String = {
        let mut chars: Vec<char> = job.trim().chars().collect();
        while chars
            .last()
            .map(|c| ROLE_SUFFIXES.contains(c))
            .unwrap_or(false)
        {
            chars.pop();
        }
        chars.iter().collect()
    };
    push(stripped.clone());
    // 2) 末尾4文字 (機械修理)。短い職種語は末尾2文字まで落とさない (誤爆防止)
    let chars: Vec<char> = stripped.chars().collect();
    if chars.len() > 4 {
        push(chars[chars.len() - 4..].iter().collect());
    }
    out.truncate(2);
    out
}

/// 職種の内容語 (職種名から汎用語尾を除いたトークン群)。照合は部分一致。
fn job_content_tokens(job: &str) -> Vec<String> {
    job.split_whitespace()
        .flat_map(|token| {
            // 「配送ドライバー」のような複合職種は2文字以上の部分語も対象にする
            let mut out = vec![token.to_string()];
            if token.chars().count() >= 4 {
                let chars: Vec<char> = token.chars().collect();
                for window in 2..chars.len() {
                    for start in 0..=(chars.len() - window) {
                        out.push(chars[start..start + window].iter().collect());
                    }
                }
            }
            out
        })
        .filter(|t| t.chars().count() >= 2)
        .filter(|t| !GENERIC_RECRUIT_TOKENS.contains(&t.as_str()))
        .collect()
}

/// 判定文の生成 (決定論・仮説形固定・「検索エンジン」表記)。
///
/// - `unique`: ページ固有語 (ノイズ除去済み)
/// - `market_missing`: 市場でよく検索される語のうち、URL側の読み取りに出なかった上位語
/// トークンが地名か (収録地名リストの前方一致。「広島」⊂「広島県」「千歳」⊂「千歳市」)。
fn is_location_token(token: &str, location_names: &[String]) -> bool {
    token.chars().count() >= 2
        && location_names.iter().any(|name| {
            name.starts_with(token) || token.starts_with(name.trim_end_matches(['県', '府', '都']))
        })
}

pub fn visibility_hypotheses(
    job: &str,
    unique: &[(String, i64)],
    market_missing: &[String],
    location_names: &[String],
) -> Vec<String> {
    let mut out = Vec::new();
    let tokens = job_content_tokens(job);
    let job_recognized = unique
        .iter()
        .take(10)
        .any(|(term, _)| tokens.iter().any(|token| term.contains(token.as_str())));
    if !job_recognized {
        out.push(format!(
            "職種「{job}」に関する言葉が、検索エンジンの読み取り結果の上位に出ていません。この求人は「{job}」を探している人の検索文脈に載りにくくなっている可能性があります。"
        ));
    }
    // 職種と無関係な強い語 (月間1,000回以上) が上位にある → 別テーマとして認識の可能性。
    // 地名語 (「広島 求人」「千歳 求人」) は勤務地の正しい認識なので対象外。
    if let Some((term, monthly)) = unique.iter().take(5).find(|(term, monthly)| {
        *monthly >= 1_000
            && !tokens.iter().any(|token| term.contains(token.as_str()))
            && !term
                .split_whitespace()
                .any(|token| is_location_token(token, location_names))
    }) {
        out.push(format!(
            "「{term}」(月間{monthly}回検索) など、職種と異なるテーマの言葉が強く読み取られています。ページ内の記述の一部が別の仕事の求人として認識され、職種の印象が薄まっている可能性があります。"
        ));
    }
    if !market_missing.is_empty() {
        let list = market_missing
            .iter()
            .take(3)
            .map(|k| format!("「{k}」"))
            .collect::<Vec<_>>()
            .join("・");
        out.push(format!(
            "この職種でよく検索される {list} との関連が、ページの読み取り結果からは確認できませんでした。これらの言葉で探している人に届きにくい可能性があります。"
        ));
    }
    if out.is_empty() {
        out.push(format!(
            "職種「{job}」の認識・市場の主要な検索語との関連とも、大きなズレは見つかりませんでした (検索エンジンの意味理解上の確認であり、検索順位や応募効果を保証するものではありません)。"
        ));
    }
    out
}

#[cfg(test)]
mod tests {
    use super::*;

    fn set(terms: &[&str]) -> HashSet<String> {
        terms.iter().map(|s| s.to_string()).collect()
    }

    /// PoC実測のノイズがすべて落ちる (逆証明: 固有語は残る)。
    #[test]
    fn noise_filters_drop_poc_noise_and_keep_signal() {
        let url = "https://hr-hacker.com/f-a-c-rikurozi/job-offers/show/11692921";
        assert!(is_generic_recruit_term("正社員 求人"));
        assert!(is_generic_recruit_term("求人 情報"));
        assert!(!is_generic_recruit_term("求人 未経験"));
        assert!(!is_generic_recruit_term("工場 求人"));
        assert!(is_fragment_term("offers jobs", url));
        assert!(is_fragment_term("職 業", url));
        assert!(is_fragment_term("fora career", url));
        // 「jal 採用」の jal はURLに無い英字 → 落とさない (実データの意味ある固有語)
        assert!(!is_fragment_term("jal 採用", url));

        let page = vec![
            ("正社員 求人".to_string(), 40_000),
            ("offers jobs".to_string(), 5_000),
            ("求人 ドライバー".to_string(), 22_200),
            ("工場 求人".to_string(), 18_100),
            ("未経験 求人".to_string(), 9_900),
        ];
        let siblings = vec![set(&["未経験 求人", "正社員 求人"])];
        let unique = unique_page_terms(&page, &siblings, url);
        let names: Vec<&str> = unique.iter().map(|(k, _)| k.as_str()).collect();
        assert_eq!(names, vec!["求人 ドライバー", "工場 求人"]);
    }

    /// PoC実例1: 電気機械修理工がドライバー/工場として認識 → 職種未認識+別テーマの両方が出る。
    #[test]
    fn hypotheses_detect_occupation_mismatch() {
        let unique = vec![
            ("求人 ドライバー".to_string(), 22_200),
            ("工場 求人".to_string(), 18_100),
        ];
        let hypotheses = visibility_hypotheses("電気機械修理工", &unique, &[], &[]);
        assert!(
            hypotheses
                .iter()
                .any(|h| h.contains("電気機械修理工") && h.contains("載りにくく")),
            "{hypotheses:?}"
        );
        assert!(
            hypotheses.iter().any(|h| h.contains("求人 ドライバー")),
            "{hypotheses:?}"
        );
        // 仮説形固定の逆証明: 断定語で終わる文が無い
        assert!(
            hypotheses.iter().all(|h| h.contains("可能性")),
            "{hypotheses:?}"
        );
    }

    /// PoC実例2: 一般事務は正しく認識 → ズレなしの文だけが出る (過剰警告しない)。
    #[test]
    fn hypotheses_pass_when_recognized() {
        let unique = vec![
            ("事務 一般".to_string(), 49_500),
            ("事務 求人".to_string(), 9_900),
            ("広島 求人".to_string(), 9_900),
        ];
        let locations = vec!["広島県".to_string(), "広島市".to_string()];
        let hypotheses = visibility_hypotheses("一般事務", &unique, &[], &locations);
        assert_eq!(hypotheses.len(), 1, "{hypotheses:?}");
        assert!(hypotheses[0].contains("大きなズレは見つかりませんでした"));
        assert!(hypotheses[0].contains("保証するものではありません"));
    }

    /// 市場語の欠落は最大3語で列挙され、仮説形で出る。
    #[test]
    fn hypotheses_report_missing_market_terms() {
        let unique = vec![("倉庫 求人".to_string(), 18_100)];
        let missing = vec![
            "倉庫 未経験".to_string(),
            "倉庫 日勤".to_string(),
            "倉庫 土日休み".to_string(),
            "倉庫 高収入".to_string(),
        ];
        let hypotheses = visibility_hypotheses("倉庫作業員", &unique, &missing, &[]);
        let joined = hypotheses.join(" ");
        assert!(joined.contains("倉庫 未経験") && joined.contains("倉庫 土日休み"));
        assert!(
            !joined.contains("倉庫 高収入"),
            "4語目まで列挙している: {joined}"
        );
        assert!(joined.contains("可能性"));
    }

    /// ニッチ職種のフォールバック候補: 役割接尾辞を剥がし段階的に短縮する。
    #[test]
    fn fallback_terms_shorten_niche_occupations() {
        assert_eq!(
            fallback_job_terms("電気機械修理工"),
            vec!["電気機械修理".to_string(), "機械修理".to_string()]
        );
        // 短い職種は候補が出ないか1件のみ (誤爆防止)
        assert!(fallback_job_terms("営業職") == vec!["営業".to_string()]);
        assert!(fallback_job_terms("事務").is_empty() || fallback_job_terms("事務").len() <= 1);
    }

    /// 地名語は「別テーマ認識」として誤検出しない (広島/千歳のPoC実データ対策)。
    #[test]
    fn location_terms_are_not_off_theme() {
        let locations = vec!["北海道".to_string(), "千歳市".to_string()];
        let unique = vec![
            ("製造 正社員".to_string(), 1_900),
            ("千歳 求人".to_string(), 1_900),
        ];
        let hypotheses = visibility_hypotheses("製造職", &unique, &[], &locations);
        assert!(
            !hypotheses.iter().any(|h| h.contains("千歳")),
            "地名を別テーマとして誤検出: {hypotheses:?}"
        );
    }

    /// 表現の規律: 生成文に「Google」を出さない (現場・顧客向け表記ルール)。
    #[test]
    fn hypotheses_never_mention_google() {
        for hypotheses in [
            visibility_hypotheses(
                "配送ドライバー",
                &[("飲食 求人".to_string(), 4_400)],
                &[],
                &[],
            ),
            visibility_hypotheses("一般事務", &[("事務 一般".to_string(), 49_500)], &[], &[]),
        ] {
            assert!(
                hypotheses.iter().all(|h| !h.contains("Google")),
                "{hypotheses:?}"
            );
        }
    }
}
