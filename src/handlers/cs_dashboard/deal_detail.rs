//! ①案件 →「案件の詳細」
//!
//! 2026-09-24 藤巻さんの要望:「個別の案件のページに、どのような MTG をして、どのような接触をしたのかを
//! まとめたい。電話の内容の解説（MiniMax M3 の要約）も出したい。時系列で並んで管理できるとなおよい」。
//!
//! 取引1件について、MTG（録画）・MTG（メール由来の推定）・電話（と要約）・担当の交代を、
//! **新しい順に1本の時系列**で返す。画面から Zoom・MiniMax は叩かない
//! （要約は日次更新が `CS_通話要約` に書いたものを読むだけ）。
//!
//! ------------------------------------------------------------------
//! 決めたこと（画面にも同じことを書いている）
//! ------------------------------------------------------------------
//! - 🔴 **事実と推定を分ける。** 録画（`CS_MTG`）と通話記録（`CS_通話明細`）は事実、
//!   メール由来の MTG 実施日（`CS_MTG実施日_メール由来`）は推定（±1日で83.3%）。
//!   行ごとに `fact` と `source_label` を付け、同じ顔で並べない。
//!   メール由来は `kind` が「実施」の行だけ出す（予定・候補・取り下げは実施ではない。件数は `counts` で返す）。
//! - 🔴 **接触の付け直し**（`contact_trend::AttachIndex`、「担当者ごとの接触」と同じ決まり）。
//!   通話は、その日に動いていた契約ではなく**あとから作られた継続先の取引**に付いていることが多い。
//!   そこで、同じ拠点の別の取引（継続先・前の契約・オプション契約）に付いた電話・録画 MTG・メール由来の MTG のうち、付け直すとこの取引に来るもの
//!   （その日にこの取引の契約期間の中で、同じ拠点に動いている本体案件がこの取引だけ）も並べ、
//!   `attach.moved_from` に元の取引を書く。
//!   逆に、この取引に付いているが契約期間の外のものも**消さずに**並べ、付け直し先（`attach.moved_to`）か、
//!   決められない（`attach.state`）ことを書く。
//!   決まりは接触（60秒超の通話と MTG）を数えるためのものだが、ここでは 60秒以下の通話にも同じ決まりで印を付ける
//!   （どの契約の期間の出来事かを示すだけで、数え方は変えていない）。
//!   `attach.moved_from.relation` は元の取引との関係（`later` 継続先 / `earlier` 前の契約 /
//!   `option` オプション契約 / `unknown`）。元は継続先とは限らないので、画面の文はこれで選ぶ。
//! - メール由来の MTG も同じ決まりで付け直す（2026-09-26 検証の指摘。付け直さないと前の契約の MTG が
//!   この契約の出来事に見え、前の契約の詳細には出なかった）。同じ日の行は1回だけ出す。
//! - 録画 MTG の `確度`（取引への結び付けの確かさ）を `link_certainty` で返す。録画は事実だが、
//!   この取引の録画だという点は推定なので、「高」以外は画面で印を付ける。
//! - 1つの通話が複数の取引に付いているとき（`CS_通話明細` は多対多）は `call_id` で1回だけ出す。
//!   この取引に直接付いている行を優先し、次に本体契約の行（オプション契約の行より先）。
//! - 電話の要約は `call_id` で結ぶ。同じ通話に取引ごとの行があるときは、元の取引の行を優先する
//!   （中身は通話ごとなので、どの行でも同じはず）。要約の行が無い通話は `summary: null`（画面は「要約なし」）。
//!   シートが読めないときは `meta.summary_sheet = "missing"`（画面は「電話の要約はまだありません」）。
//! - MTG の抽出項目（やること・顧客の懸念・前向きシグナル・リスク判定）が空なのは「まだ抽出していない」。
//!   記録が無いのとは違う（`extracted: false`、画面は「未抽出」）。
//! - 担当の交代（`CS_担当交代`）は、この取引に付いた行だけ（付け直さない）。
//! - 名札は「案件そのもの」と同じもの（`deal_rows`）。稼働中の取引にしか付けていないので、
//!   稼働中でなければ `flags: null`。
//! - 取引の指定が無いときは、案件名・拠点名の部分一致で探す（本体案件だけ・最大 `SEARCH_LIMIT` 件）。
//! - オプション契約は画面の母集団の外なので、詳細は出さない（`meta.reason`）。
//! - 母集団（`population`）と鮮度は `freshen()` が載せる。

