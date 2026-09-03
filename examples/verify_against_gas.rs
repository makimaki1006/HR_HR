//! GAS版 ⇔ Rust版 数値突合ハーネス（Rust 側）
//!
//! 2026-08-16。移植の成否を「見た目が似ている」ではなく **同じ数字が出るか** で
//! 決めるための道具。ここは **Rust 側の実測値を出す**担当で、期待値は作らない。
//!
//! # 全体の流れ
//!
//! ```text
//!   data\call_quality_monitor\*.csv   ← 唯一の入力（Sheets と同じ中身。認証不要）
//!        ├─ この example        →  実測値 _parity_rust_actual.json
//!        └─ _verify_rust_parity.py →  期待値 _parity_gas_expected.json
//!                                      （javascript.html の式を Python に書き写したもの）
//!                    ↓
//!            path 単位で突き合わせ → 一致 / 意図した差異 / 要調査の不一致 / 未突合
//! ```
//!
//! **期待値ファイルの各項目が「意図した差異かどうか」の判定は Python 側が持つ**
//! （`diff` フィールド）。ここはそれを読むだけで、分類規則を二重に持たない。
//! 規則が2箇所にあると必ず片方だけ更新されてズレる。
//!
//! # 実行
//!
//! ```text
//!   set CARGO_TARGET_DIR=C:/Users/fuji1/AppData/Local/hrhr_target
//!   cargo run --release --example verify_against_gas -- <csvディレクトリ>
//! ```
//!
//! 期待値ファイルが無ければ実測値の書き出しだけ行い、その旨を表示して終わる。
//!
//! # ここで呼ぶのは「本番と同じ実装」だけ
//!
//! 突合の意味を保つため、**集計式をこのファイルに書き写さない**。
//! `src/` の公開 API をそのまま呼ぶ:
//!   - `tabs::p1_members::collect()`  … アポ率 / 分母切替 / 足切り / NA遵守率
//!   - `heatmap::aggregate()`         … 時間帯×曜日ヒートのセル値
//!   - `tabs::rate()`                 … 分母0の扱い
//!   - `sheets::query()`              … 汎用の絞り込み・集計
//!
//! 単純な足し算（月合計など）はハーネス側で行う。これは「式」ではないため、
//! 突合の独立性を損なわない。該当箇所には `harness-sum` と注記する。
//!
//! # 突合できない項目（黙って飛ばさず JSON の `unverifiable` に出す）
//!
//! `p0_overview` / `p2_habits` / `pja` / `p12` / `p13` は集計関数が private で、
//! 公開されているのは `handle()` だけ。`handle()` は `SheetsClient`(実 HTTP + 認証)を
//! 要求し、`SheetStore` には外部からデータを流し込む口が無いため、
//! **ローカル CSV だけでは呼べない**。visibility を変えれば解決するが、
//! `src/` は担当外なのでここでは触らない。詳細は `unverifiable` を参照。

use std::collections::{BTreeMap, HashMap};
use std::sync::Arc;

use rust_dashboard::handlers::call_quality::heatmap::{aggregate, CrossRow, HeatmapQuery};
// 2026-09-03: aggregate() が第3引数に監査を取るようになったのに追随しておらず、
// まっさら clone で `cargo test` が落ちていた(cargo test は examples もビルドする)。
use rust_dashboard::handlers::call_quality::query_audit::ValueAudit;
use rust_dashboard::handlers::call_quality::sheets::{query, SheetData, SheetQuery};
use rust_dashboard::handlers::call_quality::tabs::p1_members::{collect, MembersQuery};
use rust_dashboard::handlers::call_quality::tabs::rate;

// ---------------------------------------------------------------- CSV 読み

/// 引用符つきカンマを壊さない最小限の CSV 行分割（RFC4180 の必要部分だけ）。
/// メンバー名に「,」が入っていた場合に列がずれるのを防ぐ。
fn split_csv_line(line: &str) -> Vec<String> {
    let mut out = Vec::new();
    let mut cur = String::new();
    let mut in_quotes = false;
    let mut chars = line.chars().peekable();
    while let Some(c) = chars.next() {
        match c {
            '"' if in_quotes => {
                if chars.peek() == Some(&'"') {
                    cur.push('"');
                    chars.next();
                } else {
                    in_quotes = false;
                }
            }
            '"' => in_quotes = true,
            ',' if !in_quotes => {
                out.push(std::mem::take(&mut cur));
            }
            _ => cur.push(c),
        }
    }
    out.push(cur);
    out
}

