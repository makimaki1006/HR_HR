//! 読み取りの実環境にPATCHを重ね、認可・全行競合・RAWの境界を確認する。
use super::*;
use crate::portal::sheets::SheetCell;
use serde_json::json;

fn editing() -> H {
    let h = harness();
    let mut columns = h.auth.store().columns("s1");
    for c in &mut columns {
        if c.header == "状況" || c.header == "応募者名" {
            c.writable = true;
        }
    }
    h.auth.store().replace_columns("s1", columns);
    h
}

async fn snapshot(h: &H) -> Value {
    let (s, v) = get(h, &h.a, "/api/portal/sheets/s1/rows").await;
    assert_eq!(s, StatusCode::OK);
    v["applicants"]
        .as_array()
        .unwrap()
        .iter()
        .find(|r| r["row"] == 3)
        .unwrap()
        .clone()
}

async fn patch(
    h: &H,
    who: &[String],
    sheet: &str,
    row: u32,
    revision: &str,
    column: &str,
    value: &str,
) -> (StatusCode, Value) {
    let (status, _, body) = call(
        &h.app,
        "PATCH",
        &format!("/api/portal/sheets/{sheet}/rows/{row}"),
        who,
        Some(json!({"revision":revision,"changes":[{"column":column,"value":value}]})),
        true,
    )
    .await;
    (status, body)
}

#[tokio::test]
async fn rawで指定セルだけ保存し_変更記録とキャッシュも更新する() {
    let h = editing();
    let original = snapshot(&h).await;
    let value = "=IMPORTRANGE(\"https://example.invalid\",\"A1\")";
    let (s, v) = patch(
        &h,
        &h.a,
        "s1",
        3,
        original["revision"].as_str().unwrap(),
        "状況",
        value,
    )
    .await;
    assert_eq!(s, StatusCode::OK, "{v}");
    assert_eq!(h.fake.rows("応募者情報管理")[2][3], value);
    assert_eq!(h.fake.rows("応募者情報管理")[2][4], SECRET_MEMO);
    assert_eq!(h.fake.count("batch_update"), 1);
    assert_eq!(v["applicant"]["cells"][1], value);
    assert_ne!(v["applicant"]["revision"], original["revision"]);
    let refreshed = snapshot(&h).await;
    assert_eq!(
        refreshed, v["applicant"],
        "RAW成功応答と再読み取りの署名が一致"
    );
    let logs = h.auth.store().recent_logs(100);
    let saved: Vec<_> = logs.iter().filter(|e| e.kind == "save").collect();
    assert_eq!(saved.len(), 1);
    assert_eq!(saved[0].before, "面接調整中");
    assert_eq!(saved[0].after, value);
    assert_eq!(saved[0].row, "3");
    assert_eq!(saved[0].column, "状況");
    assert_eq!(saved[0].actor, h.a_id);
    let (s, v) = patch(
        &h,
        &h.a,
        "s1",
        3,
        refreshed["revision"].as_str().unwrap(),
        "状況",
        "",
    )
    .await;
    assert_eq!(s, StatusCode::OK, "{v}");
    assert_eq!(h.fake.rows("応募者情報管理")[2][3], "");
}

#[tokio::test]
async fn 秘密列の変更も競合し_秘密は応答に出さない() {
    let h = editing();
    let original = snapshot(&h).await;
    h.fake.set_cell("応募者情報管理", 2, 4, "別の社内専用値");
    let (s, v) = patch(
        &h,
        &h.a,
        "s1",
        3,
        original["revision"].as_str().unwrap(),
        "状況",
        "採用",
    )
    .await;
    assert_eq!(s, StatusCode::CONFLICT);
    assert_eq!(v["error"], "conflict");
    assert!(v["latest"]["revision"].is_string());
    assert!(!v.to_string().contains("別の社内専用値"));
    assert_eq!(h.fake.count("batch_update"), 0);
}

#[tokio::test]
async fn 表示値が同じでも数式変更_見出し変更_設定変更は競合する() {
    for change in 0..3 {
        let h = editing();
        let original = snapshot(&h).await;
        match change {
            0 => {
                h.fake.metadata.lock().unwrap().insert(
                    ("応募者情報管理".into(), 2, 4),
                    SheetCell {
                        text: SECRET_MEMO.into(),
                        entered: json!({"formulaValue":"=A1"}),
                        validation: None,
                    },
                );
            }
            1 => {
                h.fake.set_cell("応募者情報管理", 0, 0, "応募日の変更");
            }
            _ => {
                let mut cols = h.auth.store().columns("s1");
                cols[0].label = "別の表示名".into();
                h.auth.store().replace_columns("s1", cols);
            }
        }
        let (s, v) = patch(
            &h,
            &h.a,
            "s1",
            3,
            original["revision"].as_str().unwrap(),
            "状況",
            "採用",
        )
        .await;
        assert_eq!(s, StatusCode::CONFLICT, "{change}: {v}");
        assert_eq!(h.fake.count("batch_update"), 0);
    }
}

