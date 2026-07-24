//! 戦略生成(GEM相当)のプロンプトビルダ＋レスポンススキーマ。工程②③④⑤⑥⑧。
//!
//! GEM(Gemini App)は API 呼び出し不可のため、その原プロンプト(1回で分析＋ペルソナ5＋
//! コピー15＋画像5＋スマホ原稿5＋ABテストを要求)の思想を**工程別に分解**して移植する。
//! 1回1タスクに絞ることで各工程の出力を短く・濃く保つのが狙い(設計正本 §2.1)。
//!
//! # 各工程(契約 `job_gen::strategy` に対応)
//! - ②analyze  … 求人ポテンシャル分析(表面/裏の強み/ボトルネック)。該当職種の知識を注入。
//! - ③personas … ②を受け count 人分のペルソナ(不満/環境/痛み)。
//! - ④copy     … 1ペルソナに3案(常識破壊/比較・リアルな声/感情・共感)。
//! - ⑤images   … 全ペルソナ分の画像ディレクション。
//! - ⑥mobile   … 1ペルソナ分のスマホ原稿(執筆ルール準拠)。facts_text の事実だけ使う。
//! - ⑧ab       … A/Bテスト実行の実務ステップ(CTR/CVR/CPA)。
//!
//! # 捏造防止の位置づけ
//! ここは「人間向けの戦略提案」を作る工程で、数値照合[E]や NGワード検証は**別工程**
//! ([`crate::job_gen::validate`] / [`crate::job_gen::ng_words`] / 工程①⑦)が担う。
//! ただしプロンプト自体にも共通制約([`CONSTRAINT_NO_FABRICATION`] / [`CONSTRAINT_NO_GENERIC`])
//! を必ず入れ、LLM に事実追加・無難な表現を最初から避けさせる。各 `build_*` は純粋関数。

use serde_json::{json, Value};

/// 全プロンプト共通の制約(事実追加の禁止)。表現の工夫は許すが、原文にない情報の付加は禁止。
pub const CONSTRAINT_NO_FABRICATION: &str =
    "原文にない労働条件・数値・待遇を書かない(表現の工夫は可、事実の追加は禁止)。";

/// 全プロンプト共通の制約(無難な表現の禁止)。GEM原プロンプトの核。
pub const CONSTRAINT_NO_GENERIC: &str = "誰にでも当てはまる無難な表現は厳禁。";

/// キャッチコピーの3つの訴求スタイル(工程④、固定)。
pub const COPY_STYLES: [&str; 3] = ["常識破壊", "比較・リアルな声", "感情・共感"];

/// 共通制約ブロックを組み立てる(全 `build_*` の末尾に差し込む)。
fn constraints_block() -> String {
    format!(
        "# 絶対に守る制約\n\
- {CONSTRAINT_NO_FABRICATION}\n\
- {CONSTRAINT_NO_GENERIC}\n"
    )
}

/// JSON オブジェクトから文字列フィールドを安全に取り出す(欠落・非文字列→空文字)。
fn str_field(v: &Value, key: &str) -> String {
    v.get(key)
        .and_then(Value::as_str)
        .unwrap_or("")
        .trim()
        .to_string()
}

/// JSON の文字列配列を「- 要素」の箇条書きテキストにする(欠落・空→"(記載なし)")。
fn arr_lines(v: &Value, key: &str) -> String {
    let items: Vec<String> = v
        .get(key)
        .and_then(Value::as_array)
        .map(|a| {
            a.iter()
                .filter_map(Value::as_str)
                .map(|s| s.trim())
                .filter(|s| !s.is_empty())
                .map(|s| format!("- {s}"))
                .collect()
        })
        .unwrap_or_default();
    if items.is_empty() {
        "(記載なし)".to_string()
    } else {
        items.join("\n")
    }
}

/// 分析結果(②の出力 JSON)を人間可読テキストに整形して後続工程へ渡す。
fn analysis_to_text(analysis: &Value) -> String {
    format!(
        "## 表面上の強み\n{}\n\n## 裏の強み(インサイト)\n{}\n\n## 募集のボトルネック(懸念点)\n{}",
        arr_lines(analysis, "surface_strengths"),
        arr_lines(analysis, "hidden_strengths"),
        arr_lines(analysis, "bottlenecks"),
    )
}

/// ペルソナ1件(JSON)を人間可読テキストに整形する(欠落フィールドは空文字で耐える)。
fn persona_to_text(persona: &Value) -> String {
    format!(
        "- ラベル: {}\n\
- プロフィール(年齢性別含む): {}\n\
- 現職への不満: {}\n\
- 生活環境: {}\n\
- 抱えている痛み: {}",
        str_field(persona, "label"),
        str_field(persona, "profile"),
        str_field(persona, "dissatisfaction"),
        str_field(persona, "environment"),
        str_field(persona, "pain"),
    )
}