/// CSV を `SheetData` にする。**列は全部そのまま載せる**。
/// ここで列を選ぶと「本番のシートには無い列を補ってしまう」ことが起き、
/// 列名の取り違えを突合で検出できなくなる（実際にそれが1件見つかっている）。
fn load_sheet(path: &std::path::Path) -> Result<SheetData, Box<dyn std::error::Error>> {
    let text = std::fs::read_to_string(path)
        .map_err(|e| format!("{} を読めません: {e}", path.display()))?;
    let text = text.trim_start_matches('\u{feff}');
    let mut lines = text.lines();
    let header: Vec<String> = split_csv_line(lines.next().ok_or("空ファイル")?)
        .into_iter()
        .map(|s| s.trim_start_matches('\u{feff}').trim().to_string())
        .collect();
    let n = header.len();

    // 値の種類が少ない列を intern して 20万行でもメモリを抑える（sheets.rs と同じ方針）
    let mut interner: HashMap<String, Arc<str>> = HashMap::new();
    let mut intern = |s: &str| -> Arc<str> {
        if let Some(a) = interner.get(s) {
            return Arc::clone(a);
        }
        let a: Arc<str> = Arc::from(s);
        interner.insert(s.to_string(), Arc::clone(&a));
        a
    };

    let mut rows = Vec::new();
    for line in lines {
        if line.trim().is_empty() {
            continue;
        }
        let f = split_csv_line(line);
        let mut row: Vec<Arc<str>> = Vec::with_capacity(n);
        for i in 0..n {
            row.push(intern(f.get(i).map(|s| s.trim()).unwrap_or("")));
        }
        rows.push(row);
    }
    Ok(SheetData {
        header,
        rows,
        fetched_at: std::time::Instant::now(),
    })
}

// ---------------------------------------------------------------- 収集器

/// path → 値。並びを固定したいので BTreeMap。
#[derive(Default)]
struct Values {
    nums: BTreeMap<String, Option<f64>>,
    strs: BTreeMap<String, String>,
}

impl Values {
    fn num(&mut self, path: impl Into<String>, v: f64) {
        self.nums.insert(path.into(), Some(v));
    }
    fn opt(&mut self, path: impl Into<String>, v: Option<f64>) {
        self.nums.insert(path.into(), v);
    }
    fn flag(&mut self, path: impl Into<String>, v: bool) {
        self.nums.insert(path.into(), Some(if v { 1.0 } else { 0.0 }));
    }
    fn text(&mut self, path: impl Into<String>, v: impl Into<String>) {
        self.strs.insert(path.into(), v.into());
    }
}

