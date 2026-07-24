//! 工程⑦: HRハッカーCSV 84列原稿の生成プロンプト・出力検証[E]・行組み立て。
//!
//! 移植元:
//! - `scripts/job_creation_media_engine/hrhacker_columns.py` (84列定義・スペック・スロット割当)
//! - `scripts/job_creation_media_engine/hrhacker_generate.py` (生成プロンプト[D]・検証[E]・行合成)
//!
//! 設計:
//! - 生成対象は訴求系5列(案件名/仕事内容/キャッチコピー/メリット/Indeed表示職種名)のみ。
//!   LLM生成文は必ず [`validate_generated`] で ①数値照合 ②文字数上限 ③NGワード を機械検査し、
//!   1つでも落ちたら値を空にして `review_required`(それっぽい値で埋めない)。
//! - 不変4列は検証済み事実から機械転記、スロットはv1既定割当で合成。
//! - LLM 呼び出し(再生成ループ含む)は親が配線する。本モジュールは純粋関数のみ。
//!
//! **列順の正本は [`HRHACKER_COLUMNS`] 定数**。[`assemble_row`] は `BTreeMap` を返すが、
//! `BTreeMap` のキー順(辞書順)は CSV の列順ではない。CSV 出力側は必ず
//! [`HRHACKER_COLUMNS`] の順で列を引くこと(この定数がCSV列順の唯一の正)。

use crate::job_gen::ng_words::NgRules;
use crate::job_gen::types::ExtractedFacts;
use serde::{Deserialize, Serialize};
use serde_json::Value;
use std::collections::BTreeMap;

/// HRハッカーCSV 84列(列名・同順)。`hrhacker_columns.py` の `HRHACKER_COLUMNS` と一字一句一致。
/// CSV 出力の列順はこの定数を正とする。
pub const HRHACKER_COLUMNS: [&str; 84] = [
    "求人id",
    "店舗id",
    "職種id",
    "案件名",
    "画像1",
    "画像2",
    "画像3",
    "仕事内容",
    "通勤経路",
    "最寄り駅",
    "キャッチコピー",
    "メリット",
    "仕事情報補足1のタイトル",
    "仕事情報補足2のタイトル",
    "仕事情報補足3のタイトル",
    "仕事情報補足4のタイトル",
    "仕事情報補足1の内容",
    "仕事情報補足2の内容",
    "仕事情報補足3の内容",
    "仕事情報補足4の内容",
    "雇用形態",
    "Indeed表示職種名",
    "応募資格",
    "給与形態",
    "基本給与 最小",
    "基本給与 最大",
    "タスクの所要時間",
    "タスクの単位",
    "平均稼働時間",
    "平均稼働日数",
    "固定残業代",
    "想定残業時間",
    "条件付き給与1 条件",
    "条件付き給与1 深夜帯",
    "条件付き給与1 最小給与",
    "条件付き給与1 最大給与",
    "条件付き給与2 条件",
    "条件付き給与2 深夜帯",
    "条件付き給与2 最小給与",
    "条件付き給与2 最大給与",
    "条件付き給与3 条件",
    "条件付き給与3 深夜帯",
    "条件付き給与3 最小給与",
    "条件付き給与3 最大給与",
    "給与補足",
    "試用・研修の有無",
    "試用・研修時の雇用条件",
    "試用・研修期の雇用形態",
    "試用・研修期の給与のタイプ",
    "試用・研修期の基本給与 最小",
    "試用・研修期の基本給与 最大",
    "試用・研修期のタスクの所要時間",
    "試用・研修期のタスクの単位",
    "試用・研修期の平均稼働時間",
    "試用・研修期の平均稼働日数",
    "試用・研修期の固定残業代",
    "試用・研修期の想定残業時間",
    "試用・研修の詳細情報",
    "勤務時間",
    "勤務時間帯",
    "自由項目1のタイトル",
    "自由項目2のタイトル",
    "自由項目3のタイトル",
    "自由項目4のタイトル",
    "自由項目1の内容",
    "自由項目2の内容",
    "自由項目3の内容",
    "自由項目4の内容",
    "制作メモ",
    "受動喫煙対策",
    "受動喫煙についての補足情報",
    "応募方法",
    "応募後のプロセス",
    "採用予定人数",
    "問い合わせ電話番号",
    "連絡先メールアドレス",
    "応募時通知先メールアドレス（カンマ区切りで複数指定可）",
    "フォームの種類",
    "その他の質問1",
    "その他の質問2",
    "その他の質問3",
    "公開開始日時",
    "公開終了日時",
    "公開",
];