use std::collections::{HashMap, HashSet};

use chrono::NaiveDate;
use serde_json::{json, Value};

use super::contact_trend::{main_spans, AttachIndex, Target};
use super::{
    call_date_jst, consultant_of, date10, deals_all_of, deals_of, flag_true, opt_num, Deal, Sheets,
    CONTACT_SEC,
};
use crate::handlers::call_quality::sheets::SheetData;

/// 探す欄で返す件数の上限。
pub const SEARCH_LIMIT: usize = 50;

/// 並べる順の中で、同じ日時のときの種類の順。
fn kind_rank(k: &str) -> u8 {
    match k {
        "handover" => 0,
        "mtg" => 1,
        "mail_mtg" => 2,
        _ => 3,
    }
}

/// 通話の `ts`（UTC）を日本時間の `HH:MM` にする。タイムゾーンが無ければ `None`（推測で時差を足さない）。
fn call_time_jst(ts: &str) -> Option<String> {
    chrono::DateTime::parse_from_rfc3339(ts.trim())
        .ok()
        .map(|t| {
            t.with_timezone(&chrono::FixedOffset::east_opt(9 * 3600).expect("JST"))
                .format("%H:%M")
                .to_string()
        })
}

/// `yyyy-MM-dd HH:MM…` の時刻の部分。読めなければ `None`。
fn hhmm(s: &str) -> Option<String> {
    let t = s.trim().get(11..16)?;
    chrono::NaiveTime::parse_from_str(t, "%H:%M")
        .ok()
        .map(|_| t.to_string())
}

/// 空文字は null にする（0 や空と「値が無い」を混ぜない）。
fn text(s: &str) -> Value {
    let t = s.trim();
    if t.is_empty() {
        Value::Null
    } else {
        Value::String(t.to_string())
    }
}

/// 付け直して来た元の取引が、見ている取引から見て何か。
/// `option`（同じ拠点のオプション契約。詳細の画面は無い）/ `later`（あとに始まった本体契約。継続先）/
/// `earlier`（前に始まった本体契約）/ `unknown`（開始日が読めない）。
fn relation(all: &HashMap<&str, &Deal>, from: &str, me: &str) -> &'static str {
    let (Some(f), Some(m)) = (all.get(from), all.get(me)) else {
        return "unknown";
    };
    if f.is_option() {
        return "option";
    }
    match (
        date10(&f.contract_start_date),
        date10(&m.contract_start_date),
    ) {
        (Some(a), Some(b)) if a > b => "later",
        (Some(a), Some(b)) if a < b => "earlier",
        _ => "unknown",
    }
}