// ---------------------------------------------------------------------------
// ② 市場分析
// ---------------------------------------------------------------------------

/// 工程②: 求人ポテンシャル分析のプロンプト(純粋)。
///
/// `knowledge` は該当職種の参考知識テキスト(空文字可)。無関係な職種を混ぜないため、
/// 呼び出し側([`crate::job_gen::knowledge`])が該当職種の行だけを渡す前提。
pub fn build_analyze_prompt(source_text: &str, knowledge: &str) -> String {
    let knowledge_block = if knowledge.trim().is_empty() {
        "(該当職種の参考知識は提供されていません。原文だけから読み取ってください)".to_string()
    } else {
        format!(
            "以下はこの職種の一般的な転職理由・訴求傾向などの参考知識です。\
原文の裏側にあるインサイトを読み解く補助にのみ使い、原文にない条件として書き足さないこと。\n\n{}",
            knowledge.trim()
        )
    };

    format!(
        "あなたはトップクラスの求人マーケター兼データアナリストです。\
入力された求人原文から、市場におけるポジションとターゲットの深層心理(インサイト)を読み解いてください。\n\
\n\
# タスク(この工程では分析だけを行う)\n\
次の3点を、それぞれ具体的な短文の箇条書きで出力してください。\n\
1. 企業側が提示している「表面上の強み」(原文に明記されている魅力)\n\
2. 競合や市場から見た「裏の強み(インサイト)」(原文の行間から読み取れる、応募者にとっての本当の価値)\n\
3. 募集における「ボトルネック(懸念点)」(応募をためらわせる要素)\n\
\n\
# この職種の参考知識\n\
{knowledge_block}\n\
\n\
# 求人原文\n\
{source_text}\n\
\n\
{constraints}\
- 出力は指定 JSON スキーマ(surface_strengths / hidden_strengths / bottlenecks の各文字列配列)に厳密に従う。\n",
        constraints = constraints_block(),
    )
}

/// 工程②のレスポンススキーマ。
pub fn analyze_schema() -> Value {
    let str_array = json!({"type": "array", "items": {"type": "string"}});
    json!({
        "type": "object",
        "properties": {
            "surface_strengths": str_array,
            "hidden_strengths": str_array,
            "bottlenecks": str_array,
        },
        "required": ["surface_strengths", "hidden_strengths", "bottlenecks"]
    })
}

// ---------------------------------------------------------------------------
// ③ ペルソナ設計
// ---------------------------------------------------------------------------

/// 工程③: 戦略的ターゲットペルソナのプロンプト(純粋)。
///
/// `analysis` は工程②の出力 JSON。`count` はペルソナ数(3〜5想定)。
pub fn build_personas_prompt(source_text: &str, analysis: &Value, count: usize) -> String {
    format!(
        "あなたはトップクラスの求人マーケターです。以下の求人分析をもとに、\
この求人に強く反応する戦略的ターゲットペルソナを{count}人設計してください。\n\
\n\
# タスク\n\
ちょうど{count}人のペルソナを作成する。各ペルソナには必ず以下を具体的に書く。\n\
- label: ペルソナを一言で表す短いラベル\n\
- profile: 年齢・性別を含む人物像(職歴・世帯状況など)\n\
- dissatisfaction: 現職への具体的な不満\n\
- environment: 生活環境(通勤・家庭・生活リズムなど)\n\
- pain: 抱えている痛み(この求人が解決しうる切実な悩み)\n\
それぞれのペルソナは互いに重ならないよう、痛みの種類を変えて設計する。\n\
求人原文から読み取れる勤務地の地域性(通勤事情・生活圏)を profile と environment に織り込む。\n\
\n\
# 求人分析(工程②の結果)\n\
{analysis_text}\n\
\n\
# 求人原文\n\
{source_text}\n\
\n\
{constraints}\
- ペルソナは実在しうる具体像にする。年齢・性別・環境を曖昧にぼかさない。\n\
- 出力は指定 JSON スキーマ(personas 配列、各要素 label/profile/dissatisfaction/environment/pain)に厳密に従う。\n",
        analysis_text = analysis_to_text(analysis),
        constraints = constraints_block(),
    )
}

/// 工程③のレスポンススキーマ。
pub fn personas_schema() -> Value {
    json!({
        "type": "object",
        "properties": {
            "personas": {
                "type": "array",
                "items": {
                    "type": "object",
                    "properties": {
                        "label": {"type": "string"},
                        "profile": {"type": "string"},
                        "dissatisfaction": {"type": "string"},
                        "environment": {"type": "string"},
                        "pain": {"type": "string"},
                    },
                    "required": ["label", "profile", "dissatisfaction", "environment", "pain"]
                }
            }
        },
        "required": ["personas"]
    })
}

