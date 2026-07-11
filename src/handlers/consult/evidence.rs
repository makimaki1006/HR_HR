//! 証拠レコード (計画書 §7)
//!
//! 全ての数値・文章を OBSERVED / AGGREGATED / PROXY / HYPOTHESIS の4種類へ分類し、
//! 一意ID (E-001形式) を発行する。全指標・シグナル・矛盾・仮説は本モジュールの
//! 証拠IDを参照する (§7「全ての仮説は、必ず証拠IDを参照できるようにする」)。

use serde::{Deserialize, Serialize};

/// 証拠の4分類 (§7)
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "UPPERCASE")]
pub enum EvidenceKind {
    /// 原データで直接確認できた値
    Observed,
    /// 複数観測値の集計
    Aggregated,
    /// 直接測れない対象の代理指標
    Proxy,
    /// 複数根拠から導く可能性
    Hypothesis,
}

impl EvidenceKind {
    pub fn label_ja(&self) -> &'static str {
        match self {
            EvidenceKind::Observed => "観測値",
            EvidenceKind::Aggregated => "集計値",
            EvidenceKind::Proxy => "代理指標",
            EvidenceKind::Hypothesis => "仮説",
        }
    }
}

/// データ粒度ラベル (§5.1 / feedback_column_key_sql_alias_audit: 粒度を常に明示)
pub mod granularity {
    pub const NATIONAL: &str = "全国";
    pub const PREFECTURE: &str = "都道府県";
    pub const MUNICIPALITY: &str = "市区町村";
    pub const CSV: &str = "今回CSV";
    pub const COMPANY: &str = "企業";
}

/// 証拠レコード (§7 / §15.2 evidence[])
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct Evidence {
    /// E-001 形式の連番ID
    pub id: String,
    pub kind: EvidenceKind,
    /// 指標名 (ユーザーフレンドリーな日本語。内部テーブル名は使わない)
    pub metric_name: String,
    /// 値の文字列表現
    pub value_text: String,
    /// 単位 (円/月、%、人、件 等)
    pub unit: String,
    /// 出典名 (「企業データベース」等。SalesNow という名称は出力に使わない)
    pub source_name: String,
    /// 粒度 (全国/都道府県/市区町村/今回CSV/企業)
    pub granularity: String,
    /// サンプル数 (集計値の場合)
    pub sample_n: Option<usize>,
    /// データ基準日 (取得できる場合)
    pub as_of: Option<String>,
    /// 推定条件・欠損・制約の注記
    pub note: String,
}

/// 証拠ストア: ID発行と参照整合の一元管理
#[derive(Debug, Default, Clone)]
pub struct EvidenceStore {
    items: Vec<Evidence>,
}

impl EvidenceStore {
    pub fn new() -> Self {
        Self::default()
    }

    /// 証拠を登録し、発行したIDを返す。
    ///
    /// §11.3 の「独立根拠数」判定が重複でかさ増しされないよう、同一指標
    /// (metric_name + granularity + value_text + unit + source_name) が既に
    /// 登録されている場合は **既存IDを再利用** する (名寄せ / dedup)。
    /// note・sample_n・as_of は表示補助情報のため dedup キーに含めない
    /// (同一指標なら最初に登録した note を維持する)。
    #[allow(clippy::too_many_arguments)]
    pub fn add(
        &mut self,
        kind: EvidenceKind,
        metric_name: &str,
        value_text: &str,
        unit: &str,
        source_name: &str,
        granularity: &str,
        sample_n: Option<usize>,
        as_of: Option<String>,
        note: &str,
    ) -> String {
        // 名寄せ: 同一指標が既にあれば既存IDを返す (重複登録しない)
        if let Some(existing) = self.items.iter().find(|e| {
            e.metric_name == metric_name
                && e.granularity == granularity
                && e.value_text == value_text
                && e.unit == unit
                && e.source_name == source_name
        }) {
            return existing.id.clone();
        }
        let id = format!("E-{:03}", self.items.len() + 1);
        self.items.push(Evidence {
            id: id.clone(),
            kind,
            metric_name: metric_name.to_string(),
            value_text: value_text.to_string(),
            unit: unit.to_string(),
            source_name: source_name.to_string(),
            granularity: granularity.to_string(),
            sample_n,
            as_of,
            note: note.to_string(),
        });
        id
    }

