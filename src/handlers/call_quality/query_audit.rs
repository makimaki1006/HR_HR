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
}