/// 生成フィールドのスペック。(内部キー, 列名, 文字数上限, 目安)。
/// `hrhacker_columns.py` の `GENERATION_FIELD_SPECS` と同値・同順。
/// 上限は公開1,394件の実測最大値を丸めた運用制約。
struct GenSpec {
    key: &'static str,
    column: &'static str,
    max_len: usize,
    guide: &'static str,
}

const GENERATION_FIELD_SPECS: [GenSpec; 5] = [
    GenSpec {
        key: "job_title",
        column: "案件名",
        max_len: 60,
        guide: "12字前後、職種と特徴が分かる求人名",
    },
    GenSpec {
        key: "job_description",
        column: "仕事内容",
        max_len: 2000,
        guide: "400〜800字。具体的な業務内容",
    },
    GenSpec {
        key: "catch_copy",
        column: "キャッチコピー",
        max_len: 90,
        guide: "40字前後の訴求コピー",
    },
    GenSpec {
        key: "merit",
        column: "メリット",
        max_len: 140,
        guide: "100字前後。働くメリットの箇条書き的列挙",
    },
    GenSpec {
        key: "indeed_job_title",
        column: "Indeed表示職種名",
        max_len: 30,
        guide: "12字前後の職種名",
    },
];

/// 不変項目の値をそのまま転記する列 → fact キー(v1経路)。
/// `hrhacker_columns.py` の `IMMUTABLE_TRANSCRIPTION` と同値。
/// 給与形態/基本給与最小・最大の構造化分解はv1では保留。
const IMMUTABLE_TRANSCRIPTION: [(&str, &str); 4] = [
    ("雇用形態", "employment_type"),
    ("応募資格", "required_qualifications"),
    ("勤務時間", "working_hours"),
    ("給与補足", "salary"),
];

/// スロット既定割当(v1)。`hrhacker_columns.py` の `SLOT_ASSIGNMENTS_V1` と同値。
/// 実測多数派に一致: 自由項目1=休日86%、自由項目2=福利厚生約75%。
struct SlotAssignment {
    title_column: &'static str,
    content_column: &'static str,
    title: &'static str,
    fact_fields: &'static [&'static str],
}

const SLOT_ASSIGNMENTS_V1: [SlotAssignment; 2] = [
    SlotAssignment {
        title_column: "自由項目1のタイトル",
        content_column: "自由項目1の内容",
        title: "休日・休暇",
        fact_fields: &["holidays"],
    },
    SlotAssignment {
        title_column: "自由項目2のタイトル",
        content_column: "自由項目2の内容",
        title: "福利厚生・待遇",
        fact_fields: &["insurance", "allowances"],
    },
];

/// 生成1フィールドの検証結果。
#[derive(Debug, Clone, Serialize, Deserialize, Default)]
pub struct GeneratedField {
    /// 対応するHRハッカー列名。
    pub column: String,
    /// 検証を通った本文(不合格時は空文字)。
    pub value: String,
    /// "generated_verified" | "review_required"。
    pub status: String,
    /// review_required の理由(数値/文字数/NGワード)。
    pub issues: Vec<String>,
}

