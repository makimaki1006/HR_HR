//! 職種推定グループ別 給与参考クロス表 (Round 3-C / 2026-05-09)
//!
//! ## 背景
//! Round 1-E 完全欠落 Top 1「職種×給与」と Round 2-4 真の未実装 #7 を消化。
//! Round 3-A (産業構成 e-Stat) + Round 3-B (業界推定×給与参考 CSV) に続き、
//! 職種粒度の給与水準表を MI PDF に追加する。
//!
//! ## Round 3-C' (2026-05-09): MECE 16 グループ拡張 + 信頼度ルール
//! 職種推定の語彙を 10 → 16 グループに MECE 拡張。
//! Direct (具体職種語) / Reference (広義語) / LowConfidence (多義語・会社名由来)
//! の信頼度ルールを導入し、note 列の表示を 3 段階に細分化。
//!
//! ## 設計方針
//! - **B 案採用**: per-record 職種コード / 標準化 occupation が `SurveyAggregation`
//!   に存在しないため、`by_tag_salary` (主信号) と `by_company` (補助) のキーワード
//!   からの職種推定。「職種別」断定は科学的根拠なし → 「職種推定グループ」「参考」必須。
//! - **既存集計の再利用**: Round 3-B `industry_salary` と同パターン。新規数値ロジック
//!   なし、`aggregator` 経路で既に正規化済の `avg_salary` / `median_salary` を再利用。
//! - **MI variant 専用**: `mod.rs` で MI variant のみ呼び出し、Full / Public 不変。
//! - **件数 < 3 は「参考 (低信頼)」**: 推定誤差・サンプル不足を明示。
//!
//! ## 関連 memory ルール
//! - `feedback_correlation_not_causation.md` 「相関≠因果」: caveat に明記
//! - `feedback_neutral_expression_for_targets.md` 「中立表現」: 評価語禁止
//! - Hard NG 13 用語 + HW 連想語不混入

use super::super::aggregator::SurveyAggregation;
use super::super::super::helpers::{escape_html, format_number};
use super::helpers::{render_figure_caption, render_read_hint, render_section_howto};

/// 職種推定の信頼度.
///
/// - `Direct`: 求人タグに具体職種語が hit (例: 看護師、介護福祉士、施工管理、ドライバー)
/// - `Reference`: 求人タグに広義語が hit (例: ケア、メディカル、サービス、物流、工場)
/// - `LowConfidence`: 多義語・会社名由来 (例: アテンダー、スタッフ、店員、by_company 経路全般)
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub(super) enum OccupationConfidence {
    Direct,
    Reference,
    LowConfidence,
}

/// 職種推定グループ別 給与参考 1 行分の集計結果.
#[derive(Debug, Clone, PartialEq, Eq)]
pub(super) struct OccupationSalaryRow {
    /// 職種推定グループ名 (例: "看護系", "介護系")
    pub occupation: String,
    /// 推定された求人件数
    pub count: i64,
    /// 件数加重平均給与 (ネイティブ単位、円)
    pub weighted_avg: i64,
    /// median 提示用 (`CompanyAgg.median_salary` の中央値、円)
    pub median_of_company_medians: Option<i64>,
    /// "月給" / "時給"
    pub unit_label: &'static str,
    /// 「参考 (低信頼)」/ 「参考」/ 「-」 等
    pub note: &'static str,
}

#[derive(Debug, Default)]
struct OccupationBucket {
    count: i64,
    sum_weighted_avg: i64,
    company_medians: Vec<i64>,
    /// このバケットで観測された最良の信頼度 (None=未観測 / LowConfidence / Reference / Direct)
    best_confidence: Option<OccupationConfidence>,
}

impl OccupationBucket {
    fn upgrade_confidence(&mut self, c: OccupationConfidence) {
        let rank = |x: OccupationConfidence| match x {
            OccupationConfidence::Direct => 3,
            OccupationConfidence::Reference => 2,
            OccupationConfidence::LowConfidence => 1,
        };
        match self.best_confidence {
            None => self.best_confidence = Some(c),
            Some(cur) if rank(c) > rank(cur) => self.best_confidence = Some(c),
            _ => {}
        }
    }
}

/// キーワードが「具体職種語」(Direct) かどうか判定.
fn is_direct_keyword(s_lower: &str) -> bool {
    const DIRECT: &[&str] = &[
        // 具体職種語 (国家資格名・正式職種名)
        "看護師", "准看護師", "介護福祉士", "ヘルパー", "保育士",
        "理学療法士", "作業療法士", "言語聴覚士", "薬剤師", "ケアマネジャー",
        "ケアマネージャー", "ケアマネ", "調理師", "管理栄養士", "栄養士",
        "ドライバー", "運転手", "施工管理", "現場監督", "警備員", "清掃員",
        "歯科衛生士", "歯科医師", "臨床検査技師", "放射線技師", "臨床工学技士",
        "助産師", "保健師", "社会福祉士", "精神保健福祉士",
        "プログラマ", "プログラマー", "エンジニア", "店長",
        "施設長", "サービス提供責任者", "サービス管理責任者",
        // 主要広範語でも具体性高いもの
        "看護", "准看", "介護", "保育", "リハビリ", "調理", "警備", "清掃",
        "事務", "営業", "販売",
    ];
    DIRECT.iter().any(|kw| s_lower.contains(&kw.to_lowercase()))
}