fn main() -> Result<(), Box<dyn std::error::Error>> {
    let mut args = std::env::args().skip(1);
    let dir = args.next().ok_or(
        "CSV ディレクトリを渡してください\n  例: cargo run --release --example verify_against_gas -- \"C:/Users/fuji1/OneDrive/デスクトップ/Hubspot/data/call_quality_monitor\"",
    )?;
    let dir = std::path::PathBuf::from(dir);
    let mut expected_path = dir.join("_parity_gas_expected.json");
    let mut out_path = dir.join("_parity_rust_actual.json");
    while let Some(a) = args.next() {
        match a.as_str() {
            "--expected" => expected_path = args.next().ok_or("--expected の値がありません")?.into(),
            "--out" => out_path = args.next().ok_or("--out の値がありません")?.into(),
            other => return Err(format!("不明な引数: {other}").into()),
        }
    }

    eprintln!("[1/4] CSV 読込 …");
    let monthly = load_sheet(&dir.join("feature_monthly.csv"))?;
    let owners_sheet = load_sheet(&dir.join("owner_master.csv"))?;
    let cross = load_sheet(&dir.join("time_heatmap_cross.csv"))?;
    eprintln!(
        "      月次明細 {} 行 / メンバーマスタ {} 行 / 時間帯ヒート_クロス {} 行",
        monthly.rows.len(),
        owners_sheet.rows.len(),
        cross.rows.len()
    );

    let mut v = Values::default();

    // ---- メンバー: role=sales の一覧（全タブの既定スコープの土台） ----
    let mut sales_owners: Vec<String> = Vec::new();
    let mut role_counts: BTreeMap<String, usize> = BTreeMap::new();
    for row in &owners_sheet.rows {
        let id = owners_sheet.get(row, "owner_id").trim().to_string();
        if id.is_empty() {
            continue;
        }
        let role = owners_sheet.get(row, "role").trim();
        let role = if role.is_empty() { "other" } else { role };
        *role_counts.entry(role.to_string()).or_insert(0) += 1;
        if role == "sales" {
            sales_owners.push(id);
        }
    }
    sales_owners.sort();
    sales_owners.dedup();
    for (r, c) in &role_counts {
        v.num(format!("members.role_count.{r}"), *c as f64);
    }
    v.num("members.sales_count", sales_owners.len() as f64);

    // ---- 対象月の一覧 ----
    let mut months: Vec<String> = monthly
        .rows
        .iter()
        .map(|r| monthly.get(r, "year_month").trim().to_string())
        .filter(|s| !s.is_empty())
        .collect();
    months.sort();
    months.dedup();
    v.num("months.count", months.len() as f64);

    // ================================================================
    // 突合1: アポ率 / 足切り / NA遵守率  ―  tabs::p1_members::collect()
    // ================================================================
    eprintln!("[2/4] p1_members::collect（アポ率・足切り・NA遵守率）…");

    // (a) 全期間・メンバー未選択 → role=sales が既定スコープ
    let q_all = MembersQuery::default();
    let (rows_all, matched_all) = collect(&monthly, &q_all, Some(&sales_owners));
    v.num("p1.all.matched_rows", matched_all as f64);
    v.num("p1.all.owner_count", rows_all.len() as f64);
    for m in &rows_all {
        let p = format!("p1.all.owner.{}", m.owner_id);
        v.num(format!("{p}.call_count"), m.call_count);
        v.num(format!("{p}.zoom_dial_count"), m.zoom_dial_count);
        v.num(format!("{p}.apo_count"), m.apo_count);
        v.num(format!("{p}.denominator"), m.denominator);
        v.opt(format!("{p}.apo_rate"), m.apo_rate);
        v.num(format!("{p}.na_due"), m.na_due);
        v.num(format!("{p}.na_done_ontime"), m.na_done_ontime);
        v.opt(format!("{p}.na_rate"), m.na_rate);
        v.opt(format!("{p}.avg_talk_secs"), m.avg_talk_secs);
        // 足切り: 「誰が入って誰が落ちるか」の本体。率と同じ分母で判定される。
        v.flag(format!("{p}.thin"), m.thin);
    }
    // 足切り通過者の集合と順位（順位は apo_rate 降順 → 同率は owner_id 昇順）
    let mut qualified: Vec<_> = rows_all.iter().filter(|m| !m.thin).collect();
    qualified.sort_by(|a, b| {
        b.apo_rate
            .unwrap_or(0.0)
            .partial_cmp(&a.apo_rate.unwrap_or(0.0))
            .unwrap_or(std::cmp::Ordering::Equal)
            .then_with(|| a.owner_id.cmp(&b.owner_id))
    });
    v.num("p1.all.qualified_count", qualified.len() as f64);
    for (i, m) in qualified.iter().enumerate() {
        v.text(format!("p1.all.rank.{:03}", i + 1), m.owner_id.clone());
    }

    // (b) 都道府県モード: 分母が Zoom発信 → HubSpot Call に切り替わる
    //     ※ ここで検証するのは **分母選択の規則** のみ。
    //        GAS はデータソース自体を「都道府県月次」に差し替えるが、
    //        その差し替えは未突合（unverifiable 参照）。同じ CSV を両者に食わせる。
    let q_pref = MembersQuery {
        prefecture: Some("東京都".to_string()),
        ..Default::default()
    };
    let (rows_pref, _) = collect(&monthly, &q_pref, Some(&sales_owners));
    for m in &rows_pref {
        let p = format!("p1.pref.owner.{}", m.owner_id);
        v.num(format!("{p}.denominator"), m.denominator);
        v.opt(format!("{p}.apo_rate"), m.apo_rate);
        v.flag(format!("{p}.thin"), m.thin);
    }

    // (c) メンバー個別選択: role を問わずその人だけになること
    //     先頭2名の sales と、あえて sales でない owner を1名混ぜる
    let mut picked: Vec<String> = sales_owners.iter().take(2).cloned().collect();
    let non_sales: Option<String> = owners_sheet.rows.iter().find_map(|row| {
        let id = owners_sheet.get(row, "owner_id").trim().to_string();
        let role = owners_sheet.get(row, "role").trim();
        if !id.is_empty() && role == "bpo" {
            Some(id)
        } else {
            None
        }
    });
    if let Some(id) = non_sales.clone() {
        picked.push(id);
    }
    v.text("p1.picked.owners", picked.join(","));
    let q_pick = MembersQuery {
        owners: Some(picked.join(",")),
        ..Default::default()
    };
    let (rows_pick, matched_pick) = collect(&monthly, &q_pick, Some(&sales_owners));
    v.num("p1.picked.matched_rows", matched_pick as f64);
    v.num("p1.picked.owner_count", rows_pick.len() as f64);
    for m in &rows_pick {
        v.num(format!("p1.picked.owner.{}.call_count", m.owner_id), m.call_count);
        v.opt(format!("p1.picked.owner.{}.apo_rate", m.owner_id), m.apo_rate);
    }

    // ================================================================
    // 突合3: 月次推移  ―  collect() を月ごとに呼ぶ
    // ================================================================
    eprintln!("[3/4] 月次推移（{} ヶ月）…", months.len());
    for ym in &months {
        let q = MembersQuery {
            year_month: Some(ym.clone()),
            ..Default::default()
        };
        let (rows_m, matched_m) = collect(&monthly, &q, Some(&sales_owners));
        v.num(format!("p1.month.{ym}.matched_rows"), matched_m as f64);
        v.num(format!("p1.month.{ym}.owner_count"), rows_m.len() as f64);
        // 全社合計は単純和（harness-sum）。分母選択の規則は owner 単位で突合済み。
        let sum = |f: fn(&rust_dashboard::handlers::call_quality::tabs::p1_members::MemberRow) -> f64| -> f64 {
            rows_m.iter().map(f).sum()
        };
        v.num(format!("p1.month.{ym}.total.call_count"), sum(|m| m.call_count));
        v.num(format!("p1.month.{ym}.total.zoom_dial_count"), sum(|m| m.zoom_dial_count));
        v.num(format!("p1.month.{ym}.total.apo_count"), sum(|m| m.apo_count));
        v.num(format!("p1.month.{ym}.total.na_due"), sum(|m| m.na_due));
        v.num(format!("p1.month.{ym}.total.na_done_ontime"), sum(|m| m.na_done_ontime));
        for m in &rows_m {
            let p = format!("p1.month.{ym}.owner.{}", m.owner_id);
            v.num(format!("{p}.call_count"), m.call_count);
            v.num(format!("{p}.apo_count"), m.apo_count);
            v.num(format!("{p}.denominator"), m.denominator);
            v.opt(format!("{p}.apo_rate"), m.apo_rate);
        }
    }

    // ================================================================
    // 突合5: 時間帯×曜日ヒート  ―  heatmap::aggregate()
    // ================================================================
    eprintln!("[4/4] heatmap::aggregate（時間帯×曜日）…");
    // CrossRow への正規化。`fetch_cross_rows()` が private なので、
    // **同じ行除外規則**（weekday>6 / hour>23 は集計に載せない）をここで再現する。
    // これは読み込み側の規則であって集計式ではない。Python 側にも同じ規則を書く。
    let mut cross_rows: Vec<CrossRow> = Vec::with_capacity(cross.rows.len());
    let mut skipped = 0usize;
    for row in &cross.rows {
        let wd = cross.get(row, "weekday").trim().parse::<u8>().unwrap_or(255);
        let hr = cross.get(row, "hour").trim().parse::<u8>().unwrap_or(255);
        if wd > 6 || hr > 23 {
            skipped += 1;
            continue;
        }
        let n = |s: &str| -> u32 { s.trim().parse::<f64>().ok().map(|x| x as u32).unwrap_or(0) };
        cross_rows.push(CrossRow {
            owner_id: cross.get(row, "owner_id").trim().parse::<u64>().unwrap_or(0),
            weekday: wd,
            hour: hr,
            prefecture: Arc::from(cross.get(row, "prefecture")),
            industry: Arc::from(cross.get(row, "industry")),
            dial_count: n(cross.get(row, "dial_count")),
            connect_count: n(cross.get(row, "connect_count")),
            apo_count: n(cross.get(row, "apo_count")),
        });
    }
    v.num("heat.loaded_rows", cross_rows.len() as f64);
    v.num("heat.skipped_rows", skipped as f64);

    // (a) 営業スコープあり（本来の姿。GAS も 2026-08-13 にこちらへ是正済み）
    let q_sales = HeatmapQuery {
        owners: Some(sales_owners.join(",")),
        ..Default::default()
    };
    let (cells_sales, used_sales) = aggregate(&cross_rows, &q_sales, &mut ValueAudit::new());
    v.num("heat.sales.used_rows", used_sales as f64);
    v.num("heat.sales.cell_count", cells_sales.len() as f64);
    for c in &cells_sales {
        let p = format!("heat.sales.cell.{}_{}", c.weekday, c.hour);
        v.num(format!("{p}.dial"), c.dial as f64);
        v.num(format!("{p}.connect"), c.connect as f64);
        v.num(format!("{p}.apo"), c.apo as f64);
        v.opt(format!("{p}.apo_rate"), c.apo_rate);
    }

    // (b) 営業スコープなし（全ロール混在）。GAS 旧版の姿。
    //     「営業に絞るかどうかで値がどれだけ動くか」を数字で残すために出す。
    let (cells_all, used_all) = aggregate(&cross_rows, &HeatmapQuery::default(), &mut ValueAudit::new());
    v.num("heat.allroles.used_rows", used_all as f64);
    v.num("heat.allroles.cell_count", cells_all.len() as f64);
    for c in &cells_all {
        let p = format!("heat.allroles.cell.{}_{}", c.weekday, c.hour);
        v.num(format!("{p}.dial"), c.dial as f64);
        v.opt(format!("{p}.apo_rate"), c.apo_rate);
    }

    // (c) 都道府県で絞る（cross シート固有の軸）
    let q_pref_heat = HeatmapQuery {
        prefecture: Some("東京都".to_string()),
        owners: Some(sales_owners.join(",")),
        ..Default::default()
    };
    let (cells_tokyo, used_tokyo) = aggregate(&cross_rows, &q_pref_heat, &mut ValueAudit::new());
    v.num("heat.tokyo.used_rows", used_tokyo as f64);
    v.num("heat.tokyo.cell_count", cells_tokyo.len() as f64);
    for c in &cells_tokyo {
        let p = format!("heat.tokyo.cell.{}_{}", c.weekday, c.hour);
        v.num(format!("{p}.dial"), c.dial as f64);
        v.opt(format!("{p}.apo_rate"), c.apo_rate);
    }

    // ================================================================
    // 突合: rate() の境界（分母0 / 浮動小数点）
    // ================================================================
    v.opt("rate.0_over_0", rate(0.0, 0.0));
    v.opt("rate.3_over_0", rate(3.0, 0.0));
    v.opt("rate.0_over_100", rate(0.0, 100.0));
    // 7/1000*100 は 0.7000000000000001 になる。完全一致を要求すると落ちる代表例。
    v.opt("rate.7_over_1000", rate(7.0, 1000.0));
    v.opt("rate.1_over_3", rate(1.0, 3.0));

    // ================================================================
    // 突合: sheets::query() の group_by / sum
    // ================================================================
    let mut f = HashMap::new();
    f.insert("pipeline".to_string(), String::new()); // 空 = 絞らない
    let resp = query(
        &monthly,
        &SheetQuery {
            filter: f,
            group_by: vec!["year_month".to_string()],
            sum: vec!["call_count".to_string(), "apo_count".to_string()],
            limit: None,
        },
    );
    v.num("query.bymonth.group_count", resp.rows.len() as f64);
    v.num("query.bymonth.matched_rows", resp.matched_rows as f64);
    v.flag("query.bymonth.truncated", resp.truncated);
    for r in &resp.rows {
        if r.len() >= 3 {
            v.num(
                format!("query.bymonth.{}.call_count", r[0]),
                r[1].parse::<f64>().unwrap_or(f64::NAN),
            );
            v.num(
                format!("query.bymonth.{}.apo_count", r[0]),
                r[2].parse::<f64>().unwrap_or(f64::NAN),
            );
        }
    }

    // ---------------------------------------------------------------- 出力

    let unverifiable = serde_json::json!([
        {
            "item": "p0_overview（全社サマリ: KPI / 月次3連 / トップ・要支援）",
            "reason": "集計関数 collect() が private。公開は handle() のみで、SheetsClient(実HTTP+認証) を要求する。SheetStore に外部からデータを入れる口も無いためローカル CSV では呼べない。src/ は担当外のため visibility を変更していない。`fn collect` → `pub fn collect` 1箇所で解消する。"
        },
        {
            "item": "p2_habits（習慣の差: NA消化率 / 再架電遵守・消化 / 担当流動率 / 新規開拓率 / ステージ移行率 / N回目架電 / リサイクル間隔 / コンプライアンス / ファネル / 滞留日数 / その場失注 / 商談遷移）",
            "reason": "同上。build_* が全て private。ただし本ファイルの実装時に 23 件のユニットテストで実データ相当の値を確認済み。突合対象に入れるには build_scorecards / build_rankings / build_scatters 等を pub にする必要がある。"
        },
        {
            "item": "pja / p12 / p13（求人応募・解約分析・案件タイムライン）",
            "reason": "同上（公開は handle() 系のみ）。"
        },
        {
            "item": "都道府県モードのデータソース差し替え（シート「都道府県月次」）",
            "reason": "GAS は都道府県選択時に土台シートごと差し替えるが、ローカルに feature_prefecture_monthly.csv はあるものの p1_members::collect は prefecture 列を見ない（分母切替のみ）。ここでは分母選択の規則だけを突合している。"
        },
        {
            "item": "NA消化率 / 再架電日 遵守率・消化率",
            "reason": "p1_members は na_due / na_done_ontime しか集計しない（na_done_total / rs_* は p2_habits 側）。p2_habits が private のため未突合。"
        }
    ]);

    let payload = serde_json::json!({
        "generated_at": chrono::Local::now().to_rfc3339(),
        "csv_dir": dir.to_string_lossy(),
        "meta": {
            "monthly_rows": monthly.rows.len(),
            "cross_rows": cross.rows.len(),
            "owner_master_rows": owners_sheet.rows.len(),
            "sales_owners": sales_owners.len(),
            "months": months,
        },
        "values": v.nums,
        "strings": v.strs,
        "unverifiable": unverifiable,
    });
    std::fs::write(&out_path, serde_json::to_string_pretty(&payload)?)?;
    eprintln!(
        "実測値を書き出しました: {} （数値 {} 件 / 文字列 {} 件）",
        out_path.display(),
        payload["values"].as_object().map(|o| o.len()).unwrap_or(0),
        payload["strings"].as_object().map(|o| o.len()).unwrap_or(0),
    );

    // ---------------------------------------------------------------- 突合

    if !expected_path.exists() {
        println!();
        println!("期待値ファイルが見つかりません: {}", expected_path.display());
        println!("先に GAS 版の期待値を作ってください:");
        println!(
            "  python \"C:\\Users\\fuji1\\OneDrive\\デスクトップ\\Hubspot\\scripts\\call_quality_monitor\\_verify_rust_parity.py\""
        );
        return Ok(());
    }
    let expected: serde_json::Value =
        serde_json::from_str(&std::fs::read_to_string(&expected_path)?)?;
    report(&payload, &expected);
    Ok(())
}

