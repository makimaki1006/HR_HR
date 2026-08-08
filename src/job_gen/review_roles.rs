//! 口コミ投稿者の役割分類(P1-3)。
//!
//! 正本: `claudedocs/JOURNEY_FACT_GUARD_SPEC_2026-08-07.md` P1-3。
//!
//! # なぜ必要か
//!
//! Google クチコミには「中の人(社員・元社員)の証言」と「外から来た人(納品ドライバー・
//! 来訪者)の証言」が混在する。実データ(センコー)では **待機時間・荷降ろしの不満は
//! ほぼ全て取引先ドライバーの投稿**であり、これを社員の労働実態(残業・拘束時間)の
//! 根拠に使うと事実誤認になる。一方で、会社名で検索した求職者の目には入るので
//! 「検索評判リスク」としては有効。この2用途を型で分離するのがこのモジュールの役割。
//!
//! [`RoleClassification::usable_for_working_conditions`] が `true` になるのは
//! [`SpeakerRole::Employee`] / [`SpeakerRole::FormerEmployee`] のみ。
//!
//! # 判定方式: LLM を使わない規則ベース
//!
//! 他の検証ゲート([`super::ng_words`], [`super::validate`])と同じ方針で、判定は
//! コード側の決定論的処理にする。LLM に役割を推定させると、根拠語がないのに
//! 「ドライバーと思われる」と創作する(= 事実の捏造)ため。
//!
//! 手がかり語(cue)を原文に**そのまま含むか**だけで判定し、含まなければ
//! [`SpeakerRole::Unknown`] に落とす。推測はしない。
//!
//! # 判定順序(先に当たったものを採用)
//!
//! 1. 元社員 → 2. 現社員 → 3. 訪問ドライバー/取引先 → 4. 来訪者 → 5. 不明
//!
//! 社員系を先に見るのは、社員が「待機時間が長い」と書いた場合に
//! ドライバー判定へ落とさないため(実データで最も損害が大きい誤りは
//! 「社員の証言をドライバー扱いして労働実態から除外する」方向ではなく、
//! 逆の「ドライバーの証言を社員の労働実態に混ぜる」方向なので、社員系 cue は
//! 「働いています」等の明示的な語のみに絞ってある)。
//!
//! # 誤爆抑止(blockers)
//!
//! cue の**直後**に続くと打ち消される文字列を cue ごとに持つ。
//! 例: 「待機」+「児童」= 待機児童(保育の話題)は運送の待機ではない。
//! 直後の文字列しか見ないので、前方文脈による誤爆(「息子が入社しました」等)は
//! 防げない。既知の限界としてテストに明示してある(`known_limitation_*`)。
//!
//! # evidence_phrase の保証
//!
//! [`RoleClassification::evidence_phrase`] は**必ず原文の逐語部分文字列**
//! (cue を含む一文、最大 [`MAX_EVIDENCE_CHARS`] 文字、前後の空白のみ除去)。
//! 省略記号等は付けない。これにより「引用が原文に実在するか」を機械検証できる
//! (工程①の引用実在チェックと同じ規律)。

use serde::{Deserialize, Serialize};

/// 投稿者の役割。
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum SpeakerRole {
    /// 現職の社員・従業員。
    Employee,
    /// 元社員・元従業員。
    FormerEmployee,
    /// 納品・集荷等で訪れた取引先ドライバー(社外)。
    VisitingDriver,
    /// 来訪者・利用者(社外、ドライバー以外)。
    Visitor,
    /// 手がかり語なし。推定しない。
    Unknown,
}

impl SpeakerRole {
    /// 労働実態(残業・拘束時間・休日・人間関係等)の推定に使ってよいか。
    ///
    /// 中の人の証言のみ `true`。それ以外は検索評判リスクとしてのみ使用可。
    pub fn usable_for_working_conditions(self) -> bool {
        matches!(self, SpeakerRole::Employee | SpeakerRole::FormerEmployee)
    }

    /// UI・JSON 表示用の日本語ラベル(内部語を表に出さない)。
    pub fn label_ja(self) -> &'static str {
        match self {
            SpeakerRole::Employee => "社員",
            SpeakerRole::FormerEmployee => "元社員",
            SpeakerRole::VisitingDriver => "取引先ドライバー",
            SpeakerRole::Visitor => "来訪者",
            SpeakerRole::Unknown => "不明",
        }
    }
}

