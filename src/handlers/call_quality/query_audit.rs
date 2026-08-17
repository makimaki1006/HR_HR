//! クエリ引数の監査 — **解釈できなかった引数を黙って捨てない**。
//!
//! 2026-08-17 新設。検証担当が同じ罠に2回かかったため。
//!
//! 1回目: `?year_month=2026-05` を投げたが、そのエンドポイントは `from`/`to` しか
//!        見ない。**200 が返り、それらしい数値が入っていた**ので、期間が効いて
//!        いないことに気づけなかった。
//! 2回目: 商談遷移のクロス絞込に `?industry=製造業&size_band=...` と書いた。
//!        正しくは `trans_industry` / `trans_size`。**200 が返り、KPI もペアも
//!        全部それらしい数値**だった。画面に「業界: 製造業」と表示しながら
//!        全業界の数字を出している状態を、目視で見分ける方法はない。
//!
//! つまり無音ドロップは「フィルタが効かない」だけでなく、**検証を通ったという
//! 誤った確信を生む**。`continue-on-error` がステップの結論まで success に
//! 書き換えて無音失敗を検出不能にした事故と同じ構図。
//!
//! # この層がやること / やらないこと
//!
//! - やる: 受理できなかった引数名を応答 (`ignored_params`) とサーバログの両方に出す。
//! - **やらない: 400 を返すこと**。段階を置く。`deny_unknown_fields` による 400 化は
//!   フロントの呼び出しを揃えた後（今回は「無音をやめる」ところまで）。
//! - **やらない: `year_month` → `from`/`to` のような暗黙変換**。変換規則
//!   （月初/月末の当て方）が第二の定義箇所になる。受け付けない引数は
//!   `ignored_params` に出す、それだけ。
//!
//! # なぜ `#[serde(flatten)] HashMap<String,String>` を使わないか
//!
//! axum の `Query<T>` は `serde_urlencoded` を使っており、`flatten` は
//! 自己記述的なデシリアライズを要求する。そのため `Option<u32>` のような
//! 非文字列フィールドが軒並み失敗する。**コンパイルは通って実行時に 400 になる**。
//! 実際このリポジトリでは p7 を GET にして `HashMap<String, Vec<String>>` が
//! 復元できず、同じ形（コンパイル可・実行時400）の不具合を出している
//! （`p7_data_browser.rs` の「クエリの受け渡し方式を固定する」テスト参照）。
//!
//! したがって **構造体ごとに受理する引数名を明示**する。ただしそのままだと
//! 「定義が2箇所」になり片方だけ直して腐るので、`accepted_params!` マクロが
//! 明示リストと同時に**腐り検出テスト**を生成する。
//! テストは `serde_json::to_value(T::default())` のキー集合と明示リストを
//! 突き合わせるので、フィールドを足して明示リストを忘れた瞬間に落ちる。

use std::collections::BTreeSet;

use serde::Serialize;

// ================================================================ 値の監査
//
// 2026-08-17 追加。上の `ignored_params` は **キー名しか見ない**ので、
// 「キーは正しいが値が解釈できない」経路がまるごと素通りしていた。
//
// 実測（稼働中サーバ localhost:9300）:
//   `?deals_statuss=active` → ignored_params:["deals_statuss"]（塞いだ）
//   `?deals_status=NONSENSE` → ignored_params:[] で **all に落ちて全218件**
// 利用者から見た結末は同じ「絞ったつもりで全件が出る」。
// 入口を1つ塞いだだけで、隣の入口が開いていた。
//
// `?today_ym=zzzz` はさらに悪く、当月判定が丸ごと無効化されて
// **`is_partial`（集計途中の月）が立つ月が1つも無くなる**。
//
// # ここで扱うもの / 扱わないもの
//
// - 扱う: **値があるのに解釈できず、既定値へ落ちた**。
// - 扱わない: **値が無いので既定値**。これは正常であり、報告すると
//   ほぼ全リクエストが何か言い出して狼少年になる。
// - 扱わない: 値がシートの中身（業界名・担当者名・県名）と一致しないケース。
//   サーバには「打ち間違い」と「本当にデータが無い」の区別が付かない。
//   これらは 0件として素直に見えるので、別の話として扱う。