/// 付け先の説明。`id` はこの行が付いている取引、`me` は詳細を見ている取引。
fn attach_json(
    ix: &AttachIndex,
    by_id: &HashMap<&str, &Deal>,
    id: &str,
    me: &str,
    d: NaiveDate,
) -> Value {
    let name = |x: &str| by_id.get(x).map(|x| x.name.as_str()).unwrap_or("");
    if id != me {
        // 別の取引から付け直して来たもの（呼ぶ側が Moved(me) を確かめてある）。
        // 🔴 元は継続先とは限らない（同じ拠点のオプション契約から来るものも多い）。関係を返し、文は画面が選ぶ
        return json!({
            "state": "moved_in",
            "in_span": true,
            "moved_from": {"deal_id": id, "name": name(id), "relation": relation(by_id, id, me)},
            "moved_to": null,
        });
    }
    match ix.target(id, d) {
        Target::Own => {
            json!({"state": "own", "in_span": true, "moved_from": null, "moved_to": null})
        }
        Target::Moved(to) => json!({
            "state": "moved_out", "in_span": false, "moved_from": null,
            "moved_to": {"deal_id": to, "name": name(to)},
        }),
        Target::Ambiguous => json!({
            "state": "ambiguous", "in_span": false, "moved_from": null, "moved_to": null,
        }),
        Target::Dropped => json!({
            "state": "outside", "in_span": false, "moved_from": null, "moved_to": null,
        }),
    }
}

/// 要約の索引。`call_id` → `(取引ID, 行)` の並び（シートの順）。
fn summary_index(sh: &SheetData) -> HashMap<&str, Vec<(&str, usize)>> {
    let mut out: HashMap<&str, Vec<(&str, usize)>> = HashMap::new();
    for (i, r) in sh.rows.iter().enumerate() {
        let cid = sh.get(r, "call_id").trim();
        if cid.is_empty() {
            continue;
        }
        out.entry(cid)
            .or_default()
            .push((sh.get(r, "deal_id").trim(), i));
    }
    out
}

fn summary_json(sh: &SheetData, row: usize) -> Value {
    let r = &sh.rows[row];
    let g = |c: &str| sh.get(r, c);
    json!({
        "summary": text(g("summary")),
        "next_action": text(g("next_action")),
        "concern": text(g("concern")),
        "n_utterances": opt_num(g("n_utterances")).map(|v| v as i64),
        "model": text(g("model")),
        "generated_at": text(g("generated_at")),
    })
}

/// 付け先の数え上げ。`moved_in` と、この取引に付いているが契約期間の外（`moved_out`・`ambiguous`・`outside`）。
fn count_attach(a: &Value, moved_in: &mut usize, outside: &mut usize) {
    match a["state"].as_str() {
        Some("moved_in") => *moved_in += 1,
        Some("own") | None => {}
        Some(_) => *outside += 1,
    }
}

/// 電話の「話した人」（Zoom の表示名）を画面用にそろえる。
/// - 社名・部署の接頭辞（`リクロジ＿氏名`・`リクロジ_氏名`）は `＿`/`_` より後ろだけ残す。
///   `リクロジ事業部　氏名` は「事業部」までを取る
/// - 空白（半角・全角）は取る（`星川 輝羅` と `松野日向子` の書き方をそろえる）
///
/// 姓と名の順が逆の表示名（名が先）は直せない（どちらが姓か決められない。推測で並べ替えない）。
pub fn handler_label(raw: &str) -> String {
    let t = raw.trim();
    let t = t
        .rsplit(['＿', '_'])
        .next()
        .map(str::trim)
        .filter(|x| !x.is_empty())
        .unwrap_or(t);
    // 部署名の接頭辞（`リクロジ事業部　氏名`）。空白の前が「事業部」で終わるときだけ取る
    let t = match t.split_once(char::is_whitespace) {
        Some((head, rest)) if head.ends_with("事業部") && !rest.trim().is_empty() => rest,
        _ => t,
    };
    t.chars().filter(|c| !c.is_whitespace()).collect()
}

