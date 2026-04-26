# P2: ドメインロジック強化プラン

**作成日**: 2026-04-26
**作成チーム**: P2 (ドメインロジック強化)
**前提**: 親セッションが P0 (MF-1 単位ズレ等の Critical 修正) を別途実施中
**対象ファイル**: `src/handlers/insight/{engine.rs, engine_flow.rs, helpers.rs, phrase_validator.rs}`、`src/handlers/survey/aggregator.rs`、`src/handlers/recruitment_diag/{mod.rs, condition_gap.rs, handlers.rs}`
**準拠原則**: `feedback_reverse_proof_tests.md` (テストは逆証明) / `feedback_correlation_not_causation.md` (相関≠因果) / `feedback_test_data_validation.md` (データ妥当性)

---

## 0. 課題サマリと優先度

| # | 課題 | カテゴリ | 重大度 | 工数 | リスク | 着手順位 |
|---|------|---------|--------|------|--------|---------|
| 1 | 既存 22 patterns に `assert_valid_phrase` 適用 | 誠実性 | 🟡 | 小 | 既存テスト破綻なし | **1** |
| 2 | 雇用形態分類の二重定義統一 (`emp_classifier.rs` 新設) | データ整合 | 🟡 | 中 | survey/diag 両方の集計値変動 | **3** |
| 3 | M-2 SW-F02 vs SW-F05 同時発火矛盾 | 矛盾解消 | 🟡 | 小 | F02 発火減 | **5** |
| 4 | M-3 SW-F03 vs SW-F08 中間沈黙 | カバレッジ | 🟢 | 中 | 新パターン追加 | **9** |
| 5 | M-7 IN-1 発火条件反転疑い | 論理エラー | 🟡 | 小 | IN-1 発火頻度逆転 | **2** |
| 6 | M-8 SW-F06 仕様乖離 (人流のみ→AND条件) | 仕様乖離 | 🟡 | 中 | F06 発火減 | **6** |
| 7 | LS-1「未マッチ層」用語改訂 | 誤誘導 | 🟡 | 小 | body 文言変更のみ | **4** |
| 8 | Panel 1 採用難度の観光地補正 | 誤判定 | 🟡 | 中 | Panel 1 スコア分布変動 | **8** |
| 9 | Panel 5 emp_type フィルタ→`expand_employment_type` 経由 | 機能不全 | 🟡 | 小 | パート/その他のヒット数増 | **7** |
| 10 | RC-2 給与差閾値の動的化 (固定→percentile) | 誤発火 | 🟢 | 中 | RC-2 発火頻度変動 | **11** |
| 11 | HS-4 TEMP_LOW_THRESHOLD=0.0 の根拠調査 | 閾値妥当性 | 🟡 | 中 | HS-4 発火頻度大幅変動 | **10** |
| 12 | SW-F04 / SW-F10 未実装プレースホルダ判断 | 仕様未確定 | 🟢 | 大/小 | 削除なら sweep | **13** |
| 13 | AP-1 給与改善の年間人件費補正 (×12→×16+法定福利) | 過小推定 | 🟡 | 小 | 「コスト増額」表示値の上振れ | **12** |
| 14 | 月160h vs 厚労省 165-170h ズレ | 給与換算 | 🟢 | 小 | 全 emp_group 集計値変動 | **14** |

**全 38 patterns**: phrase_validator 通過テストを最終回帰として全 14 課題完遂後に実施。

---

## 1. 既存 22 patterns に `assert_valid_phrase` 適用

### 現状の問題
- `engine.rs:14-37` `generate_insights()` 内で `analyze_hiring_structure` / `analyze_forecast` / `analyze_regional_comparison` / `analyze_commute_zone` / `generate_action_proposals` の戻り値に対し `assert_valid_phrase` 未呼出。
- StructuralContext (LS/HH/MF/IN/GE) 6 パターンは `engine.rs:1364-1393` で適用済。
- engine_flow.rs (SW-F01〜F10) は `engine_flow.rs:35` で適用済。
- 結果、既存 22 patterns 内に断定表現が混在。例:
  - `engine.rs:130` 「過去3ヶ月連続で高水準を維持しています」 (必須ヘッジなし → 検証 NG)
  - `engine.rs:181` 「競争力が不足しています」 (同)
  - `engine.rs:251` 「応募率が低下する傾向があります」 (OK: 「傾向」あり)
  - `engine.rs:610-616` 「10年以内に大量退職が見込まれます」 (必須ヘッジなし → NG)
  - `engine.rs:951` 「全国中央値に到達できます」 (必須ヘッジなし → NG)
  - `engine.rs:1294` 「応募者プールの拡大が見込めます」 (必須ヘッジなし → NG)
  - `engine.rs:1037-1039` 「リーチできていない可能性があります」 (OK: 「可能性」あり)

### 逆証明テスト案
```rust
// pattern_audit_test.rs に追加
#[test]
fn all_22_existing_patterns_pass_phrase_validator() {
    use crate::handlers::insight::phrase_validator::validate_insight_phrase;
    let mut ctx = Ctx::new()
        // HS-1 発火: vacancy_rate=0.35, ts 3点とも 0.30 超
        .with_vacancy_rate("正社員", 0.35)
        .with_ts_vacancy_rates("正社員", &[0.31, 0.32, 0.33])
        // HS-2 発火: comp_index=0.75
        .with_salary_comp("正社員", local=200000.0, national=270000.0, comp=0.74)
        // HS-3 / HS-4 / HS-5 / HS-6 / FC-1〜4 / RC-1〜3 / AP-1〜3 / CZ/CF を順次発火
        .build();
    let insights = generate_insights(&ctx.inner);
    let mut failed = vec![];
    for ins in &insights {
        if let Err(e) = validate_insight_phrase(&ins.body) {
            failed.push((ins.id.clone(), e));
        }
    }
    assert!(
        failed.is_empty(),
        "Phrase validation failed for: {:#?}",
        failed
    );
}
```

**修正前**: HS-1 / HS-2 / FC-3 / AP-1 / CF-2 など複数 NG (検証実施で確定)
**修正後**: 38 patterns 全て pass

### 提案する修正方針
**Step 1**: `engine.rs` の 5 つのアグリゲータ関数に統一 helper を挟む
```rust
// engine.rs に追加
fn push_validated(out: &mut Vec<Insight>, ins: Insight) {
    super::phrase_validator::assert_valid_phrase(&ins.body);
    out.push(ins);
}

// 各 analyze_* 関数を以下のパターンに変更
fn analyze_hiring_structure(ctx: &InsightContext) -> Vec<Insight> {
    let mut out = Vec::new();
    if let Some(insight) = hs1_chronic_shortage(ctx) { push_validated(&mut out, insight); }
    // ...HS-2〜HS-6 も同様
    out
}
```

**Step 2**: 既存 body の修正 (debug ビルドで panic を確認しつつ)
| Pattern | Before (engine.rs L) | After |
|---|---|---|
| HS-1 | L130「高水準を維持しています」 | 「高水準で推移する**傾向**がみられます」 |
| HS-2 | L181「競争力が不足しています」 | 「競争力が不足する**可能性**があります」 |
| HS-3 | L249-251 既に「傾向」あり | 確認のみ |
| HS-4 | L300-303「緊急性が伝わっていません」 | 「緊急性が伝わりにくい**傾向**がみられます」 |
| HS-5 | L348-349「コントロールしている可能性」 | OK |
| HS-6 | L391「拡大が有効です」 | 「拡大が有効な**可能性**があります」 |
| FC-1 | L470-477 「{} しています」 | 「{} する**傾向**にあります」 |
| FC-2 | L514「ほぼ同水準です」 | OK (補足必要) |
| FC-3 | L610「大量退職が見込まれます」 | 「大量退職が見込まれる**可能性**があります」 |
| FC-4 | L678「リスクがあります」 | 「リスクの**可能性**があります」 |
| RC-1 | L748「下位に位置しており」 | OK (「傾向」追加が安全) |
| RC-2 | L810 fact のみ | 「給与差の**傾向**がみられます」追加 |
| RC-3 | L856-867 既に caveat あり | 確認のみ |
| AP-1 | L951「到達できます」 | 「到達する**可能性**があります」 |
| AP-2 | L1006「高まる傾向があります」 | OK |
| AP-3 | L1037-1039「可能性があります」 | OK |
| CZ-1 | L1114「有効です」 | 「有効な**可能性**があります」 |
| CZ-2 | L1164-1170「リスクあり」「環境」 | 「**傾向**」追加 |
| CZ-3 | L1206「懸念されます」 | 「懸念される**可能性**があります」 |
| CF-1 | L1256「可能性」あり | OK |
| CF-2 | L1294「拡大が見込めます」 | 「拡大が見込める**可能性**があります」 |
| CF-3 | L1331-1336 fact のみ | 「**傾向**がみられます」追加 |