/// 解釈できなかった値1件。
///
/// **`used` を必ず載せる**のが要点。「`deals_status=NONSENSE` は読めませんでした」
/// だけでは、利用者は自分が今どの数字を見ているのか分からない。
/// 「なので `all` を使いました」まで言えば、「絞ったつもり」を自分で正せる。
#[derive(Debug, Clone, PartialEq, Eq, Serialize)]
pub struct InvalidValue {
    /// 引数名（`deals_status` 等）
    pub param: &'static str,
    /// 受け取った生の値。**そのまま返す**（伏せると利用者が自分の打ち間違いを直せない）
    pub given: String,
    /// 代わりに実際に使った値。既定値へ落とした場合はその値。
    /// 既定値へ落とさず「該当なし（0件）」として扱った場合は `None`。
    pub used: Option<String>,
    /// 受け付ける値、または書式（`all | active | continued | churned` / `YYYY-MM`）
    pub expected: String,
    /// そのまま画面に出せる日本語の一文
    pub message: String,
}

/// 1リクエストぶんの「解釈できなかった値」を集める箱。
///
/// タブ側が持つ。ルータ側では判定できない（値の意味を知っているのはタブだけ）。
/// `ignored_params` がルータ持ちなのと対になっている。
#[derive(Debug, Default)]
pub struct ValueAudit {
    items: Vec<InvalidValue>,
}

impl ValueAudit {
    pub fn new() -> Self {
        Self::default()
    }

    /// 既定値へ落としたことを記録する。
    pub fn fell_back(&mut self, param: &'static str, given: &str, used: &str, expected: &str) {
        self.items.push(InvalidValue {
            param,
            given: given.to_string(),
            used: Some(used.to_string()),
            expected: expected.to_string(),
            message: format!(
                "「{param}={given}」は解釈できないので {used} を使いました（受け付ける値: {expected}）"
            ),
        });
    }

    /// 既定値へ落とさず、そのまま「該当なし」として扱ったことを記録する。
    ///
    /// 結果は0件になるので画面上は「該当なし」と出る。**それを「本当に0件」と
    /// 読まれるのが危ない**ので、`effect` に何が起きるかを書く。
    pub fn no_match(&mut self, param: &'static str, given: &str, expected: &str, effect: &str) {
        self.items.push(InvalidValue {
            param,
            given: given.to_string(),
            used: None,
            expected: expected.to_string(),
            message: format!(
                "「{param}={given}」は解釈できません（受け付ける値: {expected}）。{effect}"
            ),
        });
    }

    /// 閉じた集合から選ぶ引数の定型。
    ///
    /// - 値が無い / 空白のみ → 既定値。**記録しない**（正常）
    /// - 値があり解釈できた   → その値。記録しない
    /// - 値があり解釈できない → 既定値 + 記録
    ///
    /// `default` が「値と表示名」を返すのは、既定値が他の引数に依存する場合が
    /// あるため（p8 の `bench_sort_dir` は `bench_sort_key` によって asc/desc が変わる）。
    pub fn choice<T>(
        &mut self,
        param: &'static str,
        raw: Option<&str>,
        expected: &str,
        parse: impl Fn(&str) -> Option<T>,
        default: impl FnOnce() -> (T, String),
    ) -> T {
        let given = match raw.map(str::trim).filter(|s| !s.is_empty()) {
            None => return default().0,
            Some(v) => v,
        };
        if let Some(v) = parse(given) {
            return v;
        }
        let (v, used) = default();
        self.fell_back(param, given, &used, expected);
        v
    }

    /// `YYYY-MM` を受ける引数の定型（`today_ym` / `current_ym`）。
    ///
    /// **これは挙動を変える**。従来は `today_ym=zzzz` がそのまま「当月」として
    /// 下流へ流れ、`ym == current_month` がどの行にも一致せず
    /// **`is_partial` が立つ月が1つも無くなっていた**（当月判定の全面無効化）。
    /// 書式が壊れている値を「当月」として扱う意味は無いので、
    /// 省略時と同じ既定（実行時の当月）へ落として、落としたことを記録する。
    pub fn year_month(
        &mut self,
        param: &'static str,
        raw: Option<&str>,
        default: impl FnOnce() -> String,
    ) -> String {
        self.choice(
            param,
            raw,
            "YYYY-MM",
            |v| {
                if is_year_month(v) {
                    Some(v.to_string())
                } else {
                    None
                }
            },
            || {
                let d = default();
                (d.clone(), d)
            },
        )
    }

