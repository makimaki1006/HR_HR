//! 架電キューで選べるパイプラインと、ステージごとのキューへの出し方 (正本はこの表 1 箇所)。
//!
//! ユーザー決定 (2026-10-08): 架電キューはパイプラインを 1 つ選んで切り替える。既定は bpo_リクロジ。
//! ステージごとの決まり ([`StageRule`]):
//! - `All` = 常にキューに出す (未済など)
//! - `Due` = 次回架電日 `bpo_13` が今日 (JST) 以前のときだけ出す
//! - `Exclude` = 出さない (アポ日確定・架電禁止など)
//!
//! この表に無いステージ (後から HubSpot に追加されたもの) は `Exclude` と同じに扱い、検索しない。
//! 表示名は HubSpot のパイプライン定義から読む (この表はステージの ID と決まりだけを持つ。
//! ステージ名の後ろのコメントは確認用で、画面には使わない)。
//!
//! bpo_リクロジ（管理）・納品管理・債権管理・計上系・請負契約系は、架電キューの対象にしない (この表に入れない)。
//!
//! React 側は `frontend/src/generated/call_queue_pipelines.json` (テスト `export_call_queue_pipelines_json` が
//! この表から書き出す) で同じ表を読む。表を変えたら `cargo test --lib` を回して JSON をコミットする。

/// ステージをキューに出す決まり
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum StageRule {
    /// 常に出す
    All,
    /// 次回架電日が今日 (JST) 以前のときだけ出す
    Due,
    /// 出さない
    Exclude,
}

impl StageRule {
    pub fn as_str(self) -> &'static str {
        match self {
            StageRule::All => "all",
            StageRule::Due => "due",
            StageRule::Exclude => "exclude",
        }
    }
    /// キューに出しうる (`All` / `Due`)
    pub fn eligible(self) -> bool {
        !matches!(self, StageRule::Exclude)
    }
}

/// 架電キューで選べるパイプライン 1 つ
#[derive(Debug)]
pub struct QueuePipeline {
    /// HubSpot のパイプライン ID
    pub id: &'static str,
    /// HubSpot から名前を読めないときだけ使う呼び名
    pub fallback_name: &'static str,
    /// ステージ ID と決まり (HubSpot の表示順)
    pub stages: &'static [(&'static str, StageRule)],
}

impl QueuePipeline {
    /// 表にあるステージの決まり。表に無ければ `None` (呼び出し側は対象外として扱う)
    pub fn rule(&self, stage: &str) -> Option<StageRule> {
        self.stages
            .iter()
            .find(|(id, _)| *id == stage)
            .map(|(_, r)| *r)
    }
    /// キューに出しうるステージか (`All` / `Due`)
    pub fn is_eligible(&self, stage: &str) -> bool {
        self.rule(stage).is_some_and(StageRule::eligible)
    }
    /// キューに出しうるステージ (表の順)
    pub fn eligible_stages(&self) -> Vec<&'static str> {
        self.stages
            .iter()
            .filter(|(_, r)| r.eligible())
            .map(|(id, _)| *id)
            .collect()
    }
}

use StageRule::{All, Due, Exclude};

/// 既定のパイプライン (bpo_リクロジ)
pub const DEFAULT_PIPELINE_ID: &str = "753186575";

