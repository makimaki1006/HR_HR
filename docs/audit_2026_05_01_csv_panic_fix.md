# 媒体分析 CSV アップロード panic 修正レビューパッケージ

**作成日**: 2026-05-01
**修正コミット**: `996ea6e` (push 済 origin/main)
**目的**: 別 AI / 別レビュアーが本修正を独立検証できるように、事象・原因・修正・検査結果を 1 枚にまとめる

---

## 1. 事象

ユーザーが Render 本番環境の **媒体分析タブ**（survey 機能）に CSV をアップロードした際に以下の panic が発生:

```
処理エラー: task 107 panicked with message
"start byte index 92 is not a char boundary; it is inside '円' (bytes 90..93)
of '電気工事・通信工事スタッフ*積水ハウス専属50年の安定感*月給30万円~*20・30代活躍中*賞与5'"
```

`task 107 panicked` は `tokio::task::spawn_blocking` で実行された CSV パース処理のスレッド panic。レスポンスとしては HTTP 500 相当のエラー文字列がフロントに返る（`src/handlers/survey/handlers.rs` 周辺）。

---

## 2. 原因

### 2.1 直接原因

`src/handlers/survey/salary_parser.rs::extract_months_before_suffix` の修正前コード:

```rust
// 修正前 (L147-153)
if let Ok(v) = num_str.parse::<f64>() {
    // 賞与/ボーナス/年 が pos の前 30 文字以内にあるか
    let window_start = before.len().saturating_sub(30);   // ← BUG: バイト演算
    let window = &before[window_start..];                 // ← マルチバイト中央でスライス
    if window.contains("賞与") || window.contains("ボーナス") || window.contains("年") {
        best = Some(best.map(|b| b.max(v)).unwrap_or(v));
    }
}
```

コメントは「前 30 文字以内」と意図しているが、`before.len()` は **バイト長**。`saturating_sub(30)` で 30 を引くとマルチバイト文字 (UTF-8 で日本語漢字 = 3 バイト) の途中に着地する可能性がある。`&str` を非 char 境界でスライスすると Rust は panic する。

### 2.2 panic を引き起こす入力経路

`parse_salary` は CSV の `salary_raw` 列ではなく、**列スコアリングで salary 候補に選ばれた任意のフィールド**（タイトル列でも description 列でもなり得る）に対して呼ばれる。`src/handlers/survey/upload.rs::parse_csv_bytes_with_hints` L310-327:

```rust
let salary_raw = {
    let mapped = get("salary");
    if score_salary(&mapped) > 0 { mapped }
    else {
        // 全列スキャン: salary 列が見つからなければ任意列を salary として採用
        for ci in 0..row.len() {
            let val = row.get(ci).unwrap_or("").trim();
            let s = score_salary(val);
            if s > best_score { best_score = s; best_val = val.to_string(); }
        }
        best_val
    }
};
let salary_parsed = parse_salary(&salary_raw, SalaryType::Monthly);
```

そのため求人タイトル文字列 (`"電気工事...賞与5ヶ月分"` 形式) が `parse_salary` に渡る経路が成立する。

### 2.3 数値の検証

失敗文字列のバイト数を実際に数えると (UTF-8):

| 文字列 | バイト範囲 |
|---|---|
| 電気工事・通信工事スタッフ (13 char) | 0..38 |
| `*` | 39 |
| 積水ハウス専属 (7 char) | 40..60 |
| `50` | 61..62 |
| 年の安定感 | 63..77 |
| `*` | 78 |
| 月給 | 79..84 |
| `30` | 85..86 |
| 万 | 87..89 |
| **円** | **90..92** |
| `~` | 93 |
| `*20・30代活躍中*賞与5` | 94..121 |
| **合計** | **122 byte** |

`before` がこの 122 byte 文字列のとき:

- `before.len() = 122`
- `before.len().saturating_sub(30) = 92`
- `&before[92..]` は `'円'` (90..93) の内部 → **panic**

panic メッセージ `byte index 92 is not a char boundary; it is inside '円' (bytes 90..93)` と完全一致。

### 2.4 トリガ条件

`extract_months_before_suffix` は `parse_bonus_months` 経由で `text` 内に suffix (`ヶ月` `ケ月` `か月` `カ月` `ヵ月` `箇月`) を **find した時のみ** ループ本体に入る。失敗 panic message に表示されている `before` 文字列が 122 byte で末尾が `5` で終わっていることから、原テキストはこの後ろに `ヶ月分` などの suffix が続いていたと推定される (例: `...賞与5ヶ月分支給`)。

---

## 3. 修正

### 3.1 コード変更

`src/handlers/survey/salary_parser.rs` L147-160:

```rust
if let Ok(v) = num_str.parse::<f64>() {
    // 賞与/ボーナス/年 が pos の前 30 文字以内にあるか
    // バイト境界ではなく char 境界で切り出す (マルチバイト対応)
    let window_start = before
        .char_indices()
        .rev()
        .nth(29)
        .map(|(i, _)| i)
        .unwrap_or(0);
    let window = &before[window_start..];
    if window.contains("賞与") || window.contains("ボーナス") || window.contains("年") {
        best = Some(best.map(|b| b.max(v)).unwrap_or(v));
    }
}
```

`char_indices().rev().nth(29)` で末尾から 30 文字目の **char 境界バイト位置**を取得。`before` が 30 文字未満なら `unwrap_or(0)` で先頭へフォールバック。

### 3.2 回帰テスト

`src/handlers/survey/salary_parser.rs` (テストモジュール内に追加):

```rust
/// 2026-05-01 マルチバイト境界パニック回帰テスト:
/// `before.len().saturating_sub(30)` でバイト演算していたためマルチバイト文字の
/// 途中でスライスして panic していたケース。
/// 失敗テキスト 122 byte の場合 122-30=92 が `円` (90..93) の途中。
#[test]
fn regression_multibyte_boundary_no_panic() {
    let text = "電気工事・通信工事スタッフ*積水ハウス専属50年の安定感*月給30万円~*20・30代活躍中*賞与5ヶ月分";
    let r = parse_salary(text, SalaryType::Monthly);
    assert_eq!(r.bonus_months, Some(5.0));
}
```

panic message に出ていた `before` 文字列に suffix (`ヶ月分`) を追加して、修正前のコードパスを完全に再現する。`Some(5.0)` まで到達することで window 検査が「賞与」を正しく検出できていることも併せて確認している。

---

## 4. 逆証明 (同種バグの網羅検査)

### 4.1 コードベース全体での `.len().saturating_sub` 走査

```
$ grep -rn '\.len()\.saturating_sub' src/
src/handlers/survey/salary_parser.rs:149  before.len().saturating_sub(30)  ← 修正済
src/handlers/survey/location_parser.rs:1150  chars.len().saturating_sub(8)  ← Vec<char> 操作 (安全)
```

`location_parser.rs:1150` は `let chars: Vec<char> = before.chars().collect();` の直後で、`Vec<char>` のインデックスとして使われている。文字単位なのでマルチバイト境界違反は発生しない。

### 4.2 `&str[..N]` 系スライスの全走査

`src/handlers/survey/` 配下の `&...[..pos]` `&...[N..]` パターンを全て検証:

| ファイル:行 | 内容 | 判定 |
|---|---|---|
| salary_parser.rs:137 | `&text[..pos]` (pos = find(suffix)) | 安全 ✓ |
| salary_parser.rs:340/360/389/405/416/427 | `&text[..*_pos]` (find 由来) | 安全 ✓ |
| salary_parser.rs:367/397/408 | `&text[pos + 'X'.len_utf8()..]` | 安全 ✓ |
| salary_parser.rs:189 | `&after[num_str.len()..]` (num_str は ASCII 数字) | 安全 ✓ |
| salary_parser.rs:192 | `&after_num['月'.len_utf8()..]` (starts_with('月') 後) | 安全 ✓ |
| location_parser.rs:943 | `&text[..station_pos + '駅'.len_utf8()]` | 安全 ✓ |
| location_parser.rs:1147 | `&clean[..pos]` (find 由来) | 安全 ✓ |
| location_parser.rs:1151 | `chars[start..]` (Vec<char> インデックス) | 安全 ✓ |
| upload.rs:913/924/938 | `&text[pos + "...".len()..]` (リテラル ASCII) | 安全 ✓ |
| upload.rs:147/152 | `&data[2..]` (`&[u8]` バイト列) | 安全 ✓ (str ではない) |

### 4.3 CSV 処理経路の全パーサー個別検査

| パーサー | バイト境界違反リスク | 状態 |
|---|---|---|
| `parse_salary` (salary_parser.rs) | あり (L149) | **修正済 + 回帰テスト** |
| `parse_bonus_months` 内全関数 | `find` + `len_utf8` で構成 | 安全 |
| `parse_location::try_station` | `chars()` ベース | 安全 |
| `parse_location::try_municipality_pattern` | `chars()` + Vec<char> | 安全 |
| `extract_annual_holidays` | `find` + リテラル ASCII 長 | 安全 |
| `decode_csv_bytes` | `&[u8]` 操作 + encoding_rs | 安全 (str ではない) |
| `normalize_text` | `chars()` イテレーション | 安全 |

### 4.4 全 lib テスト

```
cargo test --lib
test result: ok. 1069 passed; 0 failed; 2 ignored; 0 measured; 0 filtered out
```

### 4.5 別カテゴリの潜在問題（媒体分析 CSV panic とは無関係）

参考までに、走査の過程で見つけた **空 Vec / アンダーフロー panic** リスク (本件 panic とは別系統):