    pub fn is_empty(&self) -> bool {
        self.items.is_empty()
    }

    /// 応答へ載せる形にする。**空でも空配列を返す**（キーごと消さないため）。
    pub fn into_vec(self) -> Vec<InvalidValue> {
        self.items
    }
}

/// `YYYY-MM`（またはその接頭辞を持つ `YYYY-MM-DD`）か。
///
/// 判定を各タブに散らさない。`is_partial_month` / `same_ym` が
/// 先頭7文字を比べる実装なので、それと同じ粒度で見る。
pub fn is_year_month(s: &str) -> bool {
    let b = s.as_bytes();
    if b.len() < 7 {
        return false;
    }
    if !b[..4].iter().all(u8::is_ascii_digit) || b[4] != b'-' {
        return false;
    }
    if !b[5].is_ascii_digit() || !b[6].is_ascii_digit() {
        return false;
    }
    let month = (b[5] - b'0') * 10 + (b[6] - b'0');
    if !(1..=12).contains(&month) {
        return false;
    }
    // "2026-08" ちょうど、または "2026-08-01" のような日付付きだけ通す。
    // "2026-08zz" は通さない（黙って月として使われると気づけない）。
    b.len() == 7 || b[7] == b'-'
}

/// 解釈できなかった値をサーバログにも残す。
///
/// `ignored_params` と同じ理由で**応答とログの両方に出す**。
/// `curl | jq .data` のように一部だけ見ている検証では応答だけでは気づけない。
pub fn warn_invalid_values(endpoint: &str, values: &[InvalidValue]) {
    if !values.is_empty() {
        let lines: Vec<&str> = values.iter().map(|v| v.message.as_str()).collect();
        tracing::warn!(
            "架電クオリティ{endpoint}: 解釈できない値を既定値で置き換えました: {lines:?} \
             （応答の invalid_values にも同じものを載せています）"
        );
    }
}

/// この構造体が URL クエリ / JSON ボディで受理する引数名。
///
/// 手で書かず、必ず [`crate::accepted_params!`] マクロ経由で実装すること
/// （マクロが腐り検出テストも一緒に生成する）。
pub trait AcceptedParams {
    const ACCEPTED: &'static [&'static str];
}

/// 生のクエリ文字列から、`accepted` に無いキーを拾う。
///
/// - 値は見ない。**キー名だけ**が対象（`?owners=` のような空値は「指定した」と
///   解釈されるのが既存の挙動なので、無視扱いにしない）。
/// - 同じキーが複数回現れても1回だけ返す。
/// - 並びは安定させる（`BTreeSet` 経由）。画面とテストで順序が揺れないため。
/// - `+` はスペース、`%XX` はパーセントデコードする。デコードに失敗したら
///   生の文字列のまま返す（**捨てない**。捨てたらこの機能の意味がない）。
pub fn ignored_params(accepted: &[&str], raw: Option<&str>) -> Vec<String> {
    let raw = match raw {
        Some(r) if !r.is_empty() => r,
        _ => return Vec::new(),
    };

    let mut out: BTreeSet<String> = BTreeSet::new();
    for pair in raw.split('&') {
        if pair.is_empty() {
            continue;
        }
        let key_raw = pair.split('=').next().unwrap_or("");
        if key_raw.is_empty() {
            continue;
        }
        let key = decode_key(key_raw);
        if key.is_empty() {
            continue;
        }
        if !accepted.iter().any(|a| *a == key) {
            out.insert(key);
        }
    }
    out.into_iter().collect()
}

fn decode_key(raw: &str) -> String {
    let spaced = raw.replace('+', " ");
    match urlencoding::decode(&spaced) {
        Ok(d) => d.into_owned(),
        // 不正な %XX。デコードできなくても「知らないキーが来た」事実は残す。
        Err(_) => spaced,
    }
}