/// キーワードが「広義語」(Reference) かどうか判定 (Direct より弱いシグナル).
fn is_reference_keyword(s_lower: &str) -> bool {
    const REFERENCE: &[&str] = &[
        "ケア", "メディカル", "サービス", "物流", "工場", "店舗", "フロント",
    ];
    REFERENCE.iter().any(|kw| s_lower.contains(&kw.to_lowercase()))
}

/// キーワードが「多義語」(LowConfidence) かどうか判定.
fn is_ambiguous_keyword(s_lower: &str) -> bool {
    const AMBIGUOUS: &[&str] = &["アテンダー", "スタッフ", "店員"];
    AMBIGUOUS.iter().any(|kw| s_lower.contains(&kw.to_lowercase()))
}

/// 求人タイトル / タグ / 会社名から職種推定グループを判定する.
///
/// MECE 16 グループ. 専門度の高い (より具体的な) グループから順にチェックし、
/// 早期 return で優先順位を保証する。
///
/// 判定優先順位:
/// 1. 看護系 → 2. 介護系 → 3. リハビリ・療法士系 → 4. 医療技術・薬局系
/// → 5. 福祉・相談支援系 → 6. 保育・教育系 → 7. 建築・土木・設備系
/// → 8. 物流・配送・ドライバー系 → 9. 製造・軽作業系 → 10. 警備・清掃・施設管理系
/// → 11. 飲食・調理系 → 12. 事務・バックオフィス系 → 13. 営業・販売促進系
/// → 14. 販売・接客・サービス系 → 15. IT・技術専門職系 → 16. 管理・マネジメント系
pub(crate) fn map_keyword_to_occupation_group(s: &str) -> Option<&'static str> {
    let s = s.to_lowercase();
    if s.is_empty() {
        return None;
    }
    // 1. 看護系
    if s.contains("看護") || s.contains("准看") || s.contains("ナース")
        || s.contains("訪問看護") || s.contains("病棟") || s.contains("外来")
    {
        return Some("看護系");
    }
    // 2. 介護系
    // 注: "ケア" 単独は介護系扱いだが、"ケアマネ"/"ケースワーカー" は福祉・相談支援系 (5) 優先のため除外
    if s.contains("介護") || s.contains("介護士") || s.contains("介護職")
        || s.contains("介護福祉士") || s.contains("ケアワーカー")
        || s.contains("介助") || s.contains("ヘルパー")
        || s.contains("施設介護") || s.contains("老人ホーム")
        || s.contains("デイサービス") || s.contains("グループホーム")
        || s.contains("特養") || s.contains("老健") || s.contains("サ高住")
        || s.contains("有料老人") || s.contains("初任者研修")
        || s.contains("実務者研修") || s.contains("訪問介護")
        || (s.contains("ケア")
            && !s.contains("ケアマネ")
            && !s.contains("ケースワーカー"))
    {
        return Some("介護系");
    }
    // 3. リハビリ・療法士系
    if s.contains("リハビリ") || s.contains("理学療法") || s.contains("作業療法")
        || s.contains("言語聴覚") || s.contains("リハ職")
        || (s.contains("pt") && !s.contains("apt") && !s.contains("opt"))
        || s.contains("ｐｔ") || s.contains("ot") || s.contains("st")
        || s.contains("機能訓練") || s.contains("柔道整復")
        || s.contains("あん摩") || s.contains("鍼灸")
    {
        return Some("リハビリ・療法士系");
    }
    // 4. 医療技術・薬局系
    // 「メディカル」「病院」等の医療系広義語もここに分類 (Reference 信頼度)
    if s.contains("薬剤") || s.contains("薬局") || s.contains("調剤")
        || s.contains("歯科") || s.contains("歯科衛生士") || s.contains("歯科助手")
        || s.contains("臨床検査") || s.contains("放射線") || s.contains("レントゲン")
        || s.contains("検査技師") || s.contains("臨床工学")
        || (s.contains("技師") && !s.contains("整備技師"))
        || s.contains("メディカル") || s.contains("病院") || s.contains("クリニック")
        || s.contains("診療")
    {
        return Some("医療技術・薬局系");
    }
    // 5. 福祉・相談支援系
    if s.contains("相談員") || s.contains("生活相談") || s.contains("支援員")
        || s.contains("生活支援") || s.contains("就労支援")
        || s.contains("児童指導") || s.contains("ケースワーカー")
        || s.contains("ソーシャルワーカー") || s.contains("社会福祉士")
        || s.contains("精神保健福祉")
        || s.contains("サービス管理責任者") || s.contains("サ責")
        || s.contains("サービス提供責任者")
        || s.contains("障害者支援") || s.contains("ケアマネ")
    {
        return Some("福祉・相談支援系");
    }
    // 6. 保育・教育系
    if s.contains("保育") || s.contains("保育士") || s.contains("こども")
        || s.contains("子ども") || s.contains("児童") || s.contains("学童")
        || s.contains("幼稚園") || s.contains("教員") || s.contains("講師")
        || s.contains("指導員")
    {
        return Some("保育・教育系");
    }
    // 7. 建築・土木・設備系
    if s.contains("建築") || s.contains("建設") || s.contains("土木")
        || s.contains("施工") || s.contains("施工管理") || s.contains("現場監督")
        || s.contains("現場作業") || s.contains("設備") || s.contains("電気工事")
        || s.contains("管工事") || s.contains("配管") || s.contains("内装")
        || s.contains("外構") || s.contains("解体") || s.contains("cad")
        || s.contains("ｃａｄ") || s.contains("測量") || s.contains("大工")
        || s.contains("溶接") || s.contains("塗装") || s.contains("空調")
    {
        return Some("建築・土木・設備系");
    }
    // 8. 物流・配送・ドライバー系
    if s.contains("ドライバー") || s.contains("運転手") || s.contains("運転")
        || s.contains("配送") || s.contains("配達") || s.contains("送迎")
        || s.contains("物流") || s.contains("倉庫") || s.contains("仕分け")
        || s.contains("ピッキング") || s.contains("梱包")
        || s.contains("フォークリフト") || s.contains("ルート配送")
        || s.contains("軽貨物") || s.contains("トラック")
        || s.contains("運搬") || s.contains("入出庫")
    {
        return Some("物流・配送・ドライバー系");
    }
    // 9. 製造・軽作業系
    if s.contains("製造") || s.contains("工場") || s.contains("軽作業")
        || s.contains("作業員") || s.contains("組立") || s.contains("加工")
        || s.contains("検品") || s.contains("検査") || s.contains("包装")
        || s.contains("ライン") || s.contains("生産") || s.contains("品質管理")
        || s.contains("機械オペレーター") || s.contains("オペレーター")
        || s.contains("仕上げ") || s.contains("部品")
    {
        return Some("製造・軽作業系");
    }
    // 10. 警備・清掃・施設管理系
    if s.contains("警備") || s.contains("交通誘導") || s.contains("施設警備")
        || s.contains("清掃") || s.contains("ビルメン") || s.contains("ベッドメイク")
        || s.contains("設備管理") || s.contains("施設管理") || s.contains("管理人")
        || s.contains("巡回") || s.contains("守衛")
    {
        return Some("警備・清掃・施設管理系");
    }
    // 11. 飲食・調理系
    if s.contains("調理") || s.contains("厨房") || s.contains("調理補助")
        || s.contains("栄養士") || s.contains("管理栄養士") || s.contains("飲食")
        || s.contains("レストラン") || s.contains("カフェ") || s.contains("食堂")
        || s.contains("キッチン") || s.contains("給食") || s.contains("洗い場")
        || s.contains("ホールスタッフ") || s.contains("料理人")
        || s.contains("シェフ") || s.contains("クック")
    {
        return Some("飲食・調理系");
    }
    // 12. 事務・バックオフィス系
    if s.contains("事務") || s.contains("一般事務") || s.contains("医療事務")
        || s.contains("営業事務") || s.contains("受付") || s.contains("経理")
        || s.contains("総務") || s.contains("人事") || s.contains("労務")
        || s.contains("庶務") || s.contains("データ入力")
        || s.contains("コールセンター") || s.contains("カスタマーサポート")
    {
        return Some("事務・バックオフィス系");
    }
    // 13. 営業・販売促進系
    // 2026-05-14: 単独「営業」「pr」は過誤判定が多い (例: 「営業時間」「営業所」
    //             「営業日」「prefecture」「product」「spring」等の部分一致)。
    //             具体的な職種語彙のみで判定する。
    if s.contains("営業職") || s.contains("営業担当") || s.contains("営業マン")
        || s.contains("法人営業") || s.contains("個人営業")
        || s.contains("ルート営業") || s.contains("提案営業") || s.contains("反響営業")
        || s.contains("ラウンダー") || s.contains("販促") || s.contains("販売促進")
        || s.contains("インサイドセールス") || s.contains("テレアポ")
        || s.contains("テレマーケティング")
        || s.contains("広報") || s.contains("マーケティング")
    {
        return Some("営業・販売促進系");
    }
    // 14. 販売・接客・サービス系
    // 2026-05-14: 単独「販売」「ホール」「カウンター」は過誤判定が多い (例:
    //             「販売価格」「販売代理店」「コンサートホール」「ホールディングス」
    //             「カウンターパート」)。具体語のみ。
    if s.contains("販売員") || s.contains("販売職") || s.contains("販売スタッフ")
        || s.contains("接客") || s.contains("店舗")
        || s.contains("売場") || s.contains("店長") || s.contains("レジ")
        || s.contains("フロント") || s.contains("ホールスタッフ") || s.contains("ホール係")
        || s.contains("サービススタッフ")
        || s.contains("アテンダー") || s.contains("案内係") || s.contains("受付案内")
        || s.contains("カウンター業務") || s.contains("カウンタースタッフ")
    {
        return Some("販売・接客・サービス系");
    }
    // 15. IT・技術専門職系
    // 2026-05-14: 単独「it」は to_lowercase 後に "wait" "split" 等にも部分一致してしまうため除外。
    //             「ｉｔ」(全角) は誤一致が少なく残す。
    if s.contains("ｉｔ") || s.contains("エンジニア")
        || s.contains("プログラマ") || s.contains("se ")
        || s.contains("システム") || s.contains("web") || s.contains("ｗｅｂ")
        || s.contains("アプリ") || s.contains("インフラ") || s.contains("ネットワーク")
        || s.contains("サーバー") || s.contains("情シス") || s.contains("dx")
        || s.contains("ヘルプデスク") || s.contains("it職") || s.contains("itエンジニア")
    {
        return Some("IT・技術専門職系");
    }
    // 16. 管理・マネジメント系
    if s.contains("管理者") || s.contains("管理職") || s.contains("施設長")
        || s.contains("所長") || s.contains("店長候補") || s.contains("マネージャー")
        || s.contains("リーダー") || s.contains("主任") || s.contains("責任者")
        || s.contains("sv") || s.contains("スーパーバイザー")
        || s.contains("係長") || s.contains("課長")
    {
        return Some("管理・マネジメント系");
    }
    None
}