/// 取引の指定が無いときの「探す」。本体案件だけ。案件名・拠点名の部分一致（大文字小文字を問わない）。
fn search(deals: &[Deal], who: &HashMap<String, (String, bool)>, q: &str) -> Value {
    // 画面に返すのは打った言葉のまま。比べるときだけ小文字にそろえる
    let shown = q.trim();
    let q = shown.to_lowercase();
    if q.is_empty() {
        return json!({"q": "", "n_match": 0, "limit": SEARCH_LIMIT, "rows": []});
    }
    let mut hit: Vec<&Deal> = deals
        .iter()
        .filter(|d| d.name.to_lowercase().contains(&q) || d.kyoten_name.to_lowercase().contains(&q))
        .collect();
    // 稼働中を先に、同じなら開始日の新しい順（同じなら ID）
    hit.sort_by(|a, b| {
        b.is_active
            .cmp(&a.is_active)
            .then_with(|| b.contract_start_date.cmp(&a.contract_start_date))
            .then_with(|| a.id.cmp(&b.id))
    });
    let n = hit.len();
    let rows: Vec<Value> = hit
        .iter()
        .take(SEARCH_LIMIT)
        .map(|d| {
            json!({
                "deal_id": d.id, "name": d.name, "site": d.site_name(),
                "stage": d.stage_label, "start": d.contract_start_date,
                "expiration": d.contract_expiration_date, "is_active": d.is_active,
                "renewal_no": d.renewal_no,
                "consultant": who.get(&d.id).map(|x| x.0.clone()),
            })
        })
        .collect();
    json!({"q": shown, "n_match": n, "limit": SEARCH_LIMIT, "rows": rows})
}

