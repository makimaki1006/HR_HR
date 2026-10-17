//! 架電キューで選べるパイプラインと、ステージごとのキューへの出し方 (正本はこの表 1 箇所)。
//!
//! ユーザー決定 (2026-10-12): この表のパイプラインを複数選べる。既定は従来どおり1つ。
//! 架電対象は従来の決まり、すべての取引では選んだパイプラインの全ステージを検索する。
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

/// 案件管理から同じパイプライン内のステージ移動だけを許す (2026-10-16 ユーザー決定)。
/// 架電の候補・件数には使わない。自分の項目では条件に選べる。StageRule はすべて Exclude。
pub const WRITE_ONLY_PIPELINES: &[QueuePipeline] = &[
    QueuePipeline {
        id: "21596025",
        fallback_name: "リクロジ_納品管理",
        stages: &[
            ("52016153", Exclude),   // 運用MTG実施前
            ("52016154", Exclude),   // 運用MTG実施済
            ("52016155", Exclude),   // 求人出稿完了
            ("1281184160", Exclude), // 継続_定期実施前（早巻き）
            ("1049738304", Exclude), // オプション（求人追加・一次対応）
            ("121846962", Exclude),  // 基本料プラン
            ("52016156", Exclude),   // 定期1
            ("121846961", Exclude),  // 定期2
            ("164378752", Exclude),  // 定期3
            ("164378755", Exclude),  // 定期4
            ("164378758", Exclude),  // 定期5
            ("1177549843", Exclude), // 定期6
            ("1177549844", Exclude), // 定期7
            ("1177549845", Exclude), // 定期8
            ("1177549846", Exclude), // 定期9
            ("1177549847", Exclude), // 定期10
            ("1177549848", Exclude), // 定期11
            ("164378760", Exclude),  // 継続MTG前_壁打ち済
            ("52016157", Exclude),   // 継続MTG
            ("1216616080", Exclude), // Tヨミ：10％未満（継続に対して否定的意見、解約意向がある提案（10％未満））
            ("1216616081", Exclude), // Dヨミ：20％（継続意思不明だが提案中）
            ("1216616082", Exclude), // Cヨミ：50％（担当者の継続意思あり）
            ("1216616083", Exclude), // Bヨミ：80％（決裁者の継続意思あり）
            ("1281526627", Exclude), // 満了済オプション（一次対応・追加）
            ("1016664339", Exclude), // 解約済架電禁止先
            ("90598807", Exclude),   // 解約済（充足）
            ("52016159", Exclude),   // 解約済(成果不足)
            ("52016158", Exclude),   // 解約済(会社方針・その他)
            ("66848546", Exclude),   // 継続済
            ("1278456227", Exclude), // マーケ関連
        ],
    },
    QueuePipeline {
        id: "743961896",
        fallback_name: "請負契約_納品管理",
        stages: &[
            ("1082007419", Exclude), // キックオフMTG実施前
            ("1082007420", Exclude), // キックオフMTG実施済
            ("1082007421", Exclude), // 構成案提出済
            ("1082007422", Exclude), // 構成案fix
            ("1082007423", Exclude), // 初稿提出済
            ("1082007424", Exclude), // 1記事納品完了
            ("1082007425", Exclude), // 2記事納品完了
            ("1082007426", Exclude), // 3記事納品完了
            ("1082007427", Exclude), // 4記事納品完了
            ("1082007428", Exclude), // 5記事納品完了
            ("1332934971", Exclude), // 6記事納品完了
            ("1332934972", Exclude), // 7記事納品完了
            ("1332934973", Exclude), // 8記事納品完了
            ("1332934974", Exclude), // 9記事納品完了
            ("1332934975", Exclude), // 10記事納品完了
            ("1332934976", Exclude), // 11記事納品完了
            ("1082007429", Exclude), // 納品完了（全記事）
        ],
    },
];

/// 書き込みと必須ルールの照合に使う全パイプライン。
pub fn write_pipelines() -> impl Iterator<Item = &'static QueuePipeline> {
    QUEUE_PIPELINES.iter().chain(WRITE_ONLY_PIPELINES.iter())
}

pub fn find_write_pipeline(id: &str) -> Option<&'static QueuePipeline> {
    write_pipelines().find(|p| p.id == id)
}

pub fn is_write_only_pipeline(id: &str) -> bool {
    WRITE_ONLY_PIPELINES.iter().any(|p| p.id == id)
}

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

    #[test]
    fn 書き込み専用の表は架電に混ぜず全ステージを照合できる() {
        assert_eq!(WRITE_ONLY_PIPELINES.len(), 2);
        assert_eq!(WRITE_ONLY_PIPELINES[0].stages.len(), 30);
        assert_eq!(WRITE_ONLY_PIPELINES[1].stages.len(), 17);
        assert_eq!(write_pipelines().count(), 8);
        let mut pids = HashSet::new();
        let mut sids = HashSet::new();
        for p in write_pipelines() {
            assert!(pids.insert(p.id));
            for (sid, _) in p.stages {
                assert!(sids.insert(*sid), "{sid}");
            }
        }
        for p in WRITE_ONLY_PIPELINES {
            assert!(find_pipeline(p.id).is_none());
            assert!(p.eligible_stages().is_empty());
            assert_eq!(find_write_pipeline(p.id).unwrap().id, p.id);
        }
        let p = find_write_pipeline("21596025").unwrap();
        assert_eq!(p.stages.first().unwrap().0, "52016153");
        assert_eq!(p.stages.last().unwrap().0, "1278456227");
        assert!(p.rule("66848546").is_some());
        assert!(p.rule("1082007429").is_none());
        let p = find_write_pipeline("743961896").unwrap();
        assert_eq!(p.stages.first().unwrap().0, "1082007419");
        assert_eq!(p.stages.last().unwrap().0, "1082007429");
    }

    /// 表を React 側 (`frontend/src/generated/call_queue_pipelines.json`) と共有する。
    /// 形: `{"default": "...", "pipelines": [{"id", "fallback_name", "stages": [{"id", "rule"}]}]}`。
    /// 内容が同じなら書き換えない (ts-rs の再生成と同じ扱い。CI は差分ゼロを確かめる)。
    #[test]
    fn export_metric_pipelines_json() {
        let value = serde_json::json!(write_pipelines().map(|p| serde_json::json!({
            "id": p.id, "fallback_name": p.fallback_name,
            "stages": p.stages.iter().map(|(id, _)| serde_json::json!({"id": id, "rule": "exclude"})).collect::<Vec<_>>()
        })).collect::<Vec<_>>());
        let text = serde_json::to_string_pretty(&value).unwrap() + "\n";
        let path = std::path::Path::new(env!("CARGO_MANIFEST_DIR"))
            .join("frontend/src/generated/metric_pipelines.json");
        if std::fs::read_to_string(&path).unwrap_or_default() != text {
            std::fs::write(path, text).unwrap();
        }
    }

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