/// グループ判定 + 信頼度判定 をまとめて返す.
pub(crate) fn map_keyword_to_occupation_group_with_confidence(
    s: &str,
) -> Option<(&'static str, OccupationConfidence)> {
    let group = map_keyword_to_occupation_group(s)?;
    let s_lower = s.to_lowercase();
    let confidence = if is_ambiguous_keyword(&s_lower) {
        OccupationConfidence::LowConfidence
    } else if is_direct_keyword(&s_lower) {
        OccupationConfidence::Direct
    } else if is_reference_keyword(&s_lower) {
        OccupationConfidence::Reference
    } else {
        // グループには hit したが Direct/Reference/Ambiguous いずれの語彙にも含まれない
        // → タグ由来として広義 (Reference) 扱い
        OccupationConfidence::Reference
    };
    Some((group, confidence))
}

/// `SurveyAggregation` を職種推定グループ単位で再集計する.
///
/// # 戻り値
/// - 件数降順で Top 10 まで
/// - 推定不能 (キーワード非マッチ) は除外
/// - `by_tag_salary` / `by_company` が空の場合は空 Vec
pub(super) fn aggregate_occupation_salary(agg: &SurveyAggregation) -> Vec<OccupationSalaryRow> {
    let mut buckets: std::collections::HashMap<&'static str, OccupationBucket> =
        std::collections::HashMap::new();

    // 信号 A (主): by_tag_salary タグ → 職種グループ → 件数 + avg_salary
    // 信頼度: タグ語彙に応じて Direct / Reference (LowConfidence にはならない)
    for tag in &agg.by_tag_salary {
        if tag.count == 0 || tag.avg_salary <= 0 {
            continue;
        }
        let Some((group, confidence)) =
            map_keyword_to_occupation_group_with_confidence(&tag.tag)
        else {
            continue;
        };
        let bucket = buckets.entry(group).or_default();
        bucket.count += tag.count as i64;
        bucket.sum_weighted_avg += tag.avg_salary * tag.count as i64;
        bucket.upgrade_confidence(confidence);
    }

    // 信号 B (補助): by_company 会社名 → 職種グループ。信号 A 未カバーのみ加算 (二重カウント防止)
    // 信頼度: 会社名由来は常に LowConfidence (弱い信号)
    for company in &agg.by_company {
        if company.count == 0 || company.avg_salary <= 0 {
            continue;
        }
        let Some(group) = map_keyword_to_occupation_group(&company.name) else {
            continue;
        };
        if buckets.contains_key(group) {
            // 信号 A 既カバー → median のみ補完 (avg は二重カウント回避)
            if company.median_salary > 0 {
                buckets
                    .get_mut(group)
                    .unwrap()
                    .company_medians
                    .push(company.median_salary);
            }
            continue;
        }
        let bucket = buckets.entry(group).or_default();
        bucket.count += company.count as i64;
        bucket.sum_weighted_avg += company.avg_salary * company.count as i64;
        if company.median_salary > 0 {
            bucket.company_medians.push(company.median_salary);
        }
        bucket.upgrade_confidence(OccupationConfidence::LowConfidence);
    }

    let unit_label: &'static str = if agg.is_hourly { "時給" } else { "月給" };

    let mut rows: Vec<OccupationSalaryRow> = buckets
        .into_iter()
        .filter(|(_, b)| b.count > 0)
        .map(|(group, b)| {
            let weighted_avg = b.sum_weighted_avg / b.count;
            let median = if b.company_medians.is_empty() {
                None
            } else {
                let mut v = b.company_medians.clone();
                v.sort();
                let n = v.len();
                Some(if n.is_multiple_of(2) {
                    (v[n / 2 - 1] + v[n / 2]) / 2
                } else {
                    v[n / 2]
                })
            };
            // note 決定:
            // - count < 3 → "参考 (低信頼)"
            // - best_confidence が LowConfidence or None → "参考 (低信頼)"
            // - best_confidence が Reference → "参考"
            // - best_confidence が Direct → ""
            let note: &'static str = if b.count < 3 {
                "参考 (低信頼)"
            } else {
                match b.best_confidence {
                    Some(OccupationConfidence::Direct) => "",
                    Some(OccupationConfidence::Reference) => "参考",
                    Some(OccupationConfidence::LowConfidence) | None => "参考 (低信頼)",
                }
            };
            OccupationSalaryRow {
                occupation: group.to_string(),
                count: b.count,
                weighted_avg,
                median_of_company_medians: median,
                unit_label,
                note,
            }
        })
        .collect();

    rows.sort_by(|a, b| b.count.cmp(&a.count).then_with(|| b.weighted_avg.cmp(&a.weighted_avg)));
    rows.truncate(10);
    rows
}