// ---------------------------------------------------------------------------
// ④ キャッチコピー
// ---------------------------------------------------------------------------

/// 工程④: 1ペルソナ向けキャッチコピー3案のプロンプト(純粋)。
///
/// `persona` は工程③の1要素、`analysis` は工程②の出力。スタイルは [`COPY_STYLES`] 固定。
pub fn build_copy_prompt(persona: &Value, analysis: &Value) -> String {
    let styles = COPY_STYLES
        .iter()
        .map(|s| format!("- {s}"))
        .collect::<Vec<_>>()
        .join("\n");

    format!(
        "あなたはトップクラスの求人コピーライターです。以下のペルソナ1人に向けて、\
親指を止めさせるエッジの効いたキャッチコピーを作ってください。\n\
\n\
# タスク\n\
ちょうど3案を作る。3案はそれぞれ次の異なるスタイルで、style フィールドにその名称を入れる。\n\
{styles}\n\
各コピーは40字前後(全角)。このペルソナの痛みに一点集中で刺す。\n\
\n\
# 対象ペルソナ\n\
{persona_text}\n\
\n\
# 求人分析(工程②の結果)\n\
{analysis_text}\n\
\n\
{constraints}\
- style はちょうど上記3種を1案ずつ使う(重複・欠落なし)。\n\
- このペルソナ以外にも当てはまる汎用コピーは作らない。\n\
- 出力は指定 JSON スキーマ(copies 配列、各要素 style/text)に厳密に従う。\n",
        persona_text = persona_to_text(persona),
        analysis_text = analysis_to_text(analysis),
        constraints = constraints_block(),
    )
}

/// 工程④のレスポンススキーマ。`style` は3種を列挙(enum)して逸脱を防ぐ。
pub fn copy_schema() -> Value {
    json!({
        "type": "object",
        "properties": {
            "copies": {
                "type": "array",
                "items": {
                    "type": "object",
                    "properties": {
                        "style": {"type": "string", "enum": COPY_STYLES},
                        "text": {"type": "string"},
                    },
                    "required": ["style", "text"]
                }
            }
        },
        "required": ["copies"]
    })
}

// ---------------------------------------------------------------------------
// ⑤ 画像ディレクション
// ---------------------------------------------------------------------------

/// 工程⑤: 全ペルソナ分のアイキャッチ画像ディレクションのプロンプト(純粋)。
///
/// `personas` は工程③の出力 JSON(`{"personas":[...]}` 形、または配列そのもの)。
pub fn build_images_prompt(personas: &Value) -> String {
    // {"personas":[...]} でも [...] でも受けられるようにする。
    let list = personas
        .get("personas")
        .and_then(Value::as_array)
        .or_else(|| personas.as_array())
        .cloned()
        .unwrap_or_default();

    let personas_text = if list.is_empty() {
        "(ペルソナが提供されていません)".to_string()
    } else {
        list.iter()
            .enumerate()
            .map(|(i, p)| {
                let label = str_field(p, "label");
                let head = if label.is_empty() {
                    format!("## ペルソナ{}", i + 1)
                } else {
                    format!("## {label}")
                };
                format!("{head}\n{}", persona_to_text(p))
            })
            .collect::<Vec<_>>()
            .join("\n\n")
    };

    format!(
        "あなたはトップクラスのクリエイティブディレクターです。\
各ペルソナ向けのアイキャッチ画像(メイン写真)のディレクションを作ってください。\n\
\n\
# タスク\n\
提供された各ペルソナに1つずつ、画像案を出力する。persona_label にはそのペルソナのラベルを入れる。\n\
- フリー素材にありがちな「作り笑顔の集合写真」は避ける。\n\
- ターゲットが一瞬で「これは自分のことだ」と認識する、具体的な構図・人物・情景を指定する。\n\
- 誰を、どんな場面で、どんな表情・アングルで写すかまで踏み込む。\n\
\n\
# ペルソナ一覧\n\
{personas_text}\n\
\n\
{constraints}\
- 出力は指定 JSON スキーマ(directions 配列、各要素 persona_label/direction)に厳密に従う。\n",
        constraints = constraints_block(),
    )
}

/// 工程⑤のレスポンススキーマ。
pub fn images_schema() -> Value {
    json!({
        "type": "object",
        "properties": {
            "directions": {
                "type": "array",
                "items": {
                    "type": "object",
                    "properties": {
                        "persona_label": {"type": "string"},
                        "direction": {"type": "string"},
                    },
                    "required": ["persona_label", "direction"]
                }
            }
        },
        "required": ["directions"]
    })
}