/// JSON ボディ（オブジェクト）のトップレベルキーのうち、`accepted` に無いもの。
///
/// オブジェクト以外（配列・スカラ）が来た場合は空を返す。その場合は
/// どのみち本体のデシリアライズが失敗して 400 になるので、こちらで
/// 二重に文句を言う必要はない。
pub fn ignored_params_json(accepted: &[&str], body: &serde_json::Value) -> Vec<String> {
    let obj = match body.as_object() {
        Some(o) => o,
        None => return Vec::new(),
    };
    obj.keys()
        .filter(|k| !accepted.iter().any(|a| *a == k.as_str()))
        .cloned()
        .collect::<BTreeSet<String>>()
        .into_iter()
        .collect()
}

/// 無視した引数をサーバログにも残す。
///
/// 応答 JSON だけだと、`curl | jq .data` のように一部だけ見ている検証では
/// 気づけない。**両方に出す**のが要点。
pub fn warn_ignored(endpoint: &str, ignored: &[String]) {
    if !ignored.is_empty() {
        tracing::warn!(
            "架電クオリティ{endpoint}: 解釈できない引数を無視しました: {ignored:?} \
             （このエンドポイントは受け付けません。応答の ignored_params にも同じものを載せています）"
        );
    }
}

/// クエリ文字列版: 無視した引数を算出し、同時に warn ログへ出す。
///
/// 「算出したがログに出し忘れる」を防ぐため、**この1関数で両方やる**。
pub fn audit_query<T: AcceptedParams>(endpoint: &str, raw: Option<&str>) -> Vec<String> {
    let v = ignored_params(T::ACCEPTED, raw);
    warn_ignored(endpoint, &v);
    v
}

/// クエリ引数を1つも受け付けないエンドポイント用。
///
/// 「引数が無い＝何を投げても無害」ではない。`/api/call-quality/churn?year_month=…`
/// は 200 を返すが `year_month` は最初から見られていない。まさに1回目の罠なので、
/// 引数なしのエンドポイントほどこれを付ける価値がある。
pub fn audit_query_none(endpoint: &str, raw: Option<&str>) -> Vec<String> {
    let v = ignored_params(&[], raw);
    warn_ignored(endpoint, &v);
    v
}

/// JSON ボディ版。
pub fn audit_json<T: AcceptedParams>(endpoint: &str, body: &serde_json::Value) -> Vec<String> {
    let v = ignored_params_json(T::ACCEPTED, body);
    warn_ignored(endpoint, &v);
    v
}

/// 明示した受理リストが、構造体の実フィールドと一致することを確かめる。
///
/// **これが「定義2箇所の腐り」を検出する唯一の仕掛け**。
/// `serde_json::to_value(T::default())` は `#[serde(rename)]` も
/// `#[serde(flatten)]` も反映した「実際にワイヤで見えるキー」を返すので、
/// フィールドを足して `accepted_params!` を更新し忘れた瞬間にここが落ちる。
///
/// 注意: `#[serde(skip_serializing_if = ...)]` を付けた構造体には使えない
/// （既定値のときキーが消え、実在するフィールドを「無い」と誤判定する）。
/// クエリ構造体にこの属性を付けないこと。
#[cfg(test)]
pub fn assert_accepted_matches_fields<T>(type_name: &str)
where
    T: AcceptedParams + Default + serde::Serialize,
{
    let v = serde_json::to_value(T::default())
        .unwrap_or_else(|e| panic!("{type_name}: Serialize に失敗した: {e}"));
    let obj = v.as_object().unwrap_or_else(|| {
        panic!("{type_name}: クエリ構造体は JSON オブジェクトになるはず（実際: {v}）")
    });

    let actual: BTreeSet<&str> = obj.keys().map(|k| k.as_str()).collect();
    let declared: BTreeSet<&str> = T::ACCEPTED.iter().copied().collect();

    let missing: Vec<&&str> = actual.difference(&declared).collect();
    let extra: Vec<&&str> = declared.difference(&actual).collect();

    assert!(
        missing.is_empty(),
        "{type_name}: フィールドを足したのに accepted_params! に書き忘れている: {missing:?}\n\
         → このまま本番に出ると、その引数は「解釈できない引数」として ignored_params に載り、\
         実際には効いているのに「効いていない」と表示される（逆向きの嘘になる）。"
    );
    assert!(
        extra.is_empty(),
        "{type_name}: accepted_params! に、構造体に無い引数名が書いてある: {extra:?}\n\
         → フィールドを消した/リネームしたのに一覧を直していない。この引数は今後\
         黙って捨てられる（ignored_params にも載らない）。"
    );

    let mut dedup: BTreeSet<&str> = BTreeSet::new();
    for name in T::ACCEPTED {
        assert!(
            dedup.insert(name),
            "{type_name}: accepted_params! に重複した引数名がある: {name:?}"
        );
    }
}

