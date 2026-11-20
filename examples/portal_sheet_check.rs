//! ダミー専用の実Google接続確認。通常は読み取りだけ。
//! PORTAL_CHECK_KEY_FILE に既存のテスト鍵のパスを指定する（鍵の値は出力しない）。
//! --write-test のときだけ、空であることを確認したQ990に文字列を保存して読み戻し、元の空欄へ戻す。
use base64::{engine::general_purpose::STANDARD, Engine};
use rust_dashboard::portal::sheets::{a1, GoogleSheetsApi};

const SPREADSHEET: &str = "1m5DRb117MXqCQw4GhJC21spHE6N3lqE0dtqP7zxwLeg";
const TAB: &str = "応募者情報管理";
const MARKER: &str = "=変更記録の動作確認（アプリ保存テスト）";

#[tokio::main]
async fn main() -> Result<(), String> {
    let file = std::env::var("PORTAL_CHECK_KEY_FILE").map_err(|_| "テスト鍵のパスがありません")?;
    let key = std::fs::read(file).map_err(|_| "テスト鍵を読めません")?;
    std::env::set_var("PORTAL_CHECK_KEY_B64", STANDARD.encode(key));
    let api =
        GoogleSheetsApi::from_env_var("PORTAL_CHECK_KEY_B64").ok_or("テスト鍵を読み込めません")?;
    std::env::remove_var("PORTAL_CHECK_KEY_B64");
    let ranges = [a1(TAB, "A1:AD1"), a1(TAB, "A990:AD990")];
    let got = api
        .cells(SPREADSHEET, &ranges)
        .await
        .map_err(|e| format!("読込に失敗: {e:?}"))?;
    if got.len() != 2
        || got[0]
            .rows
            .first()
            .and_then(|r| r.first())
            .map(|c| c.text.as_str())
            != Some("応募発生日")
    {
        return Err("ダミーシートの見出しを確認できません".into());
    }
    let row = got[1].rows.first().cloned().unwrap_or_default();
    println!("ダミーの見出し・数式・入力規則の読込: 確認済み（値は出力しません）");
    if !std::env::args().any(|a| a == "--write-test") {
        return Ok(());
    }
    if row.iter().any(|c| !c.text.is_empty() || c.formula()) {
        return Err("テスト行に値があるため、書き込まず終了しました".into());
    }
    let target = a1(TAB, "Q990");
    let updates = [(target.clone(), vec![vec![MARKER.to_string()]])];
    let write = api.batch_update(SPREADSHEET, &updates).await;
    // 書込結果が不明な場合も読み直す。別の値になっていれば消さない。
    let read = api
        .cells(SPREADSHEET, &[target.clone()])
        .await
        .map_err(|e| format!("保存後の確認ができません。ダミーのQ990を確認してください: {e:?}"))?;
    let cell = read
        .first()
        .and_then(|g| g.rows.first())
        .and_then(|r| r.first());
    if cell.map(|c| c.text.as_str()) != Some(MARKER) {
        return Err(format!(
            "保存を確認できず、上書きせず終了しました（通信結果: {:?}）",
            write.err()
        ));
    }
    let literal = cell.is_some_and(|c| {
        !c.formula()
            && c.entered
                .get("stringValue")
                .and_then(serde_json::Value::as_str)
                == Some(MARKER)
    });
    api.batch_update(SPREADSHEET, &[(target.clone(), vec![vec![String::new()]])])
        .await
        .map_err(|e| {
            format!("テスト値の消去を確認できません。ダミーのQ990を確認してください: {e:?}")
        })?;
    let restored = api
        .cells(SPREADSHEET, &[target])
        .await
        .map_err(|e| format!("消去後の確認に失敗: {e:?}"))?;
    if restored
        .iter()
        .flat_map(|g| &g.rows)
        .flatten()
        .any(|c| !c.text.is_empty() || c.formula())
    {
        return Err("テスト値の消去を確認できません。ダミーのQ990を確認してください".into());
    }
    if !literal {
        return Err("RAWの文字列としての保存を確認できません（テスト値は消去済み）".into());
    }
    println!("RAWの保存・読戻し・テスト値の消去: 確認済み");
    Ok(())
}