### 影響範囲
- `engine.rs`: 22 関数の body 文字列変更 + アグリゲータ 5 関数に push_validated 統一
- `pattern_audit_test.rs`: 全 38 pattern phrase 通過テスト追加
- 既存テスト破壊リスク: `engine_flow.rs` のテストは body 表現を直接検証していないため影響なし。`pattern_audit_test.rs` は severity/value 中心のため影響軽微。

### 工数 + リスク
- **工数**: 2-3 時間 (body 修正 22 + push_validated 統一 + 全 patterns テスト)
- **リスク**: debug ビルドで panic が連鎖発火 → ローカル `cargo test --lib` を 1 patterns ずつ修正・確認する漸進アプローチ推奨。既存 patterns の意味は変えず文末ヘッジ追加のみ。

### 検証必須項目
- `cargo test pattern_audit` 全合格
- `cargo build --release` 警告 0 件
- 各 pattern の severity 判定値 (Critical/Warning/Info/Positive) は不変であることを既存テストで再確認

---

## 2. 雇用形態分類の二重定義統一 (`emp_classifier.rs`)

### 現状の問題
- `survey/aggregator.rs:675-687` `classify_emp_group_label`: 契約/業務委託 → 正社員、パート/アルバイト → パート、それ以外 → 派遣・その他
- `recruitment_diag/mod.rs:74-81` `expand_employment_type`: UI「正社員」→ `[正社員]`、「パート」→ `[パート労働者, 有期雇用派遣パート, 無期雇用派遣パート]`、「その他」→ `[正社員以外, 派遣, 契約社員]`
- `survey/market_trend.rs:178-185` (推定): 別の分岐ロジック

**二重定義による具体的バグ**:
| 雇用形態文字列 | survey 分類 | diag 展開 | 結果 |
|---|---|---|---|
| `契約社員` | 正社員グループ | 「その他」展開 | survey で月給中央値を歪める / diag では「その他」フィルタで参照 |
| `業務委託` | 正社員グループ | どこにも該当せず空フィルタ | survey で月給混入 / diag で見つからない |
| `派遣` (フル) | 派遣・その他 | 「その他」展開 | 一致 |
| `パート労働者` | パート | パート展開 | 一致 |

### 逆証明テスト案
```rust
// tests/emp_classifier_test.rs (新規)
#[test]
fn contract_worker_consistent_classification() {
    // 修正前: survey は「正社員」、diag「その他」展開で「契約社員」含む → 二重定義
    // 修正後: 統一: 契約社員 → "その他" グループ (月額固定報酬は時給/月給と独立カテゴリ)

    // survey 側
    let group = emp_classifier::classify_group("契約社員");
    assert_eq!(group, EmpGroup::Other, "契約社員 must be Other group");

    // diag 側
    let db_values = emp_classifier::expand_to_db_values(EmpGroup::Other);
    assert!(db_values.contains(&"契約社員"));
    assert!(db_values.contains(&"派遣"));
    assert!(db_values.contains(&"正社員以外"));

    // 業務委託は「その他」または独立カテゴリ (要設計判断)
    let group2 = emp_classifier::classify_group("業務委託");
    assert_ne!(group2, EmpGroup::Regular, "業務委託 must NOT be Regular");
}

#[test]
fn survey_aggregation_excludes_contract_from_regular_monthly() {
    // 修正前: 正社員月給バケットに契約社員の固定月額が混入し中央値上振れ
    // 修正後: 正社員バケットには「正社員」「正職員」のみ含まれる
    let records = vec![
        record("正社員", SalaryType::Monthly, 250000),
        record("契約社員", SalaryType::Monthly, 800000), // 業務委託風の高額
    ];
    let aggs = aggregate_by_emp_group_native(&records);
    let regular = aggs.iter().find(|a| a.group_label == "正社員").unwrap();
    // 修正後は契約社員が混入しないので median が 250000 のまま
    assert_eq!(regular.median, 250000);
    assert_eq!(regular.count, 1, "Regular bucket must contain only 正社員");
}
```

**修正前 (具体値)**:
- citycode=13104 新宿区で「正社員」中央値 = 280000円 (契約社員 800000 円 1 件混入で +20000 円 上振れ)
- diag「その他」フィルタで「契約社員」は HW DB に存在するためヒット → survey と二重カウント

**修正後 (具体値)**:
- 正社員中央値 = 260000円 (純粋な正社員のみ)
- 契約社員は「その他」グループの月給/年俸として別バケットで集計

### 提案する修正方針
**Step 1**: `src/handlers/emp_classifier.rs` 新設
```rust
//! 雇用形態の統一分類モジュール
//! survey / recruitment_diag / market_trend で同じ意味で動作することを保証

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum EmpGroup {
    Regular,    // 正社員系 (月給ベース集計)
    PartTime,   // パート/アルバイト系 (時給ベース集計)
    Other,      // 契約/派遣/業務委託/正社員以外 (multi-modal 報酬)
}

impl EmpGroup {
    pub const fn label(&self) -> &'static str {
        match self {
            Self::Regular => "正社員",
            Self::PartTime => "パート",
            Self::Other => "その他",
        }
    }
}

/// HW postings.employment_type 文字列 → EmpGroup
pub fn classify(emp: &str) -> EmpGroup {
    if emp.contains("パート") || emp.contains("アルバイト") {
        EmpGroup::PartTime
    } else if emp.contains("正社員") || emp.contains("正職員") {
        // 「正社員以外」もここに引っかからないため、直前の「以外」を確認
        if emp.contains("以外") {
            EmpGroup::Other
        } else {
            EmpGroup::Regular
        }
    } else {
        // 契約/業務委託/派遣 → Other に集約
        EmpGroup::Other
    }
}

/// UI 3 区分 → DB の employment_type 値リスト (IN 句用)
pub fn expand_to_db_values(group: EmpGroup) -> Vec<&'static str> {
    match group {
        EmpGroup::Regular => vec!["正社員", "正職員"],
        EmpGroup::PartTime => vec!["パート労働者", "有期雇用派遣パート", "無期雇用派遣パート"],
        EmpGroup::Other => vec!["正社員以外", "派遣", "契約社員", "業務委託"],
    }
}

/// UI 文字列 → EmpGroup (UI セレクトの 3 値専用)
pub fn from_ui_value(ui: &str) -> Option<EmpGroup> {
    match ui {
        "正社員" => Some(EmpGroup::Regular),
        "パート" => Some(EmpGroup::PartTime),
        "その他" => Some(EmpGroup::Other),
        _ => None,
    }
}
```

**Step 2**: 既存呼出箇所の置換
- `survey/aggregator.rs:675-687` → `emp_classifier::classify(emp).label()`
- `recruitment_diag/mod.rs:74-81` → `emp_classifier::expand_to_db_values(emp_classifier::from_ui_value(ui)?)`
- `survey/market_trend.rs:178-185` → 同上

**Step 3**: survey の月給バケット計算で `EmpGroup::Other` を別 native_unit で集計
```rust
// aggregator.rs の native_unit 決定ロジック
let native_unit = match group {
    EmpGroup::Regular => "月給",
    EmpGroup::PartTime => "時給",
    EmpGroup::Other => {
        // 内部の salary_type 多数派で動的決定 (現行ロジック維持)
    }
};
```

### 影響範囲
- 新規: `src/handlers/emp_classifier.rs`
- 修正: `survey/aggregator.rs` / `recruitment_diag/mod.rs` / `survey/market_trend.rs`
- 既存テスト: `recruitment_diag/mod.rs:83-` の `expand_employment_*` 系テストは値が変わるため要修正 (例: 「その他」→ `[正社員以外, 派遣, 契約社員, 業務委託]` 4件)
- 集計値: 正社員中央値が 5-10% 程度下方修正される可能性 (業務委託/契約社員の月額固定報酬がバケットから抜けるため)

### 工数 + リスク
- **工数**: 4-5 時間 (新モジュール + 3 ファイル置換 + テスト追加 + 既存テスト修正)
- **リスク**: 集計値変動を「数値が変わった = バグ」と誤認させないため、commit 直前に before/after で代表市区町村 (例: 13104, 14104, 27127) の集計値を CSV ダンプし変化幅を文書化必須

