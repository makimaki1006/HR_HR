//! アポのランク (S / A / B / C)。日程が決まった後のヒアリングで聞けた先方の答えから決める。
//!
//! 3 つの条件のうち満たした数で決める (2026-11-06 ユーザー決定)。
//!
//! - 予算: 具体額
//! - 入社時期: 3 か月以内
//! - 判断する人: 本人が判断者
//!
//! 3 つ = S、2 つ = A、1 つ = B、0 = C。聞けていない答え (不明・未定・無い) は満たさないとして数える。
//! 商談前のアンケートの同じ 3 項目 (8〜9 月の 748 件) では、成約率が
//! S 18.4% (9/49)・A 6.3% (14/222)・B 3.1% (9/289)・C 2.1% (4/188) と順に並んだ。
//!
//! ヒアリングの対象でない電話 (日程が決まらなかった・アポの記録が無い・確認の電話など) はランクを付けない。

use super::judge::StoredItem;

pub const BUDGET: &str = "aq_budget";
pub const TIMING: &str = "aq_timing";
pub const DECIDER: &str = "aq_decider";

/// ランクの並び (表の列の順)
pub const RANKS: [&str; 4] = ["S", "A", "B", "C"];

/// 満たす答え (項目, 答え)
const CONDITIONS: [(&str, &str); 3] = [
    (BUDGET, "具体額"),
    (TIMING, "3か月以内"),
    (DECIDER, "本人が判断者"),
];

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
    let met = CONDITIONS
        .iter()
        .filter(|(id, want)| answer(items, id) == Some(want))
        .count();
    Some(RANKS[3 - met])
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
            counted_from: None,
        }
    }

    fn items(budget: Option<&str>, timing: Option<&str>, decider: Option<&str>) -> Vec<StoredItem> {
        vec![
            it("early_date", "done", None),
            it(BUDGET, "done", budget),
            it(TIMING, "done", timing),
            it("aq_job", "done", Some("現場系")),
            it("aq_reason", "done", Some("欠員")),
            it(DECIDER, "done", decider),
        ]
    }

    #[test]
    fn rank_is_the_number_of_conditions_met() {
        let r = |b, t, d| rank(&items(b, t, d));
        let (b, t, d) = (Some("具体額"), Some("3か月以内"), Some("本人が判断者"));
        assert_eq!(r(b, t, d), Some("S"));
        assert_eq!(r(b, t, Some("本人以外が判断者")), Some("A"));
        assert_eq!(r(Some("未定"), t, d), Some("A"));
        assert_eq!(r(b, Some("4か月以上"), d), Some("A"));
        assert_eq!(r(b, Some("未定"), Some("不明")), Some("B"));
        assert_eq!(r(Some("無い"), Some("不明"), d), Some("B"));
        assert_eq!(
            r(
                Some("都度確保"),
                Some("4か月以上"),
                Some("本人以外が判断者")
            ),
            Some("C")
        );
        // 聞けていない答えは満たさないとして数える
        assert_eq!(r(None, None, None), Some("C"));
        // 職種・募集の理由はランクに使わない
        let mut v = items(b, t, Some("不明"));
        v[3].answer = Some("それ以外".into());
        v[4].answer = Some("増員".into());
        assert_eq!(rank(&v), Some("A"));
    }

    #[test]
    fn calls_outside_the_hearing_have_no_rank() {
        let mut v = items(Some("具体額"), Some("3か月以内"), Some("本人が判断者"));
        v[1].status = "na".into();
        assert_eq!(rank(&v), None);
        assert_eq!(rank(&[it("early_date", "done", None)]), None);
        assert_eq!(rank(&[]), None);
    }
}