// ---------------------------------------------------------------------------
// ⑤b 画像生成プロンプト化 (2026-07-24 追加)
// ---------------------------------------------------------------------------

/// 工程⑤b: ディレクション文を画像生成AIへ丸投げできる日本語プロンプトに変換する(純粋)。
///
/// `directions` は工程⑤の出力 (`{"directions":[...]}` 形、または配列そのもの)。
/// `personas` は工程③の出力 (任意。渡すと訴求の核をペルソナの不満・ペインに接地させる)。
/// 全ペルソナ分を1コールでまとめて変換する (ユーザー決定 2026-07-24: 2段階化+1コール、
/// 日本語プロンプトのみ、ネガティブ/構図・カメラ/アスペクト比/撮影指示書兼用を含める)。
///
/// 2026-07-25 強化 (ユーザー要望「ハルシネーションの猶予を与えずがっちり固める」):
/// 出力プロンプトをラベル付きセクション構造で固定し、人数・服装・表情・視線・配置まで
/// 数値と具体語で確定させる。曖昧語を禁止し、禁止事項を本文にも埋め込む
/// (ネガティブプロンプト欄を持たない生成AIでも抑制が効くように)。
pub fn build_image_prompts_prompt(directions: &Value, personas: &Value) -> String {
    let list = directions
        .get("directions")
        .and_then(Value::as_array)
        .or_else(|| directions.as_array())
        .cloned()
        .unwrap_or_default();

    // persona_label → ペルソナ本文の対応 (訴求の核をペインに接地させる材料)。
    let persona_list = personas
        .get("personas")
        .and_then(Value::as_array)
        .or_else(|| personas.as_array())
        .cloned()
        .unwrap_or_default();
    let persona_text_of = |label: &str| -> String {
        persona_list
            .iter()
            .find(|p| str_field(p, "label") == label)
            .map(persona_to_text)
            .unwrap_or_default()
    };

    let directions_text = if list.is_empty() {
        "(ディレクションが提供されていません)".to_string()
    } else {
        list.iter()
            .enumerate()
            .map(|(i, d)| {
                let label = str_field(d, "persona_label");
                let head = if label.is_empty() {
                    format!("## 案{}", i + 1)
                } else {
                    format!("## {label}")
                };
                let ptext = persona_text_of(&label);
                let persona_block = if ptext.is_empty() {
                    String::new()
                } else {
                    format!("\n### このペルソナの人物像(訴求の根拠)\n{ptext}")
                };
                format!("{head}\n### ディレクション\n{}{persona_block}", str_field(d, "direction"))
            })
            .collect::<Vec<_>>()
            .join("\n\n")
    };

    format!(
        "あなたは画像生成AIのプロンプトエンジニア兼フォトディレクターです。\
以下の画像ディレクション(求人のアイキャッチ写真の演出案)を、画像生成AIにそのまま貼り付ければ\
**1回で意図どおりの完成品が得られる**日本語プロンプトに変換してください。\n\
\n\
# 最重要原則: 解釈の余地を残さない\n\
画像生成AIは、指定されていない要素を勝手に補って破綻する(人物が増える、余計な文字が入る、\
場面がずれる)。だから**画面に写る全要素をあなたが決め切り、プロンプトで固定する**こと。\n\
- 曖昧語の禁止: 「適度に」「自然な感じ」「いい雰囲気」「〜など」は使わない。すべて具体語・数値で書く。\n\
- 人数の固定: 「画面内の人物は合計N名のみ。これ以外の人物は写さない」と必ず明記する。\n\
- 未指定要素を作らない: 背景に何が写るか・写らないかまで書く。\n\
\n\
# タスク\n\
各ディレクションに1つずつ、prompts 配列の要素を出力する。persona_label は元のラベルをそのまま入れる。\n\
\n\
## appeal_core(訴求の核)の要件\n\
- この画像がペルソナに一瞬で感じさせるべきことを1〜2文で言語化する\
(例: 「ここなら夕方に帰れる、が画面から伝わる」)。ペルソナの不満・ペインの裏返しであること。\n\
- prompt 本文の全要素は、この訴求の核を伝えるために選ぶ。核に寄与しない要素は入れない。\n\
\n\
## prompt(本文)の要件\n\
- 日本語。画像生成AIへの指示文として完結させる(前置き・解説は書かない)。\n\
- 次のラベル付きセクション構造で書く(撮影指示書としてそのまま使える形):\n\
  【被写体】人数(合計N名のみ)・年齢層・性別・体格や髪型・服装(色と種類まで)・表情(口角、目の開き)・\
視線の先・姿勢と手の位置・動作\n\
  【場面・背景】場所・時間帯・季節・天候・背景に写るもの(位置関係まで)・背景に写さないもの\n\
  【小道具】何が・どこに・どんな状態で写るか(不要なら「小道具なし」と書く)\n\
  【構図・カメラ】主被写体の画面内位置(例: 右1/3)・フレーミング(バストアップ/膝上/全身)・\
カメラ高さ(目線/腰)・アングル・レンズ焦点距離相当・絞り相当(背景ボケの程度)\n\
  【光・色調】光源の種類と方向・時間帯の光の質・全体の色調(例: 暖色寄り)・コントラストの強さ\n\
  【スタイル】写実的な実写写真であること・解像度感・加工やフィルタをかけないこと\n\
  【禁止事項】この画に入れてはいけない要素の列挙(余計な人物、文字・ロゴ・看板、CG/イラスト調、\
作り笑顔の集合写真、ディレクションの意図に反する要素)。ネガティブ欄を持たないAIでも効くよう本文に含める。\n\
- すべてのセクションを埋める。省略しない。\n\
\n\
## negative_prompt の要件\n\
- 【禁止事項】と同内容+画像生成の定番不良(歪んだ手指、崩れた顔、不自然な文字、過度な加工感、\
フリー素材感)をカンマ区切りで列挙する(ネガティブ欄対応AI用)。\n\
\n\
## aspect_ratio の要件\n\
- 求人媒体の掲載枠を想定した推奨比率を1つ選び、用途を添える(例: \"16:9(求人メイン写真)\"、\
\"1:1(SNS・サムネイル)\"、\"4:3(媒体標準枠)\")。\n\
\n\
# 画像ディレクション一覧\n\
{directions_text}\n\
\n\
# 制約\n\
- 元のディレクションの意図(誰に刺さる画か)を保つこと。勝手に別の場面へ変えない。\n\
- ペルソナの人物像が提供されている場合、被写体・場面の決定はその不満・ペインの裏返しに接地させる。\n\
- 実在の企業名・人名・ロゴ・商標は書かない。\n\
- 出力は指定 JSON スキーマ(prompts 配列、各要素 persona_label/appeal_core/prompt/negative_prompt/aspect_ratio)に厳密に従う。\n"
    )
}