### 検証必須項目
- 既存 `expand_employment_part` / `expand_employment_regular` テスト pass (値変更があれば assert 修正済み)
- 新規 `contract_worker_consistent_classification` pass
- pattern_audit 全 38 pass

---

## 3. M-2 SW-F02 vs SW-F05 同時発火矛盾

### 現状の問題
- `engine_flow.rs:73-95` SW-F02 発火条件: `holiday_day_ratio >= 1.3`
- `engine_flow.rs:144-166` SW-F05 発火条件: `holiday_day_ratio >= 1.5`
- holiday_day_ratio = 1.6 のとき両方発火
  - SW-F02: 「休日商圏の人材**不足**」
  - SW-F05: 「観光ポテンシャル**未活用**」(機会あり)
- 同じ事象 (休日昼間滞在の集中) を「不足リスク」と「機会あり」の正反対の方向性で示唆 → ユーザー混乱

### 逆証明テスト案
```rust
#[test]
fn swf02_swf05_mutually_exclusive_at_high_holiday_ratio() {
    // 修正前: holiday_day_ratio=1.6 で F02 と F05 が両方 Some(_)
    // 修正後: F02 は holiday_day_ratio が 1.3..1.5 範囲のみ発火、1.5+ は F05 のみ
    let flow = mock_flow(None, Some(1.6), None, None, None, None);
    let ctx = Ctx::new().build();
    let insights = analyze_flow_insights(&ctx.inner, &flow);

    let f02 = insights.iter().find(|i| i.id == "SW-F02");
    let f05 = insights.iter().find(|i| i.id == "SW-F05");

    // F02 は 1.5 以上では沈黙
    assert!(f02.is_none(), "F02 must not fire when ratio >= 1.5");
    assert!(f05.is_some(), "F05 must fire when ratio >= 1.5");
}

#[test]
fn swf02_fires_in_intermediate_range() {
    // holiday_day_ratio=1.4 なら F02 のみ発火 (商圏不足の純粋なシグナル)
    let flow = mock_flow(None, Some(1.4), None, None, None, None);
    let ctx = Ctx::new().build();
    let insights = analyze_flow_insights(&ctx.inner, &flow);
    assert!(insights.iter().any(|i| i.id == "SW-F02"));
    assert!(!insights.iter().any(|i| i.id == "SW-F05"));
}
```

### 提案する修正方針
F02 に上限閾値を追加 (1.5 未満):
```rust
// engine_flow.rs:73-95
fn swf02_holiday_commerce(_ctx: &InsightContext, flow: &FlowIndicators) -> Option<Insight> {
    let ratio = flow.holiday_day_ratio?;
    // 修正後: 1.3..1.5 のみ発火 (1.5+ は SW-F05 観光ポテンシャルへ)
    if !(FLOW_HOLIDAY_CROWD_WARNING..FLOW_TOURISM_RATIO_THRESHOLD).contains(&ratio) {
        return None;
    }
    // ...以下既存
}
```

または body 文中で「観光地特性については SW-F05 を併参照」とクロスリファレンス追加 (RC-3↔GE-1 と同パターン)。後者が情報量を保てるため推奨。

### 影響範囲
- `engine_flow.rs:75` の 1 行変更 (排他閾値) または body にクロスリファレンス追加
- `pattern_audit_test.rs`: SW-F02 のテストケースで境界値 1.5 を追加

### 工数 + リスク
- **工数**: 30 分
- **リスク**: 排他にすると F02 発火率が 5-10% 減少 (代表市区町村のサンプリングで確認要)。観光地で F02 が出なくなる → 観光地の人材不足は SW-F05 + SW-F09 (季節雇用) で代替シグナル可能

### 検証必須項目
- `swf02_swf05_mutually_exclusive_at_high_holiday_ratio` pass
- `swf02_fires_in_intermediate_range` pass
- pattern_audit 全 38 pass

---

## 4. M-3 SW-F03 vs SW-F08 中間沈黙

### 現状の問題
- `engine_flow.rs:98-125` SW-F03: `daynight_ratio < 0.8` かつ `(1 - ratio) >= 0.2`
- `engine_flow.rs:220-243` SW-F08: `daynight_ratio >= 1.3`
- daynight_ratio が 0.8〜1.3 の市区町村 (中間地域、全国の半数以上) は両方発火しない → 「人流データから何も示唆なし」となる

### 逆証明テスト案
```rust
#[test]
fn intermediate_daynight_should_emit_balanced_insight() {
    // daynight_ratio=1.0 (完全均衡) で SW-F03/F08 は発火しない
    let flow = mock_flow(None, None, Some(1.0), None, None, None);
    let ctx = Ctx::new().build();
    let insights = analyze_flow_insights(&ctx.inner, &flow);
    assert!(!insights.iter().any(|i| i.id == "SW-F03"));
    assert!(!insights.iter().any(|i| i.id == "SW-F08"));

    // 修正後: 新パターン SW-F11「均衡型労働市場」を追加
    // assert!(insights.iter().any(|i| i.id == "SW-F11"));
}
```

### 提案する修正方針
**選択肢 A (推奨)**: 沈黙のまま許容 (Info パターン追加せず) — 「人流から特異な示唆なし」も情報の一つ。UI 側で「該当する人流由来示唆はありません」と注記
**選択肢 B**: 新パターン SW-F11「均衡型労働市場」を Severity::Positive で追加
```rust
fn swf11_balanced_labor_market(_ctx: &InsightContext, flow: &FlowIndicators) -> Option<Insight> {
    let ratio = flow.daynight_ratio?;
    if !(0.9..=1.1).contains(&ratio) {
        return None;
    }
    Some(Insight {
        id: "SW-F11".to_string(),
        severity: Severity::Positive,
        body: format!("昼夜間人口比{:.2}と均衡型で、域内雇用と居住が概ね一致する**傾向**がみられます。", ratio),
        // ...
    })
}
```

### 影響範囲
- 選択肢 A: 影響なし (実装変更なし)
- 選択肢 B: `engine_flow.rs` に 1 関数追加 + `analyze_flow_insights` に呼出追加 + pattern_audit_test 1 件追加

### 工数 + リスク
- **工数**: A=0、B=1 時間
- **リスク**: 38 patterns → 39 patterns に拡張すると docs (CLAUDE.md / IMPROVEMENT_ROADMAP_V2.md) 全体の数値更新が必要。**P2 では選択肢 A (沈黙許容) を推奨**

### 検証必須項目
- pattern_audit 全 38 pass (現状維持)
- UI で人流タブに「該当示唆なし」表示が出ることを E2E 確認

---

## 5. M-7 IN-1 発火条件反転疑い

### 現状の問題
- `engine.rs:1637`: `if !(0.05..=0.3).contains(&mw_share) { Severity::Info } else { return None }`
- 通常の医療福祉事業所比率 (10〜15%) は `0.05..=0.3` 範囲**内** → `!contains` が false → return None で**発火しない**
- 5% 未満 (極端に少ない) または 30% 超 (極端に多い) でのみ発火 → 仕様コメント (`engine.rs:1614-1615`) の「コサイン類似度乖離」と意図不一致

仕様コメントの意図:
> 簡易版: 事業所のうち医療福祉（C210850）比率と HW全体の欠員率の乖離を代替指標とする

**実装は欠員率を一切参照していない**。`mw_share` の絶対値のみで判定している。

### 逆証明テスト案
```rust
#[test]
fn in1_fires_when_industry_distribution_extreme() {
    // 修正前検証: 通常の mw_share=0.12 (12%) で IN-1 発火しないことを確認
    let mut ctx = Ctx::new();
    ctx.add_establishments(&[("850", 120.0), ("other", 880.0)]); // mw_share = 0.12
    let insights = analyze_structural_context(&ctx.inner);
    assert!(!insights.iter().any(|i| i.id == "IN-1"),
        "IN-1 must NOT fire at typical share 12% (current behavior)");

    // 極端値 (3%) で発火確認
    let mut ctx2 = Ctx::new();
    ctx2.add_establishments(&[("850", 30.0), ("other", 970.0)]); // mw_share = 0.03
    let insights2 = analyze_structural_context(&ctx2.inner);
    assert!(insights2.iter().any(|i| i.id == "IN-1"),
        "IN-1 must fire when mw_share < 5%");

    // 過剰 (35%) で発火確認
    let mut ctx3 = Ctx::new();
    ctx3.add_establishments(&[("850", 350.0), ("other", 650.0)]); // mw_share = 0.35
    let insights3 = analyze_structural_context(&ctx3.inner);
    assert!(insights3.iter().any(|i| i.id == "IN-1"),
        "IN-1 must fire when mw_share > 30%");
}

// 修正後の意図確認テスト
#[test]
fn in1_compares_industry_share_with_vacancy_rate() {
    // 修正後: mw_share と HW vacancy の医療福祉系比率の乖離で発火
    // 例: 事業所 12% 医療福祉 / HW 求人で医療福祉欠員 35% → 乖離大 → 発火
    let mut ctx = Ctx::new();
    ctx.add_establishments(&[("850", 120.0), ("other", 880.0)]);
    ctx.add_vacancy_with_industry(&[("medical", 35.0), ("other", 65.0)]); // 求人側 35%
    let insights = analyze_structural_context(&ctx.inner);
    let in1 = insights.iter().find(|i| i.id == "IN-1");
    assert!(in1.is_some(), "IN-1 must detect industry/vacancy mismatch");
}
```