/// 選べるパイプライン (並びは画面の選択肢の順)
pub const QUEUE_PIPELINES: &[QueuePipeline] = &[
    QueuePipeline {
        id: "753186575",
        fallback_name: "bpo_リクロジ",
        stages: &[
            ("1095387442", All),     // 未済
            ("1095387443", Due),     // 不通
            ("1095387444", Due),     // 受付ブロック
            ("1095387445", Due),     // 不在
            ("1274330477", Due),     // 番号検索依頼中
            ("1095387446", Due),     // 担当者ブロック
            ("1409897995", Due),     // 成果報酬のみ
            ("1095387447", Due),     // ニーズなし/無料のみ
            ("1325087323", Due),     // ニーズなし/有料あり
            ("1325087324", Due),     // ニーズあり/無料のみ
            ("1095387448", Due),     // ニーズあり/有料あり
            ("1448079987", Due),     // SV依頼案件
            ("1319310149", Due),     // 日程確保
            ("1095457875", Exclude), // アポ日確定
            ("1095457877", Due),     // 案件差戻
            ("1095457878", Exclude), // 架電禁止 ※リーダーのみ変更
            ("1325086466", Exclude), // 商談実施処理
            ("1330563334", Due),     // 商談未実施処理
            ("1369739056", Due),     // リスト精査前
        ],
    },
    QueuePipeline {
        id: "default",
        fallback_name: "リクロジ受注管理_アポ前",
        stages: &[
            ("appointmentscheduled", All),  // 未済
            ("presentationscheduled", Due), // 受付ブロック
            ("decisionmakerboughtin", Due), // 担当者不在
            ("closedwon", Due),             // 担当者接触/担当者ブロック
            ("122445644", Due),             // 担当者_有料サービス利用経験ナシ
            ("122445645", Due),             // 担当者_有料サービス利用経験アリ
            ("1435377571", Due),            // 決定者_有料サービス利用経験ナシ
            ("1435377572", Due),            // 決定者_有料サービス利用経験アリ
            ("1435440973", Due),            // 決裁者_有料サービス利用経験ナシ
            ("1435376679", Due),            // 決裁者_有料サービス利用経験アリ
            ("1332175104", Due),            // 日程確保済み
            ("1366400580", Due),            // アポヨミ
            ("51997752", Exclude),          // TEL_アポ日確定
            ("1404977820", Exclude),        // 契約先企業
            ("1251636516", Exclude),        // 過去契約先（2025/4月以前満了）
            ("89363529", Exclude),          // 架電禁止先
            ("qualifiedtobuy", Due),        // TEL_不通
            ("1387263026", Due),            // 成果報酬のみ
            ("999032072", Exclude),         // 大分BPOリスト
            ("973220404", Exclude),         // 本社一括管理
            ("1404735259", Exclude),        // ターゲット外（業種、職種、費用0など）
            ("1430705153", Due),            // パート募集のみ
        ],
    },
    QueuePipeline {
        id: "62583420",
        fallback_name: "リクロジ_商談済リード",
        stages: &[
            ("155012220", Exclude), // T_ターゲット外案件
            ("1422059151", Due),    // 非価値合意案件
            ("155012221", Due),     // D_担当者価値合意案件
            ("155012222", Due),     // C_決定者価値合意案件
            ("155012223", Due),     // B_決裁者価値合意案件
            ("155012224", Due),     // A_決裁者価値合意案件_成約見込み
        ],
    },
    QueuePipeline {
        id: "21724969",
        fallback_name: "リクロジ受注管理_商談",
        stages: &[
            ("52035886", Exclude),   // アポ日確定
            ("1422048803", Due),     // 先方都合キャンセル
            ("1422048804", Due),     // 当社都合キャンセル
            ("52035887", Due),       // 進捗確認（商談実施済）
            ("52035888", Due),       // Dヨミ：10％（担当者との価値合意）
            ("52035889", Due),       // Cヨミ：30％（決定者との価値合意）
            ("52035890", Due),       // Bヨミ：70％（決裁者との価値合意）
            ("52035891", Due),       // Aヨミ：90％（契約手続き中）
            ("52017683", Exclude),   // 成約
            ("1074024975", Exclude), // ※使わない　大分BPO（IS失注）
            ("71794963", Exclude),   // ※使わない　日程再調整中
            ("1382780764", Exclude), // ※使わない　担当者リーチ
            ("1382780765", Exclude), // ※使わない　担当者良いね
            ("1382780766", Exclude), // ※使わない　決裁者リーチ
            ("1382780767", Exclude), // ※使わない　決済者良いね
        ],
    },
    QueuePipeline {
        id: "681393283",
        fallback_name: "リクロジエージェント",
        stages: &[
            ("1016664325", All),     // TEL_未済
            ("1025335864", All),     // TEL_未済(30名未満)
            ("1025314204", Exclude), // TEL_架電禁止先
            ("1025314205", Due),     // TEL_不通
            ("1025346953", Exclude), // TEL_本社一括
            ("1031941506", Exclude), // 人材紹介/成果報酬のみ
            ("1025346954", Due),     // TEL_担当者不在
            ("1025346955", Due),     // TEL_受付ブロック
            ("1025346956", Due),     // TEL_担当者ブロック
            ("1031941507", Due),     // TEL_担当者ブロック/可能性あり
            ("1025314206", Exclude), // TEL_アポ確定
            ("1016858793", Due),     // 再調整
            ("1031941508", Due),     // ニーズなし/無料のみ
            ("1031941509", Due),     // ニーズなし/有料あり
            ("1031941510", Due),     // ニーズあり/無料のみ
            ("1031941511", Due),     // ニーズあり/有料あり
            ("1053920383", Due),     // 長期交渉先
            ("1016858792", Exclude), // 失注
            ("998359423", Due),      // Dヨミ
            ("998359424", Due),      // Cヨミ
            ("998359425", Due),      // Bヨミ
            ("998359426", Due),      // Aヨミ
            ("998359422", Exclude),  // 導入先
            ("1075968013", Exclude), // 解約済
        ],
    },
    QueuePipeline {
        id: "913508269",
        fallback_name: "新　商談実施済リード",
        stages: &[
            ("1388087372", Exclude), // 再アプローチの必要なし
            ("1387986832", Due),     // 担当者到達 提案未実施
            ("1388087375", Due),     // 担当者到達 提案済
            ("1388087376", Due),     // 担当者価値合意
            ("1388087377", Due),     // 決裁者到達
            ("1388087378", Due),     // 決裁者の価値合意
        ],
    },
];