/// 生成項目[D]の執筆プロンプト。移植元 `build_generation_prompt`。
///
/// `facts_text` = 検証済み事実(verified のみ)の整形テキスト([`crate::job_gen::types::facts_to_text`])。
/// `strategy_hint` = 戦略工程②〜⑥の要約(空文字可)。空でなければプロンプト先頭寄りに注入する。
///
/// Python版は原文 `source_text[:8000]` も同梱していたが、契約シグネチャでは
/// `facts_text` に絞る設計。数値・待遇の裏付けは事実テキストで担保し、
/// 出力は必ず [`validate_generated`] が原文と照合する。
pub fn build_generation_prompt(facts_text: &str, strategy_hint: &str) -> String {
    let facts_block = if facts_text.trim().is_empty() {
        "(検証済み事実なし)".to_string()
    } else {
        facts_text.trim().to_string()
    };

    let spec_lines = GENERATION_FIELD_SPECS
        .iter()
        .map(|s| format!("- {}({}): {}。{}字以内", s.key, s.column, s.guide, s.max_len))
        .collect::<Vec<_>>()
        .join("\n");

    let strategy_block = if strategy_hint.trim().is_empty() {
        String::new()
    } else {
        format!("\n【戦略の要約(訴求の方向性。事実の追加根拠にはしない)】\n{}\n", strategy_hint.trim())
    };

    format!(
        "あなたは求人票の訴求文ライターです。以下の検証済み事実だけを材料に、訴求文フィールドを執筆してください。\n\
\n\
絶対ルール:\n\
- 原文にない数字(給与額・時間・日数・年数・人数など)を新しく書かない。数値を使うときは検証済み事実にある数値をそのまま使う。\n\
- 「数値+単位」(例: 40代、10日、150円)は、その組み合わせのまま事実に存在するものだけ使う。別の数字を組み替えて新しい表現を作らない。\n\
- 給与・勤務時間・休日などの労働条件を言い換えたり要約で変えたりしない(条件の本文は別フィールドで機械転記される)。\n\
- 事実に根拠のない事項(資格・待遇・設備など)を追加しない。\n\
- 誇張表現(「業界No.1」「絶対」など)を使わない。\n\
- 誰にでも当てはまる無難な表現は厳禁。この求人固有の特徴で書く。\n\
- 各フィールドの文字数上限を守る。\n\
- JSONのみ出力。\n\
{strategy_block}\n\
執筆フィールド:\n\
{spec_lines}\n\
\n\
【検証済み事実】\n\
{facts_block}\n"
    )
}

/// 生成LLM応答のJSONスキーマ(5列すべて string・必須)。移植元 `GENERATION_SCHEMA`。
pub fn response_schema() -> Value {
    let mut props = serde_json::Map::new();
    let mut required = Vec::new();
    for s in GENERATION_FIELD_SPECS.iter() {
        props.insert(s.key.to_string(), serde_json::json!({"type": "string"}));
        required.push(Value::String(s.key.to_string()));
    }
    serde_json::json!({
        "type": "object",
        "properties": Value::Object(props),
        "required": required,
    })
}