/// MI variant PDF に「職種推定グループ別 給与参考」セクションを描画する.
pub(super) fn render_section_occupation_salary(html: &mut String, agg: &SurveyAggregation) {
    let rows = aggregate_occupation_salary(agg);
    if rows.is_empty() {
        return;
    }
    let unit_yen_label = if agg.is_hourly { "円/時" } else { "円" };
    let manyen_or_yen_label = if agg.is_hourly { "円/時" } else { "万円" };

    html.push_str("<div class=\"section\" data-testid=\"occupation-salary-section\">\n");
    html.push_str("<h2>職種推定グループ別 給与参考</h2>\n");

    render_section_howto(
        html,
        &[
            "アップロードした CSV を求人タグ・企業名のキーワードから推定した職種グループ単位で集約し、給与の参考値を提示します",
            "原 CSV に標準化された職種コードが無いため、キーワードから推定したグループです（公的職業分類とは一致しない場合があります）",
            "件数 3 件未満のグループは「参考 (低信頼)」と表示します",
        ],
    );

    render_figure_caption(
        html,
        "表 6-4",
        "職種推定グループ別 給与参考（タグ・企業名由来の推定、件数 Top 10）",
    );

    html.push_str(
        "<p class=\"mi-table-note\" style=\"font-size:9pt;color:#6b7280;margin-bottom:6px;\">\
        \u{26A0} 推定・参考値: 本表は CSV に標準化された職種コードがないため、求人タグ・企業名から推定した職種グループです。\
        給与値は求人 CSV 上の給与情報を月給換算した参考値であり、\
        公的職業分類（総務省統計局 日本標準職業分類等）や法人 DB の正式分類とは一致しない場合があります。\
        全体給与中央値（表紙ハイライト KPI）と一致しない指標です。\
        件数 3 件以上を集計対象とし、3 件未満は「参考 (低信頼)」として併記します。\
        </p>\n",
    );

    html.push_str(
        "<table class=\"sortable-table zebra\" data-testid=\"occupation-salary-table\">\n",
    );
    html.push_str(&format!(
        "<thead><tr>\
        <th>#</th>\
        <th>職種推定グループ</th>\
        <th style=\"text-align:right\">件数</th>\
        <th style=\"text-align:right\">{unit} 参考平均</th>\
        <th style=\"text-align:right\">{unit} 推定グループ中央値</th>\
        <th>信頼度</th>\
        </tr></thead>\n<tbody>\n",
        unit = match agg.is_hourly {
            true => "時給",
            false => "月給",
        },
    ));

    for (i, r) in rows.iter().enumerate() {
        let avg_text = format_value_text(r.weighted_avg, agg.is_hourly);
        let median_text = match r.median_of_company_medians {
            Some(m) => format_value_text(m, agg.is_hourly),
            None => "-".to_string(),
        };
        html.push_str(&format!(
            "<tr>\
                <td>{rank}</td>\
                <td>{name}</td>\
                <td class=\"num\">{count}件</td>\
                <td class=\"num\">{avg} {unit}</td>\
                <td class=\"num\">{med} {unit}</td>\
                <td>{note}</td>\
            </tr>\n",
            rank = i + 1,
            name = escape_html(&r.occupation),
            count = format_number(r.count),
            avg = avg_text,
            med = median_text,
            unit = manyen_or_yen_label,
            note = r.note,
        ));
    }
    html.push_str("</tbody></table>\n");

    html.push_str(&format!(
        "<p class=\"caveat\" style=\"font-size:9pt;color:#475569;margin-top:8px;\">\
        \u{26A0} 職種推定はタグ・企業名のキーワードからの推定（例:「看護師」「介護福祉士」「リハビリ」「ドライバー」等）で、原 CSV に職種コードがない場合に限界があります。\
        参考平均は件数による重み付け平均、推定グループ中央値は企業別中央値の中央値で算出した近似値です（per-record の中央値とは異なります）。\
        値の単位は{unit_native}（{unit_yen}）。本表は CSV ベースの参考値であり、地域全体の職種別給与水準を代表するものではありません。\
        全体給与中央値（表紙ハイライト KPI）と直接比較できる指標ではありません。\
        本表は相関の可視化であり、因果の証明ではありません。\
        </p>\n",
        unit_native = if agg.is_hourly { "時給" } else { "月給" },
        unit_yen = unit_yen_label,
    ));

    render_read_hint(
        html,
        "職種推定グループ間で給与の参考値に差が見られる場合、業務内容・経験要件・労働時間・夜勤の有無等の複合要因を示唆します。\
         具体的な原因解釈は別途現場ヒアリング等で検証してください。",
    );

    html.push_str("</div>\n");
}

