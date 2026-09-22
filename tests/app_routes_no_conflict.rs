//! `build_app()` を実際に呼んで、ルートの重複で起動できなくなることを防ぐ
//!
//! 2026-09-21 追加。
//!
//! ------------------------------------------------------------------
//! なぜ要るか
//! ------------------------------------------------------------------
//! **`cargo test` が全部通っても、サーバが起動しないことがある。**
//! axum は同じパスを2度登録すると `Router::merge` / `Router::route` の時点で
//! panic するが、それは `build_app()` を通らないと起きない。
//! 各ハンドラのユニットテストは自分の `router()` しか見ないので、
//! **別のモジュールと同じパスを取り合っていても気づけない**。
//!
//! 実害の記録: マージで同じルートが2行残り、本番が 21時間出なかった。
//!
//! ここでは本物の `build_app()` をそのまま呼ぶ。panic すればテストが落ちる。
//!
//! ------------------------------------------------------------------
//! AppState について
//! ------------------------------------------------------------------
//! DB も Turso も監査も `Option` なので、**全部 `None` で組める**。
//! ルートの重複はデータに関係なく `Router` の組み立てだけで決まるので、
//! 重い依存を用意する必要はない。

use std::sync::Arc;

use rust_dashboard::auth::session::RateLimiter;
use rust_dashboard::config::AppConfig;
use rust_dashboard::db::cache::AppCache;
use rust_dashboard::{build_app, AppState};

fn bare_state() -> Arc<AppState> {
    let config = AppConfig::from_env();
    let cache = AppCache::new(config.cache_ttl_secs, config.cache_max_entries);
    let rate_limiter = RateLimiter::new(
        config.rate_limit_max_attempts,
        config.rate_limit_lockout_secs,
    );
    Arc::new(AppState {
        config,
        hw_db: None,
        indeed_db: None,
        turso_db: None,
        salesnow_db: None,
        scout_db: None,
        cache,
        rate_limiter,
        company_geo_cache: None,
        audit: None,
    })
}

/// 同じパスを2つのルータが登録していたら、ここで panic して落ちる。
#[test]
fn build_appがルートの重複で落ちない() {
    let _router = build_app(bare_state());
}

/// コンサルダッシュボードのパスが、実際に `build_app()` の中に入っていること。
///
/// 重複していないことと、**そもそも配線されていること**は別。
/// `.merge()` を書き忘れても上のテストは通ってしまう。
///
/// 🔴 `protected_routes` は最後に `route_layer(auth_middleware)` を当てているので、
/// 未ログインだと 303（/login へのリダイレクト）になる。
/// **404 でなければ配線されている**、という見方をする。
#[tokio::test]
async fn コンサルダッシュボードのパスが配線されている() {
    use axum::body::Body;
    use axum::http::{Request, StatusCode};
    use tower::ServiceExt;

    let app = build_app(bare_state());
    for path in [
        "/consulting",
        "/api/consulting/focus",
        "/api/consulting/renewal",
        "/api/consulting/customer",
        "/api/consulting/headquarters",
        "/api/consulting/mtg-quality",
        "/api/consulting/phone",
        "/api/consulting/rampup",
        "/api/consulting/outcome",
        "/api/consulting/data-quality",
    ] {
        let res = app
            .clone()
            .oneshot(
                Request::builder()
                    .uri(path)
                    .body(Body::empty())
                    .expect("リクエストを組めない"),
            )
            .await
            .expect("ルータが応答しない");
        assert_ne!(
            res.status(),
            StatusCode::NOT_FOUND,
            "{path} が 404。build_app() に配線されていない\
（src/lib.rs の protected_routes に .merge() を足したか確認）"
        );
    }
}

