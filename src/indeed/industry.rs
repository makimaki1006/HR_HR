//! Indeed の 20 分類を、話が通じる 5 つの業界にまとめる。
//!
//! # なぜまとめるか
//! Indeed の分類は「製造・生産」と「製造・開発（電気・機械・金属・化学）」のように、
//! 募集する会社が重なるのに別々になっているものがある。20 個のまま並べると、
//! 同じ会社の話が離れた場所に出て、読む人が繋げられない。
//!
//! # この対応表の出どころ
//! 顧客に配る見本（`claudedocs/indeed_newsletter.html` の
//! 「4.1 業界のまとめ方」）で、まとめ方とその理由まで説明した状態で既に使っている。
//! 紙とアプリで違うまとめ方をすると、同じ会社の話が食い違う。**見本と同じにする。**
//!
//! # 5 つに入らないものは、無理に入れない
//! 農林水産の仕事は他と性質が違い、Indeed 側の分類が実態と離れているものもある。
//! 見本と同じく「5 業界の外」として別に数える。分からないものを既存の枠に
//! 押し込むと、その業界の数字が静かに歪む。

/// 業界 1 つ。
pub struct Industry {
    /// 画面に出す名前
    pub name: &'static str,
    /// この業界にまとめる Indeed の分類
    pub categories: &'static [&'static str],
    /// なぜこうまとめたか。顧客レポートの「業界のまとめ方」に出す
    pub why: &'static str,
}

/// 5 業界。並びは見本と同じ。
pub const INDUSTRIES: [Industry; 5] = [
    Industry {
        name: "物流・運輸",
        categories: &["物流・配送", "軽作業", "送迎ドライバー"],
        why: "荷物を運ぶ仕事と、倉庫の中で荷物を扱う仕事をまとめています。\
              人を乗せる運転（タクシー・バス）も同じ枠に入れました。",
    },
    Industry {
        name: "製造・生産",
        categories: &["製造・生産", "製造・開発 (電気・機械・金属・化学)"],
        why: "工場のライン作業と、設計・技術の仕事を同じ枠にしています。\
              Indeed では別々の分類ですが、募集する会社が重なるためまとめました。",
    },
    Industry {
        name: "建設・設備・整備",
        categories: &["建設・土木", "建築・インテリア・造園", "保全・管理（設備・建物）"],
        why: "つくる仕事と、できたものを保つ仕事をまとめています。\
              Indeed の「保全・管理（設備・建物）」に自動車整備士やタイヤ交換が入っているため、\
              この枠には車の整備も含まれます。",
    },
    Industry {
        name: "サービス・販売",
        categories: &["接客・販売", "飲食・フード", "清掃", "警備・誘導"],
        why: "お客さまや現場に人が立つ仕事をまとめています。\
              店舗・飲食・清掃・警備は、募集の出し方や時給の水準が近い枠として扱いました。",
    },
    Industry {
        name: "事務・管理",
        categories: &[
            "事務・オフィスワーク",
            "経営・管理・企画・戦略",
            "営業 無形商材",
            "会計・監査法人",
            "法律",
            "未分類",
        ],
        why: "机まわりの仕事をまとめています。\
              Indeed 側で分類がついていない職種（「管理」）もここに入れました。",
    },
];

/// 5 業界に入れないものの呼び名。
pub const OUTSIDE: &str = "5 業界の外";

/// 分類名から業界名を引く。5 つに入らなければ `None`。
///
/// 知らない分類は「事務・管理」などに寄せず、必ず外に出す。
/// 分からないものを既存の枠に入れると、その業界の数字が静かに増える。
pub fn of_category(category: &str) -> Option<&'static str> {
    INDUSTRIES
        .iter()
        .find(|i| i.categories.contains(&category))
        .map(|i| i.name)
}

/// 業界名から説明を引く。
pub fn why(name: &str) -> Option<&'static str> {
    INDUSTRIES.iter().find(|i| i.name == name).map(|i| i.why)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn 見本と同じまとめ方になっている() {
        assert_eq!(of_category("物流・配送"), Some("物流・運輸"));
        assert_eq!(of_category("軽作業"), Some("物流・運輸"));
        assert_eq!(of_category("送迎ドライバー"), Some("物流・運輸"));
        assert_eq!(of_category("製造・生産"), Some("製造・生産"));
        assert_eq!(
            of_category("製造・開発 (電気・機械・金属・化学)"),
            Some("製造・生産")
        );
        assert_eq!(of_category("保全・管理（設備・建物）"), Some("建設・設備・整備"));
        assert_eq!(of_category("清掃"), Some("サービス・販売"));
        assert_eq!(of_category("未分類"), Some("事務・管理"));
    }

    /// 知らない分類を既存の枠に寄せないこと。
    #[test]
    fn 知らない分類は外に出す() {
        assert_eq!(of_category("農林水産業"), None);
        assert_eq!(of_category("ホテル・旅行・冠婚葬祭"), None);
        assert_eq!(of_category("その他"), None);
        assert_eq!(of_category(""), None);
        assert_eq!(of_category("まだ無い分類"), None);
    }

    /// 同じ分類が 2 つの業界に入っていないこと。
    ///
    /// 重複すると合計が全体を超える。
    #[test]
    fn 分類が二重に登録されていない() {
        let mut seen: Vec<&str> = Vec::new();
        for ind in INDUSTRIES.iter() {
            for c in ind.categories {
                assert!(
                    !seen.contains(c),
                    "{c} が複数の業界に入っている（合計が全体を超える）"
                );
                seen.push(c);
            }
        }
        assert_eq!(seen.len(), 18, "分類の数が変わっている: {seen:?}");
    }

    /// まとめ方の説明が全業界にあること。顧客レポートに出すため。
    #[test]
    fn まとめた理由が全業界にある() {
        for ind in INDUSTRIES.iter() {
            assert!(!ind.why.is_empty(), "{} の説明が空", ind.name);
            assert!(why(ind.name).is_some());
        }
        assert_eq!(why("存在しない業界"), None);
    }
}