/// 期待値と実測値を突き合わせて 4 分類で出す。
///
/// 分類規則そのもの（何が「意図した差異」か）は期待値ファイルの `diff` に入っている。
/// ここは判定を持たない。
fn report(actual: &serde_json::Value, expected: &serde_json::Value) {
    let a_nums = actual["values"].as_object().cloned().unwrap_or_default();
    let a_strs = actual["strings"].as_object().cloned().unwrap_or_default();
    let exp = expected["expected"].as_object().cloned().unwrap_or_default();

    let mut ok = 0usize;
    let mut intended: Vec<(String, String, String)> = Vec::new();
    let mut bad: Vec<(String, String, String)> = Vec::new();
    let mut missing_in_rust: Vec<String> = Vec::new();

    let fmt = |v: &serde_json::Value| -> String {
        if v.is_null() {
            "null".to_string()
        } else {
            v.to_string()
        }
    };

    for (path, spec) in &exp {
        let want = &spec["value"];
        let tol = spec["tol"].as_f64().unwrap_or(1e-9);
        let diff_note = spec["diff"].as_str();

        let got: Option<serde_json::Value> = a_nums
            .get(path)
            .cloned()
            .or_else(|| a_strs.get(path).cloned());
        let Some(got) = got else {
            missing_in_rust.push(path.clone());
            continue;
        };

        let same = match (want.as_f64(), got.as_f64()) {
            (Some(w), Some(g)) => {
                let scale = w.abs().max(g.abs()).max(1.0);
                (w - g).abs() <= tol * scale
            }
            // どちらも数値でない場合は文字列 / null として厳密比較
            _ => want == &got,
        };

        if same {
            ok += 1;
        } else if let Some(note) = diff_note {
            intended.push((path.clone(), format!("GAS={} / Rust={}", fmt(want), fmt(&got)), note.to_string()));
        } else {
            bad.push((path.clone(), format!("GAS={} / Rust={}", fmt(want), fmt(&got)), String::new()));
        }
    }

    let extra: Vec<&String> = a_nums
        .keys()
        .chain(a_strs.keys())
        .filter(|k| !exp.contains_key(*k))
        .collect();

    println!();
    println!("================ GAS版 ⇔ Rust版 突合結果 ================");
    println!("突合した項目          : {}", exp.len());
    println!("  一致                : {ok}");
    println!("  意図した差異        : {}", intended.len());
    println!("  要調査の不一致      : {}", bad.len());
    println!("未突合(Rust側に値なし): {}", missing_in_rust.len());
    println!("未突合(GAS側に期待値なし): {}", extra.len());

    if !intended.is_empty() {
        println!();
        println!("---- 意図した差異（GAS 側が旧仕様。Rust が正しい） ----");
        // 同じ理由のものは代表1件 + 件数でまとめる（同じ話を数百行出さない）
        let mut by_note: BTreeMap<String, (usize, String, String)> = BTreeMap::new();
        for (p, d, note) in &intended {
            let e = by_note.entry(note.clone()).or_insert((0, p.clone(), d.clone()));
            e.0 += 1;
        }
        for (note, (n, p, d)) in by_note {
            println!("  [{n:>5}件] {note}");
            println!("           例: {p}  {d}");
        }
    }

    if !bad.is_empty() {
        println!();
        println!("---- ★要調査の不一致 ----");
        for (p, d, _) in bad.iter().take(60) {
            println!("  {p}  {d}");
        }
        if bad.len() > 60 {
            println!("  … 他 {} 件", bad.len() - 60);
        }
    }

    if !missing_in_rust.is_empty() {
        println!();
        println!("---- 未突合: GAS 側に期待値があるが Rust 側に同じ path が無い ----");
        for p in missing_in_rust.iter().take(20) {
            println!("  {p}");
        }
        if missing_in_rust.len() > 20 {
            println!("  … 他 {} 件", missing_in_rust.len() - 20);
        }
    }

    if let Some(un) = actual["unverifiable"].as_array() {
        println!();
        println!("---- 未突合: そもそも呼べない項目 ----");
        for u in un {
            println!("  ● {}", u["item"].as_str().unwrap_or("?"));
            println!("    理由: {}", u["reason"].as_str().unwrap_or("?"));
        }
    }

    println!();
    if bad.is_empty() {
        println!("結果: 要調査の不一致は 0 件。");
    } else {
        println!("結果: ★要調査の不一致が {} 件あります。上記を確認してください。", bad.len());
    }
    println!("========================================================");
}
