//! アポのランク (S / A / B / C)。日程が決まった後のヒアリングで聞けた先方の答えから決める。
//!
//! 基準は「リクロジ 営業戦略イメージ (2025年10月〜2026年10月 商談分析)」の「アンケートから見た優先層」
//! (2026-11-05 ユーザー決定)。電話で聞けなかった答え (不明・未定) は、資料どおり C に入れる (同日ユーザー決定)。
//!
//! - S: 予算が具体額 × 入社時期が 3 か月以内 × 職種が現場系 (製造・倉庫・ドライバー)
//! - A: 予算が具体額 × 入社時期が 3 か月以内 (S 以外)、または 予算が具体額 × 募集の理由が欠員・若返り
//! - B: 予算が具体額 × 入社時期が 4 か月以上・未定
//! - C: 上のどれでもない (予算が無い・都度確保・未定・聞けていない、または 予算は具体額でも時期が聞けていない)
//!
//! ヒアリングの対象でない電話 (日程が決まらなかった・アポの記録が無い・確認の電話など) はランクを付けない。

use super::judge::StoredItem;

pub const BUDGET: &str = "aq_budget";
pub const TIMING: &str = "aq_timing";
pub const JOB: &str = "aq_job";
pub const REASON: &str = "aq_reason";

/// ランクの並び (表の列の順)
pub const RANKS: [&str; 4] = ["S", "A", "B", "C"];

fn answer<'a>(items: &'a [StoredItem], id: &str) -> Option<&'a str> {
    items
        .iter()
        .find(|i| i.id == id)
        .and_then(|i| i.answer.as_deref())
}

/// 判定済みの電話の項目からランクを決める。予算の項目が無い・該当なし (`na`) ならヒアリングの対象外で `None`
pub fn rank(items: &[StoredItem]) -> Option<&'static str> {
    let budget = items.iter().find(|i| i.id == BUDGET)?;
    if budget.status == "na" {
        return None;
    }
    if budget.answer.as_deref() != Some("具体額") {
        return Some("C");
    }
    let timing = answer(items, TIMING);
    if timing == Some("3か月以内") {
        return Some(if answer(items, JOB) == Some("現場系") {
            "S"
        } else {
            "A"
        });
    }
    if matches!(answer(items, REASON), Some("欠員" | "若返り")) {
        return Some("A");
    }
    if matches!(timing, Some("4か月以上" | "未定")) {
        return Some("B");
    }
    Some("C")
}

#[cfg(test)]
mod tests {
    use super::*;

    fn it(id: &str, status: &str, answer: Option<&str>) -> StoredItem {
        StoredItem {
            id: id.into(),
            status: status.into(),
            evidence: vec![],
            quote: None,
            advice: None,
            note: None,
            answer: answer.map(Into::into),
            asked: None,
            answered: None,
        }
    }

    fn items(
        budget: Option<&str>,
        timing: Option<&str>,
        job: Option<&str>,
        reason: Option<&str>,
    ) -> Vec<StoredItem> {
        vec![
            it("early_date", "done", None),
            it(BUDGET, "done", budget),
            it(TIMING, "done", timing),
            it(JOB, "done", job),
            it(REASON, "done", reason),
        ]
    }

    #[test]
    fn ranks_follow_the_priority_layers_of_the_sales_strategy_document() {
        let r = |b, t, j, re| rank(&items(b, t, j, re));
        assert_eq!(
            r(Some("具体額"), Some("3か月以内"), Some("現場系"), None),
            Some("S")
        );
        assert_eq!(
            r(Some("具体額"), Some("3か月以内"), Some("それ以外"), None),
            Some("A")
        );
        assert_eq!(
            r(
                Some("具体額"),
                Some("3か月以内"),
                Some("不明"),
                Some("増員")
            ),
            Some("A")
        );
        assert_eq!(
            r(
                Some("具体額"),
                Some("4か月以上"),
                Some("現場系"),
                Some("欠員")
            ),
            Some("A")
        );
        assert_eq!(
            r(Some("具体額"), Some("未定"), None, Some("若返り")),
            Some("A")
        );
        assert_eq!(
            r(
                Some("具体額"),
                Some("4か月以上"),
                Some("現場系"),
                Some("増員")
            ),
            Some("B")
        );
        assert_eq!(r(Some("具体額"), Some("未定"), None, None), Some("B"));
        // 聞けていない答えは資料どおり C
        assert_eq!(
            r(Some("具体額"), Some("不明"), Some("現場系"), Some("増員")),
            Some("C")
        );
        assert_eq!(r(Some("具体額"), None, None, None), Some("C"));
        for b in [
            Some("無い"),
            Some("都度確保"),
            Some("未定"),
            Some("不明"),
            None,
        ] {
            assert_eq!(
                r(b, Some("3か月以内"), Some("現場系"), Some("欠員")),
                Some("C"),
                "{b:?}"
            );
        }
    }

    #[test]
    fn calls_outside_the_hearing_have_no_rank() {
        let mut v = items(Some("具体額"), Some("3か月以内"), Some("現場系"), None);
        v[1].status = "na".into();
        assert_eq!(rank(&v), None);
        assert_eq!(rank(&[it("early_date", "done", None)]), None);
        assert_eq!(rank(&[]), None);
    }
}