/// 案件の詳細。`summary` は `CS_通話要約`（読めなければ `None`）。
pub fn build_deal_detail(
    sheets: &Sheets,
    summary: Option<&SheetData>,
    deal_id: Option<&str>,
    q: Option<&str>,
    today: NaiveDate,
) -> Value {
    let all = deals_all_of(&sheets.deal);
    let deals = deals_of(&sheets.deal);
    let who = consultant_of(&sheets.owner_hist);
    let summary_state = match summary {
        None => "missing",
        Some(s) if s.col("call_id").is_none() || s.rows.is_empty() => "empty",
        Some(_) => "ok",
    };
    let mut meta = json!({
        "today": today.to_string(),
        "all_cached": sheets.all_cached,
        "found": false,
        "deal_id": deal_id,
        "reason": null,
        // ok / empty（シートはあるが行が無い）/ missing（シートが読めない）
        "summary_sheet": summary_state,
        "summary_rows": summary.map(|s| s.rows.len()).unwrap_or(0),
        "contact_sec": CONTACT_SEC,
    });

    let Some(id) = deal_id.map(str::trim).filter(|x| !x.is_empty()) else {
        return json!({"meta": meta, "search": search(&deals, &who, q.unwrap_or(""))});
    };
    let Some(d) = all.iter().find(|x| x.id == id) else {
        meta["reason"] = json!("この取引は見つかりません（取引が消えたか、IDが違います）");
        return json!({"meta": meta});
    };
    if d.is_option() {
        meta["reason"] = json!(
            "オプション契約（求人追加・AirWork広告運用・一次対応・エントリーフォーム・追加）は、\
この画面の母集団の外なので詳細を出していません。本体の契約から開いてください"
        );
        return json!({"meta": meta});
    }
    meta["found"] = json!(true);

    let by_id: HashMap<&str, &Deal> = all.iter().map(|x| (x.id.as_str(), x)).collect();
    let (spans, _) = main_spans(&deals);
    let ix = AttachIndex::new(&all, &spans);
    let my_site = ix.site_of(id);
    // 同じ拠点の取引（オプションも含む。付け直して来る元になりうる）
    let same_site: HashSet<&str> = match my_site {
        Some(k) => all
            .iter()
            .filter(|x| x.id != id && x.kyoten_key.trim() == k)
            .map(|x| x.id.as_str())
            .collect(),
        None => HashSet::new(),
    };
    let is_opt = |x: &str| by_id.get(x).is_some_and(|d| d.is_option());
    // この行を並べるか。並べるなら付け先の説明を返す
    let pick = |row_deal: &str, day: NaiveDate| -> Option<Value> {
        if row_deal == id {
            return Some(attach_json(&ix, &by_id, row_deal, id, day));
        }
        if same_site.contains(row_deal) && ix.target(row_deal, day) == Target::Moved(id) {
            return Some(attach_json(&ix, &by_id, row_deal, id, day));
        }
        None
    };

    let mut events: Vec<(NaiveDate, String, u8, String, Value)> = Vec::new();

    // ---- MTG（録画・事実）----
    let mtg = &sheets.mtg;
    let mut n_mtg = 0usize;
    let mut n_mtg_extracted = 0usize;
    let (mut n_mtg_moved_in, mut n_mtg_outside, mut n_mtg_link_not_high) = (0usize, 0usize, 0usize);
    let mut rec_days: HashSet<NaiveDate> = HashSet::new();
    for (i, r) in mtg.rows.iter().enumerate() {
        let g = |c: &str| mtg.get(r, c);
        let Some(day) = date10(g("開催日")) else {
            continue;
        };
        let Some(attach) = pick(g("deal_id").trim(), day) else {
            continue;
        };
        // 🔴 抽出前は空。「記録が無い」ではない
        let extracted = !g("抽出").trim().is_empty()
            || ["やること", "顧客の懸念", "前向きシグナル", "リスク判定"]
                .iter()
                .any(|c| !g(c).trim().is_empty());
        n_mtg += 1;
        if extracted {
            n_mtg_extracted += 1;
        }
        count_attach(&attach, &mut n_mtg_moved_in, &mut n_mtg_outside);
        // 🔴 録画があったことは事実だが、どの取引の録画かは推定（CS_MTG の「確度」高・中）。
        //    確度が「高」以外は数えて、画面で印を付ける
        let link_certainty = g("確度").trim();
        if link_certainty != "高" {
            n_mtg_link_not_high += 1;
        }
        rec_days.insert(day);
        let time = hhmm(g("開催日時(JST)"));
        events.push((
            day,
            time.clone().unwrap_or_default(),
            kind_rank("mtg"),
            format!("m{i:06}"),
            json!({
                "kind": "mtg", "date": day.to_string(), "time": time,
                "fact": true, "source_label": "Zoom 録画（事実）",
                "subject": text(g("件名")), "host": text(g("ホスト氏名")),
                "minutes": opt_num(g("所要分")),
                "mtg_type": text(g("MTG種別")),
                "extracted": extracted,
                "todo": text(g("やること")), "concern": text(g("顧客の懸念")),
                "positive": text(g("前向きシグナル")), "risk": text(g("リスク判定")),
                "risk_reason": text(g("リスク理由")), "next": text(g("次回予定")),
                // 取引への結び付けの確かさ（録画そのものではなく、この取引の録画だという点）
                "link_certainty": text(link_certainty), "link_reason": text(g("紐づけの理由")),
                "attach": attach,
            }),
        ));
    }

    // ---- MTG（メール由来・推定）----
    // 🔴 電話・録画と同じ決まりで付け直す（2026-09-26 検証の指摘: 付け直さずに並べると、
    //    前の契約の MTG がこの契約の出来事に見え、前の契約の詳細には出なかった）。
    //    同じ拠点の別の取引に付いた行も、この契約の期間の日なら並べる。
    //    同じ日の行が複数の取引に付いているときは1回だけ（この取引の行を優先、次に本体契約）。
    let mail = &sheets.mail_mtg;
    let (mut n_mail, mut n_mail_moved_in, mut n_mail_outside) = (0usize, 0usize, 0usize);
    let mut mail_other: HashMap<String, usize> = HashMap::new();
    let mut mail_order: Vec<usize> = Vec::new();
    let mut mail_later: Vec<(bool, usize)> = Vec::new();
    for (i, r) in mail.rows.iter().enumerate() {
        let did = mail.get(r, "deal_id").trim();
        if did == id {
            let kind = mail.get(r, "kind").trim();
            if kind != "実施" {
                // 実施ではないもの（予定・候補・取り下げ）は並べず、この取引の分だけ数を返す
                *mail_other.entry(kind.to_string()).or_insert(0) += 1;
                continue;
            }
            mail_order.push(i);
        } else if same_site.contains(did) && mail.get(r, "kind").trim() == "実施" {
            mail_later.push((is_opt(did), i));
        }
    }
    mail_later.sort(); // 本体契約（false）を先に。同じなら行の順
    mail_order.extend(mail_later.into_iter().map(|(_, i)| i));
    let mut mail_days: HashSet<NaiveDate> = HashSet::new();
    for i in mail_order {
        let r = &mail.rows[i];
        let Some(day) = date10(mail.get(r, "date")) else {
            continue;
        };
        let Some(attach) = pick(mail.get(r, "deal_id").trim(), day) else {
            continue;
        };
        if !mail_days.insert(day) {
            continue; // 同じ日のメール由来の MTG を2回出さない
        }
        n_mail += 1;
        count_attach(&attach, &mut n_mail_moved_in, &mut n_mail_outside);
        let cert = mail.get(r, "certainty").trim();
        events.push((
            day,
            String::new(),
            kind_rank("mail_mtg"),
            format!("e{i:06}"),
            json!({
                "kind": "mail_mtg", "date": day.to_string(), "time": null,
                "fact": false,
                "source_label": "メール由来（推定）",
                "certainty": if cert.is_empty() { "推定".to_string() } else { cert.to_string() },
                // 同じ日に録画もある（同じ MTG を2回数えている見込みが高い）
                "same_day_recording": rec_days.contains(&day),
                "attach": attach,
            }),
        ));
    }

    // ---- 電話（事実）と要約 ----
    let call = &sheets.call;
    let sidx = summary.map(summary_index).unwrap_or_default();
    // 🔴 この取引に直接付いた行を先に見る（同じ通話が付け直しでも来るときは直接の行を採る）
    let mut order: Vec<usize> = Vec::new();
    let mut later: Vec<(bool, usize)> = Vec::new();
    for (i, r) in call.rows.iter().enumerate() {
        let did = call.get(r, "deal_id").trim();
        if did == id {
            order.push(i);
        } else if same_site.contains(did) {
            later.push((is_opt(did), i));
        }
    }
    // 🔴 同じ通話が本体契約とオプション契約の両方に付いているときは、本体契約の行を元にする
    //    （シートの行の順で元が変わらないように。オプション契約には詳細の画面が無い）
    later.sort();
    order.extend(later.into_iter().map(|(_, i)| i));
    let mut seen: HashSet<&str> = HashSet::new();
    let (mut n_call, mut n_contact, mut n_sum, mut n_tr, mut n_moved_in, mut n_outside) =
        (0usize, 0usize, 0usize, 0usize, 0usize, 0usize);
    for i in order {
        let r = &call.rows[i];
        let g = |c: &str| call.get(r, c);
        let did = g("deal_id").trim();
        let ts = g("ts");
        let Some(day) = call_date_jst(ts) else {
            continue;
        };
        let Some(attach) = pick(did, day) else {
            continue;
        };
        let cid = g("call_id").trim();
        if !cid.is_empty() && !seen.insert(cid) {
            continue; // 同じ通話を2回出さない
        }
        let secs = opt_num(g("duration_sec"));
        let contact = secs.is_some_and(|s| s > CONTACT_SEC);
        let transcript = flag_true(g("has_transcript"));
        let sm = sidx.get(cid).and_then(|v| {
            v.iter()
                .find(|(sd, _)| *sd == did)
                .or_else(|| v.first())
                .map(|&(_, row)| row)
        });
        let sj = match (summary, sm) {
            (Some(sh), Some(row)) => summary_json(sh, row),
            _ => Value::Null,
        };
        n_call += 1;
        if contact {
            n_contact += 1;
        }
        if transcript {
            n_tr += 1;
        }
        if !sj.is_null() {
            n_sum += 1;
        }
        count_attach(&attach, &mut n_moved_in, &mut n_outside);
        let time = call_time_jst(ts);
        events.push((
            day,
            time.clone().unwrap_or_default(),
            kind_rank("call"),
            format!("c{i:06}"),
            json!({
                "kind": "call", "date": day.to_string(), "time": time,
                "fact": true, "source_label": "通話記録（事実）",
                "call_id": cid, "duration_sec": secs, "contact": contact,
                "direction": text(g("direction")),
                // 話した人（Zoom）。無ければ HubSpot の担当
                // 🔴 Zoom の表示名は書き方がそろっていない（社名の接頭辞・空白の有無）。
                //    画面に出す名前は `handler_label` でそろえ、元の値は `handler` に残す
                "handler": text(g("handler")), "handler_label": text(&handler_label(g("handler"))),
                "owner": text(g("owner")),
                "has_transcript": transcript,
                "summary": sj,
                "attach": attach,
            }),
        ));
    }

    // ---- 担当の交代。この取引の行だけ ----
    let hv = &sheets.handover;
    let mut n_ho = 0usize;
    for (i, r) in hv.rows.iter().enumerate() {
        if hv.get(r, "deal_id").trim() != id {
            continue;
        }
        let Some(day) = date10(hv.get(r, "date")) else {
            continue;
        };
        n_ho += 1;
        events.push((
            day,
            String::new(),
            kind_rank("handover"),
            format!("h{i:06}"),
            json!({
                "kind": "handover", "date": day.to_string(), "time": null,
                "fact": true, "source_label": "担当の交代（MTG のホストが替わった日）",
                "from": text(hv.get(r, "from")), "to": text(hv.get(r, "to")),
                "to_retired": flag_true(hv.get(r, "to_retired")),
                "reflected": text(hv.get(r, "reflected")),
                "record_gap_days": opt_num(hv.get(r, "record_gap_days")).map(|v| v as i64),
            }),
        ));
    }

    // 🔴 新しい順。同じ日は時刻の新しい順（時刻の無いものはその日の後ろ）、同じなら種類・行の順で決める
    events.sort_by(|a, b| {
        b.0.cmp(&a.0)
            .then_with(|| b.1.cmp(&a.1))
            .then_with(|| a.2.cmp(&b.2))
            .then_with(|| a.3.cmp(&b.3))
    });

    // ---- 契約の連なり（同じ拠点の本体契約）----
    let mut chain: Vec<&Deal> = match my_site {
        Some(k) => deals.iter().filter(|x| x.kyoten_key.trim() == k).collect(),
        None => deals.iter().filter(|x| x.id == id).collect(),
    };
    chain.sort_by(|a, b| {
        a.contract_start_date
            .cmp(&b.contract_start_date)
            .then_with(|| a.id.cmp(&b.id))
    });
    let pos = chain.iter().position(|x| x.id == id);

    // ---- 名札（案件そのものと同じ。稼働中だけ）----
    let flags = if d.is_active {
        let (rows, _) = super::routes::deal_rows(sheets, today);
        rows.into_iter()
            .find(|r| r["deal_id"] == id)
            .map(|r| r["flags"].clone())
    } else {
        None
    };

    let (owner, retired) = who.get(id).cloned().unwrap_or_default();
    let mut mail_other: Vec<(String, usize)> = mail_other.into_iter().collect();
    mail_other.sort();

    json!({
        "meta": meta,
        "deal": {
            "deal_id": d.id, "name": d.name, "site": d.site_name(),
            "stage": d.stage_label, "kind": d.contract_kind,
            "start": d.contract_start_date, "expiration": d.contract_expiration_date,
            "period": d.contract_period,
            "consultant": if owner.is_empty() { None } else { Some(owner) },
            "consultant_retired": retired,
            "amount": d.amount, "renewal_no": d.renewal_no,
            "is_active": d.is_active, "right_censored": d.right_censored,
            "flags": flags,
        },
        "chain": {
            "has_site": my_site.is_some(),
            "position": pos,
            "prev": pos.and_then(|p| p.checked_sub(1)).map(|p| chain[p].id.clone()),
            "next": pos.and_then(|p| chain.get(p + 1)).map(|x| x.id.clone()),
            "rows": chain.iter().map(|x| json!({
                "deal_id": x.id, "name": x.name, "stage": x.stage_label,
                "start": x.contract_start_date, "expiration": x.contract_expiration_date,
                "renewal_no": x.renewal_no, "amount": x.amount, "is_active": x.is_active,
                "current": x.id == id,
            })).collect::<Vec<_>>(),
        },
        "counts": {
            "mtg": n_mtg, "mtg_extracted": n_mtg_extracted,
            // 取引への結び付けの確度が「高」ではない録画 MTG
            "mtg_link_not_high": n_mtg_link_not_high,
            "mtg_moved_in": n_mtg_moved_in, "mtg_outside": n_mtg_outside,
            "mail_mtg": n_mail,
            "mail_moved_in": n_mail_moved_in, "mail_outside": n_mail_outside,
            // メール由来のうち、実施ではないので並べていないもの（予定・候補・取り下げ）
            "mail_not_held": mail_other.iter().map(|(k, n)| json!({"kind": k, "n": n})).collect::<Vec<_>>(),
            "call": n_call, "call_contact": n_contact, "call_transcript": n_tr,
            "call_summarized": n_sum,
            "handover": n_ho,
            // 別の取引から付け直して並べた電話 / この取引に付いているが契約期間の外の電話
            "call_moved_in": n_moved_in, "call_outside": n_outside,
        },
        "events": events.into_iter().map(|e| e.4).collect::<Vec<_>>(),
        "rules": {
            "fact": "録画の MTG と通話記録は事実です。メール由来の MTG は、メールの文面から実施日を起こした推定です\
    （録画と突き合わせると ±1日で83.3% が一致）。同じ日に録画があるメール由来の行は、同じ MTG の見込みが高いです",
            "attach": "電話や MTG は、その日に動いていた契約ではなく、あとから作られた継続の取引や、\
    同じ拠点のオプション契約に付いていることがよくあります。\
    そこで、同じ拠点の別の取引に付いた電話・録画 MTG・メール由来の MTG のうち、その日にこの取引の契約期間の中にあるもの\
    （同じ拠点で動いている本体案件がこの取引だけの日）も並べ、どこから付け直したかを書いています\
    （「担当者ごとの接触」と同じ決まり）。この取引に付いていても契約期間の外のものは、消さずに印を付けています",
            "contact": "接触 ＝ MTG または60秒超の通話（メールは数えない）。通話の日時は日本時間です。\
    60秒以下の通話は、どの取引でも接触には数えません（どの契約の期間の日かの印だけ付けています）",
            "link": "録画の MTG は、録画があったこと自体は事実ですが、どの取引の録画かは録画の情報から結び付けた推定です\
    （確度 高・中）。確度が中のものには印を付けています",
            "handler": "電話の「話した人」は Zoom の表示名です。社名の接頭辞と空白を取ってそろえていますが、\
    名が先に書かれた表示名はそのままなので、担当欄（HubSpot）と書き方が違うことがあります",
            "summary": "電話の要約は、Zoom Phone の文字起こしを MiniMax-M3 で要約したものです（日次更新で作成）。\
    文字起こしが取れない・短すぎる通話には要約がありません。要約は話された内容の要約で、評価ではありません",
            "extracted": "MTG の抽出項目（やること・顧客の懸念・前向きシグナル・リスク判定）が空なのは、\
    まだ抽出していないためです。記録が無いのとは違います",
            "handover": "担当の交代は、MTG のホストが替わった日です（HubSpot の担当欄が書き換わった日ではありません）",
        },
    })
}
