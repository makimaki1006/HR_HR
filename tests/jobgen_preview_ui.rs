//! ⑥スマホ原稿の「求人ページ風 作成例」プレビューの UI 契約 (2026-08-07)
//!
//! このプレビューは実際の求人媒体のページに似せた見た目にしているため、
//! 「イメージであること」と「表示する値の出所」が崩れると誤解を招く。
//! HTML を直接編集したときに静かに壊れないよう、要となる文字列を固定する。

const JOBGEN_HTML: &str = include_str!("../static/jobgen.html");

/// 作成例が実物の求人ページと誤認されないための固定表示。
#[test]
fn preview_states_it_is_only_an_example() {
    assert!(
        JOBGEN_HTML.contains("※この作成例はあくまでイメージです。"),
        "作成例の免責文言が消えている"
    );
    assert!(
        JOBGEN_HTML.contains("写真はイメージ"),
        "写真プレースホルダがイメージである旨の表示が消えている"
    );
    // 応募ボタンは飾り。disabled が外れると押せてしまい、応募できると誤解される。
    assert!(
        JOBGEN_HTML.contains(r#"<button class="japply" type="button" disabled>"#),
        "応募ボタン風の飾りが disabled でない"
    );
}

/// プレビューに出す労働条件は①事実抽出で原文と照合できた値だけ。
/// rejected / missing の値を出すと、未検証の条件が求人ページ風の体裁で表示される。
#[test]
fn preview_shows_only_verified_facts() {
    assert!(
        JOBGEN_HTML.contains(r#"function pvFact(k){const f=(S.facts||{})[k];return (f&&f.status==="verified"&&f.value)"#),
        "作成例が verified 以外の事実も表示するようになっている"
    );
}

/// 意図ポップアップは③ペルソナ・④コピー・⑤画像案の生成結果を出すだけで、
/// 値が無いときは行ごと出さない (意図を創作しない)。
#[test]
fn intent_popup_drops_empty_rows() {
    assert!(
        JOBGEN_HTML.contains("const rs=(rows||[]).filter(r=>r&&r[1]&&String(r[1]).trim());"),
        "意図ポップアップが空の項目を捨てる処理が消えている"
    );
    assert!(
        JOBGEN_HTML.contains("if(!rs.length)return null;"),
        "表示できる意図が1件も無いときポップアップを付けない処理が消えている"
    );
    assert!(
        JOBGEN_HTML.contains("data-ipop"),
        "意図ポップアップの配線 (data-ipop) が消えている"
    );
}

/// 生成された原稿そのもの (プレーンテキスト) を読む導線とコピー導線を残す。
/// 見た目を作り込んだ結果、原稿本文が確認できなくなると工程⑥の目的を失う。
#[test]
fn plain_manuscript_stays_available() {
    assert!(
        JOBGEN_HTML.contains("原稿テキスト（プレーン）"),
        "プレーン原稿の折りたたみが消えている"
    );
    assert!(
        JOBGEN_HTML.contains("原稿テキストをコピー"),
        "原稿テキストのコピー導線が消えている"
    );
}