/// 月給は万円表示、時給は円/時 のままで小数点 1 桁に整形する.
fn format_value_text(yen: i64, is_hourly: bool) -> String {
    if is_hourly {
        format_number(yen)
    } else {
        format!("{:.1}", yen as f64 / 10_000.0)
    }
}

// =====================================================================
// Tests
// =====================================================================

#[cfg(test)]
mod tests {
    use super::*;
    use crate::handlers::survey::aggregator::{CompanyAgg, TagSalaryAgg};

    fn agg_with_tags(tags: Vec<TagSalaryAgg>) -> SurveyAggregation {
        let mut agg = SurveyAggregation::default();
        agg.total_count = tags.iter().map(|t| t.count).sum();
        agg.is_hourly = false;
        agg.by_tag_salary = tags;
        agg
    }

    fn tag(name: &str, count: usize, avg: i64) -> TagSalaryAgg {
        TagSalaryAgg {
            tag: name.to_string(),
            count,
            avg_salary: avg,
            diff_from_avg: 0,
            diff_percent: 0.0,
        }
    }

    fn co(name: &str, count: usize, avg: i64, median: i64) -> CompanyAgg {
        CompanyAgg {
            name: name.to_string(),
            count,
            avg_salary: avg,
            median_salary: median,
        }
    }