/// 生成5列を検証する[E]。移植元 `check_generated_fields` + 合否判定。
///
/// 各フィールドに (a) 数値照合 [`crate::job_gen::validate::find_unsupported_numbers`]
/// (b) 文字数上限 (c) NGワード [`NgRules::detect`] を課し、
/// 全通過なら `generated_verified`(値は trim して反映)、
/// 1つでも落ちれば value を空にして `review_required` + issues に理由。
///
/// 返り値のキーは内部フィールドキー(job_title 等)。
pub fn validate_generated(
    source_text: &str,
    raw: &Value,
    ng: &NgRules,
) -> BTreeMap<String, GeneratedField> {
    let mut out = BTreeMap::new();
    for spec in GENERATION_FIELD_SPECS.iter() {
        let text = raw
            .get(spec.key)
            .and_then(Value::as_str)
            .unwrap_or("")
            .to_string();

        let mut issues: Vec<String> = Vec::new();

        if text.trim().is_empty() {
            issues.push("empty_output".to_string());
        }

        // (a) 数値照合(段①数字 + 段②数値単位ペア)。
        let unsupported = crate::job_gen::validate::find_unsupported_numbers(source_text, &text);
        if !unsupported.is_empty() {
            issues.push(format!("unsupported_numbers:{}", unsupported.join(",")));
        }

        // (b) 文字数上限(Unicode コードポイント数。Python の len(str) と一致)。
        let len = text.chars().count();
        if len > spec.max_len {
            issues.push(format!("length_exceeded:{}>{}", len, spec.max_len));
        }

        // (c) NGワード検証。
        for v in ng.detect(&text) {
            issues.push(format!("ng_word:{}:{}", v.reason, v.matched));
        }

        let field = if issues.is_empty() {
            GeneratedField {
                column: spec.column.to_string(),
                value: text.trim().to_string(),
                status: "generated_verified".to_string(),
                issues: Vec::new(),
            }
        } else {
            // 検証を通らない項目はそれっぽく埋めず空欄で人間レビューへ。
            GeneratedField {
                column: spec.column.to_string(),
                value: String::new(),
                status: "review_required".to_string(),
                issues,
            }
        };
        out.insert(spec.key.to_string(), field);
    }
    out
}

/// verified な事実値を trim して取り出す(空・未verified はスキップ)。
fn verified_value<'a>(facts: &'a ExtractedFacts, key: &str) -> Option<&'a str> {
    let f = facts.get(key)?;
    if f.status == "verified" {
        let v = f.value.trim();
        if !v.is_empty() {
            return Some(v);
        }
    }
    None
}