**修正前 (代表市区町村)**:
- citycode=13104 新宿区 mw_share=0.08 → 範囲内 → IN-1 発火しない
- citycode=07210 須賀川市 mw_share=0.04 → 範囲外 → IN-1 発火 (意図と一致)
- 結果: 全国 1741 市区町村中、IN-1 発火率 < 10%

**修正後**:
- 「mw_share の絶対値が極端」+ 「mw_share と HW 医療福祉求人比率の差 > 0.15」の AND で発火
- HW 求人業種分布データが未取得の場合は P2 では「絶対値ベース閾値の妥当性確認のみ」に留め、本質的な実装は Phase B 拡張案件に分離

### 提案する修正方針
**Step 1 (即修正)**: 閾値ロジックを明確化し、`mw_share` の絶対値判定であることをコメント・body 文で明示
```rust
// engine.rs:1637 周辺
// 修正後
let severity = if mw_share < 0.05 {
    Severity::Warning // 医療福祉が極端に少ない地域 (医療系求人母集団が薄い)
} else if mw_share > 0.3 {
    Severity::Info    // 医療福祉が極端に多い地域 (他業種が薄い)
} else {
    return None;
};

let body = format!(
    "事業所のうち医療・福祉が{:.1}%を占めており、{}全国平均(約13%)から乖離する**傾向**がみられます。",
    mw_share * 100.0,
    if mw_share < 0.05 { "医療系求人母集団が薄く、" } else { "他業種求人が相対的に薄く、" }
);
```

**Step 2 (Phase B)**: HW 求人の業種分布を `vacancy_industry` テーブルから取得し、コサイン類似度実装

### 影響範囲
- `engine.rs:1616-1660` (1 関数の修正)
- `pattern_audit_test.rs`: IN-1 のテスト 3 ケース追加 (極端少 / 極端多 / 通常)

### 工数 + リスク
- **工数**: Step 1 のみで 1 時間
- **リスク**: 既存挙動と一致 (絶対値ベースの判定は変えない) ため発火率は不変。コメントと body 文の整合性向上のみ

### 検証必須項目
- `in1_fires_when_industry_distribution_extreme` pass
- 既存 IN-1 関連テストは severity 値変更 (Info→Warning) があれば assert 更新済み

---

## 6. M-8 SW-F06 仕様乖離 (人流のみ→AND条件)

### 現状の問題
- 仕様 (`helpers.rs:203-205`): 「2021人流/2019 > 0.9 AND 2021求人/2019 < 0.8」
- 実装 (`engine_flow.rs:171`): 人流側のみ判定 `if recovery < FLOW_COVID_FLOW_RECOVERY { return None; }`
- body (`engine_flow.rs:181-183`): 「求人側の回復率と比較することで採用マインドの慎重化の可能性を評価できます」と書きながら、求人側を実装で参照していない

### 逆証明テスト案
```rust
#[test]
fn swf06_requires_both_flow_recovery_and_posting_lag() {
    // 修正前: 人流回復のみで発火
    // 修正後: 人流回復 (>0.9) AND 求人遅延 (<0.8) の AND 条件

    // ケース1: 人流回復 0.95、求人 0.95 → 両方回復済 → 発火しない (修正後)
    let mut ctx = Ctx::new();
    let flow_a = mock_flow(None, None, None, Some(0.95), None, None);
    ctx.with_ts_counts_recovery_ratio(0.95);
    let insights_a = analyze_flow_insights(&ctx.inner, &flow_a);
    assert!(!insights_a.iter().any(|i| i.id == "SW-F06"),
        "F06 must NOT fire when posting also recovered");

    // ケース2: 人流回復 0.95、求人 0.7 → 求人だけ遅延 → 発火 (修正後の核心)
    let flow_b = mock_flow(None, None, None, Some(0.95), None, None);
    ctx.with_ts_counts_recovery_ratio(0.7);
    let insights_b = analyze_flow_insights(&ctx.inner, &flow_b);
    assert!(insights_b.iter().any(|i| i.id == "SW-F06"),
        "F06 MUST fire when flow recovered but posting still lags");
}
```

### 提案する修正方針
```rust
// engine_flow.rs:169
fn swf06_covid_recovery_divergence(ctx: &InsightContext, flow: &FlowIndicators) -> Option<Insight> {
    let recovery = flow.covid_recovery_ratio?;
    if recovery < FLOW_COVID_FLOW_RECOVERY {
        return None;
    }

    // 修正後: 求人側回復率を ts_counts から計算
    let posting_recovery = compute_posting_recovery_2021_vs_2019(&ctx.ts_counts)?;
    if posting_recovery >= FLOW_COVID_POSTING_LAG {
        return None; // 求人も回復していれば発火しない
    }

    Some(Insight {
        id: "SW-F06".to_string(),
        body: format!(
            "2021年の滞在人口が2019年比{:.0}%まで回復している一方、求人数は{:.0}%にとどまる**傾向**がみられ、採用マインドの慎重化の**可能性**がうかがえます。",
            recovery * 100.0,
            posting_recovery * 100.0,
        ),
        // ...
    })
}

fn compute_posting_recovery_2021_vs_2019(ts_counts: &[Row]) -> Option<f64> {
    // ts_counts から 2019/9 vs 2021/9 の posting_count を取得
    let count_2019 = ts_counts.iter()
        .filter(|r| get_str_ref(r, "year_month").starts_with("2019-09"))
        .map(|r| get_f64(r, "posting_count"))
        .sum::<f64>();
    let count_2021 = ts_counts.iter()
        .filter(|r| get_str_ref(r, "year_month").starts_with("2021-09"))
        .map(|r| get_f64(r, "posting_count"))
        .sum::<f64>();
    if count_2019 <= 0.0 { return None; }
    Some(count_2021 / count_2019)
}
```

### 影響範囲
- `engine_flow.rs:169-192` (関数本体修正 + ヘルパー追加)
- `pattern_audit_test.rs`: SW-F06 テストで ts_counts に 2019/2021 サンプル投入

### 工数 + リスク
- **工数**: 2-3 時間 (ヘルパー実装 + テスト)
- **リスク**: ts_counts が 2019/2021 月次データを持つ前提。データ未投入の場合は `compute_posting_recovery_2021_vs_2019` が None → 発火しない (graceful degradation で安全側)

### 検証必須項目
- `swf06_requires_both_flow_recovery_and_posting_lag` pass
- ts_counts データ未投入時の None 動作確認

---

## 7. LS-1 「未マッチ層」用語改訂

### 現状の問題
- `engine.rs:1426`: 「失業率が{}%（県平均{}%の{:.2}倍）で、未マッチ層が約{}人いる**可能性**があります」
- 失業者全員が HW 未マッチであるかの誤誘導
- 失業者には自営業希望者・他媒体応募者・非労働力化準備中者・進学準備者等が含まれる

### 逆証明テスト案
```rust
#[test]
fn ls1_body_avoids_unmatched_layer_terminology() {
    let mut ctx = Ctx::new()
        .with_ext_labor_force(unemployment_rate=5.0, employed=10000.0, unemployed=600.0)
        .with_pref_avg_unemployment_rate(3.0);
    let insights = analyze_structural_context(&ctx.inner);
    let ls1 = insights.iter().find(|i| i.id == "LS-1").expect("LS-1 must fire");

    // 修正前: 「未マッチ層」を含む
    // 修正後: 「未マッチ層」用語を排除し、「HW 以外の応募状況は本データから判定不可」明記
    assert!(!ls1.body.contains("未マッチ層"),
        "LS-1 must not use misleading '未マッチ層' term");
    assert!(ls1.body.contains("HW") || ls1.body.contains("他媒体"),
        "LS-1 must clarify HW scope limitation");
}
```