    /// キーワード分類器の正当性 (主要グループ + 未分類).
    #[test]
    fn map_keyword_to_occupation_group_classifies_known_keywords() {
        assert_eq!(map_keyword_to_occupation_group("看護師"), Some("看護系"));
        assert_eq!(map_keyword_to_occupation_group("准看護師"), Some("看護系"));
        assert_eq!(map_keyword_to_occupation_group("介護福祉士"), Some("介護系"));
        assert_eq!(map_keyword_to_occupation_group("ヘルパー"), Some("介護系"));
        assert_eq!(map_keyword_to_occupation_group("保育士"), Some("保育・教育系"));
        assert_eq!(map_keyword_to_occupation_group("理学療法士"), Some("リハビリ・療法士系"));
        assert_eq!(map_keyword_to_occupation_group("作業療法士"), Some("リハビリ・療法士系"));
        assert_eq!(map_keyword_to_occupation_group("薬剤師"), Some("医療技術・薬局系"));
        assert_eq!(map_keyword_to_occupation_group("ケアマネジャー"), Some("福祉・相談支援系"));
        assert_eq!(map_keyword_to_occupation_group("調理師"), Some("飲食・調理系"));
        assert_eq!(map_keyword_to_occupation_group("ドライバー"), Some("物流・配送・ドライバー系"));
        assert_eq!(map_keyword_to_occupation_group("施工管理"), Some("建築・土木・設備系"));
        assert_eq!(map_keyword_to_occupation_group("一般事務"), Some("事務・バックオフィス系"));
        // 未分類
        assert_eq!(map_keyword_to_occupation_group("ABC123"), None);
        assert_eq!(map_keyword_to_occupation_group(""), None);
    }

    /// 16 グループ完全網羅テスト.
    #[test]
    fn map_classifies_full_16_groups() {
        let cases = [
            ("看護師", "看護系"),
            ("ヘルパー", "介護系"),
            ("ケア", "介護系"),
            ("メディカル", "医療技術・薬局系"),
            ("リハビリ", "リハビリ・療法士系"),
            ("PT", "リハビリ・療法士系"),
            ("ケアマネ", "福祉・相談支援系"),
            ("保育士", "保育・教育系"),
            ("施工管理", "建築・土木・設備系"),
            ("CAD", "建築・土木・設備系"),
            ("ドライバー", "物流・配送・ドライバー系"),
            ("倉庫", "物流・配送・ドライバー系"),
            ("製造", "製造・軽作業系"),
            ("軽作業", "製造・軽作業系"),
            ("警備", "警備・清掃・施設管理系"),
            ("清掃", "警備・清掃・施設管理系"),
            ("調理", "飲食・調理系"),
            ("管理栄養士", "飲食・調理系"),
            ("一般事務", "事務・バックオフィス系"),
            ("コールセンター", "事務・バックオフィス系"),
            ("法人営業", "営業・販売促進系"),
            ("ラウンダー", "営業・販売促進系"),
            // 2026-05-14: 「販売」単独は誤マッチ過多のため「販売員」に厳格化
            ("販売員", "販売・接客・サービス系"),
            ("アテンダー", "販売・接客・サービス系"),
            ("エンジニア", "IT・技術専門職系"),
            ("ヘルプデスク", "IT・技術専門職系"),
            ("店長候補", "販売・接客・サービス系"),
            ("施設長", "管理・マネジメント系"),
        ];
        // 注: 「ケア」単独 → 介護系 (group 2)。「ケアマネ」は除外条件で福祉・相談支援系 (5) へ。
        // 注: 「店長候補」は「店長」を含むため販売・接客・サービス系 (14) に hit (16 より優先順位上)。
        // 注: 「施設長」は他の上位キーワードに hit せず、管理・マネジメント系 (16) に分類される。
        for (kw, expected) in cases {
            let actual = map_keyword_to_occupation_group(kw);
            assert_eq!(
                actual,
                Some(expected),
                "kw={} expected={} actual={:?}",
                kw,
                expected,
                actual
            );
        }
    }