#[tokio::test]
async fn 書けない列_秘密列_記入例_見えない行には保存しない() {
    let h = editing();
    let original = snapshot(&h).await;
    for column in ["電話番号", SECRET_HEADER, "知らない見出し"] {
        let (s, v) = patch(
            &h,
            &h.a,
            "s1",
            3,
            original["revision"].as_str().unwrap(),
            column,
            "変更",
        )
        .await;
        assert_eq!(s, StatusCode::UNPROCESSABLE_ENTITY, "{v}");
    }
    assert_eq!(
        patch(
            &h,
            &h.a,
            "s1",
            2,
            original["revision"].as_str().unwrap(),
            "状況",
            "変更"
        )
        .await
        .0,
        StatusCode::UNPROCESSABLE_ENTITY
    );
    assert_eq!(h.fake.count("batch_update"), 0);
}

#[tokio::test]
async fn 別顧客_未ログイン_停止_csrfを拒否し_googleを呼ばない() {
    let h = editing();
    let original = snapshot(&h).await;
    let rev = original["revision"].as_str().unwrap();
    for sheet in ["s2", "s-store", "missing"] {
        let calls = h.fake.count("batch_get");
        assert_eq!(
            patch(&h, &h.a, sheet, 3, rev, "状況", "変更").await.0,
            StatusCode::NOT_FOUND
        );
        assert_eq!(h.fake.count("batch_get"), calls);
    }
    assert_eq!(
        patch(&h, &[], "s1", 3, rev, "状況", "変更").await.0,
        StatusCode::UNAUTHORIZED
    );
    let (s, _, _) = call(
        &h.app,
        "PATCH",
        "/api/portal/sheets/s1/rows/3",
        &h.a,
        Some(json!({"revision":rev,"changes":[]})),
        false,
    )
    .await;
    assert_eq!(s, StatusCode::FORBIDDEN);
    h.auth.disable(&h.a_id, Utc::now()).unwrap();
    assert_eq!(
        patch(&h, &h.a, "s1", 3, rev, "状況", "変更").await.0,
        StatusCode::UNAUTHORIZED
    );
    assert_eq!(h.fake.count("batch_update"), 0);
}

#[tokio::test]
async fn 同じ署名の同時保存は一つだけ成功する() {
    let h = editing();
    let original = snapshot(&h).await;
    let rev = original["revision"].as_str().unwrap();
    *h.fake.read_delay.lock().unwrap() = Some(Duration::from_millis(10));
    let (a, b) = tokio::join!(
        patch(&h, &h.a, "s1", 3, rev, "状況", "採用"),
        patch(&h, &h.a, "s1", 3, rev, "状況", "不採用")
    );
    let statuses = [a.0, b.0];
    assert!(statuses.contains(&StatusCode::OK));
    assert!(statuses.contains(&StatusCode::CONFLICT));
    assert_eq!(h.fake.count("batch_update"), 1);
}

#[tokio::test]
async fn 数式セルと未対応入力規則は編集不可_正本の選択肢を検査する() {
    for kind in ["formula", "unsupported", "list"] {
        let h = editing();
        let mut cell = SheetCell::literal("面接調整中".into());
        match kind {
            "formula" => cell.entered = json!({"formulaValue":"=A1"}),
            "unsupported" => {
                cell.validation = Some(
                    json!({"condition":{"type":"ONE_OF_RANGE","values":[{"userEnteredValue":"別範囲!A:A"}]}}),
                )
            }
            _ => {
                cell.validation = Some(
                    json!({"condition":{"type":"ONE_OF_LIST","values":[{"userEnteredValue":"採用"},{"userEnteredValue":"不採用"}]}}),
                )
            }
        }
        h.fake
            .metadata
            .lock()
            .unwrap()
            .insert(("応募者情報管理".into(), 2, 3), cell);
        let original = snapshot(&h).await;
        let (_, v) = get(&h, &h.a, "/api/portal/sheets/s1/rows").await;
        assert_eq!(v["columns"][1]["writable"], kind == "list");
        let (s, v) = patch(
            &h,
            &h.a,
            "s1",
            3,
            original["revision"].as_str().unwrap(),
            "状況",
            "不正な選択",
        )
        .await;
        assert_eq!(s, StatusCode::UNPROCESSABLE_ENTITY, "{kind}: {v}");
        assert_eq!(h.fake.count("batch_update"), 0);
        if kind == "list" {
            assert_eq!(
                patch(
                    &h,
                    &h.a,
                    "s1",
                    3,
                    original["revision"].as_str().unwrap(),
                    "状況",
                    "採用"
                )
                .await
                .0,
                StatusCode::OK
            );
        }
    }
}