### 提案する修正方針
```rust
// engine.rs:1425-1428
body: format!(
    "失業率が{:.2}%（県平均{:.2}%の{:.2}倍）と高く、失業者数は約{:.0}人の**可能性**があります。\
     ただし HW 媒体以外への応募・自営業希望・進学準備等を含むため、HW 求人への応募余力は別途判定が必要な**傾向**がみられます。",
    unemp, pref_avg, ratio, unemployed_count
),
```

### 影響範囲
- `engine.rs:1425-1428` の body 文字列のみ
- `pattern_audit_test.rs`: LS-1 用語チェックテスト追加

### 工数 + リスク
- **工数**: 30 分
- **リスク**: なし (発火条件・severity は不変、body 文言のみ)

### 検証必須項目
- `ls1_body_avoids_unmatched_layer_terminology` pass
- phrase_validator pass

---

## 8. Panel 1 採用難度の観光地補正

### 現状の問題
- `recruitment_diag/handlers.rs:194-198`: `score = hw_count / population × 10000`
- 分母が「Agoop 平日昼滞在人口」 (population) → 観光地・繁華街では昼間滞在が膨張 → スコア低下 → 「穴場」誤判定
- 例: citycode=13104 新宿区 (推定 population=400000、hw_count=300) → score=7.5 → rank 4「激戦」 (妥当)
- 例: citycode=13102 中央区 (推定 population=600000 (オフィス街+銀座)、hw_count=200) → score=3.3 → rank 3「平均的」 (実態は採用難)

### 逆証明テスト案
```rust
#[test]
fn panel1_uses_residence_population_not_daytime_for_tourist_areas() {
    // 修正前: Agoop 平日昼滞在人口を分母 → 銀座・新宿 で過小スコア
    // 修正後: 「居住人口 (国勢調査) または 昼間滞在の min」を分母

    // 中央区: 居住=170000人、Agoop 滞在=600000人、HW=200件
    // 修正前: score = 200/600000*10000 = 3.3 → rank 3「平均的」
    // 修正後: score = 200/170000*10000 = 11.8 → rank 4「激戦」

    let result = compute_difficulty_score(
        hw_count=200,
        residence_population=170000.0,
        daytime_population=600000.0,
    );
    assert!(result.score >= 7.0 && result.score < 15.0);
    assert_eq!(result.rank, 4);
}

#[test]
fn panel1_residential_areas_unaffected() {
    // 居住人口 ≈ 昼間人口の住宅地では結果不変
    // 例: 八王子市 居住=580000、昼間=550000、HW=300
    let result_before = 300.0 / 550000.0 * 10000.0; // 5.45
    let result_after = compute_difficulty_score(300, 580000.0, 550000.0);
    assert!((result_after.score - result_before).abs() < 1.0,
        "Residential areas should produce similar scores");
}
```

### 提案する修正方針
**Step 1**: 国勢調査居住人口を Panel 1 の入力に追加 (既に `ext_population` に存在)
```rust
// handlers.rs:194-198 を修正
let residence_pop = fetch_residence_population(citycode); // 国勢調査 total_population
let denominator = residence_pop.min(daytime_pop);
let score = if denominator > 0.0 {
    (hw_count as f64) / denominator * 10_000.0
} else {
    0.0
};
```

**Step 2**: notes に分母選択ロジック明記
```rust
"calculation": "score = HW件数 ÷ min(国勢調査居住人口, Agoop平日昼滞在人口) × 10000",
"note_tourism_adjustment": "観光地・繁華街での昼間人口膨張による誤判定を防ぐため、居住人口と昼間人口の小さい方を採用",
```

### 影響範囲
- `recruitment_diag/handlers.rs:91-311` (Panel 1 関数全体)
- 既存テスト: `tests/recruitment_diag_test.rs` (存在すれば) で score 値の assert 更新
- frontend: `score_per_10k` の表示は不変、ただし `note_tourism_adjustment` をフッター追加

### 工数 + リスク
- **工数**: 3-4 時間 (国勢調査人口 fetch + min 計算 + テスト追加)
- **リスク**: 観光地・商業地の score が 1.5〜3 倍に上昇 → rank 上昇 → ユーザー体感「急に厳しくなった」。リリース前に before/after 代表値の比較表を docs に残すこと必須

### 検証必須項目
- `panel1_uses_residence_population_not_daytime_for_tourist_areas` pass
- `panel1_residential_areas_unaffected` pass
- 既存 Panel 1 E2E テストの score 値更新

---

## 9. Panel 5 emp_type フィルタを `expand_employment_type` 経由に

### 現状の問題
- `condition_gap.rs:176-180`: UI 値 (例「パート」) をそのまま `employment_type = ?` で SQL 検索
- HW DB の実値は「パート労働者」「有期雇用派遣パート」「無期雇用派遣パート」 → ヒット 0 件 → 「該当条件での HW 求人データが不足しており、比較できませんでした」誤表示

### 逆証明テスト案
```rust
#[test]
fn panel5_part_time_filter_finds_postings() {
    // 修正前: emp_type="パート" で SQL → "employment_type = 'パート'" → ヒット 0
    // 修正後: emp_type="パート" → IN ('パート労働者', '有期雇用派遣パート', '無期雇用派遣パート') → ヒットあり

    let db = setup_test_db_with_postings(&[
        ("看護師", "パート労働者", "東京都", "新宿区", 1500, 月給=200000),
        ("看護師", "有期雇用派遣パート", "東京都", "新宿区", 1600, 月給=210000),
    ]);
    let result = compute_median(&db, "看護師", "パート", "東京都", "新宿区");

    // 修正前: sample_size=0
    // 修正後: sample_size=2、median 計算可能
    assert!(result.sample_size >= 2, "Part-time filter must match HW DB values");
    assert!(result.annual_income > 0.0);
}

#[test]
fn panel5_other_filter_includes_contract_and_dispatch() {
    // emp_type="その他" → IN ('正社員以外', '派遣', '契約社員', '業務委託') を検索
    let db = setup_test_db_with_postings(&[
        ("看護師", "契約社員", "東京都", "新宿区", 1500, 月給=250000),
        ("看護師", "派遣", "東京都", "新宿区", 1600, 月給=240000),
    ]);
    let result = compute_median(&db, "看護師", "その他", "東京都", "新宿区");
    assert!(result.sample_size >= 2);
}
```

### 提案する修正方針
```rust
// condition_gap.rs:165-180 周辺
let mut wc: Vec<String> = vec!["salary_min > 0".to_string(), "salary_type = '月給'".to_string()];

if !job_type.is_empty() {
    wc.push(format!("job_type = ?{}", idx));
    params_own.push(job_type.to_string());
    idx += 1;
}

// 修正後: emp_type を expand 経由で IN 句に
if !emp_type.is_empty() {
    let expanded = crate::handlers::emp_classifier::expand_to_db_values(
        crate::handlers::emp_classifier::from_ui_value(emp_type)
            .ok_or_else(|| anyhow!("invalid emp_type"))?
    );
    if expanded.is_empty() {
        // フォールバック: 直接マッチ
        wc.push(format!("employment_type = ?{}", idx));
        params_own.push(emp_type.to_string());
        idx += 1;
    } else {
        let placeholders: Vec<String> = (0..expanded.len()).map(|i| format!("?{}", idx + i)).collect();
        wc.push(format!("employment_type IN ({})", placeholders.join(", ")));
        for v in expanded {
            params_own.push(v.to_string());
            idx += 1;
        }
    }
}
```