/// 84列の行(列名→値)を組み立てる。移植元 `build_hrhacker_row` + `build_slots`。
///
/// - 不変4列: [`IMMUTABLE_TRANSCRIPTION`] を検証済み事実から機械転記(status=verified のみ)。
/// - 生成列: `generated` のうち `generated_verified` のみ反映。それ以外は空文字。
/// - スロット: [`SLOT_ASSIGNMENTS_V1`](自由項目1=休日・休暇 / 自由項目2=福利厚生・待遇)。
///   対象事実が verified で非空なら結合(複数は改行連結)、無ければタイトルごと空欄(捏造しない)。
/// - 運用列(ID・画像・応募動線・公開制御)は空のまま。
///
/// 返す `BTreeMap` はキー辞書順で並ぶため列順の正ではない。CSV 出力は
/// [`HRHACKER_COLUMNS`] の順で引くこと。
pub fn assemble_row(
    facts: &ExtractedFacts,
    generated: &BTreeMap<String, GeneratedField>,
) -> BTreeMap<String, String> {
    // 84列を空文字で初期化。
    let mut row: BTreeMap<String, String> = HRHACKER_COLUMNS
        .iter()
        .map(|c| (c.to_string(), String::new()))
        .collect();

    // 不変項目の機械転記。
    for (column, fact_key) in IMMUTABLE_TRANSCRIPTION {
        if let Some(v) = verified_value(facts, fact_key) {
            row.insert(column.to_string(), v.to_string());
        }
    }

    // 生成列(検証済みのみ)。
    for field in generated.values() {
        if field.status == "generated_verified" {
            row.insert(field.column.clone(), field.value.clone());
        }
    }

    // スロット合成。
    for slot in SLOT_ASSIGNMENTS_V1.iter() {
        let parts: Vec<&str> = slot
            .fact_fields
            .iter()
            .filter_map(|ff| verified_value(facts, ff))
            .collect();
        let content = parts.join("\n");
        let title = if content.is_empty() { "" } else { slot.title };
        row.insert(slot.title_column.to_string(), title.to_string());
        row.insert(slot.content_column.to_string(), content);
    }

    row
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::job_gen::types::FactField;

    // NG ワード最小ルール(契約の NgRules::load_from_str をインラインJSONで使う)。
    // standalone=true のトリガーは major 自身なので、禁止語を major に置く。
    fn test_ng() -> NgRules {
        let json = r#"{"groups":[{"reason":"誇張表現","major":"絶対","minors":[],"standalone":true}]}"#;
        NgRules::load_from_str(json).expect("ng rules")
    }

    fn verified(value: &str) -> FactField {
        FactField {
            value: value.to_string(),
            evidence_quote: value.to_string(),
            status: "verified".to_string(),
        }
    }

    fn raw5(job_title: &str, desc: &str, catch: &str, merit: &str, indeed: &str) -> Value {
        serde_json::json!({
            "job_title": job_title,
            "job_description": desc,
            "catch_copy": catch,
            "merit": merit,
            "indeed_job_title": indeed,
        })
    }

    #[test]
    fn 列数は84かつ重複なし() {
        assert_eq!(HRHACKER_COLUMNS.len(), 84);
        let set: std::collections::HashSet<_> = HRHACKER_COLUMNS.iter().collect();
        assert_eq!(set.len(), 84, "列名に重複がある");
    }

    #[test]
    fn 生成列名は84列に実在する() {
        let cols: std::collections::HashSet<_> = HRHACKER_COLUMNS.iter().copied().collect();
        for s in GENERATION_FIELD_SPECS.iter() {
            assert!(cols.contains(s.column), "{} が84列に無い", s.column);
        }
        for (c, _) in IMMUTABLE_TRANSCRIPTION {
            assert!(cols.contains(c), "{c} が84列に無い");
        }
        for slot in SLOT_ASSIGNMENTS_V1.iter() {
            assert!(cols.contains(slot.title_column));
            assert!(cols.contains(slot.content_column));
        }
    }

    #[test]
    fn 検証全通過は生成済みになる() {
        let source = "月給250,000円 介護のお仕事 未経験歓迎";
        let raw = raw5(
            "介護スタッフ募集",
            "介護業務全般をお任せします。未経験歓迎です。",
            "未経験から始める介護のお仕事",
            "・未経験歓迎\n・研修あり",
            "介護スタッフ",
        );
        let g = validate_generated(source, &raw, &test_ng());
        for (k, f) in &g {
            assert_eq!(f.status, "generated_verified", "{k}: {:?}", f.issues);
        }
        assert_eq!(g["job_title"].value, "介護スタッフ募集");
    }

    #[test]
    fn 文字数超過はレビュー行き() {
        let source = "短い原文";
        // indeed_job_title は上限30字。31字を渡す。
        let long = "あ".repeat(31);
        let raw = raw5("案件名", "内容", "コピー", "メリット", &long);
        let g = validate_generated(source, &raw, &test_ng());
        let f = &g["indeed_job_title"];
        assert_eq!(f.status, "review_required");
        assert_eq!(f.value, "");
        assert!(
            f.issues.iter().any(|i| i.starts_with("length_exceeded:")),
            "{:?}",
            f.issues
        );
    }

    #[test]
    fn 原文にない数値はレビュー行き() {
        let source = "介護のお仕事です。未経験歓迎。";
        // 賞与年3回 の "3" は原文に無い。
        let raw = raw5("案件名", "賞与年3回支給します", "コピー", "メリット", "介護");
        let g = validate_generated(source, &raw, &test_ng());
        let f = &g["job_description"];
        assert_eq!(f.status, "review_required");
        assert!(
            f.issues.iter().any(|i| i.starts_with("unsupported_numbers:")),
            "{:?}",
            f.issues
        );
    }

    #[test]
    fn ngワード違反はレビュー行き() {
        // 数値混入を避け、NGワード(「絶対」)のみで落ちることを確認する。
        let source = "介護のお仕事です。安心の職場です。";
        let raw = raw5("案件名", "絶対に安心の職場です", "コピー", "メリット", "介護");
        let g = validate_generated(source, &raw, &test_ng());
        let f = &g["job_description"];
        assert_eq!(f.status, "review_required");
        assert!(
            f.issues.iter().any(|i| i.starts_with("ng_word:")),
            "{:?}",
            f.issues
        );
    }

    #[test]
    fn 空出力はレビュー行き() {
        let source = "原文";
        let raw = raw5("", "", "", "", "");
        let g = validate_generated(source, &raw, &test_ng());
        for f in g.values() {
            assert_eq!(f.status, "review_required");
            assert!(f.issues.contains(&"empty_output".to_string()));
        }
    }

    #[test]
    fn assemble_row_不変転記とスロット() {
        let mut facts = ExtractedFacts::new();
        facts.insert("employment_type".into(), verified("正社員"));
        facts.insert("required_qualifications".into(), verified("普通自動車免許"));
        facts.insert("working_hours".into(), verified("8時30分〜17時30分"));
        facts.insert("salary".into(), verified("192,000円〜195,000円"));
        facts.insert("holidays".into(), verified("週休二日制 年次有給休暇10日"));
        facts.insert("insurance".into(), verified("雇用保険 労災保険 健康保険 厚生年金"));
        facts.insert("allowances".into(), verified("夜勤手当3,500円/回"));

        let generated = BTreeMap::new();
        let row = assemble_row(&facts, &generated);

        assert_eq!(row.len(), 84, "84列");
        // 不変転記。
        assert_eq!(row["雇用形態"], "正社員");
        assert_eq!(row["応募資格"], "普通自動車免許");
        assert_eq!(row["勤務時間"], "8時30分〜17時30分");
        assert_eq!(row["給与補足"], "192,000円〜195,000円");
        // スロット。
        assert_eq!(row["自由項目1のタイトル"], "休日・休暇");
        assert_eq!(row["自由項目1の内容"], "週休二日制 年次有給休暇10日");
        assert_eq!(row["自由項目2のタイトル"], "福利厚生・待遇");
        assert_eq!(
            row["自由項目2の内容"],
            "雇用保険 労災保険 健康保険 厚生年金\n夜勤手当3,500円/回"
        );
        // 運用列は空。
        assert_eq!(row["求人id"], "");
    }

    #[test]
    fn assemble_row_未verified事実は転記しない() {
        let mut facts = ExtractedFacts::new();
        // rejected は転記しない。
        facts.insert(
            "employment_type".into(),
            FactField {
                value: "捏造正社員".into(),
                evidence_quote: "".into(),
                status: "rejected".into(),
            },
        );
        // holidays が無い → スロット1はタイトルごと空欄。
        let row = assemble_row(&facts, &BTreeMap::new());
        assert_eq!(row["雇用形態"], "");
        assert_eq!(row["自由項目1のタイトル"], "");
        assert_eq!(row["自由項目1の内容"], "");
    }

    #[test]
    fn assemble_row_生成は検証済みのみ反映() {
        let facts = ExtractedFacts::new();
        let mut generated = BTreeMap::new();
        generated.insert(
            "job_title".into(),
            GeneratedField {
                column: "案件名".into(),
                value: "介護スタッフ".into(),
                status: "generated_verified".into(),
                issues: vec![],
            },
        );
        generated.insert(
            "catch_copy".into(),
            GeneratedField {
                column: "キャッチコピー".into(),
                value: String::new(),
                status: "review_required".into(),
                issues: vec!["length_exceeded:100>90".into()],
            },
        );
        let row = assemble_row(&facts, &generated);
        assert_eq!(row["案件名"], "介護スタッフ");
        assert_eq!(row["キャッチコピー"], "", "review_required は反映しない");
    }

    #[test]
    fn response_schemaは5列必須() {
        let schema = response_schema();
        let required = schema["required"].as_array().unwrap();
        assert_eq!(required.len(), 5);
        assert!(schema["properties"]["job_title"].is_object());
    }
}