/// 受理する引数名を宣言し、同時に**腐り検出テスト**を生成する。
///
/// ```ignore
/// crate::accepted_params!(P2Query, p2query_accepted => "from", "to", "pipeline");
/// ```
///
/// 第2引数はテスト用モジュール名（`concat_idents!` が安定化されていないので
/// 手で書く。ただし**これを書き忘れてもコンパイルが通らない**ので腐らない）。
///
/// 対象の型は `Default + Serialize` を導出していること。
/// 導出できない型（必須フィールドの enum など）は手書きの `impl Default` で良い。
/// **手書きの `Default` はフィールド追加時にコンパイルエラーになる**ので、
/// むしろ腐り検出が1段強くなる。
#[macro_export]
macro_rules! accepted_params {
    ($t:ty, $test_mod:ident => $($name:literal),* $(,)?) => {
        impl $crate::handlers::call_quality::query_audit::AcceptedParams for $t {
            const ACCEPTED: &'static [&'static str] = &[$($name),*];
        }

        #[cfg(test)]
        mod $test_mod {
            #[allow(unused_imports)]
            use super::*;

            #[test]
            fn 受理する引数名の一覧が構造体の実フィールドと一致する() {
                $crate::handlers::call_quality::query_audit::assert_accepted_matches_fields::<$t>(
                    stringify!($t),
                );
            }
        }
    };
}

#[cfg(test)]
mod tests {
    use super::*;

    const ACCEPTED: &[&str] = &["from", "to", "pipeline"];

    #[test]
    fn 知らないキーを拾う() {
        // 実際に検証担当が踏んだ1回目のケース
        let v = ignored_params(ACCEPTED, Some("year_month=2026-05"));
        assert_eq!(v, vec!["year_month".to_string()]);
    }

    #[test]
    fn 実際に検証担当が踏んだ2回目のケース() {
        let v = ignored_params(
            &["trans_industry", "trans_size"],
            Some("industry=%E8%A3%BD%E9%80%A0%E6%A5%AD&size_band=100-299"),
        );
        assert_eq!(v, vec!["industry".to_string(), "size_band".to_string()]);
    }

    #[test]
    fn 受理するキーは載らない() {
        assert!(ignored_params(ACCEPTED, Some("from=2026-01&to=2026-06")).is_empty());
    }

    #[test]
    fn クエリ無しは空() {
        assert!(ignored_params(ACCEPTED, None).is_empty());
        assert!(ignored_params(ACCEPTED, Some("")).is_empty());
    }

    #[test]
    fn 空値でも指定は指定として扱う() {
        // `?owners=` は「空文字を指定した」であって「無視した」ではない。
        assert!(ignored_params(&["owners"], Some("owners=")).is_empty());
        // 一方、知らないキーは値が空でも報告する
        assert_eq!(
            ignored_params(&["owners"], Some("owner=")),
            vec!["owner".to_string()]
        );
    }

    #[test]
    fn 同じキーが複数回来ても1回だけ() {
        assert_eq!(
            ignored_params(ACCEPTED, Some("nope=1&nope=2&nope=3")),
            vec!["nope".to_string()]
        );
    }