    /// 16 グループのうち、明示的に異なる優先順位検証.
    #[test]
    fn map_priority_order_specific_over_management() {
        // 「販売店長」 → 販売・接客・サービス系 (14) が hit ("販売" もしくは "店長" でマッチ)
        assert_eq!(
            map_keyword_to_occupation_group("販売店長"),
            Some("販売・接客・サービス系")
        );
        // 「建築CAD」 → 建築・土木・設備系 (7) を優先
        assert_eq!(
            map_keyword_to_occupation_group("建築CAD"),
            Some("建築・土木・設備系")
        );
    }

    /// 信頼度ルール: 具体職種語は Direct.
    #[test]
    fn confidence_rule_specific_keyword_is_direct() {
        let (_, conf) = map_keyword_to_occupation_group_with_confidence("看護師").unwrap();
        assert!(matches!(conf, OccupationConfidence::Direct));
    }

    /// 信頼度ルール: 広義語は Reference.
    #[test]
    fn confidence_rule_broad_keyword_is_reference() {
        let (_, conf) = map_keyword_to_occupation_group_with_confidence("メディカル").unwrap();
        assert!(matches!(conf, OccupationConfidence::Reference));
    }

    /// 信頼度ルール: 多義語は LowConfidence.
    #[test]
    fn confidence_rule_ambiguous_keyword_is_low() {
        let (_, conf) = map_keyword_to_occupation_group_with_confidence("アテンダー").unwrap();
        assert!(matches!(conf, OccupationConfidence::LowConfidence));
    }

    /// タグから職種推定 → 同グループの件数を加算、加重平均を計算する.
    #[test]
    fn occupation_salary_aggregates_by_occupation() {
        let agg = agg_with_tags(vec![
            tag("看護師", 10, 280_000),
            tag("准看護師", 5, 240_000),
            tag("介護福祉士", 8, 220_000),
        ]);
        let rows = aggregate_occupation_salary(&agg);
        assert_eq!(rows.len(), 2, "2 グループに集約されるはず: {:?}", rows);
        assert_eq!(rows[0].occupation, "看護系");
        assert_eq!(rows[0].count, 15);
        assert_eq!(rows[0].weighted_avg, (280_000 * 10 + 240_000 * 5) / 15);
        assert_eq!(rows[1].occupation, "介護系");
        assert_eq!(rows[1].count, 8);
    }

    /// 推定不能 (キーワード非マッチ) なタグは除外される.
    #[test]
    fn occupation_salary_excludes_unclassifiable_tags() {
        let agg = agg_with_tags(vec![
            tag("ABC123", 5, 250_000),
            tag("看護師", 3, 280_000),
        ]);
        let rows = aggregate_occupation_salary(&agg);
        assert_eq!(rows.len(), 1);
        assert_eq!(rows[0].occupation, "看護系");
        assert_eq!(rows[0].count, 3);
    }

    /// 件数 < 3 は note="参考 (低信頼)" でマークされる.
    #[test]
    fn occupation_salary_low_count_marked_as_low_confidence() {
        let agg = agg_with_tags(vec![tag("看護師", 2, 280_000)]);
        let rows = aggregate_occupation_salary(&agg);
        assert_eq!(rows.len(), 1);
        assert_eq!(rows[0].count, 2);
        assert_eq!(rows[0].note, "参考 (低信頼)");
    }

    /// is_hourly=true なら unit_label="時給"、HTML も時給ラベル.
    #[test]
    fn occupation_salary_hourly_mode_uses_hourly_unit_label() {
        let mut agg = agg_with_tags(vec![tag("看護師", 10, 1_500)]);
        agg.is_hourly = true;
        let rows = aggregate_occupation_salary(&agg);
        assert_eq!(rows[0].unit_label, "時給");
        let mut html = String::new();
        render_section_occupation_salary(&mut html, &agg);
        assert!(html.contains("時給 参考平均"), "is_hourly=true で時給ラベル");
        assert!(!html.contains("月給 参考平均"), "is_hourly=true で月給ラベル不在");
    }

    /// HW 連想語を出力に含めない.
    #[test]
    fn occupation_salary_does_not_emit_hw_terms() {
        let agg = agg_with_tags(vec![tag("看護師", 10, 280_000)]);
        let mut html = String::new();
        render_section_occupation_salary(&mut html, &agg);
        for forbidden in [
            "ハローワーク",
            "HW 求人",
            "有効求人倍率",
            "欠員補充率",
            "求人継続率",
        ] {
            assert!(
                !html.contains(forbidden),
                "Round 3-C に HW 連想語 '{}' が混入してはならない",
                forbidden
            );
        }
    }

    /// MI variant でデータあり時に出力 / 空時は fail-soft.
    #[test]
    fn occupation_salary_section_appears_in_mi_variant_only() {
        let agg = SurveyAggregation::default();
        let mut html = String::new();
        render_section_occupation_salary(&mut html, &agg);
        assert_eq!(html, "", "空集計時は何も出力しない");

        let agg2 = agg_with_tags(vec![tag("看護師", 10, 280_000)]);
        let mut html2 = String::new();
        render_section_occupation_salary(&mut html2, &agg2);
        assert!(html2.contains("職種推定グループ別 給与参考"));
    }