/// 工程⑤bのレスポンススキーマ。
pub fn image_prompts_schema() -> Value {
    json!({
        "type": "object",
        "properties": {
            "prompts": {
                "type": "array",
                "items": {
                    "type": "object",
                    "properties": {
                        "persona_label": {"type": "string"},
                        "appeal_core": {"type": "string"},
                        "prompt": {"type": "string"},
                        "negative_prompt": {"type": "string"},
                        "aspect_ratio": {"type": "string"},
                    },
                    "required": ["persona_label", "appeal_core", "prompt", "negative_prompt", "aspect_ratio"]
                }
            }
        },
        "required": ["prompts"]
    })
}

// ---------------------------------------------------------------------------
// ⑥ スマホ原稿
// ---------------------------------------------------------------------------

/// 工程⑥: 1ペルソナ向けスマホ最適化リード文のプロンプト(純粋)。
///
/// `facts_text` は検証済み事実([`crate::job_gen::types::facts_to_text`]の出力)。
/// スマホ執筆ルールを本文で強制する。空行は `lines` の空文字列要素で表現させる。
pub fn build_mobile_prompt(persona: &Value, facts_text: &str) -> String {
    let facts_block = if facts_text.trim().is_empty() {
        "(検証済み事実が提供されていません。この場合は労働条件の数値・待遇を一切書かないこと)"
            .to_string()
    } else {
        facts_text.trim().to_string()
    };

    format!(
        "あなたはトップクラスの求人コピーライターです。以下のペルソナ1人に向けて、\
スマートフォンでの縦スクロール・流し読みに特化した実戦用リード文を書いてください。\n\
\n\
# スマホ執筆ルール(厳守)\n\
- 1文は最大40字程度。\n\
- 2〜3行ごとに必ず空行を入れる(空行は lines 配列の空文字列 \"\" で表現する)。\n\
- 句読点をあえて省き、改行のリズムで読ませる広告コピー的な記述にする。\n\
- スクロールを止めさせるため、重要なキーワードを各文の文頭に置く。\n\
\n\
# 出力形式\n\
lines は本文を1行ずつ並べた配列。空行を入れたい箇所は空文字列 \"\" を要素として挟む。\n\
\n\
# 対象ペルソナ\n\
{persona_text}\n\
\n\
# 使ってよい検証済み事実(この情報の範囲内だけで書く)\n\
{facts_block}\n\
\n\
{constraints}\
- 上記「検証済み事実」に無い労働条件・数値・待遇は書かない(この工程では特に厳守)。\n\
- このペルソナ以外にも当てはまる無難なリード文にしない。\n\
- 出力は指定 JSON スキーマ(lines 文字列配列)に厳密に従う。\n",
        persona_text = persona_to_text(persona),
        constraints = constraints_block(),
    )
}