    #[test]
    fn 並びは安定する() {
        // HashMap の反復順をそのまま返さない（tabs/mod.rs の約束4）
        let a = ignored_params(ACCEPTED, Some("zzz=1&aaa=2&mmm=3"));
        let b = ignored_params(ACCEPTED, Some("mmm=3&zzz=1&aaa=2"));
        assert_eq!(a, b);
        assert_eq!(a, vec!["aaa".to_string(), "mmm".to_string(), "zzz".to_string()]);
    }

    #[test]
    fn パーセントデコードとプラスを解く() {
        // 日本語キー名を投げられても読める形で報告する
        assert_eq!(
            ignored_params(ACCEPTED, Some("%E6%A5%AD%E7%95%8C=1")),
            vec!["業界".to_string()]
        );
        assert_eq!(
            ignored_params(ACCEPTED, Some("two+words=1")),
            vec!["two words".to_string()]
        );
    }

    #[test]
    fn 不正なパーセントでも捨てない() {
        // デコードできないからといって黙殺したら、この機能の目的そのものを裏切る
        let v = ignored_params(ACCEPTED, Some("bad%ZZkey=1"));
        assert_eq!(v.len(), 1, "デコード失敗でも1件は報告する: {v:?}");
    }

    #[test]
    fn 値に等号が入っていてもキーだけ見る() {
        assert_eq!(
            ignored_params(ACCEPTED, Some("nope=a=b")),
            vec!["nope".to_string()]
        );
    }

    #[test]
    fn 空セグメントは無視する() {
        assert!(ignored_params(ACCEPTED, Some("&&from=1&&")).is_empty());
        assert!(ignored_params(ACCEPTED, Some("=1")).is_empty());
    }

    #[test]
    fn 引数を受け付けないエンドポイントは全部が無視対象() {
        assert_eq!(
            ignored_params(&[], Some("year_month=2026-05")),
            vec!["year_month".to_string()]
        );
    }

    #[test]
    fn json版はトップレベルキーだけ見る() {
        let body = serde_json::json!({
            "sheet": "月次明細",
            "filters": {"pipeline": ["A"]},
            "nope": 1
        });
        assert_eq!(
            ignored_params_json(&["sheet", "filters"], &body),
            vec!["nope".to_string()],
            "filters の中身（pipeline）は列名であって引数名ではない"
        );
    }

    #[test]
    fn jsonがオブジェクトでなければ空() {
        assert!(ignored_params_json(&["sheet"], &serde_json::json!([1, 2])).is_empty());
        assert!(ignored_params_json(&["sheet"], &serde_json::json!("x")).is_empty());
    }

    // ---- 値の監査（2026-08-17 追加） ----