#[tokio::test]
async fn 保存失敗は成功記録を作らず再読み取りできる() {
    let h = editing();
    let original = snapshot(&h).await;
    *h.fake.fail_writes.lock().unwrap() = Some(GoogleError::Network);
    let (s, v) = patch(
        &h,
        &h.a,
        "s1",
        3,
        original["revision"].as_str().unwrap(),
        "状況",
        "採用",
    )
    .await;
    assert_eq!(s, StatusCode::SERVICE_UNAVAILABLE);
    assert_eq!(v["error"], "save_failed");
    assert_eq!(h.fake.rows("応募者情報管理")[2][3], "面接調整中");
    let logs = h.auth.store().recent_logs(100);
    assert_eq!(logs.iter().filter(|e| e.kind == "save").count(), 0);
    assert_eq!(logs.iter().filter(|e| e.kind == "save_failed").count(), 1);
    assert_eq!(snapshot(&h).await["cells"][1], "面接調整中");
}

#[tokio::test]
async fn 同値の保存は_google書込と記録を増やさない() {
    let h = editing();
    let original = snapshot(&h).await;
    assert_eq!(
        patch(
            &h,
            &h.a,
            "s1",
            3,
            original["revision"].as_str().unwrap(),
            "状況",
            "面接調整中"
        )
        .await
        .0,
        StatusCode::OK
    );
    assert_eq!(h.fake.count("batch_update"), 0);
    assert_eq!(
        h.auth
            .store()
            .recent_logs(100)
            .iter()
            .filter(|e| e.kind == "save")
            .count(),
        0
    );
}

#[tokio::test]
async fn 読み取り待機中の登録移動と列変更は古い秘密を返さず拒否する() {
    let h = editing();
    let original = snapshot(&h).await;
    *h.fake.read_delay.lock().unwrap() = Some(Duration::from_millis(30));
    let change_settings = async {
        tokio::time::sleep(Duration::from_millis(5)).await;
        let mut reg = h.auth.store().sheet("s1").unwrap();
        reg.header_row = 2;
        h.auth.store().update_sheet(reg).unwrap();
        let mut cols = h.auth.store().columns("s1");
        cols.iter_mut()
            .find(|c| c.header == SECRET_HEADER)
            .unwrap()
            .visible = true;
        h.auth.store().replace_columns("s1", cols);
    };
    let (response, _) = tokio::join!(
        patch(
            &h,
            &h.a,
            "s1",
            3,
            original["revision"].as_str().unwrap(),
            "状況",
            "採用"
        ),
        change_settings
    );
    assert_eq!(response.0, StatusCode::CONFLICT);
    assert!(response.1.get("latest").is_none());
    assert!(!response.1.to_string().contains(SECRET_MEMO));
    assert_eq!(h.fake.count("batch_update"), 0);
}

#[tokio::test]
async fn 書込可否だけを変えても版が変わり_旧署名は再読込を要求する() {
    let h = editing();
    let original = snapshot(&h).await;
    let (_, old) = get(&h, &h.a, "/api/portal/sheets/s1/version").await;
    let mut cols = h.auth.store().columns("s1");
    cols.iter_mut()
        .find(|c| c.header == "状況")
        .unwrap()
        .writable = false;
    h.auth.store().replace_columns("s1", cols);
    let (_, new) = get(&h, &h.a, "/api/portal/sheets/s1/version").await;
    assert_ne!(old["version"], new["version"]);
    let (s, v) = patch(
        &h,
        &h.a,
        "s1",
        3,
        original["revision"].as_str().unwrap(),
        "状況",
        "採用",
    )
    .await;
    assert_eq!(s, StatusCode::CONFLICT);
    assert!(v.get("latest").is_none());
}

#[tokio::test]
async fn 不正な変更リストと別アカウントの署名は保存しない() {
    let h = editing();
    let original = snapshot(&h).await;
    let (_, cookie) = sign_in(&h.auth, "c@example.com", "c1");
    assert_eq!(
        patch(
            &h,
            &[cookie],
            "s1",
            3,
            original["revision"].as_str().unwrap(),
            "状況",
            "採用"
        )
        .await
        .0,
        StatusCode::CONFLICT
    );
    for body in [
        json!({"revision":original["revision"],"changes":[]}),
        json!({"revision":original["revision"],"changes":[{"column":"状況","value":"採用"},{"column":"状況","value":"不採用"}]}),
    ] {
        assert_eq!(
            call(
                &h.app,
                "PATCH",
                "/api/portal/sheets/s1/rows/3",
                &h.a,
                Some(body),
                true
            )
            .await
            .0,
            StatusCode::UNPROCESSABLE_ENTITY
        );
    }
    assert_eq!(h.fake.count("batch_update"), 0);
}