| 箇所 | リスク内容 |
|---|---|
| `src/handlers/insight/engine.rs:110` `ts_rates[ts_rates.len() - 3..]` | 時系列が 3 点未満で panic |
| `src/handlers/analysis/render/subtab5_anomaly.rs:550/585/790` | 空 Vec で `len()-1` panic |
| `src/handlers/survey/job_seeker.rs:101` `ranges.len() - narrow - wide` | アンダーフロー panic |
| `src/handlers/survey/report_html/helpers.rs:292` `sorted[idx.min(sorted.len() - 1)]` | 空 Vec で panic |

これらは DB 由来データへの操作で、CSV アップロード経路には乗らない。本修正のスコープ外として残置。

---

## 5. 関係ファイル一覧（レビュー対象）

### 5.1 修正ファイル

| パス | 役割 | 変更行数 |
|---|---|---|
| `src/handlers/survey/salary_parser.rs` | 給与文字列パーサー本体 + テスト | +21 / -1 |

### 5.2 修正コード周辺の関連ファイル

レビュアーが修正の文脈を把握するために読むと有用なファイル:

| パス | 役割 |
|---|---|
| `src/handlers/survey/upload.rs` | CSV エンコーディング判定 + パース本体 (`parse_csv_bytes_with_hints`)。L378 で `parse_salary` 呼び出し |
| `src/handlers/survey/handlers.rs` | HTTP ハンドラ。L140 で `tokio::task::spawn_blocking` を介して `parse_csv_bytes_with_hints` を起動 (panic を起こした task) |
| `src/handlers/survey/location_parser.rs` | 同様に CSV テキストを処理する地名パーサー (バイト境界違反は無いことを 4.2/4.3 で確認済) |
| `src/handlers/survey/salary_parser.rs::extract_bonus_months_after_keyword` (L163-208) | `parse_bonus_months` のもう一つのパス。同様に検査済 |

### 5.3 テスト

| パス | テスト名 | 役割 |
|---|---|---|
| `src/handlers/survey/salary_parser.rs` | `regression_multibyte_boundary_no_panic` | 失敗入力の完全再現テスト |
| `src/handlers/survey/salary_parser.rs` | `fixa_bonus_parse_*` (8 件) | 賞与パースの既存テスト (全て PASS) |
| `src/handlers/survey/salary_parser.rs` | `test_*` (基本系 19 件) | 給与パースの既存テスト (全て PASS) |

### 5.4 関連ドキュメント

| パス | 内容 |
|---|---|
| `docs/RELEASE_NOTES_2026-04-26.md` | Fix-A リリースノート (`bonus_months` 追加経緯) |
| `docs/audit_2026_04_24/` | 直近の他監査ドキュメント (本ファイルと同じ命名規則) |

---

## 6. レビューチェックリスト (別 AI 用)

修正の妥当性検証に必要な確認項目:

- [ ] **C-1**: `char_indices().rev().nth(29)` が「末尾から 30 文字目の char 境界バイト位置」を返すことを確認
  - `nth(29)` = 0-indexed で 30 個目 → 末尾から数えて 30 文字分を含む先頭位置
  - 30 文字未満なら `None` → `unwrap_or(0)` で先頭
- [ ] **C-2**: `before.char_indices().rev()` が **逆順イテレーション** であり、`nth(29)` が末尾起点で動くこと (`DoubleEndedIterator` 実装)
- [ ] **C-3**: 修正後コードで「前 30 文字以内」のセマンティクスが正しく保たれているか (修正前のバイト演算では英数字混在文字列で実質 10〜30 文字程度しか見ていなかった可能性 = ロジック上の窓は若干広がる方向)
- [ ] **C-4**: 回帰テスト `regression_multibyte_boundary_no_panic` が修正前コードで本当に panic するか (リバート → cargo test で再確認可能)
- [ ] **C-5**: 同種パターン `.len()` ベースのスライスが他に残っていないか (4.2 の表)
- [ ] **C-6**: CSV 処理パス以外の同種バグ調査は本修正のスコープ外でよいか (4.5 の別系統リスク)

### 修正前コードでの panic 再現手順 (任意)

```bash
# salary_parser.rs:149 を修正前コードに戻す
git revert 996ea6e --no-commit
cargo test salary_parser::tests::regression_multibyte_boundary_no_panic
# → panicked at 'byte index 92 is not a char boundary'
git revert --abort
```

---

## 7. デプロイ状態

- **コミット**: `996ea6e Fix multibyte boundary panic in parse_bonus_months`
- **Push**: `origin/main` 反映済 (`ebfa8bb..996ea6e`)
- **Render 本番**: 自動デプロイ対象。Render の auto-deploy が有効なら数分以内に反映。Manual Deploy が必要な状態であれば本コミット含む 5 コミット (`d4c7291`〜`996ea6e`) を一括反映する。