/// 工程⑥のレスポンススキーマ。
pub fn mobile_schema() -> Value {
    json!({
        "type": "object",
        "properties": {
            "lines": {"type": "array", "items": {"type": "string"}}
        },
        "required": ["lines"]
    })
}

// ---------------------------------------------------------------------------
// ⑧ A/Bテスト助言
// ---------------------------------------------------------------------------

/// 工程⑧: A/Bテスト実行への実務アドバイスのプロンプト(純粋)。
///
/// `strategy_summary` はここまでの戦略成果物の要約(空文字可)。
pub fn build_ab_prompt(strategy_summary: &str) -> String {
    let summary_block = if strategy_summary.trim().is_empty() {
        "(戦略要約は提供されていません。一般的な求人広告の検証手順として答えてください)".to_string()
    } else {
        strategy_summary.trim().to_string()
    };

    format!(
        "あなたは求人広告運用の実務家です。以下の戦略に対して、\
掲載後にデータで検証し改善するための実務ステップを提案してください。\n\
\n\
# タスク\n\
CTR(クリック率)・CVR(応募転換率)・CPA(応募単価)の追い方を、実行できる手順として出力する。\n\
各ステップは metric(追う指標)と action(具体的な検証・改善アクション)の組で書く。\n\
- 何を変えて何と比較するか(コピー/画像/ターゲット等のどの要素をA/Bするか)を具体的に。\n\
- 指標が悪いときに次に何を打つか、判断の順序まで含める。\n\
\n\
# 対象の戦略要約\n\
{summary_block}\n\
\n\
{constraints}\
- 抽象論(「PDCAを回す」等)で終わらせず、この求人で実行できる粒度にする。\n\
- 出力は指定 JSON スキーマ(steps 配列、各要素 metric/action)に厳密に従う。\n",
        constraints = constraints_block(),
    )
}

/// 工程⑧のレスポンススキーマ。
pub fn ab_schema() -> Value {
    json!({
        "type": "object",
        "properties": {
            "steps": {
                "type": "array",
                "items": {
                    "type": "object",
                    "properties": {
                        "metric": {"type": "string"},
                        "action": {"type": "string"},
                    },
                    "required": ["metric", "action"]
                }
            }
        },
        "required": ["steps"]
    })
}

#[cfg(test)]
mod tests {
    use super::*;

    /// 全プロンプトに共通制約2文が入っていること。
    fn assert_common_constraints(p: &str) {
        assert!(p.contains(CONSTRAINT_NO_FABRICATION), "事実追加禁止が無い:\n{p}");
        assert!(p.contains(CONSTRAINT_NO_GENERIC), "無難表現禁止が無い:\n{p}");
    }

    fn sample_analysis() -> Value {
        json!({
            "surface_strengths": ["週休2日", "賞与年2回"],
            "hidden_strengths": ["未経験でも先輩が伴走する育成文化"],
            "bottlenecks": ["夜勤の有無が不明"]
        })
    }

    fn sample_persona() -> Value {
        json!({
            "label": "子育て中の元介護士",
            "profile": "32歳女性、介護職5年、小学生の子あり",
            "dissatisfaction": "現職は残業が読めず保育園のお迎えに間に合わない",
            "environment": "郊外在住・車通勤・夫は多忙で家事分担が薄い",
            "pain": "収入は落としたくないが時間の自由がほしい"
        })
    }

    #[test]
    fn analyze_prompt_embeds_source_and_knowledge_and_constraints() {
        let p = build_analyze_prompt("大型ドライバー募集 月給35万円", "運送業の転職理由は労働時間");
        assert_common_constraints(&p);
        // 入力値がプロンプトに埋まっている。
        assert!(p.contains("大型ドライバー募集 月給35万円"), "{p}");
        assert!(p.contains("運送業の転職理由は労働時間"), "{p}");
        // 分析3観点の指示語。
        assert!(p.contains("表面上の強み"));
        assert!(p.contains("裏の強み"));
        assert!(p.contains("ボトルネック"));
    }

    #[test]
    fn analyze_prompt_survives_empty_knowledge() {
        let p = build_analyze_prompt("介護スタッフ募集", "");
        assert_common_constraints(&p);
        assert!(p.contains("介護スタッフ募集"));
        // 空 knowledge でも壊れず、無い旨の断りが入る。
        assert!(p.contains("参考知識は提供されていません"), "{p}");
    }