    pub fn items(&self) -> &[Evidence] {
        &self.items
    }

    pub fn contains_id(&self, id: &str) -> bool {
        self.items.iter().any(|e| e.id == id)
    }

    pub fn get(&self, id: &str) -> Option<&Evidence> {
        self.items.iter().find(|e| e.id == id)
    }

    pub fn len(&self) -> usize {
        self.items.len()
    }

    pub fn is_empty(&self) -> bool {
        self.items.is_empty()
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn id_format_is_sequential_e_prefixed() {
        let mut store = EvidenceStore::new();
        let id1 = store.add(
            EvidenceKind::Observed,
            "求人件数",
            "120",
            "件",
            "今回の求人CSV集計",
            granularity::CSV,
            Some(120),
            None,
            "",
        );
        let id2 = store.add(
            EvidenceKind::Aggregated,
            "給与中央値",
            "250,000",
            "円/月",
            "今回の求人CSV集計",
            granularity::CSV,
            Some(100),
            None,
            "",
        );
        assert_eq!(id1, "E-001");
        assert_eq!(id2, "E-002");
        assert!(store.contains_id("E-001"));
        assert!(store.contains_id("E-002"));
        assert!(!store.contains_id("E-003"));
    }

    #[test]
    fn kind_labels_are_japanese() {
        assert_eq!(EvidenceKind::Observed.label_ja(), "観測値");
        assert_eq!(EvidenceKind::Hypothesis.label_ja(), "仮説");
    }

    #[test]
    fn serde_kind_is_uppercase() {
        let v = serde_json::to_string(&EvidenceKind::Aggregated).unwrap();
        assert_eq!(v, "\"AGGREGATED\"");
    }

    #[test]
    fn identical_metric_is_deduped_to_single_id() {
        // 実例: 有効求人倍率 1.35倍 が複数箇所から登録されても1つのIDにまとまる
        let mut store = EvidenceStore::new();
        let id1 = store.add(
            EvidenceKind::Observed,
            "有効求人倍率",
            "1.35",
            "倍",
            "一般職業紹介状況",
            granularity::PREFECTURE,
            None,
            None,
            "初回登録",
        );
        let id2 = store.add(
            EvidenceKind::Observed,
            "有効求人倍率",
            "1.35",
            "倍",
            "一般職業紹介状況",
            granularity::PREFECTURE,
            None,
            None,
            "別ロジックからの再登録",
        );
        let id3 = store.add(
            EvidenceKind::Observed,
            "有効求人倍率",
            "1.35",
            "倍",
            "一般職業紹介状況",
            granularity::PREFECTURE,
            None,
            None,
            "さらに別ロジック",
        );
        assert_eq!(id1, id2, "同一指標は名寄せされ同じID");
        assert_eq!(id1, id3);
        assert_eq!(store.len(), 1, "重複登録されず1件のみ");
    }

    #[test]
    fn different_value_or_source_is_not_deduped() {
        let mut store = EvidenceStore::new();
        let id1 = store.add(
            EvidenceKind::Observed,
            "有効求人倍率",
            "1.35",
            "倍",
            "一般職業紹介状況",
            granularity::PREFECTURE,
            None,
            None,
            "",
        );
        // 値が違う → 別ID
        let id2 = store.add(
            EvidenceKind::Observed,
            "有効求人倍率",
            "1.50",
            "倍",
            "一般職業紹介状況",
            granularity::PREFECTURE,
            None,
            None,
            "",
        );
        // 出典が違う → 別ID
        let id3 = store.add(
            EvidenceKind::Observed,
            "有効求人倍率",
            "1.35",
            "倍",
            "別の出典",
            granularity::PREFECTURE,
            None,
            None,
            "",
        );
        assert_ne!(id1, id2);
        assert_ne!(id1, id3);
        assert_eq!(store.len(), 3);
    }
}