    fn dummy_parse(v: &str) -> Option<&'static str> {
        match v {
            "active" => Some("active"),
            "churned" => Some("churned"),
            _ => None,
        }
    }

    fn choice_of(raw: Option<&str>) -> (&'static str, Vec<InvalidValue>) {
        let mut a = ValueAudit::new();
        let v = a.choice(
            "deals_status",
            raw,
            "all | active | churned",
            dummy_parse,
            || ("all", "all".to_string()),
        );
        (v, a.into_vec())
    }

    #[test]
    fn 値なしは既定値でも記録しない() {
        // **陰性対照**。「値が無いので既定値」は正常。ここで報告し始めると
        // ほぼ全リクエストが何か言い出して、誰も読まなくなる。
        let (v, inv) = choice_of(None);
        assert_eq!(v, "all");
        assert!(inv.is_empty(), "値なしは正常: {inv:?}");

        // 空文字・空白のみも「指定なし」と同じ扱い
        assert!(choice_of(Some("")).1.is_empty());
        assert!(choice_of(Some("   ")).1.is_empty());
    }

    #[test]
    fn 正しい値は記録しない() {
        // 陰性対照その2
        let (v, inv) = choice_of(Some("active"));
        assert_eq!(v, "active");
        assert!(inv.is_empty(), "正しい値では警告を出さない: {inv:?}");
        // 前後の空白は許す（画面のセレクタが空白を付けることがある）
        assert!(choice_of(Some(" churned ")).1.is_empty());
    }

    #[test]
    fn 解釈できない値は既定値と一緒に記録する() {
        let (v, inv) = choice_of(Some("NONSENSE"));
        assert_eq!(v, "all", "既定値へ落とす挙動自体は変えない（画面が落ちる）");
        assert_eq!(inv.len(), 1);
        assert_eq!(inv[0].param, "deals_status");
        assert_eq!(inv[0].given, "NONSENSE");
        assert_eq!(
            inv[0].used.as_deref(),
            Some("all"),
            "**何に落としたか**まで言わないと「絞ったつもり」を自分で正せない"
        );
        assert!(inv[0].message.contains("NONSENSE"));
        assert!(inv[0].message.contains("all"));
    }

    #[test]
    fn 既定値へ落とさない場合はusedがnull() {
        let mut a = ValueAudit::new();
        a.no_match(
            "alert_category",
            "mtg_no_folowup",
            "mtg_no_followup | contact_zero_2week | na_overdue_no_action | __all__",
            "この絞り込みは0件になります（アラートが無いという意味ではありません）",
        );
        let inv = a.into_vec();
        assert_eq!(inv[0].used, None, "既定値に落ちていないのに落ちたと言わない");
        assert!(inv[0].message.contains("0件"));
    }

    #[test]
    fn 年月の書式を判定する() {
        assert!(is_year_month("2026-08"));
        assert!(is_year_month("2026-08-17"), "日付付きも当月判定には使える");
        assert!(is_year_month("2026-01"));
        assert!(is_year_month("2026-12"));

        assert!(!is_year_month("zzzz"), "実測で当月判定を全滅させた値");
        assert!(!is_year_month("2026-13"), "13月は存在しない");
        assert!(!is_year_month("2026-00"));
        assert!(!is_year_month("2026-8"), "1桁月は先頭7文字比較で必ず外れる");
        assert!(!is_year_month("202608"));
        assert!(!is_year_month("2026-08zz"), "接尾に何か付いていたら通さない");
        assert!(!is_year_month(""));
    }

    #[test]
    fn 壊れた年月は当月へ落として記録する() {
        let mut a = ValueAudit::new();
        let ym = a.year_month("today_ym", Some("zzzz"), || "2026-08".to_string());
        assert_eq!(
            ym, "2026-08",
            "壊れた値をそのまま「当月」として流すと is_partial が全消滅する"
        );
        let inv = a.into_vec();
        assert_eq!(inv.len(), 1);
        assert_eq!(inv[0].param, "today_ym");
        assert_eq!(inv[0].used.as_deref(), Some("2026-08"));
        assert_eq!(inv[0].expected, "YYYY-MM");
    }

    #[test]
    fn 正しい年月は記録せずそのまま使う() {
        // 陰性対照
        let mut a = ValueAudit::new();
        let ym = a.year_month("today_ym", Some("2026-05"), || "2026-08".to_string());
        assert_eq!(ym, "2026-05");
        assert!(a.is_empty());

        // 省略時も静か
        let mut a = ValueAudit::new();
        assert_eq!(
            a.year_month("today_ym", None, || "2026-08".to_string()),
            "2026-08"
        );
        assert!(a.is_empty());
    }

    #[test]
    fn 複数の不正値は投げた順に並ぶ() {
        let mut a = ValueAudit::new();
        a.fell_back("bench_sort_key", "NONSENSE", "churn_rate", "name | churn_rate");
        a.fell_back("bench_sort_dir", "sideways", "asc", "asc | desc");
        let inv = a.into_vec();
        assert_eq!(inv.len(), 2);
        assert_eq!(inv[0].param, "bench_sort_key");
        assert_eq!(inv[1].param, "bench_sort_dir");
    }

    #[test]
    fn 不正値はjsonで配列になる() {
        let mut a = ValueAudit::new();
        a.fell_back("deals_status", "NONSENSE", "all", "all | active");
        let v = serde_json::to_value(a.into_vec()).unwrap();
        assert_eq!(v[0]["param"], "deals_status");
        assert_eq!(v[0]["given"], "NONSENSE");
        assert_eq!(v[0]["used"], "all");
        assert!(v[0]["message"].is_string());
    }
}