    #[test]
    fn personas_prompt_reflects_count_and_analysis() {
        let p = build_personas_prompt("介護スタッフ募集 賞与年2回", &sample_analysis(), 4);
        assert_common_constraints(&p);
        // count が複数箇所に反映される。
        assert!(p.contains("ペルソナを4人設計"), "{p}");
        assert!(p.contains("ちょうど4人"), "{p}");
        // 工程②の分析テキストが差し込まれている。
        assert!(p.contains("未経験でも先輩が伴走する育成文化"), "{p}");
        assert!(p.contains("夜勤の有無が不明"), "{p}");
        // 地域性を織り込む指示。
        assert!(p.contains("地域性"), "{p}");
    }

    #[test]
    fn personas_prompt_count_varies() {
        let p3 = build_personas_prompt("src", &sample_analysis(), 3);
        let p5 = build_personas_prompt("src", &sample_analysis(), 5);
        assert!(p3.contains("3人設計") && !p3.contains("5人設計"));
        assert!(p5.contains("5人設計") && !p5.contains("3人設計"));
    }

    #[test]
    fn copy_prompt_has_three_styles_and_persona() {
        let p = build_copy_prompt(&sample_persona(), &sample_analysis());
        assert_common_constraints(&p);
        // 3スタイル全てが列挙されている。
        for s in COPY_STYLES {
            assert!(p.contains(s), "style {s} が無い:\n{p}");
        }
        assert!(p.contains("ちょうど3案"), "{p}");
        // ペルソナのフィールドが埋まっている。
        assert!(p.contains("子育て中の元介護士"), "{p}");
        assert!(p.contains("保育園のお迎えに間に合わない"), "{p}");
        assert!(p.contains("40字前後"), "{p}");
    }

    #[test]
    fn images_prompt_accepts_wrapped_and_bare_personas() {
        let wrapped = json!({"personas": [sample_persona(), {"label": "ベテラン転職者"}]});
        let p = build_images_prompt(&wrapped);
        assert_common_constraints(&p);
        assert!(p.contains("子育て中の元介護士"), "{p}");
        assert!(p.contains("ベテラン転職者"), "{p}");
        // フリー素材回避の核となる指示。
        assert!(p.contains("作り笑顔"), "{p}");
        assert!(p.contains("自分のことだ"), "{p}");

        // 配列そのものを渡しても壊れない。
        let bare = json!([sample_persona()]);
        let p2 = build_images_prompt(&bare);
        assert!(p2.contains("子育て中の元介護士"), "{p2}");

        // 空でも壊れない。
        let empty = json!({"personas": []});
        let p3 = build_images_prompt(&empty);
        assert!(p3.contains("ペルソナが提供されていません"), "{p3}");
    }

    #[test]
    fn image_prompts_prompt_accepts_wrapped_and_bare_directions() {
        let wrapped = json!({"directions": [
            {"persona_label": "子育て中の元介護士", "direction": "夕方の送迎車の前で、利用者と笑い合う30代女性スタッフを斜めから"},
            {"persona_label": "ベテラン転職者", "direction": "記録業務をタブレットで済ませる場面"},
        ]});
        let personas = json!({"personas": [sample_persona()]});
        let p = build_image_prompts_prompt(&wrapped, &personas);
        // 元ディレクションとラベルが埋まっている。
        assert!(p.contains("子育て中の元介護士"), "{p}");
        assert!(p.contains("ベテラン転職者"), "{p}");
        assert!(p.contains("夕方の送迎車の前で"), "{p}");
        // ラベル一致したペルソナの人物像 (訴求の根拠) が注入される。
        assert!(p.contains("訴求の根拠"), "{p}");
        assert!(p.contains("保育園のお迎えに間に合わない"), "{p}");
        // ユーザー指定の4要素 (ネガティブ/構図・カメラ/アスペクト比/撮影指示書兼用) の指示。
        assert!(p.contains("negative_prompt"), "{p}");
        assert!(p.contains("aspect_ratio"), "{p}");
        assert!(p.contains("撮影指示書"), "{p}");
        // 2026-07-25 強化: 固定化の核となる指示群。
        assert!(p.contains("解釈の余地を残さない"), "{p}");
        assert!(p.contains("曖昧語の禁止"), "{p}");
        assert!(p.contains("合計N名のみ"), "{p}");
        assert!(p.contains("【被写体】"), "{p}");
        assert!(p.contains("【禁止事項】"), "{p}");
        assert!(p.contains("appeal_core"), "{p}");
        assert!(p.contains("レンズ焦点距離"), "{p}");
        // 日本語プロンプト指定・意図保持・商標禁止。
        assert!(p.contains("日本語"), "{p}");
        assert!(p.contains("意図"), "{p}");
        assert!(p.contains("商標"), "{p}");

        // 配列そのもの+ペルソナなしでも壊れない。
        let bare = json!([{"persona_label": "A", "direction": "屋外で"}]);
        let p2 = build_image_prompts_prompt(&bare, &Value::Null);
        assert!(p2.contains("屋外で"), "{p2}");

        // 空でも壊れない。
        let empty = json!({"directions": []});
        let p3 = build_image_prompts_prompt(&empty, &Value::Null);
        assert!(p3.contains("ディレクションが提供されていません"), "{p3}");
    }