/// 分類結果。
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct RoleClassification {
    /// 推定された役割。
    pub role: SpeakerRole,
    /// 判定根拠となった原文の逐語引用(cue を含む一文)。`Unknown` では `None`。
    pub evidence_phrase: Option<String>,
    /// 労働実態の推定に使ってよいか(= `role.usable_for_working_conditions()`)。
    pub usable_for_working_conditions: bool,
}

/// 手がかり語1件。
struct Cue {
    /// 原文にこの文字列がそのまま含まれれば該当。
    pattern: &'static str,
    /// この文字列が cue の**直後**に続く場合は該当としない。
    blockers: &'static [&'static str],
}

const fn cue(pattern: &'static str) -> Cue {
    Cue {
        pattern,
        blockers: &[],
    }
}

const fn cue_unless(pattern: &'static str, blockers: &'static [&'static str]) -> Cue {
    Cue { pattern, blockers }
}

/// 元社員の手がかり語。
///
/// 「退社し」は**入れない**(「定時に退社しています」= 現職の退勤の意味があり、
/// 退職と区別できないため)。
const FORMER_EMPLOYEE_CUES: &[Cue] = &[
    cue("働いていました"),
    cue("働いてました"),
    cue("勤めていました"),
    cue("勤めてました"),
    cue("勤務していました"),
    cue("勤務してました"),
    cue("退職し"),
    cue("退職した"),
    cue("在籍していた"),
    cue("在籍してました"),
    cue("元社員"),
    cue("元従業員"),
    cue("辞めました"),
    cue("以前働いて"),
];

/// 現社員の手がかり語。
///
/// 「働く」単独や「社員」単独は入れない(第三者の話・求人文の引用で誤爆するため)。
const EMPLOYEE_CUES: &[Cue] = &[
    cue("働いています"),
    cue("働いてます"),
    cue("働いております"),
    cue("働いてる"),
    cue("勤務しています"),
    cue("勤務してます"),
    cue("勤務しております"),
    cue("勤めています"),
    cue("勤めてます"),
    cue("入社し"),
    cue("弊社"),
    cue("当社"),
];

/// 訪問ドライバー・取引先の手がかり語。
///
/// 「待機」は保育文脈(待機児童)で誤爆するため blocker を置く。
/// 「納品」は「納品書」(社内事務の可能性)を除外する。
const VISITING_DRIVER_CUES: &[Cue] = &[
    cue_unless("待機", &["児童", "電力", "児"]),
    cue("待たされ"),
    cue("荷待ち"),
    cue("荷降ろし"),
    cue("荷下ろし"),
    cue("荷卸し"),
    cue("積み込み"),
    cue("積込み"),
    cue("積み下ろし"),
    cue_unless("納品", &["書"]),
    cue("配達に行っ"),
    cue("配達に来"),
    cue("配送に行っ"),
    cue("集荷"),
    cue("入構"),
    cue("傭車"),
    cue("ドライバーです"),
    cue("運転手です"),
];

/// 来訪者の手がかり語(上記いずれにも当たらない場合のみ採用)。
const VISITOR_CUES: &[Cue] = &[
    cue("行きました"),
    cue("行ってきました"),
    cue("訪問し"),
    cue("見学"),
    cue("来ました"),
    cue("伺いました"),
    cue("お邪魔しました"),
    cue("利用しました"),
    cue("利用させていただき"),
];

/// evidence_phrase の最大文字数(文字数であってバイト数ではない)。
pub const MAX_EVIDENCE_CHARS: usize = 60;

/// evidence_phrase で cue の手前に含める文脈の文字数。
const CONTEXT_BEFORE_CHARS: usize = 15;

/// 文の区切りとみなす文字。
const SENTENCE_DELIMS: [char; 6] = ['。', '！', '？', '!', '?', '\n'];

/// 口コミ本文から投稿者の役割を分類する。
///
/// 手がかり語が1つも無ければ [`SpeakerRole::Unknown`](推測しない)。
pub fn classify_speaker(review_text: &str) -> RoleClassification {
    let groups: [(SpeakerRole, &[Cue]); 4] = [
        (SpeakerRole::FormerEmployee, FORMER_EMPLOYEE_CUES),
        (SpeakerRole::Employee, EMPLOYEE_CUES),
        (SpeakerRole::VisitingDriver, VISITING_DRIVER_CUES),
        (SpeakerRole::Visitor, VISITOR_CUES),
    ];

    for (role, cues) in groups {
        if let Some((start, len)) = find_first_cue(review_text, cues) {
            return RoleClassification {
                role,
                evidence_phrase: Some(evidence_snippet(review_text, start, len)),
                usable_for_working_conditions: role.usable_for_working_conditions(),
            };
        }
    }

    RoleClassification {
        role: SpeakerRole::Unknown,
        evidence_phrase: None,
        usable_for_working_conditions: false,
    }
}

/// cue 群のうち、原文で最も手前に出現するものの (開始バイト位置, バイト長) を返す。
///
/// blocker が直後に続く出現はスキップして次の出現を探す。
fn find_first_cue(text: &str, cues: &[Cue]) -> Option<(usize, usize)> {
    let mut best: Option<(usize, usize)> = None;
    for c in cues {
        let mut from = 0usize;
        while let Some(rel) = text[from..].find(c.pattern) {
            let start = from + rel;
            let end = start + c.pattern.len();
            let blocked = c.blockers.iter().any(|b| text[end..].starts_with(b));
            if blocked {
                from = end;
                continue;
            }
            if best.is_none_or(|(bs, _)| start < bs) {
                best = Some((start, c.pattern.len()));
            }
            break;
        }
    }
    best
}

/// cue を含む一文を原文から逐語で切り出す(最大 [`MAX_EVIDENCE_CHARS`] 文字)。
///
/// 戻り値は必ず `text` の部分文字列(前後の空白除去のみ)。
fn evidence_snippet(text: &str, cue_start: usize, cue_len: usize) -> String {
    let cue_end = cue_start + cue_len;

    // 文頭: cue より前の直近の区切り文字の次から。
    let sent_start = text[..cue_start]
        .rfind(SENTENCE_DELIMS)
        .map(|i| i + text[i..].chars().next().map_or(1, char::len_utf8))
        .unwrap_or(0);
    // 文末: cue より後の最初の区切り文字の手前まで。
    let sent_end = text[cue_end..]
        .find(SENTENCE_DELIMS)
        .map(|i| cue_end + i)
        .unwrap_or(text.len());

    if text[sent_start..sent_end].chars().count() <= MAX_EVIDENCE_CHARS {
        return text[sent_start..sent_end].trim().to_string();
    }

    // 長文は cue の周辺だけを窓で切る(cue は必ず窓に入る)。
    let win_start = nth_char_back(text, sent_start, cue_start, CONTEXT_BEFORE_CHARS);
    let win_end = nth_char_forward(text, win_start, sent_end, MAX_EVIDENCE_CHARS).max(cue_end);
    text[win_start..win_end].trim().to_string()
}

/// `to` から後方へ最大 `n` 文字戻ったバイト位置(`floor` 未満には戻らない)。
fn nth_char_back(text: &str, floor: usize, to: usize, n: usize) -> usize {
    text[floor..to]
        .char_indices()
        .rev()
        .take(n)
        .last()
        .map_or(to, |(i, _)| floor + i)
}

/// `from` から前方へ最大 `n` 文字進んだバイト位置(`ceil` は超えない)。
fn nth_char_forward(text: &str, from: usize, ceil: usize, n: usize) -> usize {
    text[from..ceil]
        .char_indices()
        .nth(n)
        .map_or(ceil, |(i, _)| from + i)
}

#[cfg(test)]
mod tests {
    use super::*;

    /// センコー実クチコミ(`C:\Users\fuji1\Downloads\テスト_センコー\センコー　口コミ.csv`)より逐語。
    const SENKO_WAIT_7H: &str =
        "過去最長に待たされましたなんと！！7時間待機\n遅くなるなら一言言えばいいのになんも言わない24年問題大丈夫なのか？Gメン来て？";
    const SENKO_WAIT_EGUI: &str = "待機時間えぐいって。\n\nどーにかしてくれ。";
    const SENKO_LOADING: &str =
        "積み込みに来ました。\n\n受付にて立って待ってたけど、フロントの方々はガン無視でした。";

    /// 全ての evidence_phrase は原文の逐語部分文字列でなければならない(引用実在チェック)。
    fn assert_evidence_is_verbatim(text: &str, r: &RoleClassification) {
        if let Some(ev) = &r.evidence_phrase {
            assert!(
                text.contains(ev.as_str()),
                "evidence_phrase が原文に存在しない: {ev:?} / 原文: {text:?}"
            );
            assert!(!ev.is_empty(), "evidence_phrase が空");
            assert!(
                ev.chars().count() <= MAX_EVIDENCE_CHARS,
                "evidence_phrase が長すぎる: {} 文字",
                ev.chars().count()
            );
        }
    }

    #[test]
    fn senko_wait_7h_is_visiting_driver() {
        let r = classify_speaker(SENKO_WAIT_7H);
        assert_eq!(r.role, SpeakerRole::VisitingDriver);
        assert!(
            !r.usable_for_working_conditions,
            "取引先の証言を労働実態に使ってはならない"
        );
        let ev = r.evidence_phrase.clone().expect("根拠引用が必要");
        assert!(
            ev.contains("待たされ") || ev.contains("待機"),
            "根拠に待機系の語がない: {ev}"
        );
        assert_evidence_is_verbatim(SENKO_WAIT_7H, &r);
    }

    #[test]
    fn senko_wait_egui_is_visiting_driver() {
        let r = classify_speaker(SENKO_WAIT_EGUI);
        assert_eq!(r.role, SpeakerRole::VisitingDriver);
        assert!(!r.usable_for_working_conditions);
        let ev = r.evidence_phrase.clone().expect("根拠引用が必要");
        assert!(ev.contains("待機"), "根拠に待機系の語がない: {ev}");
        assert_evidence_is_verbatim(SENKO_WAIT_EGUI, &r);
    }

    /// 「来ました」(来訪者cue)と「積み込み」(ドライバーcue)が同居する実データ。
    /// ドライバーを優先する。
    #[test]
    fn senko_loading_prefers_driver_over_visitor() {
        let r = classify_speaker(SENKO_LOADING);
        assert_eq!(r.role, SpeakerRole::VisitingDriver);
        assert!(!r.usable_for_working_conditions);
        assert_evidence_is_verbatim(SENKO_LOADING, &r);
    }

    /// センコー実データの本文ありクチコミ全6件。1件も「中の人の証言」ではない。
    /// = このケースの労働実態(残業・拘束時間)をクチコミから推定してはならない。
    #[test]
    fn senko_all_nonempty_reviews_are_unusable_for_working_conditions() {
        let reviews = [
            SENKO_LOADING,
            "毎度毎度、爆音マフラーや突き出たパーツの違法改造トラックが出入りしていることを許容している事業所",
            "AM11時に受付して15時になっても呼ばれない。去年行政指導受けたんちがうの？",
            "2024年問題ガン無視！！\n長時間待機当たり前！！",
            SENKO_WAIT_7H,
            SENKO_WAIT_EGUI,
        ];
        for text in reviews {
            let r = classify_speaker(text);
            assert!(
                !r.usable_for_working_conditions,
                "社員の証言と誤判定した: role={:?} text={text:?}",
                r.role
            );
            assert_evidence_is_verbatim(text, &r);
        }

        // うち待機・積み込みに言及した4件はドライバーとして拾えている
        // (残り2件は cue が無く Unknown = 上の known_limitation_* を参照)。
        let drivers = [
            SENKO_LOADING,
            "2024年問題ガン無視！！\n長時間待機当たり前！！",
            SENKO_WAIT_7H,
            SENKO_WAIT_EGUI,
        ];
        for text in drivers {
            assert_eq!(
                classify_speaker(text).role,
                SpeakerRole::VisitingDriver,
                "ドライバーとして拾えなかった: {text:?}"
            );
        }
    }

    #[test]
    fn current_employee_is_usable() {
        let text = "3年働いていますが残業は少ないです";
        let r = classify_speaker(text);
        assert_eq!(r.role, SpeakerRole::Employee);
        assert!(r.usable_for_working_conditions);
        assert_eq!(r.evidence_phrase.as_deref(), Some(text));
        assert_evidence_is_verbatim(text, &r);
    }

    #[test]
    fn former_employee_is_detected() {
        let text = "昨年退職しました";
        let r = classify_speaker(text);
        assert_eq!(r.role, SpeakerRole::FormerEmployee);
        assert!(
            r.usable_for_working_conditions,
            "元社員の証言も労働実態に使える"
        );
        assert_evidence_is_verbatim(text, &r);
    }

    /// 過去形/現在形を取り違えない。
    #[test]
    fn past_tense_beats_present_tense_pattern() {
        let r = classify_speaker("以前ここで働いていましたが今は別の会社です");
        assert_eq!(r.role, SpeakerRole::FormerEmployee);
        let r2 = classify_speaker("今もここで働いてます");
        assert_eq!(r2.role, SpeakerRole::Employee);
    }

    #[test]
    fn no_cue_is_unknown() {
        for text in ["普通", "駐車場が広い", "★★★", ""] {
            let r = classify_speaker(text);
            assert_eq!(r.role, SpeakerRole::Unknown, "誤検出: {text:?}");
            assert!(r.evidence_phrase.is_none());
            assert!(!r.usable_for_working_conditions);
        }
    }

    #[test]
    fn visitor_when_only_visit_cue() {
        let text = "工場見学に行きました。とても綺麗でした。";
        let r = classify_speaker(text);
        assert_eq!(r.role, SpeakerRole::Visitor);
        assert!(
            !r.usable_for_working_conditions,
            "来訪者の証言は労働実態に使えない"
        );
        assert_evidence_is_verbatim(text, &r);
    }

    /// 逆証明: 「待機児童」は運送の待機ではない。blocker で打ち消す。
    #[test]
    fn taiki_jidou_does_not_trigger_driver() {
        let r = classify_speaker("待機児童の解消に取り組んでいる地域です");
        assert_ne!(r.role, SpeakerRole::VisitingDriver, "待機児童で誤爆した");
        assert_eq!(r.role, SpeakerRole::Unknown);

        // 社員 cue が同居していれば当然 Employee(待機児童に引きずられない)。
        let r2 = classify_speaker("待機児童問題の保育園でパート勤務しています");
        assert_eq!(r2.role, SpeakerRole::Employee);
        assert!(r2.usable_for_working_conditions);
    }

    /// 逆証明: 社員が待機に言及しても社員のまま(社員系 cue を先に見る)。
    #[test]
    fn employee_mentioning_wait_stays_employee() {
        let text = "配送センターで働いています。待機時間が長く拘束が10時間を超えます。";
        let r = classify_speaker(text);
        assert_eq!(r.role, SpeakerRole::Employee);
        assert!(r.usable_for_working_conditions);
    }

    /// 長文でも evidence は原文の逐語部分文字列かつ上限内。
    #[test]
    fn evidence_is_verbatim_substring_for_long_text() {
        let long = "この事業所には毎週のように大型トラックで来ていますがとにかく順番が回ってこず今日も朝から夕方まで待機させられて本当に参りましたもう少し何とかならないものでしょうか";
        let r = classify_speaker(long);
        assert_eq!(r.role, SpeakerRole::VisitingDriver);
        assert_evidence_is_verbatim(long, &r);
        let ev = r.evidence_phrase.unwrap();
        assert!(ev.contains("待機"), "cue が窓から外れた: {ev}");
    }

    /// 既知の限界(誤検出): 家族・知人の勤務に言及した投稿を現職と誤る。
    /// 直後 blocker では前方文脈を見られないため、現状は Employee になる。
    /// 修正時はこのテストを更新すること。
    #[test]
    fn known_limitation_family_mention_is_misclassified_as_employee() {
        let r = classify_speaker("主人がこちらで働いていますが評判は良いようです");
        assert_eq!(
            r.role,
            SpeakerRole::Employee,
            "限界が解消されたならテストを更新する"
        );
    }

    /// 既知の限界(取りこぼし): cue 語が無いドライバー投稿は Unknown になる。
    /// センコー実データの「AM11時に受付して15時になっても呼ばれない」がこれ。
    /// 「受付して」を cue にすると来訪者・利用者で誤爆するため v1 では入れない。
    #[test]
    fn known_limitation_driver_without_cue_is_unknown() {
        let r = classify_speaker(
            "AM11時に受付して15時になっても呼ばれない。去年行政指導受けたんちがうの？",
        );
        assert_eq!(
            r.role,
            SpeakerRole::Unknown,
            "cue を追加したならテストを更新する"
        );
        assert!(
            !r.usable_for_working_conditions,
            "Unknown は労働実態に使えない"
        );
    }

    /// 既知の限界(取りこぼし): 「態度が悪い」等の評判のみの投稿は役割不明。
    /// 検索評判リスクとしては有効なので、呼び出し側は Unknown を捨てないこと。
    #[test]
    fn known_limitation_reputation_only_review_is_unknown() {
        let r = classify_speaker("違法改造トラックが出入りしていることを許容している事業所");
        assert_eq!(r.role, SpeakerRole::Unknown);
    }

    #[test]
    fn usable_flag_matches_role_for_all_variants() {
        for role in [
            SpeakerRole::Employee,
            SpeakerRole::FormerEmployee,
            SpeakerRole::VisitingDriver,
            SpeakerRole::Visitor,
            SpeakerRole::Unknown,
        ] {
            let expected = matches!(role, SpeakerRole::Employee | SpeakerRole::FormerEmployee);
            assert_eq!(role.usable_for_working_conditions(), expected);
        }
    }
}