/// 架電クオリティと営業KPI のパスも生きていること。
///
/// コンサルを足したせいで既存の画面が消えていないか（同じパスを取り合って
/// 片方が負けていないか）を見る。
#[tokio::test]
async fn 既存の画面のパスが消えていない() {
    use axum::body::Body;
    use axum::http::{Request, StatusCode};
    use tower::ServiceExt;

    let app = build_app(bare_state());
    for path in [
        "/call-quality",
        "/api/call-quality/tabs",
        "/api/call-quality/overview",
        "/sales-kpi",
        "/api/sales-kpi/data",
    ] {
        let res = app
            .clone()
            .oneshot(
                Request::builder()
                    .uri(path)
                    .body(Body::empty())
                    .expect("リクエストを組めない"),
            )
            .await
            .expect("ルータが応答しない");
        assert_ne!(
            res.status(),
            StatusCode::NOT_FOUND,
            "{path} が 404。コンサルを足したときに既存の画面を消していないか確認"
        );
    }
}

/// 🔴 コンサルダッシュボードが**認証の内側**に入っていること。
///
/// `protected_routes` に `.merge()` するのを間違えて外側に置くと、
/// **顧客ごとの契約金額が誰でも見られる**ことになる。
/// 未ログインで 303（`/login` へのリダイレクト）になることで確かめる。
/// 200 が返ったら、認証の外に出ている。
#[tokio::test]
async fn コンサルダッシュボードは認証の内側にある() {
    use axum::body::Body;
    use axum::http::{Request, StatusCode};
    use tower::ServiceExt;

    let app = build_app(bare_state());
    for path in [
        "/consulting",
        "/api/consulting/focus",
        "/api/consulting/customer",
    ] {
        let res = app
            .clone()
            .oneshot(
                Request::builder()
                    .uri(path)
                    .body(Body::empty())
                    .expect("リクエストを組めない"),
            )
            .await
            .expect("ルータが応答しない");
        assert_eq!(
            res.status(),
            StatusCode::SEE_OTHER,
            "{path} が {} を返した。未ログインなら 303 で /login へ飛ぶはず。200 なら認証の外に出ている（protected_routes の .route_layer より前に merge しているか確認）",
            res.status()
        );
    }
}

// ================================================================ 画面から辿れるか