    #[test]
    fn image_prompts_schema_requires_all_fields() {
        let s = image_prompts_schema();
        assert_eq!(s["properties"]["prompts"]["type"], "array");
        let req = s["properties"]["prompts"]["items"]["required"]
            .as_array()
            .unwrap()
            .iter()
            .filter_map(|v| v.as_str())
            .collect::<Vec<_>>();
        for k in ["persona_label", "appeal_core", "prompt", "negative_prompt", "aspect_ratio"] {
            assert!(req.contains(&k), "required に {k} が無い: {req:?}");
        }
    }

    #[test]
    fn mobile_prompt_has_rules_and_facts() {
        let facts = "給与: 月給30万円\n勤務時間: 9:00-18:00";
        let p = build_mobile_prompt(&sample_persona(), facts);
        assert_common_constraints(&p);
        // スマホ執筆ルール。
        assert!(p.contains("最大40字"), "{p}");
        assert!(p.contains("空行"), "{p}");
        assert!(p.contains("文頭"), "{p}");
        // facts_text が埋まり、「事実の範囲内だけ」を強調。
        assert!(p.contains("月給30万円"), "{p}");
        assert!(p.contains("範囲内だけで書く"), "{p}");
    }

    #[test]
    fn mobile_prompt_survives_empty_facts() {
        let p = build_mobile_prompt(&sample_persona(), "");
        assert_common_constraints(&p);
        assert!(p.contains("検証済み事実が提供されていません"), "{p}");
    }

    #[test]
    fn ab_prompt_has_metrics_and_summary() {
        let p = build_ab_prompt("介護求人。子育て層に時間の自由で訴求。");
        assert_common_constraints(&p);
        assert!(p.contains("CTR"));
        assert!(p.contains("CVR"));
        assert!(p.contains("CPA"));
        assert!(p.contains("子育て層に時間の自由で訴求"), "{p}");
    }

    #[test]
    fn ab_prompt_survives_empty_summary() {
        let p = build_ab_prompt("");
        assert_common_constraints(&p);
        assert!(p.contains("戦略要約は提供されていません"), "{p}");
    }

    #[test]
    fn all_schemas_are_valid_objects_with_required_keys() {
        // analyze
        let a = analyze_schema();
        assert_eq!(a["type"], "object");
        let req: Vec<&str> = a["required"].as_array().unwrap().iter().map(|v| v.as_str().unwrap()).collect();
        assert_eq!(req, vec!["surface_strengths", "hidden_strengths", "bottlenecks"]);

        // personas: items の required に5フィールド。
        let pe = personas_schema();
        assert_eq!(pe["required"][0], "personas");
        let pitem_req: Vec<&str> = pe["properties"]["personas"]["items"]["required"]
            .as_array().unwrap().iter().map(|v| v.as_str().unwrap()).collect();
        assert_eq!(pitem_req, vec!["label", "profile", "dissatisfaction", "environment", "pain"]);

        // copy: style は enum で3種固定。
        let c = copy_schema();
        assert_eq!(c["required"][0], "copies");
        let enum_vals: Vec<&str> = c["properties"]["copies"]["items"]["properties"]["style"]["enum"]
            .as_array().unwrap().iter().map(|v| v.as_str().unwrap()).collect();
        assert_eq!(enum_vals, COPY_STYLES.to_vec());
        let citem_req: Vec<&str> = c["properties"]["copies"]["items"]["required"]
            .as_array().unwrap().iter().map(|v| v.as_str().unwrap()).collect();
        assert_eq!(citem_req, vec!["style", "text"]);

        // images
        let im = images_schema();
        assert_eq!(im["required"][0], "directions");
        let iitem_req: Vec<&str> = im["properties"]["directions"]["items"]["required"]
            .as_array().unwrap().iter().map(|v| v.as_str().unwrap()).collect();
        assert_eq!(iitem_req, vec!["persona_label", "direction"]);

        // mobile
        let mo = mobile_schema();
        assert_eq!(mo["required"][0], "lines");
        assert_eq!(mo["properties"]["lines"]["type"], "array");
        assert_eq!(mo["properties"]["lines"]["items"]["type"], "string");

        // ab
        let ab = ab_schema();
        assert_eq!(ab["required"][0], "steps");
        let abitem_req: Vec<&str> = ab["properties"]["steps"]["items"]["required"]
            .as_array().unwrap().iter().map(|v| v.as_str().unwrap()).collect();
        assert_eq!(abitem_req, vec!["metric", "action"]);
    }
}