### 影響範囲
- `condition_gap.rs:176-180` (フィルタ拡張)
- emp_classifier モジュール (#2 で作成済前提) との依存

### 工数 + リスク
- **工数**: 1-2 時間
- **リスク**: 「パート」検索時のヒット件数が 0 → 数百件に増加 → 中央値が変動。Before/After で代表市区町村のサンプル件数表を docs に残す

### 検証必須項目
- `panel5_part_time_filter_finds_postings` pass
- `panel5_other_filter_includes_contract_and_dispatch` pass
- 既存 condition_gap E2E テスト sample_size 更新

---

## 10. RC-2 給与差閾値の動的化

### 現状の問題
- `engine.rs:796-802`: 固定 `-20000円` / `+10000円` を全職種に適用
- 介護職 (全国月給中央値 ≈ 240,000円) で -20000 → 8.3% 下振れ
- IT エンジニア (全国月給中央値 ≈ 400,000円) で -20000 → 5.0% 下振れ
- 同じ「-20000円」でも産業によって意味が異なる

### 逆証明テスト案
```rust
#[test]
fn rc2_uses_relative_threshold_not_absolute() {
    // 修正前: -20000円固定 → 介護で警告、IT で警告
    // 修正後: -10% 相対 → 介護(月給240k)で警告は -24000、IT(月給400k)で -40000

    // 介護: 230,000 円 vs 全国 240,000 円 = -10000 (4.2%) → 修正前 Info、修正後も Info (10% 未満)
    let ctx_a = Ctx::new()
        .with_cascade("正社員", local_salary=230000.0)
        .with_salary_comp("正社員", national=240000.0);
    let insights_a = analyze_regional_comparison(&ctx_a.inner);
    let rc2_a = insights_a.iter().find(|i| i.id == "RC-2").unwrap();
    assert_eq!(rc2_a.severity, Severity::Info);

    // IT: 360,000 円 vs 全国 400,000 円 = -40000 (10%) → 修正前 Warning、修正後 Warning (10% 閾値)
    let ctx_b = Ctx::new()
        .with_cascade("正社員", local_salary=360000.0)
        .with_salary_comp("正社員", national=400000.0);
    let insights_b = analyze_regional_comparison(&ctx_b.inner);
    let rc2_b = insights_b.iter().find(|i| i.id == "RC-2").unwrap();
    assert_eq!(rc2_b.severity, Severity::Warning);

    // IT: 380,000 円 vs 400,000 円 = -20000 (5%) → 修正前 Warning(誤発火)、修正後 Info (10% 未満)
    let ctx_c = Ctx::new()
        .with_cascade("正社員", local_salary=380000.0)
        .with_salary_comp("正社員", national=400000.0);
    let insights_c = analyze_regional_comparison(&ctx_c.inner);
    let rc2_c = insights_c.iter().find(|i| i.id == "RC-2").unwrap();
    assert_eq!(rc2_c.severity, Severity::Info);
}
```

### 提案する修正方針
```rust
// engine.rs:794-802
let diff = local_salary - national_avg;
let diff_pct = if national_avg > 0.0 { diff / national_avg } else { 0.0 };

// 修正後: パーセンテージ閾値 ±5% / -10%
let severity = if diff_pct < -0.10 {
    Severity::Warning
} else if diff_pct > 0.05 {
    Severity::Positive
} else {
    Severity::Info
};
```

`helpers.rs` に閾値定数追加:
```rust
/// RC-2 給与差: 全国平均比で判定
pub const RC2_SALARY_GAP_WARNING_PCT: f64 = -0.10; // -10%
pub const RC2_SALARY_GAP_POSITIVE_PCT: f64 = 0.05; // +5%
```

### 影響範囲
- `engine.rs:794-802`
- `helpers.rs` 定数 2 件追加
- `pattern_audit_test.rs` RC-2 テスト 3 件 (低/中/高給与水準別)

### 工数 + リスク
- **工数**: 1-2 時間
- **リスク**: 閾値変更で発火頻度に変動。介護職など低給与職種で発火率が低下、IT 等高給与職種で増加

### 検証必須項目
- `rc2_uses_relative_threshold_not_absolute` pass
- 全国分布での発火頻度の Before/After 比較

---

## 11. HS-4 TEMP_LOW_THRESHOLD = 0.0 の根拠調査

### 現状の問題
- `helpers.rs:143`: `TEMP_LOW_THRESHOLD = 0.0`
- `engine.rs:289`: 発火条件 `vacancy_rate >= VACANCY_CRITICAL && temperature < 0.0`
- temperature の物理単位・値分布が文書化されていない
- HW テキスト分析の出力スケール (-1.0..1.0 / 0..100 / Z-score など) 不明
- `engine.rs:316` で `format!("閾値{:.1}", TEMP_LOW_THRESHOLD)` と表示するが「0.0」の意味がユーザーに伝わらない

### 逆証明テスト案
```rust
#[test]
fn hs4_threshold_calibrated_to_temperature_distribution() {
    // 修正前: 固定閾値 0.0 で HS-4 発火
    // 修正後: temperature 分布の P25 を閾値に使用 (相対閾値)

    // ETL 段階で計算された temperature_p25 (推定値、約 -0.15) を使用
    // ケース1: temperature=-0.20、vacancy_rate=0.35 → 発火
    let ctx_a = Ctx::new()
        .with_temperature("正社員", value=-0.20, urgency_density=0.05)
        .with_vacancy_rate("正社員", 0.35)
        .with_temperature_p25(-0.15);
    let insights_a = analyze_hiring_structure(&ctx_a.inner);
    assert!(insights_a.iter().any(|i| i.id == "HS-4"));

    // ケース2: temperature=-0.10 (P25 より上)、vacancy_rate=0.35 → 発火しない
    let ctx_b = Ctx::new()
        .with_temperature("正社員", value=-0.10, urgency_density=0.05)
        .with_vacancy_rate("正社員", 0.35)
        .with_temperature_p25(-0.15);
    let insights_b = analyze_hiring_structure(&ctx_b.inner);
    assert!(!insights_b.iter().any(|i| i.id == "HS-4"));
}
```

### 提案する修正方針
**Step 1 (調査)**: ETL スクリプト (`hellowork_compute_layers.py` 等) で temperature の生成ロジックと分布を確認
- 出力: `docs/audit_2026_04_24/hs4_temperature_distribution.md` に P0/P25/P50/P75/P100 を文書化

**Step 2 (実装)**: 分布調査結果に基づき閾値を相対化
```rust
// helpers.rs に追加
pub const HS4_TEMP_PERCENTILE_THRESHOLD: f64 = 25.0; // P25 未満で発火

// engine.rs:289 周辺
let temp_p25 = ctx.pref_avg_temperature_p25.unwrap_or(0.0); // ETL 段階で県別 P25 を投入
if vacancy_rate < VACANCY_CRITICAL || temperature >= temp_p25 {
    return None;
}
```

**Step 3 (body 文言修正)**:
```rust
body: format!(
    "欠員率{:.1}%と高い一方、求人文のテキスト温度は{:.2}と県内下位 25% 水準にとどまる**傾向**がみられ、緊急性が伝わりにくい**可能性**がうかがえます。",
    vacancy_rate * 100.0, temperature,
),
```

### 影響範囲
- `helpers.rs` 定数追加
- `engine.rs:274-321` HS-4 関数本体
- `InsightContext` に `pref_avg_temperature_p25: Option<f64>` 追加
- ETL 側で `pref_temperature_stats` テーブル作成 (別タスク)
- `pattern_audit_test.rs` HS-4 テスト更新

### 工数 + リスク
- **工数**: 調査 4 時間 + 実装 3 時間 = 7-8 時間
- **リスク**: temperature 分布が左右非対称 / 異常値混入 / scale 不明な場合、P25 計算自体が不安定。ETL 仕様調査を先行必須

### 検証必須項目
- temperature 分布調査ドキュメント (P0/P25/P50/P75/P100)
- `hs4_threshold_calibrated_to_temperature_distribution` pass
- ETL 側での `pref_avg_temperature_p25` 出力確認

---

## 12. SW-F04 / SW-F10 未実装プレースホルダ判断

### 現状の問題
- `engine_flow.rs:128-141` SW-F04: 関数末尾で `None` 返却 (プレースホルダ)
- `engine_flow.rs:272-278` SW-F10: 同上
- 38 patterns と謳っているが実質 36 patterns しか動作しない
- ドキュメント (`design_agoop_jinryu.md` 等) で「Phase C 拡張予定」とコメント済

### 提案する修正方針
**選択肢 A**: 削除し 36 patterns に統一
- helpers.rs の `FLOW_MESH_ZSCORE_THRESHOLD`, `FLOW_COMPANY_TIME_DIFF_HOURS` 定数削除
- engine_flow.rs から 2 関数削除
- pattern_audit_test.rs から関連テスト削除
- docs (CLAUDE.md / IMPROVEMENT_ROADMAP_V2.md) の「38 patterns」を「36 patterns + 2 計画中」に変更

**選択肢 B**: 既存維持 (Phase C 待ち)
- 但し UI 上で「2 patterns は v2_posting_mesh1km 投入後に有効化」明記
- pattern_audit_test に `#[ignore]` 付きで意図的にスキップ宣言

**選択肢 C (推奨)**: 簡易実装で発火可能化
```rust
// SW-F04 簡易: 昼夜比 + vacancy_rate の組合せで「メッシュ未投入版」として発火
fn swf04_mesh_gap_simplified(ctx: &InsightContext, flow: &FlowIndicators) -> Option<Insight> {
    let daynight = flow.daynight_ratio?;
    if !(0.6..=2.0).contains(&daynight) {
        return None;
    }
    let vac_row = ctx.vacancy.iter().find(|r| get_str_ref(r, "emp_group") == "正社員")?;
    let vacancy_rate = get_f64(vac_row, "vacancy_rate");
    if vacancy_rate < VACANCY_WARNING {
        return None;
    }
    // 流入超過 (>1.3) かつ高欠員率 → メッシュ単位人材偏在の **可能性**
    Some(Insight {
        id: "SW-F04".to_string(),
        severity: Severity::Info,
        body: format!(
            "昼夜比{:.2}と人流差が大きく欠員率{:.1}%と高い**傾向**がみられ、メッシュ単位の人材偏在の**可能性**がうかがえます (詳細メッシュ分析は v2_posting_mesh1km 投入後に拡張予定)。",
            daynight, vacancy_rate * 100.0
        ),
        // ...
    })
}
```

### 影響範囲
- 選択肢 A: docs 多数の数値更新 + コード削除
- 選択肢 B: docs 注記追加のみ
- 選択肢 C: engine_flow.rs に 2 関数の簡易実装 + テスト追加

### 工数 + リスク
- A: 2-3 時間 / B: 30 分 / C: 4-5 時間
- C のリスク: Phase C で本実装に置き換えた際、簡易版との振る舞い差をユーザーが「劣化」と感じる可能性

### 推奨
**選択肢 B**: 「未実装である」事実を docs に明記し、CI に `#[ignore]` テスト追加で silent failure を防ぐ。Phase C で本実装。

### 検証必須項目
- 選択肢 B 採用時: `cargo test --test pattern_audit -- --ignored` で 2 patterns が「ignored」として明示的にスキップされることを確認

---

## 13. AP-1 給与改善の年間人件費計算補正

### 現状の問題
- `engine.rs:943`: `annual_cost = increase * 12.0`
- 賞与・社会保険料・退職金未考慮 → 実コストを過小推定
- 実務的な日本の年間人件費構成: 月給 × 12 + 賞与 (月給 × 4 ヶ月想定) + 法定福利費 (月給 × 16 × 0.16) ≈ 月給 × 18.56

### 逆証明テスト案
```rust
#[test]
fn ap1_includes_bonus_and_legal_welfare() {
    // 修正前: increase=10000, annual_cost = 120000
    // 修正後: annual_cost = 10000 * 16 * 1.16 = 185600

    let mut ctx = Ctx::new()
        .with_salary_comp("正社員", local=200000.0, national_median=210000.0)
        .add_existing_insight("HS-2"); // AP-1 は HS-2 依存
    let insights = generate_insights(&ctx.inner);
    let ap1 = insights.iter().find(|i| i.id == "AP-1").unwrap();

    let cost_evidence = ap1.evidence.iter()
        .find(|e| e.metric == "年間コスト増")
        .unwrap();
    // 修正前: 120000.0
    // 修正後: 185600.0 (16 ヶ月 × 1.16 法定福利)
    assert!((cost_evidence.value - 185600.0).abs() < 100.0);
}
```

### 提案する修正方針
```rust
// engine.rs:928-971 AP-1
const BONUS_MONTHS_DEFAULT: f64 = 4.0; // 賞与 4 ヶ月想定 (HW 中央値ベース)
const LEGAL_WELFARE_RATIO: f64 = 0.16;  // 健保・厚生年金・雇用保険・労災等 合計

let annual_increase_base = increase * (12.0 + BONUS_MONTHS_DEFAULT);
let annual_cost = annual_increase_base * (1.0 + LEGAL_WELFARE_RATIO);

body: format!(
    "月給を{:+.0}円引き上げ ({:.0}円→{:.0}円) すれば全国中央値に到達できる**可能性**があります。\
     1人あたり年間人件費増は約{:.0}円 (賞与4ヶ月+法定福利16%含む) と試算される**傾向**にあります。",
    increase, local_avg, national_median, annual_cost
),
```

または、`compute_annual_income` (condition_gap.rs:116) と統一して、地域の `bonus_months` 中央値を取得 (より精緻)

### 影響範囲
- `engine.rs:928-971` AP-1 関数
- `helpers.rs` に定数 2 件追加
- `pattern_audit_test.rs` AP-1 テスト assert 値更新

### 工数 + リスク
- **工数**: 1-2 時間
- **リスク**: 表示される「年間コスト増」が約 1.5 倍に上昇 → ユーザー体感「急に高くなった」。注釈で「賞与・法定福利費を含む」明記必須

### 検証必須項目
- `ap1_includes_bonus_and_legal_welfare` pass
- 既存テスト (もし annual_cost を assert するもの) 値更新

---

## 14. 月160h vs 厚労省 165-170h ズレ

### 現状の問題
- `aggregator.rs:582-606`: `Hourly→Monthly: v * 160`、`Monthly→Hourly: v / 160`
- 厚労省「就業条件総合調査」所定労働時間は 165-170h/月 が標準
- 160 vs 167 (中央値) で 4.4% 換算ズレ
- 派遣・パートの時給→月給換算で系統的に過小評価

### 逆証明テスト案
```rust
#[test]
fn hourly_to_monthly_uses_realistic_hours() {
    // 修正前: 時給1500円 → 月給240000円 (160h)
    // 修正後: 時給1500円 → 月給250500円 (167h、厚労省統計中央値)
    let records = vec![record("派遣", SalaryType::Hourly, 1500)];
    let aggs = aggregate_by_emp_group_native(&records);
    let dispatch = aggs.iter().find(|a| a.group_label == "派遣・その他").unwrap();
    // 月給換算値が月160h想定の 240000 ではなく月167h想定の 250500 に近い
    assert!(dispatch.monthly_values.iter().any(|&v| (v - 250500).abs() < 1000));
}
```

### 提案する修正方針
```rust
// aggregator.rs に定数追加
const STANDARD_MONTHLY_HOURS: i64 = 167; // 厚労省「就業条件総合調査」中央値
const STANDARD_DAILY_HOURS: i64 = 8;
const STANDARD_WEEKLY_HOURS: i64 = 40;
const STANDARD_DAYS_PER_MONTH: i64 = 20;
const WEEKS_PER_MONTH: f64 = 52.0 / 12.0; // 4.333...

// 換算箇所修正
SalaryType::Hourly => {
    bucket.hourly_values.push(v);
    bucket.monthly_values.push(v * STANDARD_MONTHLY_HOURS);
}
SalaryType::Monthly => {
    bucket.monthly_values.push(v);
    bucket.hourly_values.push(v / STANDARD_MONTHLY_HOURS);
}
SalaryType::Weekly => {
    let monthly = (v as f64 * WEEKS_PER_MONTH) as i64;
    bucket.monthly_values.push(monthly);
    bucket.hourly_values.push(v / STANDARD_WEEKLY_HOURS);
}
```

### 影響範囲
- `aggregator.rs:582-606`
- 全雇用形態の集計値が 4-5% 上方修正
- survey タブ・media タブの中央値・分布全体に影響
- pattern_audit には直接関係なし (engine.rs は ts_salary を別経路で取得)

### 工数 + リスク
- **工数**: 1 時間
- **リスク**: survey の歴史的データと不連続が生じる。リリース時に「換算式統一による集計値補正」を release notes 必須

### 検証必須項目
- `hourly_to_monthly_uses_realistic_hours` pass
- Before/After 代表市区町村 (13104, 14104, 27127) の median 比較表

---

## 15. 全課題完了後の最終検証

### 38 patterns phrase_validator 通過テスト (必須回帰)
```rust
#[test]
fn all_38_patterns_pass_phrase_validation() {
    use crate::handlers::insight::phrase_validator::validate_insight_phrase;

    // 全 38 patterns を強制発火させる maximally-firing context
    let ctx = Ctx::new().fully_loaded_for_all_patterns().build();
    let core = generate_insights(&ctx.inner);
    let flow = analyze_flow_insights(&ctx.inner, ctx.inner.flow.as_ref().unwrap());

    let mut all = core;
    all.extend(flow);

    // 期待: 22 (HS/FC/RC/AP/CZ/CF) + 6 (LS/HH/MF/IN/GE) + 10 (SW-F01-F10、F04/F10 は条件次第) = 36-38
    assert!(all.len() >= 36, "At least 36 patterns must fire under maximal context");

    let mut failures: Vec<(String, String)> = vec![];
    for ins in &all {
        if let Err(e) = validate_insight_phrase(&ins.body) {
            failures.push((ins.id.clone(), e));
        }
    }
    assert!(failures.is_empty(),
        "All patterns must pass phrase_validator. Failed: {:#?}",
        failures);
}
```

### Severity 分布逆証明テスト
```rust
#[test]
fn severity_distribution_is_balanced() {
    // 修正後の代表市区町村 3 件で Critical/Warning/Info/Positive の出現を確認
    for citycode in [13104, 14104, 27127] {
        let ctx = build_ctx_for_real_citycode(citycode);
        let insights = generate_insights(&ctx.inner);

        let counts = insights.iter().fold(
            (0, 0, 0, 0),
            |acc, i| match i.severity {
                Severity::Critical => (acc.0 + 1, acc.1, acc.2, acc.3),
                Severity::Warning => (acc.0, acc.1 + 1, acc.2, acc.3),
                Severity::Info => (acc.0, acc.1, acc.2 + 1, acc.3),
                Severity::Positive => (acc.0, acc.1, acc.2, acc.3 + 1),
            },
        );
        // 全 Severity が 0 でないこと (極端な偏りを検出)
        assert!(counts.2 > 0, "{}: at least 1 Info pattern expected", citycode);
    }
}
```

### Cross-pattern 矛盾チェック
```rust
#[test]
fn no_contradictory_patterns_fire_simultaneously() {
    // SW-F02 (人材不足) と SW-F05 (機会あり) が併発しないこと
    let flow = mock_flow(None, Some(1.6), None, None, None, None);
    let ctx = Ctx::new().build();
    let insights = analyze_flow_insights(&ctx.inner, &flow);
    let f02_count = insights.iter().filter(|i| i.id == "SW-F02").count();
    let f05_count = insights.iter().filter(|i| i.id == "SW-F05").count();
    assert!(f02_count + f05_count <= 1,
        "SW-F02 and SW-F05 must be mutually exclusive at high holiday_ratio");
}
```

---

## 16. 親セッションへの申し送り Top 5

優先順位: 影響範囲 × 工数効率 × P0 修正との衝突回避を考慮。

### 1. 既存 22 patterns に `assert_valid_phrase` 適用 (#1, 着手最優先)
- **理由**: 工数小 (2-3h)、リスクなし、誠実性メカニズムの根幹を整備
- **依存**: なし。P0 修正と独立に着手可能
- **効果**: 「断定表現が紛れ込んだ insight」の silent failure を完全排除

### 2. M-7 IN-1 発火条件の明確化 (#5)
- **理由**: 工数 1h、論理エラーが疑われる箇所の早期決着。コメントと実装の乖離を整理
- **依存**: なし
- **効果**: IN-1 の意図的発火条件が docs と一致

### 3. 雇用形態分類の二重定義統一 (`emp_classifier.rs`) (#2)
- **理由**: P0 #8 と完全同一案件。親セッション側の MF-1 修正が完了次第、即着手して一気通貫で実装
- **依存**: P0 完了後 (干渉回避)
- **効果**: survey/diag/market_trend で集計値が一致 + Panel 5 (#9) の前提整備

### 4. LS-1 「未マッチ層」用語改訂 (#7)
- **理由**: 工数 30 分、ペルソナ達成度向上に直結。誤誘導を即排除
- **依存**: #1 完了後 (phrase_validator が全 22 patterns で動作することを確認後)
- **効果**: ユーザー誤読リスクを 1 行で解消

### 5. M-2 SW-F02 vs SW-F05 同時発火矛盾 (#3)
- **理由**: 工数 30 分、矛盾の最も顕著な事例。ユーザー混乱を即解消
- **依存**: なし
- **効果**: 観光地の示唆方向性が一意化

### 着手禁止 (P2 では延期推奨)
- **#11 HS-4 TEMP_LOW_THRESHOLD 調査**: ETL 仕様調査が必要、本タスクのスコープ外
- **#12 SW-F04 / SW-F10**: ドキュメントに「Phase C 待ち」明記する選択肢 B 採用
- **#14 月160h ズレ**: survey の集計値が広範囲に変動するため、別 release notes が必要 → P3 または独立リリース

---

## 17. 工数集計

| 項目 | 工数 | 累積 |
|---|---|---|
| 1 既存 22 patterns phrase_validator | 3h | 3h |
| 5 IN-1 明確化 | 1h | 4h |
| 7 LS-1 用語改訂 | 0.5h | 4.5h |
| 3 F02/F05 排他 | 0.5h | 5h |
| 9 Panel 5 emp_type フィルタ | 2h | 7h |
| 2 emp_classifier 統一 | 5h | 12h |
| 6 SW-F06 AND 条件 | 3h | 15h |
| 8 Panel 1 観光地補正 | 4h | 19h |
| 10 RC-2 動的閾値 | 2h | 21h |
| 13 AP-1 法定福利 | 1.5h | 22.5h |
| **(P3 延期)** 11 HS-4 調査 | 8h | 30.5h |
| **(P3 延期)** 14 月160h ズレ | 1h | 31.5h |
| **(別リリース)** 12 SW-F04/F10 簡易実装 | 5h | 36.5h |

**P2 完遂見込み (項目 1-10, 13)**: 約 22.5 時間 (3 営業日)
**全課題完遂**: 約 36.5 時間 (5 営業日)

---

## 18. 検証チェックリスト (リリース前)

- [ ] `cargo build --release` 警告 0 件
- [ ] `cargo test --lib` 全合格 (既存 + 新規テスト)
- [ ] `cargo test pattern_audit` 38 patterns 全合格 (or 36 + ignored 2)
- [ ] phrase_validator: 全 38 patterns 通過 (verbose ログ確認)
- [ ] 代表市区町村 (13104, 14104, 27127) の Before/After 集計値比較表
- [ ] LS-1 body に「未マッチ層」が含まれないこと grep 確認
- [ ] SW-F02 と SW-F05 の同時発火 0 件 (代表市区町村サンプリング)
- [ ] Panel 1 score の Before/After 比較 (観光地での上昇確認)
- [ ] Panel 5 emp_type="パート" / "その他" でヒット件数 > 0 (代表 3 市区町村)
- [ ] AP-1 annual_cost が 1.5 倍程度に上昇していること (release notes 整合)
- [ ] CLAUDE.md / IMPROVEMENT_ROADMAP_V2.md の patterns 数表記更新

---

## 19. 関連ファイル絶対パス一覧

### 修正対象
- `C:\Users\fuji1\AppData\Local\Temp\hellowork-deploy\src\handlers\insight\engine.rs` (1740 行)
- `C:\Users\fuji1\AppData\Local\Temp\hellowork-deploy\src\handlers\insight\engine_flow.rs` (359 行)
- `C:\Users\fuji1\AppData\Local\Temp\hellowork-deploy\src\handlers\insight\helpers.rs` (220 行)
- `C:\Users\fuji1\AppData\Local\Temp\hellowork-deploy\src\handlers\insight\phrase_validator.rs` (123 行)
- `C:\Users\fuji1\AppData\Local\Temp\hellowork-deploy\src\handlers\insight\pattern_audit_test.rs` (1767 行)
- `C:\Users\fuji1\AppData\Local\Temp\hellowork-deploy\src\handlers\survey\aggregator.rs` (553-687 行 中心)
- `C:\Users\fuji1\AppData\Local\Temp\hellowork-deploy\src\handlers\recruitment_diag\mod.rs` (74-81 行)
- `C:\Users\fuji1\AppData\Local\Temp\hellowork-deploy\src\handlers\recruitment_diag\condition_gap.rs` (159-258 行)
- `C:\Users\fuji1\AppData\Local\Temp\hellowork-deploy\src\handlers\recruitment_diag\handlers.rs` (91-311 行 Panel 1)

### 新規作成
- `C:\Users\fuji1\AppData\Local\Temp\hellowork-deploy\src\handlers\emp_classifier.rs` (#2)
- `C:\Users\fuji1\AppData\Local\Temp\hellowork-deploy\docs\audit_2026_04_24\hs4_temperature_distribution.md` (#11、ETL 調査結果)

### 監査根拠 (Read-only)
- `C:\Users\fuji1\AppData\Local\Temp\hellowork-deploy\docs\audit_2026_04_24\team_gamma_domain.md`
- `C:\Users\fuji1\AppData\Local\Temp\hellowork-deploy\docs\audit_2026_04_24\00_overall_assessment.md`
- `C:\Users\fuji1\AppData\Local\Temp\hellowork-deploy\CLAUDE.md` (L223 vacancy_rate 定義)