/// 選べるパイプラインを ID で引く。表に無ければ `None` (400 `invalid_param`)
pub fn find_pipeline(id: &str) -> Option<&'static QueuePipeline> {
    QUEUE_PIPELINES.iter().find(|p| p.id == id)
}

/// 既定のパイプライン
pub fn default_pipeline() -> &'static QueuePipeline {
    find_pipeline(DEFAULT_PIPELINE_ID).expect("既定のパイプラインは表にある")
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::collections::HashSet;

    #[test]
    fn 表の形() {
        assert_eq!(QUEUE_PIPELINES.len(), 6);
        assert_eq!(default_pipeline().id, "753186575");
        // パイプライン ID・ステージ ID は表全体で重複しない (HubSpot 内でステージ ID は一意)
        let mut pids = HashSet::new();
        let mut sids = HashSet::new();
        for p in QUEUE_PIPELINES {
            assert!(pids.insert(p.id), "{}", p.id);
            assert!(!p.eligible_stages().is_empty(), "{}", p.id);
            for (s, _) in p.stages {
                assert!(sids.insert(*s), "{s}");
            }
        }
        // 既定 (bpo_リクロジ) は従来どおり: 未済 1 + 次回日が来たら 15、対象外 3
        let d = default_pipeline();
        assert_eq!(d.rule("1095387442"), Some(All));
        assert_eq!(d.stages.iter().filter(|(_, r)| *r == Due).count(), 15);
        for x in ["1095457875", "1095457878", "1325086466"] {
            assert_eq!(d.rule(x), Some(Exclude), "{x}");
        }
        // 対象にしないパイプラインは入っていない
        assert!(find_pipeline("bpo_リクロジ（管理）").is_none());
        assert!(find_pipeline("21596025").is_none(), "納品管理");
        assert_eq!(
            find_pipeline("default")
                .unwrap()
                .rule("appointmentscheduled"),
            Some(All)
        );
        assert_eq!(
            find_pipeline("681393283")
                .unwrap()
                .stages
                .iter()
                .filter(|(_, r)| *r == All)
                .count(),
            2
        );
        assert!(find_pipeline("62583420")
            .unwrap()
            .stages
            .iter()
            .all(|(_, r)| *r != All));
    }

    /// 表を React 側 (`frontend/src/generated/call_queue_pipelines.json`) と共有する。
    /// 形: `{"default": "...", "pipelines": [{"id", "fallback_name", "stages": [{"id", "rule"}]}]}`。
    /// 内容が同じなら書き換えない (ts-rs の再生成と同じ扱い。CI は差分ゼロを確かめる)。
    #[test]
    fn export_call_queue_pipelines_json() {
        let value = serde_json::json!({
            "default": DEFAULT_PIPELINE_ID,
            "pipelines": QUEUE_PIPELINES.iter().map(|p| serde_json::json!({
                "id": p.id,
                "fallback_name": p.fallback_name,
                "stages": p.stages.iter().map(|(id, r)| serde_json::json!({"id": id, "rule": r.as_str()})).collect::<Vec<_>>(),
            })).collect::<Vec<_>>(),
        });
        let mut text = serde_json::to_string_pretty(&value).unwrap();
        text.push('\n');
        let path = std::path::Path::new(env!("CARGO_MANIFEST_DIR"))
            .join("frontend/src/generated/call_queue_pipelines.json");
        let current = std::fs::read_to_string(&path).unwrap_or_default();
        if current.replace("\r\n", "\n") != text {
            std::fs::create_dir_all(path.parent().unwrap()).unwrap();
            std::fs::write(&path, &text).unwrap();
        }
        let written = std::fs::read_to_string(&path)
            .unwrap()
            .replace("\r\n", "\n");
        assert_eq!(written, text);
        let parsed: serde_json::Value = serde_json::from_str(&written).unwrap();
        assert_eq!(parsed["default"], "753186575");
        assert_eq!(parsed["pipelines"].as_array().unwrap().len(), 6);
        assert_eq!(
            parsed["pipelines"][1]["stages"][0]["id"],
            "appointmentscheduled"
        );
        assert_eq!(parsed["pipelines"][1]["stages"][0]["rule"], "all");
    }
}
