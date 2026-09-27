//! その職種が「職種名で探されるのか、条件で探されるのか」（2026-09-27）。
//!
//! # なぜ要るか
//! `insight_title.search_style` に 125 職種ぶん入っているが、
//! コードが 1 度も参照していなかった（実測 0 箇所）。
//!
//! この軸が無いと、**打ち手が職種で違うことに気づけない**。
//! 実測（2026-09-27、直近月 2026-08）:
//!
//! ```text
//! 探され方           職種数   職種名を含む語のシェア   1求人あたり
//! 職種名で探される       48           10.7%          9.0
//! 混在                57            4.5%         14.1
//! 条件で探される         20            2.2%         15.5
//! ```
//!
//! 職種名を含む語のシェアが単調に下がる。つまり区分は実態と合っている。
//!
//! # 何が変わるか
//! [`crate::indeed::wordbrief::audit_title`] が作る「職種名を市場の語に寄せる」
//! という指摘は、**「職種名で探される」48 職種でしか効かない**。
//!
//! 「条件で探される」20 職種（販売スタッフ・ホールスタッフ・梱包・包装スタッフ・
//! 検査・接客 など）では、職種名を何に変えても見つけてもらえる量は変わらない。
//! そこでは条件（雇用形態・働き方）を書くほうが効く。
//!
//! 監査を出す前にこの軸で仕分けないと、**効かない相手に効かない助言を出す**ことになる。

/// その職種がどう探されるか。
///
/// 値は `insight_title.search_style` の 3 種類をそのまま写したもの。
/// 分析層（`scripts/indeed_build_insights.js`）が決めている。
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum SearchStyle {
    /// 職種名そのもので探される。職種名を市場の語に寄せると効く
    ByTitle,
    /// どちらもある
    Mixed,
    /// 条件（雇用形態・働き方）で探される。職種名を変えても効きにくい
    ByCondition,
}

impl SearchStyle {
    /// DB の文字列から読む。知らない値は [`None`]。
    ///
    /// # 勝手に既定値へ倒さない
    /// 読めない値を「混在」に寄せると、分析層が新しい区分を足したときに
    /// 黙って飲み込んでしまう。読めなければ読めないと返す。
    pub fn parse(s: &str) -> Option<Self> {
        match s.trim() {
            "職種名で探される" => Some(Self::ByTitle),
            "混在" => Some(Self::Mixed),
            "条件で探される" => Some(Self::ByCondition),
            _ => None,
        }
    }

    pub fn label(self) -> &'static str {
        match self {
            Self::ByTitle => "職種名で探される",
            Self::Mixed => "職種名と条件が混ざる",
            Self::ByCondition => "条件で探される",
        }
    }

    /// 職種名の監査が効く相手か。
    ///
    /// 「混在」は効く側に入れる。職種名で探す人も一定数いるため。
    pub fn title_audit_works(self) -> bool {
        matches!(self, Self::ByTitle | Self::Mixed)
    }

    /// 画面に出す一文。何をすべきかまで書く。
    pub fn reading(self) -> &'static str {
        match self {
            Self::ByTitle => {
                "この職種は、求職者が職種名そのものを打って探しています。\
                 求人票の職種名を、実際に打たれている語に寄せると見つけてもらいやすくなります。"
            }
            Self::Mixed => {
                "この職種は、職種名で探す人と条件で探す人が混ざっています。\
                 職種名を寄せることも、条件を明記することも、どちらも効く余地があります。"
            }
            Self::ByCondition => {
                "この職種は、求職者が職種名ではなく条件（雇用形態・働き方）で探しています。\
                 職種名を変えても見つけてもらえる量は変わりにくいので、\
                 条件欄の書き方を先に見直すほうが現実的です。"
            }
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn 三つの区分を読める() {
        assert_eq!(
            SearchStyle::parse("職種名で探される"),
            Some(SearchStyle::ByTitle)
        );
        assert_eq!(SearchStyle::parse("混在"), Some(SearchStyle::Mixed));
        assert_eq!(
            SearchStyle::parse("条件で探される"),
            Some(SearchStyle::ByCondition)
        );
    }

    #[test]
    fn 前後の空白があっても読める() {
        assert_eq!(SearchStyle::parse("  混在 "), Some(SearchStyle::Mixed));
    }

    #[test]
    fn 知らない値を既定値へ倒さない() {
        // 分析層が区分を足したときに、黙って「混在」に飲み込まれないこと
        for s in ["", "unknown", "職種名", "条件", "その他"] {
            assert_eq!(SearchStyle::parse(s), None, "「{s}」を読めてしまっている");
        }
    }

    #[test]
    fn 条件で探される職種には職種名の監査を出さない() {
        assert!(SearchStyle::ByTitle.title_audit_works());
        assert!(SearchStyle::Mixed.title_audit_works());
        assert!(!SearchStyle::ByCondition.title_audit_works());
    }

    #[test]
    fn 説明文は何をすべきかまで書く() {
        // 「〜です」で終わって打ち手が無い文を置かない
        for s in [
            SearchStyle::ByTitle,
            SearchStyle::Mixed,
            SearchStyle::ByCondition,
        ] {
            let r = s.reading();
            assert!(
                r.contains("と")
                    || r.contains("直す")
                    || r.contains("見直す")
                    || r.contains("寄せる"),
                "打ち手が書かれていない: {r}"
            );
            assert!(!r.contains("search_style"), "内部の列名が表に出ている: {r}");
        }
    }

    #[test]
    fn 条件で探される職種の説明は職種名を勧めない() {
        let r = SearchStyle::ByCondition.reading();
        assert!(r.contains("条件欄"), "条件欄へ誘導していない: {r}");
        assert!(
            r.contains("変わりにくい"),
            "効かないことを言っていない: {r}"
        );
    }
}
