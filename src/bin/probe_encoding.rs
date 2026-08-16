//! 媒体分析 CSV パーサのエンコーディング / ソース判定を実データで検査する一時バイナリ。
//!
//! 既存コードは一切変更せず、公開関数 (decode_csv_bytes / detect_csv_source /
//! parse_csv_bytes / parse_csv_bytes_with_hints) をそのまま呼んで
//! 「生の行数 / パース件数 / ソース判定 / 検出エンコーディング / 欠損率」および
//! 「自動判定 vs 媒体明示指定の差分」を出力する。コミット対象ではない。
//!
//! 実行:
//!   cargo run --release --bin probe_encoding -- <CSV パス>...

use rust_dashboard::handlers::survey::upload::{
    decode_csv_bytes, detect_csv_source, parse_csv_bytes, parse_csv_bytes_with_hints, SurveyRecord,
    UserSourceHint,
};

fn trunc(s: &str) -> String {
    s.chars().take(28).collect()
}

/// 2 つのパース結果を row_index で突き合わせ、主要フィールドの不一致件数を返す
fn diff_records(a: &[SurveyRecord], b: &[SurveyRecord]) -> (usize, usize, usize, usize, usize) {
    let map_b: std::collections::HashMap<usize, &SurveyRecord> =
        b.iter().map(|r| (r.row_index, r)).collect();
    let (mut d_title, mut d_comp, mut d_sal, mut d_emp, mut d_desc) = (0, 0, 0, 0, 0);
    for ra in a {
        if let Some(rb) = map_b.get(&ra.row_index) {
            if ra.job_title != rb.job_title {
                d_title += 1;
            }
            if ra.company_name != rb.company_name {
                d_comp += 1;
            }
            if ra.salary_raw != rb.salary_raw {
                d_sal += 1;
            }
            if ra.employment_type != rb.employment_type {
                d_emp += 1;
            }
            if ra.description != rb.description {
                d_desc += 1;
            }
        }
    }
    (d_title, d_comp, d_sal, d_emp, d_desc)
}

fn main() {
    let args: Vec<String> = std::env::args().skip(1).collect();
    if args.is_empty() {
        eprintln!("使い方: probe_encoding <CSV パス>...");
        std::process::exit(2);
    }

    println!("file\tencoding\thdr_cols\traw_rows\tparsed\tretention%\tsource_auto\tempty_desc\tempty_emp\tnew_ratio%\tvs_forced_Indeed(title/comp/sal/emp/desc)");
    for path in &args {
        let short = path.rsplit(['/', '\\']).next().unwrap_or(path).to_string();
        let data = match std::fs::read(path) {
            Ok(d) => d,
            Err(e) => {
                println!("{short}\tREAD_ERROR: {e}");
                continue;
            }
        };
        let (decoded, enc) = decode_csv_bytes(&data);

        let mut rdr = csv::ReaderBuilder::new()
            .flexible(true)
            .has_headers(true)
            .from_reader(decoded.as_slice());
        let headers: Vec<String> = match rdr.headers() {
            Ok(h) => h.iter().map(|s| s.to_string()).collect(),
            Err(e) => {
                println!("{short}\t{enc}\tHEADER_ERROR: {e}");
                continue;
            }
        };
        let raw_rows = rdr.records().filter(|r| r.is_ok()).count();
        let source = detect_csv_source(&headers);

        let auto = match parse_csv_bytes(&data, None) {
            Ok(r) => r,
            Err(e) => {
                println!(
                    "{short}\t{enc}\t{}\t{raw_rows}\t0\t0.0\t{source:?}\t-\t-\t-\tPARSE_ERROR: {e}",
                    headers.len()
                );
                continue;
            }
        };
        let forced =
            parse_csv_bytes_with_hints(&data, None, UserSourceHint::Indeed).unwrap_or_default();

        let n = auto.len();
        let pct = if raw_rows == 0 {
            0.0
        } else {
            n as f64 * 100.0 / raw_rows as f64
        };
        let empty_desc = auto.iter().filter(|r| r.description.is_empty()).count();
        let empty_emp = auto.iter().filter(|r| r.employment_type.is_empty()).count();
        let empty_title = auto.iter().filter(|r| r.job_title.is_empty()).count();
        let empty_comp = auto.iter().filter(|r| r.company_name.is_empty()).count();
        // 雇用形態として妥当でない値 (タグ文字列の混入) を数える
        const KNOWN_EMP: [&str; 8] = [
            "正社員",
            "契約社員",
            "アルバイト・パート",
            "パート",
            "アルバイト",
            "業務委託",
            "派遣社員",
            "新卒",
        ];
        let bogus_emp: Vec<&str> = auto
            .iter()
            .map(|r| r.employment_type.as_str())
            .filter(|e| !e.is_empty() && !KNOWN_EMP.iter().any(|k| e.contains(k)))
            .collect();
        eprintln!(
            "@@ {short}: empty_title={empty_title} empty_company={empty_comp} bogus_emp={} 例={:?}",
            bogus_emp.len(),
            bogus_emp
                .iter()
                .take(3)
                .map(|s| trunc(s))
                .collect::<Vec<_>>()
        );
        let new_ratio = if n == 0 {
            0.0
        } else {
            auto.iter().filter(|r| r.is_new).count() as f64 * 100.0 / n as f64
        };
        let (dt, dc, ds, de, dd) = diff_records(&auto, &forced);
        println!(
            "{short}\t{enc}\t{}\t{raw_rows}\t{n}\t{pct:.1}\t{source:?}\t{empty_desc}\t{empty_emp}\t{new_ratio:.1}\t{dt}/{dc}/{ds}/{de}/{dd} (forced_n={})",
            headers.len(),
            forced.len()
        );
        // 雇用形態が食い違ったサンプルを 2 件表示
        let map_f: std::collections::HashMap<usize, &SurveyRecord> =
            forced.iter().map(|r| (r.row_index, r)).collect();
        let mut shown = 0;
        for ra in &auto {
            if shown >= 2 {
                break;
            }
            if let Some(rf) = map_f.get(&ra.row_index) {
                if ra.employment_type != rf.employment_type || ra.description != rf.description {
                    shown += 1;
                    eprintln!(
                        "    [{short} row{}] auto(emp={:?} desc={:?}) vs Indeed(emp={:?} desc={:?})",
                        ra.row_index,
                        trunc(&ra.employment_type),
                        trunc(&ra.description),
                        trunc(&rf.employment_type),
                        trunc(&rf.description)
                    );
                }
            }
        }
    }
}