    /// 見出しに「職種別」断定不可、「職種推定」「参考」必須.
    #[test]
    fn occupation_salary_heading_uses_estimation_phrasing() {
        let agg = agg_with_tags(vec![tag("看護師", 10, 280_000)]);
        let mut html = String::new();
        render_section_occupation_salary(&mut html, &agg);
        assert!(
            html.contains("職種推定") || html.contains("推定グループ"),
            "見出しに「職種推定」「推定グループ」を含むこと"
        );
        assert!(html.contains("参考"), "見出し or 注記に「参考」を含むこと");
        assert!(
            !html.contains(">職種別 給与水準<"),
            "断定タイトル「職種別 給与水準」を h2 に使ってはならない"
        );
    }

    /// 注記に CSV 職種コード不在・公的職業分類との不一致・全体中央値との非一致を含む.
    #[test]
    fn occupation_salary_note_includes_caveat() {
        let agg = agg_with_tags(vec![tag("看護師", 10, 280_000)]);
        let mut html = String::new();
        render_section_occupation_salary(&mut html, &agg);
        assert!(html.contains("推定"));
        assert!(html.contains("参考値"));
        assert!(
            html.contains("公的職業分類") && html.contains("一致しない"),
            "注記に「公的職業分類…一致しない」を含むこと"
        );
        assert!(
            html.contains("全体給与中央値"),
            "注記に「全体給与中央値…一致しない」を含むこと"
        );
    }

    /// 信号 A (タグ) で既カバーのグループに信号 B (会社名) の件数を二重加算しない.
    #[test]
    fn occupation_salary_does_not_double_count_tag_and_company_signals() {
        let mut agg = agg_with_tags(vec![tag("看護師", 10, 280_000)]);
        agg.by_company = vec![
            co("○○病院", 100, 290_000, 285_000),
            // 「□□建設会社」 → 建築・土木・設備系 (新グループ名)
            co("□□建設会社", 5, 320_000, 315_000),
        ];
        let rows = aggregate_occupation_salary(&agg);
        let nursing = rows.iter().find(|r| r.occupation == "看護系").unwrap();
        assert_eq!(
            nursing.count, 10,
            "信号 A 既カバーの職種に信号 B を二重加算しないこと"
        );
        let construction = rows
            .iter()
            .find(|r| r.occupation == "建築・土木・設備系");
        assert!(
            construction.is_some(),
            "信号 A 未カバーの職種は信号 B で補完されること"
        );
        assert_eq!(construction.unwrap().count, 5);
    }

    /// 件数 < 3 のときに HTML 出力に「参考 (低信頼)」が表示される.
    #[test]
    fn occupation_salary_low_confidence_label_in_html() {
        let agg = agg_with_tags(vec![tag("看護師", 2, 280_000)]);
        let mut html = String::new();
        render_section_occupation_salary(&mut html, &agg);
        assert!(html.contains("参考 (低信頼)"));
    }

    /// 列ヘッダが Round 3-B/3-C 表現規約に揃う.
    #[test]
    fn occupation_salary_column_headers_use_reference_phrasing() {
        let agg = agg_with_tags(vec![tag("看護師", 10, 280_000)]);
        let mut html = String::new();
        render_section_occupation_salary(&mut html, &agg);
        assert!(html.contains("職種推定グループ"));
        assert!(html.contains("参考平均"));
        assert!(html.contains("推定グループ中央値"));
        assert!(html.contains("信頼度"));
    }

    /// note: count >= 3 + 全 LowConfidence (会社名のみ) → "参考 (低信頼)".
    #[test]
    fn note_low_confidence_when_only_company_signal() {
        let mut agg = SurveyAggregation::default();
        agg.is_hourly = false;
        // 「介護センター○○」 → 介護系 (group 2)、by_company 経路のみ → LowConfidence
        agg.by_company = vec![co("介護センター○○", 5, 280_000, 270_000)];
        let rows = aggregate_occupation_salary(&agg);
        assert_eq!(rows.len(), 1);
        assert_eq!(rows[0].count, 5);
        assert_eq!(
            rows[0].note, "参考 (低信頼)",
            "by_company のみは LowConfidence → 参考 (低信頼)"
        );
    }

    /// note: count >= 3 + Direct タグあり → "" (空).
    #[test]
    fn note_empty_when_direct_tag_signal() {
        let agg = agg_with_tags(vec![tag("看護師", 10, 280_000)]);
        let rows = aggregate_occupation_salary(&agg);
        assert_eq!(rows.len(), 1);
        assert_eq!(rows[0].count, 10);
        assert_eq!(rows[0].note, "", "Direct タグ + count>=3 は note 空");
    }

    /// note: count >= 3 + Reference タグのみ → "参考".
    #[test]
    fn note_reference_when_only_broad_tag_signal() {
        let agg = agg_with_tags(vec![tag("メディカル", 5, 250_000)]);
        let rows = aggregate_occupation_salary(&agg);
        assert_eq!(rows.len(), 1);
        assert_eq!(rows[0].count, 5);
        assert_eq!(
            rows[0].note, "参考",
            "Reference タグ + count>=3 は note=参考"
        );
    }
}