/// 🔴 **ルートが生きていることと、画面から辿れることは別。**
///
/// 2026-09-21 の実害: `/consulting` は 303 を返していた（＝配線済み）のに、
/// **既存レイアウトにリンクが無かった**ので、画面を見ている人には
/// 存在しないのと同じだった。URL を直接叩けば出る、では使われない。
///
/// ここではテンプレートの中身を見る。レンダリングを通さないのは、
/// このナビが `{{JOBGEN_TAB}}` のような差し込みを含んでいて、
/// 組み立てに `AppState` の中身が要るため。**リンクが書かれているか**だけを見る。
#[test]
fn 既存の画面からコンサルへ行ける() {
    let nav = include_str!("../templates/dashboard_inline.html");
    assert!(
        nav.contains(r#"href="/consulting""#),
        "templates/dashboard_inline.html に /consulting へのリンクが無い。\
ルートが生きていても、画面から辿れなければ存在しないのと同じ"
    );
    // 先例が消えていないことも一緒に見る（並びごと消える事故を捕まえる）
    assert!(
        nav.contains(r#"href="/sales-kpi""#),
        "templates/dashboard_inline.html から /sales-kpi のリンクが消えている"
    );

    // 🔴 ラベルも固定する（2026-09-21 ユーザー指定「営業KPIの隣にコンサルKPI」）。
    //    リンクだけ見ていると、文言が英語や内部IDに変わっても気づけない。
    assert!(
        nav.contains("コンサルKPI"),
        "ナビのラベルが「コンサルKPI」でない。営業KPI の隣に並べる文言として指定されている"
    );
    assert!(
        nav.contains("営業KPI"),
        "ナビから「営業KPI」のラベルが消えている"
    );
}

/// 行きっぱなしで戻れないと使えない。
#[test]
fn コンサルから既存の画面へ戻れる() {
    let page = include_str!("../templates/tabs/cs_dashboard.html");
    assert!(
        page.contains(r#"href="/""#),
        "templates/tabs/cs_dashboard.html に戻り導線が無い。\
/sales-kpi と同じく「← ダッシュボードへ戻る」を置くこと"
    );
}

/// 🔴 画面の JS が壊れていると、**どのテストにも引っかからずに無言で死ぬ**。
///
/// 2026-09-21 の実害: テンプレートの JS に構文エラーが1つあり、`<script>` 全体が
/// 実行されなかった。API は1本も呼ばれず画面は空。それでも
///   - `cargo test` は 3,393 件すべて通る（Rust 側は無関係）
///   - `curl` で `/api/consulting/*` は 200（サーバは正常）
///   - `/consulting` も 200（HTML は出ている）
/// だったので、どの確認にも掛からなかった。
///
/// 原因は JS の文字列に `\\"` と書いたこと。JS では `\` が
/// バックスラッシュ1つになるので、次の `"` で文字列が終わってしまう。
///
/// ここでは**その1パターンだけ**を見る。構文そのものの検査は Rust からは
/// できないので、`tests/consulting_page_js.js`（Node）が担当する。
/// こちらは cargo test で毎回走る安い見張り。
#[test]
fn 画面のjsに文字列を壊すエスケープが無い() {
    for (name, src) in [(
        "templates/tabs/cs_dashboard.html",
        include_str!("../templates/tabs/cs_dashboard.html"),
    )] {
        let bad: Vec<(usize, &str)> = src
            .lines()
            .enumerate()
            .filter(|(_, l)| l.contains("\\\\\""))
            .map(|(i, l)| (i + 1, l.trim()))
            .collect();
        assert!(
            bad.is_empty(),
            "{name} の JS に `\\\\\"` がある。JS では文字列がそこで終わってしまい、\
<script> 全体が動かなくなる。HTML 属性を書きたいなら外側をシングルクォートにすること。\n{:?}",
            bad
        );
    }
}

/// 🔴 この画面は毎朝見るもの。**「このデータはいつのものか」が出ていること。**
///
/// `meta.today` は計算に使った基準日で、シートを作り直した日時とは**別物**。
/// シートは手で作り直しているので、基準日だけ今日になっていて中身は何日も前、
/// ということが起きる。画面が作成日時を出さなくなったらここで落ちる。
#[test]
fn 画面にデータの作成日時が出る() {
    let html = std::fs::read_to_string("templates/tabs/cs_dashboard.html")
        .expect("templates/tabs/cs_dashboard.html が読めない");
    for needle in [
        "generated_at",                         // シートを作り直した時刻
        "source_as_of",                         // 🔴 元データを落とした時刻。作成時刻とは別物
        "source_age_days",                      // 何日前のデータか
        "cs-fresh",                             // 出す場所
        "setFresh",                             // 出す処理
        "このデータがいつのものか分かりません", // 取れなかったときに嘘をつかない
    ] {
        assert!(html.contains(needle), "画面から「{needle}」が消えている");
    }
}

/// ②の担当者名から③へ辿れること。
#[test]
fn コンサルタント一覧から案件の立ち位置へ辿れる() {
    let html = std::fs::read_to_string("templates/tabs/cs_dashboard.html").expect("テンプレート");
    assert!(html.contains("drillToBoard"), "②→③の関数が無い");
    assert!(html.contains("a.drill"), "担当者名にリンクが張られていない");
    assert!(
        html.contains("boardFilter = { consultant: name"),
        "③へ飛んだときに担当で絞られていない"
    );
}

/// 🔴 **絞ったら「◯件中 N件を表示」を必ず出す。**
/// 絞ったことを忘れて「全件がこう見える」と読み違えるのを防ぐため。
#[test]
fn 案件の立ち位置は絞り込みの件数を出す() {
    let html = std::fs::read_to_string("templates/tabs/cs_dashboard.html").expect("テンプレート");
    for needle in [
        "bf-consultant", // 担当
        "bf-flag",       // 名札
        "bf-expiry",     // 満了までの期間
        "bf-q",          // 案件名の部分一致
        "bf-clear",      // 外す
        "board-count",
        " 件中 ",
        "を表示",
    ] {
        assert!(
            html.contains(needle),
            "③の絞り込みから「{needle}」が消えている"
        );
    }
}

// ================================================================ 見た目の決まりごと

/// 画面の `<style>` から `--<名前>: #rrggbb` を拾う。
/// `which` は 0=明るい地、1=暗い地（media query）。
fn tokens(html: &str, which: usize) -> std::collections::HashMap<String, String> {
    let css = html
        .split_once("<style>")
        .and_then(|(_, r)| r.split_once("</style>"))
        .map(|(c, _)| c)
        .expect("style が無い");
    // `:root{` で始まるブロックを順に拾う
    let mut blocks = Vec::new();
    let mut rest = css;
    while let Some(i) = rest.find(":root") {
        let after = &rest[i..];
        if let Some(s) = after.find('{') {
            let mut depth = 0i32;
            let mut end = 0usize;
            for (j, ch) in after[s..].char_indices() {
                if ch == '{' {
                    depth += 1;
                } else if ch == '}' {
                    depth -= 1;
                    if depth == 0 {
                        end = s + j;
                        break;
                    }
                }
            }
            blocks.push(&after[s..end]);
            rest = &after[end.max(s + 1)..];
        } else {
            break;
        }
    }
    let block = blocks.get(which).unwrap_or_else(|| {
        panic!(
            "{which} 番目の :root ブロックが無い（{} 個しか無い）",
            blocks.len()
        )
    });
    let mut out = std::collections::HashMap::new();
    let mut it = block.split("--");
    it.next();
    for part in it {
        if let Some((name, tail)) = part.split_once(':') {
            let v: String = tail
                .trim_start()
                .chars()
                .take_while(|c| c.is_ascii_hexdigit() || *c == '#')
                .collect();
            if v.len() == 7 && v.starts_with('#') {
                out.insert(name.trim().to_string(), v);
            }
        }
    }
    out
}

/// sRGB の相対輝度（WCAG の定義そのまま）。
fn luminance(hex: &str) -> f64 {
    let h = hex.trim_start_matches('#');
    let ch = |i: usize| {
        let v = u8::from_str_radix(&h[i..i + 2], 16).expect("16進") as f64 / 255.0;
        if v <= 0.04045 {
            v / 12.92
        } else {
            ((v + 0.055) / 1.055).powf(2.4)
        }
    };
    0.2126 * ch(0) + 0.7152 * ch(2) + 0.0722 * ch(4)
}

fn contrast(a: &str, b: &str) -> f64 {
    let (x, y) = (luminance(a), luminance(b));
    (x.max(y) + 0.05) / (x.min(y) + 0.05)
}

/// 🔴 **文字のコントラストは 4.5:1 以上。**
///
/// 2026-09-22 に実測したら `--ink-3` が paper の上で **2.86:1**、
/// `--ghost` が **1.98:1** しかなかった。どちらも読ませる文字に使っている
/// （節番号・図の軸ラベル・「値が無い(—)」「記録なし」）。
/// 色を薄くし直したときにここで落ちる。
#[test]
fn 文字のコントラストが足りている() {
    let html = std::fs::read_to_string("templates/tabs/cs_dashboard.html").expect("テンプレート");
    // 文字に使うトークン × 背景に使うトークン
    let text = [
        "ink", "ink-2", "ink-3", "ghost", "ai", "hi", "ki", "midori", "murasaki",
    ];
    let bg = ["paper", "panel", "panel-2", "panel-3"];
    for (which, theme) in [(0usize, "明るい地"), (1usize, "暗い地")] {
        let t = tokens(&html, which);
        for f in text {
            let fg = t.get(f).unwrap_or_else(|| panic!("{theme}: --{f} が無い"));
            for b in bg {
                let Some(back) = t.get(b) else { continue };
                let r = contrast(fg, back);
                assert!(
                    r >= 4.5,
                    "{theme}: --{f}({fg}) を --{b}({back}) の上に置くと {r:.2}:1。\
                     文字は 4.5:1 が要る。薄くするなら、その色を文字に使っていないことを先に確かめること"
                );
            }
        }
    }
}

/// 押せる部品の枠は 3:1（WCAG 1.4.11）。
///
/// 表の罫線（`--rule`）まで濃くすると画面が重くなるので、
/// **部品の枠だけ** `--rule-strong` を使う。その値がここで守られる。
#[test]
fn 操作できる部品の枠が見える() {
    let html = std::fs::read_to_string("templates/tabs/cs_dashboard.html").expect("テンプレート");
    for (which, theme) in [(0usize, "明るい地"), (1usize, "暗い地")] {
        let t = tokens(&html, which);
        let strong = t
            .get("rule-strong")
            .unwrap_or_else(|| panic!("{theme}: --rule-strong が無い"));
        for b in ["paper", "panel", "panel-2"] {
            let Some(back) = t.get(b) else { continue };
            let r = contrast(strong, back);
            assert!(
                r >= 3.0,
                "{theme}: --rule-strong を --{b} の上に置くと {r:.2}:1（3:1 が要る）"
            );
        }
    }
    assert!(
        html.contains("border:1px solid var(--rule-strong)"),
        "--rule-strong を定義しただけで、部品に当てていない"
    );
}

/// 🔴 **並び替えはキーボードでもできること。**
///
/// `th` の `onclick` だけにしていたせいで、2026-09-22 の実測では
/// **Tab でたどり着けず、Enter でも並び替わらなかった**（tabIndex -1）。
/// `th` の中に `button` を置く形に直した。`th` の onclick に戻すとここで落ちる。
#[test]
fn 並び替えがキーボードでできる() {
    let html = std::fs::read_to_string("templates/tabs/cs_dashboard.html").expect("テンプレート");
    assert!(
        html.contains("<button type=\"button\" class=\"sort\""),
        "見出しが button になっていない"
    );
    assert!(
        html.contains("th.sortable button.sort"),
        "button に配線していない（th 側に onclick を付け直していないか）"
    );
    assert!(
        html.contains("th button.sort:focus-visible"),
        "見出しのフォーカスリングが無い"
    );
    assert!(
        html.contains("aria-sort="),
        "いま並んでいる列を読み上げに伝えていない"
    );
    // 読み上げ用の説明（見た目には出ない）
    assert!(
        html.contains("押すとこの列で並び替わります"),
        "並び替えできることが読み上げに伝わらない"
    );
}

/// 押せるところが 24px 未満にならないこと（WCAG 2.2 AA / Target Size Minimum）。
///
/// 担当者名から③へ飛ぶリンクが実測 42.1 x **13px** しかなかった。
#[test]
fn 押せるところが小さすぎない() {
    let html = std::fs::read_to_string("templates/tabs/cs_dashboard.html").expect("テンプレート");
    for needle in [
        "a.drill{ display:inline-block; min-height:24px",
        "min-height:28px", // 操作列の select / input / button
        "a.drill:focus-visible",
    ] {
        assert!(
            html.contains(needle),
            "当たり判定の指定「{needle}」が消えている"
        );
    }
}

/// 🔴 **色だけで意味を伝えない。**
///
/// 名札の図は3つの意味を色で分けている。棒の右に分類の言葉も書くので、
/// 色が見分けられなくても、白黒に印刷しても読める。
#[test]
fn 図は色だけで意味を伝えない() {
    let html = std::fs::read_to_string("templates/tabs/cs_dashboard.html").expect("テンプレート");
    assert!(
        html.contains("function flagGroup("),
        "名札の分類が1か所にまとまっていない"
    );
    assert!(
        html.matches("note: flagGroup(").count() >= 2,
        "分類の言葉を棒に書いていない（①と③の両方に要る）"
    );
    assert!(
        html.contains("色が見分けられなくても"),
        "色に頼っていないことが画面に書かれていない"
    );
}

/// いちばん小さい文字が 11px を下回らないこと。
#[test]
fn 小さすぎる文字が無い() {
    let html = std::fs::read_to_string("templates/tabs/cs_dashboard.html").expect("テンプレート");
    let css = html
        .split_once("<style>")
        .and_then(|(_, r)| r.split_once("</style>"))
        .map(|(c, _)| c)
        .expect("style");
    let mut small = Vec::new();
    for part in css.split("font-size:").skip(1) {
        let v: String = part
            .chars()
            .take_while(|c| c.is_ascii_digit() || *c == '.')
            .collect();
        if let Ok(px) = v.parse::<f64>() {
            if part[v.len()..].starts_with("px") && px < 11.0 {
                small.push(px);
            }
        }
    }
    assert!(
        small.is_empty(),
        "11px より小さい文字がある: {small:?}。読ませる情報なら 11px 以上にすること"
    );
}

/// 表以外がページ全体を横に流さないこと。
///
/// ③は14列あり、`.scroll` に入れていなかったせいで
/// 1440px 幅のページが 2640px に広がっていた（2026-09-22 実測）。
#[test]
fn 広い表は枠の中でスクロールする() {
    let html = std::fs::read_to_string("templates/tabs/cs_dashboard.html").expect("テンプレート");
    for needle in [
        "scroll(boardTable(shown, boardSort,",          // ③
        "scroll(boardTable(D.rows, { key: \"n_flags\"", // ①
    ] {
        assert!(
            html.contains(needle),
            "表が .scroll に入っていない: {needle}"
        );
    }
    // 画面の中に px 直書きの幅を持ち込んでいないこと（max-width は可）
    assert!(
        !html.contains("style=\"width:180px\""),
        "固定 px 幅がインラインで残っている"
    );
}

/// 🔴 **画面に出す文章に絵文字を混ぜない。**
///
/// コードの注記に 🔴 を使うのはこの案件の決まりごとだが、
/// それが JSON の文字列や画面の文言に紛れ込むと、実際に赤丸が表示される
/// （2026-09-22 のスクリーンショットで「スコアや確率は出していません」の前に出ていた）。
/// 強調は `<b>` と「読み方」の見出しが担うので、印は要らない。
#[test]
fn 画面に出る文章に絵文字が無い() {
    // 拾うのはコメントでない行だけ。`//` `///` `/*` `*` で始まる行は注記なので見逃す。
    fn displayed(src: &str) -> Vec<(usize, String)> {
        let mut out = Vec::new();
        let mut in_block = false;
        for (i, raw) in src.lines().enumerate() {
            let t = raw.trim_start();
            if t.starts_with("/*") {
                in_block = true;
            }
            let skip =
                in_block || t.starts_with("//") || t.starts_with('*') || t.starts_with("<!--");
            if t.contains("*/") {
                in_block = false;
            }
            if skip {
                continue;
            }
            if raw.chars().any(is_emoji) {
                out.push((i + 1, raw.trim().chars().take(70).collect()));
            }
        }
        out
    }
    fn is_emoji(c: char) -> bool {
        matches!(c as u32,
            0x1F300..=0x1FAFF | 0x2600..=0x27BF | 0x2B00..=0x2BFF | 0xFE0F | 0x1F000..=0x1F0FF)
        // ▲▼◯— など、この画面が意味を持たせて使う記号は絵文字ではない（別の範囲）
    }

    for path in [
        "src/handlers/cs_dashboard/routes.rs",
        "templates/tabs/cs_dashboard.html",
    ] {
        let src = std::fs::read_to_string(path).unwrap_or_else(|e| panic!("{path}: {e}"));
        let hits = displayed(&src);
        assert!(
            hits.is_empty(),
            "{path} の画面に出る文字列に絵文字がある:\n{}",
            hits.iter()
                .map(|(n, t)| format!("  {n}行: {t}"))
                .collect::<Vec<_>>()
                .join("\n")
        );
    }
}
